use anyhow::Result;
use async_trait::async_trait;
use dropshot::{Extractor, RequestContext, ServerContext};
use dropshot_verify_request::sig::HmacSignatureVerifier;
use hmac::Hmac;
use log::{info, warn};
use sha2::Sha256;
use std::sync::Arc;

use crate::http::Headers;

#[derive(Debug)]
pub struct DocusignWebhookVerification;

#[async_trait]
impl HmacSignatureVerifier for DocusignWebhookVerification {
    type Algo = Hmac<Sha256>;

    async fn key<Context: ServerContext>(_: Arc<RequestContext<Context>>) -> Result<Vec<u8>> {
        Ok(std::env::var("DOCUSIGN_WH_KEY")
            .map(|key| key.into_bytes())
            .map_err(|err| {
                warn!("Failed to find webhook key for verifying DocuSign webhooks");
                err
            })?)
    }

    async fn signature<Context: ServerContext>(rqctx: Arc<RequestContext<Context>>) -> Result<Vec<u8>> {
        let headers = Headers::from_request(rqctx.clone()).await?;
        let signature = headers
            .0
            .get("X-DocuSign-Signature-1")
            .ok_or_else(|| anyhow::anyhow!("DocuSign webhook is missing signature"))
            .and_then(|header_value| Ok(header_value.to_str()?))
            .and_then(|header| Ok(hex::decode(header)?))
            .map_err(|err| {
                info!("DocuSign webhook is missing a well-formed signature: {}", err);
                err
            })?;

        Ok(signature)
    }
}
