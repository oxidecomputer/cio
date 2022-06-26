use anyhow::Result;
use async_trait::async_trait;
use cio_api::{companies::Company, db::Database};
use dropshot::{Extractor, RequestContext, ServerContext};
use dropshot_auth::sig::HmacSignatureVerifier;
use hmac::Hmac;
use log::info;
use sha2::Sha256;
use std::{borrow::Cow, sync::Arc};

use crate::{http::Headers};

#[derive(Debug)]
pub struct CheckrWebhookVerification;

#[async_trait]
impl HmacSignatureVerifier for CheckrWebhookVerification {
    type Algo = Hmac<Sha256>;

    async fn key<'a, Context: ServerContext>(_: &'a Arc<RequestContext<Context>>) -> Result<Cow<'a, [u8]>> {
        // We only have a generic context here so we can not take values out. Instead construct a
        // new db connection in the meantime
        let db = Database::new().await;

        Ok(Company::get_from_db(&db, "Oxide".to_string())
            .await
            .map(|company| Cow::Owned(company.checkr_api_key.into_bytes()))
            .ok_or_else(|| anyhow::anyhow!("Failed to find company API key for Checkr"))?)
    }

    async fn signature<'a, Context: ServerContext>(rqctx: &'a Arc<RequestContext<Context>>) -> Result<Cow<'a, [u8]>> {
        let headers = Headers::from_request(rqctx.clone()).await?;
        let signature = headers
            .0
            .get("X-Checkr-Signature")
            .ok_or_else(|| anyhow::anyhow!("Checkr webhook is missing signature"))
            .and_then(|header_value| Ok(header_value.to_str()?))
            .and_then(|header| Ok(hex::decode(header)?))
            .map_err(|err| {
                info!("Checkr webhook is missing a well-formed signature: {}", err);
                err
            })?;

        Ok(Cow::Owned(signature))
    }
}
