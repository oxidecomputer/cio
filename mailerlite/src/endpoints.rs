use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use derive_builder::Builder;
use reqwest::{Client, RequestBuilder, Response, StatusCode};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{collections::HashMap, net::Ipv4Addr};

use crate::{
    ApiSubscriber, FailedToTranslateDateError, FormattedDateTime, MailerliteClientContext, MailerliteError, Subscriber,
    SubscriberFields, SubscriberStatus,
};

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
    fn to_request_builder<Tz>(
        &self,
        base_url: &str,
        client: &Client,
        ctx: &MailerliteClientContext<Tz>,
    ) -> RequestBuilder
    where
        Tz: TimeZone + Send + Sync;

    async fn handle_response<Tz>(
        &self,
        response: Response,
        _ctx: &MailerliteClientContext<Tz>,
    ) -> Result<Self::Response, MailerliteError>
    where
        Self::Response: DeserializeOwned,
        Tz: TimeZone + Send + Sync,
    {
        Ok(response.json::<Self::Response>().await?)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Builder)]
#[builder(pattern = "owned")]
pub struct GetSubscriberRequest {
    /// Subscriber identifer can be either and id number or an email
    subscriber_identifier: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GetSubscriberResponse<T> {
    Success { data: T },
    NotFound,
}

#[async_trait]
impl MailerliteEndpoint for GetSubscriberRequest {
    type Response = GetSubscriberResponse<Subscriber>;

    fn to_request_builder<Tz>(
        &self,
        base_url: &str,
        client: &Client,
        _ctx: &MailerliteClientContext<Tz>,
    ) -> RequestBuilder
    where
        Tz: TimeZone + Send + Sync,
    {
        client.get(format!("{}/subscribers/{}", base_url, self.subscriber_identifier))
    }

