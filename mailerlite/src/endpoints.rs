use async_trait::async_trait;
use derive_builder::Builder;
use reqwest::{Client, RequestBuilder, Response};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::{MailerliteError, Subscriber, SubscriberStatus};

trait AddOptionalQueryParam {
    fn optional_query<T: Serialize + ?Sized>(self, key: &str, query: Option<&T>) -> Self;
}

impl AddOptionalQueryParam for RequestBuilder {
    fn optional_query<T: Serialize + ?Sized>(self, key: &str, query: Option<&T>) -> Self {
        if let Some(value) = query {
            self.query(&[(key, value)])
        } else {
            self
        }
    }
}

#[async_trait]
pub trait MailerliteEndpoint {
    type Response;
    fn to_request_builder(&self, base_url: &str, client: &Client) -> RequestBuilder;

    async fn handle_response(&self, response: Response) -> Result<Self::Response, MailerliteError>
    where
        Self::Response: DeserializeOwned,
    {
        Ok(response.json::<Self::Response>().await?)
    }
}

#[derive(Debug, Clone, Builder)]
pub struct ListSegmentSubscribersRequest {
    segment_id: String,
    #[builder(default)]
    filter_status: Option<SubscriberStatus>,
    #[builder(default)]
    limit: Option<u64>,
    #[builder(default)]
    after: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ListSegmentSubscribersResponse {
    data: Vec<Subscriber>,
    meta: ListSegmentSubscribersResponseMeta,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ListSegmentSubscribersResponseMeta {
    total: u64,
    count: u64,
    last: u64,
}

#[async_trait]
impl MailerliteEndpoint for ListSegmentSubscribersRequest {
    type Response = ListSegmentSubscribersResponse;

    fn to_request_builder(&self, base_url: &str, client: &Client) -> RequestBuilder {
        client
            .get(format!("{}/segments/{}/subscribers", base_url, self.segment_id))
            .optional_query("filter[status]", self.filter_status.as_ref())
            .optional_query("limit", self.limit.as_ref())
            .optional_query("after", self.after.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_list_segment_subscriber_request() {
        let req = ListSegmentSubscribersRequest {
            segment_id: "test_segment".to_string(),
            filter_status: Some(SubscriberStatus::Junk),
            limit: Some(5),
            after: Some("2".to_string()),
        };

        let builder = req.to_request_builder("https://localhost:1234/api", &Client::new());
        let request = builder.build().unwrap();

        let expected = reqwest::Url::parse(
            "https://localhost:1234/api/segments/test_segment/subscribers?filter%5Bstatus%5D=junk&limit=5&after=2",
        )
        .unwrap();

        assert_eq!(&expected, request.url());
    }
}
