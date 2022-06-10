use async_trait::async_trait;
use dropshot::{RequestContext, ExtractorMetadata, UntypedBody, HttpError, ServerContext, Extractor};
use ecdsa::{Signature, VerifyingKey, signature::Verifier};
use k256::Secp256k1;
use std::sync::Arc;

use crate::http::{forbidden, unauthorized, Headers};

pub struct SendGridWebhookVerification;

#[async_trait]
impl Extractor for SendGridWebhookVerification {
    async fn from_request<Context: ServerContext>(rqctx: Arc<RequestContext<Context>>) -> Result<SendGridWebhookVerification, HttpError> {
        let headers = Headers::from_request(rqctx.clone()).await?;
        let body = UntypedBody::from_request(rqctx.clone()).await?;

        let signature = headers.0.get("X-Twilio-Email-Event-Webhook-Signature").and_then(|header_value| header_value.to_str().ok()).and_then(|signature| {
            base64::decode(signature).ok()
        }).and_then(|decoded| {
            Signature::<Secp256k1>::from_der(&decoded).ok()
        }).ok_or_else(unauthorized)?;

        let timestamp = headers.0.get("X-Twilio-Email-Event-Webhook-Timestamp").and_then(|header_value| header_value.to_str().ok()).ok_or_else(unauthorized)?;

        let verification_key = VerifyingKey::from_sec1_bytes(&[0]).unwrap();

        let payload = [timestamp.as_bytes(), body.as_bytes()].concat();

        verification_key.verify(&payload, &signature).map(|_| SendGridWebhookVerification).map_err(|_| forbidden())
    }

    fn metadata() -> ExtractorMetadata {
        ExtractorMetadata {
            paginated: false,
            parameters: vec![],
        }
    }
}