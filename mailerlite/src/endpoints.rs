use async_trait::async_trait;
use derive_builder::Builder;
use reqwest::{Client, RequestBuilder, Response};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{collections::HashMap, net::Ipv4Addr};

use crate::{MailerliteError, Subscriber, SubscriberStatus, SubscriberFields, FormattedDateTime};

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

#[derive(Debug, Clone, Serialize, Deserialize, Builder)]
pub struct GetSubscriberRequest {
    /// Subscriber identifer can be either and id number or an email
    subscriber_identifier: String
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GetSubscriberResponse {
    Success {
        data: Subscriber
    },
    NotFound
}

#[async_trait]
impl MailerliteEndpoint for GetSubscriberRequest {
    type Response = GetSubscriberResponse;

    fn to_request_builder(&self, base_url: &str, client: &Client) -> RequestBuilder {
        client.get(format!("{}/subscribers/{}", base_url, self.subscriber_identifier))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Builder)]
pub struct WriteSubscriberRequest {
    email: String,
    #[builder(default)]
    fields: Option<SubscriberFields>,
    #[builder(default)]
    groups: Option<Vec<String>>,
    #[builder(default)]
    status: Option<SubscriberStatus>,
    #[builder(default)]
    subscribed_at: Option<FormattedDateTime>,
    #[builder(default)]
    ip_address: Option<Ipv4Addr>,
    #[builder(default)]
    opted_in_at: Option<FormattedDateTime>,
    #[builder(default)]
    optin_up: Option<Ipv4Addr>,
    #[builder(default)]
    unsubscribed_at: Option<FormattedDateTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WriteSubscriberResponse {
    Success {
        data: Subscriber
    },
    Error {
        message: String,
        errors: HashMap<String, Vec<String>>,
    }
}

#[async_trait]
impl MailerliteEndpoint for WriteSubscriberRequest {
    type Response = WriteSubscriberResponse;

    fn to_request_builder(&self, base_url: &str, client: &Client) -> RequestBuilder {
        client
            .post(format!("{}/subscribers", base_url))
            .json(self)
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
    use chrono::NaiveDateTime;
    use super::*;

    use crate::types::SubscriberFieldValue;

    #[test]
    fn test_creates_get_subscriber_request() {
        let req = GetSubscriberRequest {
            subscriber_identifier: "test-email@test-domain.com".to_string(),
        };

        let builder = req.to_request_builder("https://localhost:1234/api", &Client::new());
        let request = builder.build().unwrap();

        let expected_url = reqwest::Url::parse("https://localhost:1234/api/subscribers/test-email@test-domain.com").unwrap();

        assert_eq!(&reqwest::Method::GET, request.method());
        assert_eq!(&expected_url, request.url());
    }

    #[test]
    fn test_creates_write_subscriber_request() {
        let mut fields = HashMap::new();
        fields.insert("Foo".to_string(), Some(SubscriberFieldValue::String("Value".to_string())));

        let req = WriteSubscriberRequest {
            email: "test-email@test-domain.com".to_string(),
            fields: Some(fields),
            groups: Some(vec![]),
            status: Some(SubscriberStatus::Junk),
            subscribed_at: None,
            ip_address: None,
            opted_in_at: Some(FormattedDateTime(NaiveDateTime::from_timestamp(1666708534, 0))),
            optin_up: Some(std::net::Ipv4Addr::new(127, 0, 0, 1)),
            unsubscribed_at: None,
        };

        let builder = req.to_request_builder("https://localhost:1234/api", &Client::new());
        let request = builder.build().unwrap();

        let expected_url = reqwest::Url::parse("https://localhost:1234/api/subscribers").unwrap();
        let expected_body = r#"{"email":"test-email@test-domain.com","fields":{"Foo":"Value"},"groups":[],"status":"junk","subscribed_at":null,"ip_address":null,"opted_in_at":"2022-10-25 14:35:34","optin_up":"127.0.0.1","unsubscribed_at":null}"#;

        assert_eq!(&reqwest::Method::POST, request.method());
        assert_eq!(&expected_url, request.url());
        assert_eq!(expected_body, std::str::from_utf8(request.body().unwrap().as_bytes().unwrap()).unwrap());
    }

    #[test]
    fn test_creates_list_segment_subscriber_request() {
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
