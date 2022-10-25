use anyhow::{anyhow, Result};
use mailerlite::{
    endpoints::{ListSegmentSubscribersRequestBuilder, ListSegmentSubscribersResponse, WriteSubscriberRequestBuilder, WriteSubscriberResponse, GetSubscriberResponse, GetSubscriberRequest},
    MailerliteClient, MailerliteResponse, SubscriberFieldValue
};

pub struct Mailerlite {
    pub client: MailerliteClient,
    pub segments: MailerliteSegments,
}

pub struct MailerliteSegments {
    pub mailing_list: String,
    pub wait_list: String,
}

impl Mailerlite {
    pub fn new() -> Result<Self> {
        Ok(Self {
            client: MailerliteClient::new(std::env::var("MAILERLITE_API_KEY")?),
            segments: MailerliteSegments::new()?,
        })
    }

    pub async fn pending_mailing_list_subscribers(&self) -> Result<ListSegmentSubscribersResponse> {
        self.get_pending_list(&self.segments.mailing_list).await
    }

    pub async fn mark_mailing_list_subscriber(&self, email: &str) -> Result<WriteSubscriberResponse> {
        self.mark_subscriber(email, "mailing_list").await
    }

    pub async fn pending_wait_list_subscribers(&self) -> Result<ListSegmentSubscribersResponse> {
        self.get_pending_list(&self.segments.wait_list).await
    }

    pub async fn mark_wait_list_subscriber(&self, email: &str) -> Result<WriteSubscriberResponse> {
        self.mark_subscriber(email, "wait_list").await
    }

    async fn get_pending_list(&self, segment_id: &str) -> Result<ListSegmentSubscribersResponse> {
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

    async fn mark_subscriber(&self, email: &str, state_marker: &str) -> Result<WriteSubscriberResponse> {

        // Race condition here if multiple cios attempt to update the record at the same time
        let response = self.client.run(GetSubscriberRequest { subscriber_identifier: email.to_string() }).await?;

        match response {
            MailerliteResponse::AuthenticationError { .. } => {
                Err(anyhow!("Failed to authenticate with Mailerlite"))
            }
            MailerliteResponse::EndpointResponse(GetSubscriberResponse::Success { data: subscriber }) => {
                let fields = subscriber.fields;

                if let Some(Some(mut cio_state)) = subscriber.fields.get("cio_state") {
                    match cio_state {
                        SubscriberFieldValue::String(current_value) => {
                            let new_marker = state_marker.to_string();
                            let mut state_markers = current_value.split(',').map(|s| s.to_string()).collect::<Vec<String>>();

                            if !state_markers.contains(&new_marker) {
                                state_markers.push(new_marker);
                            }
        
                            fields.insert("cio_state".to_string(), Some(state_markers.join(",").into()));
                        }
                        _ => log::warn!("Invalid value type found for stored cio_state")
                    }
                    
                }

                self.client.run(
                        WriteSubscriberRequestBuilder::default().
                            email(email.to_string())
                            .fields(Some(fields))
                            .build()?
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
            MailerliteResponse::EndpointResponse(GetSubscriberResponse::NotFound) => Err(anyhow!("Failed to find subscriber"))
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
