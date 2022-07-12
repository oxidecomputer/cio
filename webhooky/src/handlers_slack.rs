use anyhow::Result;
use async_trait::async_trait;
use dropshot::{Extractor, RequestContext, ServerContext, UntypedBody};
use dropshot_auth::sig::HmacSignatureVerifier;
use hmac::Hmac;
use log::{info, warn};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::{borrow::Cow, sync::Arc};

use crate::http::Headers;

#[derive(Debug)]
pub struct SlackWebhookVerification;

#[async_trait]
impl HmacSignatureVerifier for SlackWebhookVerification {
    type Algo = Hmac<Sha256>;

    async fn key<Context: ServerContext>(_: Arc<RequestContext<Context>>) -> Result<Vec<u8>> {
        Ok(std::env::var("SLACK_WH_KEY")
            .map(|key| key.into_bytes())
            .map_err(|err| {
                warn!("Failed to find webhook key for verifying Slack webhooks: {}", err);
                err
            })?)
    }

    async fn signature<Context: ServerContext>(rqctx: Arc<RequestContext<Context>>) -> Result<Vec<u8>> {
        let headers = Headers::from_request(rqctx.clone()).await?;
        let signature = headers
            .0
            .get("X-Slack-Signature")
            .ok_or_else(|| anyhow::anyhow!("Slack webhook is missing signature"))
            .and_then(|header_value| Ok(header_value.to_str()?))
            .and_then(|header| {
                log::debug!("Found Slack signature header {}", header);
                Ok(hex::decode(header.trim_start_matches("v0="))?)
            })
            .map_err(|err| {
                info!("Slack webhook is missing a well-formed signature: {}", err);
                err
            })?;

        Ok(signature)
    }

    async fn content<'a, 'b, Context: ServerContext>(
        rqctx: &'a Arc<RequestContext<Context>>,
        body: &'b UntypedBody,
    ) -> anyhow::Result<Cow<'b, [u8]>> {
        let headers = Headers::from_request(rqctx.clone()).await?;
        let timestamp = headers
            .0
            .get("X-Slack-Request-Timestamp")
            .ok_or_else(|| anyhow::anyhow!("Slack webhook is missing timestamp"))
            .and_then(|header_value| Ok(header_value.to_str()?))
            .map_err(|err| {
                info!("Slack webhook is missing a well-formed timestamp: {}", err);
                err
            })?;

        let mut content = ("v0".to_string() + ":" + timestamp + ":").into_bytes();
        content.append(&mut body.as_bytes().to_vec());

        Ok(Cow::Owned(content))
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, JsonSchema)]
pub struct InteractiveEvent {
    pub payload: String,
}
