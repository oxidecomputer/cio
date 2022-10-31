use anyhow::{anyhow, Result};
use mailerlite::{
    endpoints::{
        GetSubscriberRequestBuilder, GetSubscriberResponse, ListSegmentSubscribersRequestBuilder,
        ListSegmentSubscribersResponse, WriteSubscriberRequestBuilder, WriteSubscriberResponse,
    },
    MailerliteClient, MailerliteResponse, Subscriber, SubscriberFieldValue,
};

pub struct Mailerlite<Tz> {
    pub client: MailerliteClient<Tz>,
    pub segments: MailerliteSegments,
}

pub struct MailerliteSegments {
    pub mailing_list: String,
    pub wait_list: String,
}

impl Mailerlite<chrono_tz::Tz> {
    pub fn new() -> Result<Self> {
        let tz: chrono_tz::Tz = std::env::var("MAILERLITE_TIME_ZONE")?
            .parse()
            .map_err(|err| anyhow::anyhow!("Failed to parse mailerlite time zone from environment: {}", err))?;

        Ok(Self {
            client: MailerliteClient::new(std::env::var("MAILERLITE_API_KEY")?, tz),
            segments: MailerliteSegments::new()?,
        })
    }

    pub async fn pending_mailing_list_subscribers(&self) -> Result<ListSegmentSubscribersResponse<Subscriber>> {
        self.get_pending_list(&self.segments.mailing_list).await
    }

    pub async fn mark_mailing_list_subscriber(&self, email: &str) -> Result<WriteSubscriberResponse<Subscriber>> {
        self.mark_subscriber(email, "mailing_list").await
    }

    pub async fn pending_wait_list_subscribers(&self) -> Result<ListSegmentSubscribersResponse<Subscriber>> {
        self.get_pending_list(&self.segments.wait_list).await
    }

    pub async fn mark_wait_list_subscriber(&self, email: &str) -> Result<WriteSubscriberResponse<Subscriber>> {
        self.mark_subscriber(email, "wait_list").await
    }

    async fn get_pending_list(&self, segment_id: &str) -> Result<ListSegmentSubscribersResponse<Subscriber>> {
        self.client
            .run(
                ListSegmentSubscribersRequestBuilder::default()
                    .segment_id(segment_id.to_string())
                    .build()?,
            )
            .await
            .map_err(anyhow::Error::new)
            .and_then(|response| match response {
                MailerliteResponse::AuthenticationError { .. } => {
                    Err(anyhow!("Failed to authenticate with Mailerlite"))
                }
                MailerliteResponse::EndpointResponse(data) => Ok(data),
            })
    }

    async fn mark_subscriber(&self, email: &str, state_marker: &str) -> Result<WriteSubscriberResponse<Subscriber>> {
        // Race condition here if multiple cios attempt to update the record at the same time
        let response = self
            .client
            .run(
                GetSubscriberRequestBuilder::default()
                    .subscriber_identifier(email.to_string())
                    .build()?,
            )
            .await?;

        match response {
            MailerliteResponse::AuthenticationError { .. } => Err(anyhow!("Failed to authenticate with Mailerlite")),
            MailerliteResponse::EndpointResponse(GetSubscriberResponse::Success { data: subscriber }) => {
                let Subscriber { id, mut fields, .. } = subscriber;

                let new_value = if let Some(cio_state) = fields.get_mut("cio_state").and_then(|v| v.as_mut()) {
                    match cio_state {
                        SubscriberFieldValue::String(current_value) => {
                            let new_marker = state_marker.to_string();
                            let mut state_markers =
                                current_value.split(',').map(|s| s.to_string()).collect::<Vec<String>>();

                            if !state_markers.contains(&new_marker) {
                                state_markers.push(new_marker);
                            }

                            state_markers.join(",").into()
                        }
                        invalid_field_value => {
                            log::warn!("Invalid value type found for stored cio_state on subscriber {}", id);
                            invalid_field_value.to_owned()
                        }
                    }
                } else {
                    state_marker.to_string().into()
                };

                fields.insert("cio_state".to_string(), Some(new_value));

                self.client
                    .run(
                        WriteSubscriberRequestBuilder::default()
                            .email(email.to_string())
                            .fields(Some(fields))
                            .build()?,
                    )
                    .await
                    .map_err(anyhow::Error::new)
                    .and_then(|response| match response {
                        MailerliteResponse::AuthenticationError { .. } => {
                            Err(anyhow!("Failed to authenticate with Mailerlite"))
                        }
                        MailerliteResponse::EndpointResponse(data) => Ok(data),
                    })
            }
            MailerliteResponse::EndpointResponse(GetSubscriberResponse::NotFound) => {
                Err(anyhow!("Failed to find subscriber"))
            }
        }
    }
}

impl MailerliteSegments {
    pub fn new() -> Result<Self> {
        Ok(MailerliteSegments {
            mailing_list: std::env::var("MAILERLITE_MAILING_LIST_SEGMENT")?,
            wait_list: std::env::var("MAILERLITE_WAIT_LIST_SEGMENT")?,
        })
    }
}
