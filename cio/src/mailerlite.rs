use anyhow::{anyhow, Result};
use mailerlite::{
    endpoints::{
        BatchRequestBuilder, BatchRequestEntryBuilder, BatchRequestEntryBuilderError, BatchResponse,
        GetSubscriberRequestBuilder, GetSubscriberResponse, ListSegmentSubscribersRequestBuilder,
        ListSegmentSubscribersResponse, WriteSubscriberRequestBuilder, WriteSubscriberRequestBuilderError,
        WriteSubscriberResponse,
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

    pub async fn mark_mailing_list_subscribers(&self, subscribers: Vec<Subscriber>) -> Result<BatchResponse> {
        self.mark_batch(subscribers, "mailing_list").await
    }

    pub async fn pending_wait_list_subscribers(&self) -> Result<Vec<Subscriber>> {
        self.get_pending_list(&self.segments.wait_list).await
    }

    pub async fn mark_wait_list_subscriber(&self, email: &str) -> Result<WriteSubscriberResponse<Subscriber>> {
        self.mark_subscriber(email, "wait_list").await
    }

    pub async fn mark_wait_list_subscribers(&self, subscribers: Vec<Subscriber>) -> Result<BatchResponse> {
        self.mark_batch(subscribers, "wait_list").await
    }

    async fn get_pending_list_page(
        &self,
        segment_id: &str,
        cursor: Option<String>,
    ) -> Result<ListSegmentSubscribersResponse<Subscriber>> {
        let req = ListSegmentSubscribersRequestBuilder::default()
            .segment_id(segment_id.to_string())
            .limit(1000)
            .cursor(cursor);

        let response = self.client.run(req.build()?).await?;

        log::info!(
            "[mailerlite:get_list] Rate-limit max: {:?} remaining: {:?}",
            response.rate_limit,
            response.rate_limit_remaining,
        );

        match response.response {
            MailerliteResponse::AuthenticationError { .. } => Err(anyhow!("Failed to authenticate with Mailerlite")),
            MailerliteResponse::EndpointResponse(data) => Ok(data),
        }
    }

    async fn get_pending_list(&self, segment_id: &str) -> Result<Vec<Subscriber>> {
        let mut subscribers: Vec<Subscriber> = vec![];

        let mut cursor = None;

        loop {
            let response = self.get_pending_list_page(segment_id, cursor.take()).await?;

            match response {
                ListSegmentSubscribersResponse::Success { mut data, meta, .. } => {
                    subscribers.append(&mut data);

                    match meta.next_cursor {
                        Some(next) => cursor = Some(next),
                        None => break,
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

        log::info!(
            "[mailerlite:get_subscriber] Rate-limit max: {:?} remaining: {:?}",
            response.rate_limit,
            response.rate_limit_remaining,
        );

        match response.response {
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

                let response = self
                    .client
                    .run(
                        WriteSubscriberRequestBuilder::default()
                            .email(email.to_string())
                            .fields(Some(fields))
                            .build()?,
                    )
                    .await?;

                log::info!(
                    "[mailerlite:update_subscriber] Rate-limit max: {:?} remaining: {:?}",
                    response.rate_limit,
                    response.rate_limit_remaining,
                );

                match response.response {
                    MailerliteResponse::AuthenticationError { .. } => {
                        Err(anyhow!("Failed to authenticate with Mailerlite"))
                    }
                    MailerliteResponse::EndpointResponse(data) => Ok(data),
                }
            }
            MailerliteResponse::EndpointResponse(GetSubscriberResponse::NotFound) => {
                Err(anyhow!("Failed to find subscriber"))
            }
        }
    }

    async fn mark_batch(&self, subscribers: Vec<Subscriber>, state_marker: &str) -> Result<BatchResponse> {
        let requests = subscribers
            .into_iter()
            .map(
                |Subscriber {
                     id, email, mut fields, ..
                 }| {
                    let new_value: Result<SubscriberFieldValue, serde_json::Error> =
                        if let Some(cio_state) = fields.get_mut("cio_state").and_then(|v| v.as_mut()) {
                            match cio_state {
                                SubscriberFieldValue::String(current_value) => {
                                    let new_marker = state_marker.to_string();

                                    serde_json::from_str::<CioState>(current_value).and_then(|mut state| {
                                        if !state.processed_groups.contains(&new_marker) {
                                            state.processed_groups.push(new_marker);
                                        }

                                        serde_json::to_string(&state).map(|value| value.into())
                                    })
                                }
                                invalid_field_value => {
                                    log::warn!("Invalid value type found for stored cio_state on subscriber {}", id);
                                    Ok(invalid_field_value.to_owned())
                                }
                            }
                        } else {
                            serde_json::to_string(&CioState {
                                processed_groups: vec![state_marker.to_string()],
                            })
                            .map(|value| value.into())
                        };

                    new_value.map(|value| {
                        fields.insert("cio_state".to_string(), Some(value));

                        WriteSubscriberRequestBuilder::default()
                            .email(email.to_string())
                            .fields(Some(fields))
                            .build()
                            .map(|body| {
                                BatchRequestEntryBuilder::default()
                                    .method("POST".to_string())
                                    .path("api/subscribers".to_string())
                                    .body(body)
                                    .build()
                            })
                    })
                },
            )
            .collect::<Result<
                Result<Result<Vec<_>, BatchRequestEntryBuilderError>, WriteSubscriberRequestBuilderError>,
                serde_json::Error,
            >>()???;

        let request = BatchRequestBuilder::default().requests(requests).build()?;

        let response = self.client.run(request).await?;

        log::info!(
            "[mailerlite:batch_update] Rate-limit max: {:?} remaining: {:?}",
            response.rate_limit,
            response.rate_limit_remaining,
        );

        match response.response {
            MailerliteResponse::AuthenticationError { .. } => Err(anyhow!("Failed to authenticate with Mailerlite")),
            MailerliteResponse::EndpointResponse(data) => Ok(data),
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
