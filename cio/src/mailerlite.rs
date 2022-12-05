use anyhow::{anyhow, Result};
use mailerlite::{
    endpoints::{
        GetSubscriberRequestBuilder, GetSubscriberResponse, ListSegmentSubscribersRequestBuilder,
        ListSegmentSubscribersResponse, WriteSubscriberRequestBuilder, WriteSubscriberResponse,
    },
    MailerliteClient, MailerliteResponse, Subscriber, SubscriberFieldValue,
};
use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub struct Mailerlite<Tz> {
    pub client: MailerliteClient<Tz>,
    pub segments: MailerliteSegments,
}

#[derive(Debug)]
pub struct MailerliteSegments {
    pub mailing_list: String,
    pub wait_list: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CioState {
    #[serde(default)]
    processed_groups: Vec<String>,
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

    pub async fn pending_mailing_list_subscribers(&self) -> Result<Vec<Subscriber>> {
        self.get_pending_list(&self.segments.mailing_list).await
    }

    pub async fn mark_mailing_list_subscriber(&self, email: &str) -> Result<WriteSubscriberResponse<Subscriber>> {
        self.mark_subscriber(email, "mailing_list").await
    }

    pub async fn pending_wait_list_subscribers(&self) -> Result<Vec<Subscriber>> {
        self.get_pending_list(&self.segments.wait_list).await
    }

    pub async fn mark_wait_list_subscriber(&self, email: &str) -> Result<WriteSubscriberResponse<Subscriber>> {
        self.mark_subscriber(email, "wait_list").await
    }

    async fn get_pending_list_page(
        &self,
        segment_id: &str,
        start: Option<u64>,
    ) -> Result<ListSegmentSubscribersResponse<Subscriber>> {
        let mut req = ListSegmentSubscribersRequestBuilder::default()
            .segment_id(segment_id.to_string())
            .limit(1000);

        if let Some(start) = start {
            req = req.after(start);
        }

        self.client
            .run(req.build()?)
            .await
            .map_err(anyhow::Error::new)
            .and_then(|response| match response {
                MailerliteResponse::AuthenticationError { .. } => {
                    Err(anyhow!("Failed to authenticate with Mailerlite"))
                }
                MailerliteResponse::EndpointResponse(data) => Ok(data),
            })
    }

    async fn get_pending_list(&self, segment_id: &str) -> Result<Vec<Subscriber>> {
        let mut subscribers: Vec<Subscriber> = vec![];

        // We are going to loop below until we hit our expected total. This is an unfortunate side
        // effect to the API not have a good way of reporting when you have hit the end of a
        // paginated response. Instead the API returns a generic "Server Error" response. We
        // snapshot the total on the first iteration as more users may be added to our list as we
        // are looping. This does not address the case of users being removed though. If users are
        // removed while a loop is progressing, then this block is likely to hit the error case.
        let mut total: Option<u64> = None;
        let mut last: Option<u64> = None;

        loop {
            let response = self.get_pending_list_page(segment_id, last).await?;

            match response {
                ListSegmentSubscribersResponse::Success { mut data, meta } => {
                    subscribers.append(&mut data);

                    total = total.or(Some(meta.total));
                    last = meta.last;

                    if subscribers.len() >= total.unwrap_or(0) as usize {
                        break;
                    }
                }
                ListSegmentSubscribersResponse::Error { message } => {
                    return Err(anyhow!(
                        "Requesting segment {} from Mailerlite failed with {}",
                        segment_id,
                        message
                    ))
                }
            }
        }

        Ok(subscribers)
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

        println!("get resp {:#?}", response);

        match response {
            MailerliteResponse::AuthenticationError { .. } => Err(anyhow!("Failed to authenticate with Mailerlite")),
            MailerliteResponse::EndpointResponse(GetSubscriberResponse::Success { data: subscriber }) => {
                let Subscriber { id, mut fields, .. } = subscriber;

                let new_value = if let Some(cio_state) = fields.get_mut("cio_state").and_then(|v| v.as_mut()) {
                    match cio_state {
                        SubscriberFieldValue::String(current_value) => {
                            let new_marker = state_marker.to_string();

                            let mut state: CioState = serde_json::from_str(current_value)?;

                            if !state.processed_groups.contains(&new_marker) {
                                state.processed_groups.push(new_marker);
                            }

                            serde_json::to_string(&state)?.into()
                        }
                        invalid_field_value => {
                            log::warn!("Invalid value type found for stored cio_state on subscriber {}", id);
                            invalid_field_value.to_owned()
                        }
                    }
                } else {
                    serde_json::to_string(&CioState {
                        processed_groups: vec![state_marker.to_string()],
                    })?
                    .into()
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