    async fn handle_response<Tz>(
        &self,
        response: Response,
        ctx: &MailerliteClientContext<Tz>,
    ) -> Result<Self::Response, MailerliteError>
    where
        Self::Response: DeserializeOwned,
        Tz: TimeZone + Send + Sync,
    {
        if response.status() == StatusCode::NOT_FOUND {
            Ok(GetSubscriberResponse::NotFound)
        } else {
            let raw_subscriber_data: GetSubscriberResponse<ApiSubscriber> = response.json().await?;

            Ok(match raw_subscriber_data {
                GetSubscriberResponse::Success { data } => GetSubscriberResponse::Success {
                    data: data.into_subscriber(&ctx.time_zone)?,
                },
                _ => GetSubscriberResponse::NotFound,
            })
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Builder)]
#[builder(pattern = "owned")]
pub struct WriteSubscriberRequest {
    email: String,
    #[builder(default)]
    fields: Option<SubscriberFields>,
    #[builder(default)]
    groups: Vec<String>,
    #[builder(default)]
    status: Option<SubscriberStatus>,
    #[builder(default)]
    subscribed_at: Option<DateTime<Utc>>,
    #[builder(default)]
    ip_address: Option<Ipv4Addr>,
    #[builder(default)]
    opted_in_at: Option<DateTime<Utc>>,
    #[builder(default)]
    optin_up: Option<Ipv4Addr>,
    #[builder(default)]
    unsubscribed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteSubscriberRequestWithFormattedDateTimes {
    email: String,
    fields: Option<SubscriberFields>,
    groups: Vec<String>,
    status: Option<SubscriberStatus>,
    subscribed_at: Option<FormattedDateTime>,
    ip_address: Option<Ipv4Addr>,
    opted_in_at: Option<FormattedDateTime>,
    optin_up: Option<Ipv4Addr>,
    unsubscribed_at: Option<FormattedDateTime>,
}

impl WriteSubscriberRequestWithFormattedDateTimes {
    fn new(req: WriteSubscriberRequest, time_zone: &impl TimeZone) -> WriteSubscriberRequestWithFormattedDateTimes {
        Self {
            email: req.email,
            fields: req.fields,
            groups: req.groups,
            status: req.status,
            subscribed_at: req
                .subscribed_at
                .map(|dt| dt.with_timezone(time_zone).naive_local().into()),
            ip_address: req.ip_address,
            opted_in_at: req
                .opted_in_at
                .map(|dt| dt.with_timezone(time_zone).naive_local().into()),
            optin_up: req.optin_up,
            unsubscribed_at: req
                .unsubscribed_at
                .map(|dt| dt.with_timezone(time_zone).naive_local().into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WriteSubscriberResponse<T> {
    Success {
        data: T,
    },
    Error {
        message: String,
        errors: HashMap<String, Vec<String>>,
    },
}

#[async_trait]
impl MailerliteEndpoint for WriteSubscriberRequest {
    type Response = WriteSubscriberResponse<Subscriber>;

    fn to_request_builder<Tz>(
        &self,
        base_url: &str,
        client: &Client,
        ctx: &MailerliteClientContext<Tz>,
    ) -> RequestBuilder
    where
        Tz: TimeZone + Send + Sync,
    {
        let formatted_req = WriteSubscriberRequestWithFormattedDateTimes::new(self.clone(), &ctx.time_zone);

        client.post(format!("{}/subscribers", base_url)).json(&formatted_req)
    }

    async fn handle_response<Tz>(
        &self,
        response: Response,
        ctx: &MailerliteClientContext<Tz>,
    ) -> Result<Self::Response, MailerliteError>
    where
        Self::Response: DeserializeOwned,
        Tz: TimeZone + Send + Sync,
    {
        let response: WriteSubscriberResponse<ApiSubscriber> = response.json().await?;

        Ok(match response {
            WriteSubscriberResponse::Success { data } => WriteSubscriberResponse::Success {
                data: data.into_subscriber(&ctx.time_zone)?,
            },
            WriteSubscriberResponse::Error { message, errors } => Self::Response::Error { message, errors },
        })
    }
}

#[derive(Debug, Clone, Builder)]
#[builder(pattern = "owned")]
pub struct ListSegmentSubscribersRequest {
    segment_id: String,
    #[builder(setter(strip_option), default)]
    filter_status: Option<SubscriberStatus>,
    #[builder(setter(strip_option), default)]
    limit: Option<u64>,
    #[builder(setter(strip_option), default)]
    after: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ListSegmentSubscribersResponse<T> {
    Success {
        data: Vec<T>,
        meta: ListSegmentSubscribersResponseMeta,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ListSegmentSubscribersResponseMeta {
    pub total: u64,
    pub count: u64,
    pub last: Option<u64>,
}

#[async_trait]
impl MailerliteEndpoint for ListSegmentSubscribersRequest {
    type Response = ListSegmentSubscribersResponse<Subscriber>;

    fn to_request_builder<Tz>(
        &self,
        base_url: &str,
        client: &Client,
        _ctx: &MailerliteClientContext<Tz>,
    ) -> RequestBuilder
    where
        Tz: TimeZone + Send + Sync,
    {
        client
            .get(format!("{}/segments/{}/subscribers", base_url, self.segment_id))
            .optional_query("filter[status]", self.filter_status.as_ref())
            .optional_query("limit", self.limit.as_ref())
            .optional_query("after", self.after.as_ref())
    }

    async fn handle_response<Tz>(
        &self,
        response: Response,
        ctx: &MailerliteClientContext<Tz>,
    ) -> Result<Self::Response, MailerliteError>
    where
        Self::Response: DeserializeOwned,
        Tz: TimeZone + Send + Sync,
    {
        if response.status() == StatusCode::NOT_FOUND {
            Ok(ListSegmentSubscribersResponse::Success {
                data: vec![],
                meta: ListSegmentSubscribersResponseMeta {
                    total: 0,
                    count: 0,
                    last: None,
                },
            })
        } else {
            let response: ListSegmentSubscribersResponse<ApiSubscriber> = response.json().await?;

            match response {
                ListSegmentSubscribersResponse::Success { data, meta } => Ok(ListSegmentSubscribersResponse::Success {
                    data: data
                        .into_iter()
                        .map(|s| s.into_subscriber(&ctx.time_zone))
                        .collect::<Result<Vec<Subscriber>, FailedToTranslateDateError>>()?,
                    meta,
                }),
                ListSegmentSubscribersResponse::Error { message } => {
                    Ok(ListSegmentSubscribersResponse::Error { message })
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    use crate::types::SubscriberFieldValue;

    fn ctx() -> MailerliteClientContext<chrono_tz::Tz> {
        MailerliteClientContext {
            time_zone: chrono_tz::America::New_York,
        }
    }

    #[test]
    fn test_creates_get_subscriber_request() {
        let req = GetSubscriberRequest {
            subscriber_identifier: "test-email@test-domain.com".to_string(),
        };

        let builder = req.to_request_builder("https://localhost:1234/api", &Client::new(), &ctx());
        let request = builder.build().unwrap();

        let expected_url =
            reqwest::Url::parse("https://localhost:1234/api/subscribers/test-email@test-domain.com").unwrap();

        assert_eq!(&reqwest::Method::GET, request.method());
        assert_eq!(&expected_url, request.url());
    }

    #[tokio::test]
    async fn test_get_subscriber_request_fails_to_find() {
        let req = GetSubscriberRequest {
            subscriber_identifier: "test-email@test-domain.com".to_string(),
        };

        let response: reqwest::Response = http::response::Response::builder()
            .status(http::status::StatusCode::NOT_FOUND)
            .body("")
            .unwrap()
            .into();

        let result = req.handle_response(response, &ctx()).await.unwrap();

        match result {
            GetSubscriberResponse::Success { .. } => panic!("Received data result, but expected a NotFound"),
            _ => {}
        }
    }

    #[tokio::test]
    async fn test_converts_api_datetimes_to_utc() {
        let body = r#"{
    "data": {
        "id": "31986843064993537",
        "email": "test-email@test-domain.com",
        "status": "active",
        "source": "api",
        "sent": 0,
        "opens_count": 0,
        "clicks_count": 0,
        "open_rate": 0,
        "click_rate": 0,
        "ip_address": null,
        "subscribed_at": "2021-09-01 14:03:50",
        "unsubscribed_at": null,
        "created_at": "2021-09-01 14:03:50",
        "updated_at": "2021-09-01 14:03:50",
        "fields": {
            "city": null,
            "company": null,
            "country": null,
            "last_name": "Testerson",
            "name": "Dummy",
            "phone": null,
            "state": null,
            "z_i_p": null
        },
        "groups": [],
        "opted_in_at": null,
        "optin_ip": null
    }
}"#;

        let req = GetSubscriberRequest {
            subscriber_identifier: "test-email@test-domain.com".to_string(),
        };

        let response: reqwest::Response = http::response::Response::builder()
            .status(http::status::StatusCode::OK)
            .body(body)
            .unwrap()
            .into();

        let parsed = req.handle_response(response, &ctx()).await.unwrap();

        match parsed {
            GetSubscriberResponse::Success { data } => {
                let expected_date_time = Utc.timestamp(1630519430, 0);

                assert_eq!(data.subscribed_at, Some(expected_date_time));
                assert_eq!(data.created_at, expected_date_time);
                assert_eq!(data.updated_at, expected_date_time);
            }
            _ => unreachable!("This test is covering the OK case and should always receive a success case"),
        }
    }

    #[test]
    fn test_creates_write_subscriber_request() {
        let mut fields = HashMap::new();
        fields.insert(
            "Foo".to_string(),
            Some(SubscriberFieldValue::String("Value".to_string())),
        );

        let req = WriteSubscriberRequest {
            email: "test-email@test-domain.com".to_string(),
            fields: Some(fields),
            groups: vec![],
            status: Some(SubscriberStatus::Junk),
            subscribed_at: None,
            ip_address: None,
            // 2022-10-25T14:35:34Z
            opted_in_at: Some(Utc.timestamp(1666708534, 0)),
            optin_up: Some(std::net::Ipv4Addr::new(127, 0, 0, 1)),
            unsubscribed_at: None,
        };

        let builder = req.to_request_builder("https://localhost:1234/api", &Client::new(), &ctx());
        let request = builder.build().unwrap();

        let expected_url = reqwest::Url::parse("https://localhost:1234/api/subscribers").unwrap();
        let expected_body = r#"{"email":"test-email@test-domain.com","fields":{"Foo":"Value"},"groups":[],"status":"junk","subscribed_at":null,"ip_address":null,"opted_in_at":"2022-10-25 10:35:34","optin_up":"127.0.0.1","unsubscribed_at":null}"#;

        assert_eq!(&reqwest::Method::POST, request.method());
        assert_eq!(&expected_url, request.url());
        assert_eq!(
            expected_body,
            std::str::from_utf8(request.body().unwrap().as_bytes().unwrap()).unwrap()
        );
    }

    #[test]
    fn test_creates_list_segment_subscriber_request() {
        let req = ListSegmentSubscribersRequest {
            segment_id: "test_segment".to_string(),
            filter_status: Some(SubscriberStatus::Junk),
            limit: Some(5),
            after: Some(2),
        };

        let builder = req.to_request_builder("https://localhost:1234/api", &Client::new(), &ctx());
        let request = builder.build().unwrap();

        let expected = reqwest::Url::parse(
            "https://localhost:1234/api/segments/test_segment/subscribers?filter%5Bstatus%5D=junk&limit=5&after=2",
        )
        .unwrap();

        assert_eq!(&expected, request.url());
    }
}
