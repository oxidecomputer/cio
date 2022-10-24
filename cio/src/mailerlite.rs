use anyhow::{anyhow, Result};
use mailerlite::{
    endpoints::{ListSegmentSubscribersRequestBuilder, ListSegmentSubscribersResponse},
    MailerliteClient, MailerliteResponse,
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

    pub async fn pending_wait_list_subscribers(&self) -> Result<ListSegmentSubscribersResponse> {
        self.get_pending_list(&self.segments.wait_list).await
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
}

impl MailerliteSegments {
    pub fn new() -> Result<Self> {
        Ok(MailerliteSegments {
            mailing_list: std::env::var("MAILERLITE_MAILING_LIST_SEGMENT")?,
            wait_list: std::env::var("MAILERLITE_WAIT_LIST_SEGMENT")?,
        })
    }
}
