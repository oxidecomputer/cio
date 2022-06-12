use async_trait::async_trait;
use digest::KeyInit;
use dropshot::{Extractor, ExtractorMetadata, HttpError, RequestContext, ServerContext, UntypedBody};
use hmac::Mac;
use std::{borrow::Cow, marker::PhantomData, sync::Arc};

use crate::http::{unauthorized, internal_error};

// listen_checkr_background_update_webhooks
// listen_docusign_envelope_update_webhooks
// listen_emails_incoming_sendgrid_parse_webhooks
// listen_github_webhooks
// listen_mailchimp_mailing_list_webhooks
// listen_mailchimp_rack_line_webhooks
// listen_shippo_tracking_update_webhooks
// listen_slack_commands_webhooks

pub struct HmacVerifiedBody<T> {
    audit: HmacVerifiedBodyAudit<T>
}

impl<T> HmacVerifiedBody<T> {
    #[allow(dead_code)]
    pub fn into_inner(self) -> UntypedBody {
        self.audit.into_inner()
    }

    pub fn into_inner_as<U>(self) -> Result<U, HttpError>
    where
        U: serde::de::DeserializeOwned + Send + Sync + schemars::JsonSchema,
    {
        self.audit.into_inner_as::<U>()
    }
}

pub struct HmacVerifiedBodyAudit<T> {
    body: UntypedBody,
    verified: bool,
    _verifier: PhantomData<T>,
}

impl<T> HmacVerifiedBodyAudit<T> {
    #[allow(dead_code)]
    pub fn into_inner(self) -> UntypedBody {
        self.body
    }

    pub fn into_inner_as<U>(self) -> Result<U, HttpError>
    where
        U: serde::de::DeserializeOwned + Send + Sync + schemars::JsonSchema,
    {
        serde_json::from_slice::<U>(self.body.as_bytes())
            .map_err(|e| HttpError::for_bad_request(None, format!("Failed to parse body: {}", e)))
    }
}

#[async_trait]
pub trait HmacSignatureVerifier {
    type Algo: Mac + KeyInit;

    async fn key<'a, Context: ServerContext>(rqctx: &'a Arc<RequestContext<Context>>) -> anyhow::Result<Cow<'a, [u8]>>;
    async fn signature<'a, Context: ServerContext>(
        rqctx: &'a Arc<RequestContext<Context>>,
    ) -> anyhow::Result<Cow<'a, [u8]>>;
}

#[async_trait]
impl<T> Extractor for HmacVerifiedBody<T>
where
    T: HmacSignatureVerifier + Send + Sync,
{
    async fn from_request<Context: ServerContext>(
        rqctx: Arc<RequestContext<Context>>,
    ) -> Result<HmacVerifiedBody<T>, HttpError> {
        let audit = HmacVerifiedBodyAudit::<T>::from_request(rqctx.clone()).await?;

        if audit.verified {
            Ok(HmacVerifiedBody {
                audit
            })
        } else {
            Err(unauthorized())
        }
    }

    fn metadata() -> ExtractorMetadata {
        ExtractorMetadata {
            paginated: false,
            parameters: vec![],
        }
    }
}

#[async_trait]
impl<T> Extractor for HmacVerifiedBodyAudit<T>
where
    T: HmacSignatureVerifier + Send + Sync,
{
    async fn from_request<Context: ServerContext>(
        rqctx: Arc<RequestContext<Context>>,
    ) -> Result<HmacVerifiedBodyAudit<T>, HttpError> {
        let body = UntypedBody::from_request(rqctx.clone()).await?;
        let key = T::key(&rqctx).await.map_err(|_| internal_error())?;

        let verified = if let Ok(signature) = T::signature(&rqctx).await {
            if let Ok(mut mac) = <T::Algo as Mac>::new_from_slice(&*key) {
                mac.update(body.as_bytes());
                mac.verify_slice(&*signature).is_ok()
            } else {
                false
            }
        } else {
            false
        };

        Ok(HmacVerifiedBodyAudit {
            body,
            verified,
            _verifier: PhantomData,
        })
    }

    fn metadata() -> ExtractorMetadata {
        ExtractorMetadata {
            paginated: false,
            parameters: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    use super::SignatureVerification;

    #[test]
    fn test_verifies_new_signature() {
        struct Verifier;

        impl SignatureVerification for Verifier {
            type Algo = Hmac<Sha256>;
        }

        let test_key = "vkPkH4G2k8XNC5HWA6QgZd08v37P8KcVZMjaP4zgGWc=";
        let test_body = "{\"fake\": \"message\"}";

        type HmacSha256 = Hmac<Sha256>;

        let mut mac = HmacSha256::new_from_slice(test_key.as_bytes()).unwrap();
        mac.update(test_body.as_bytes());

        let result = mac.finalize().into_bytes();

        assert!(Verifier::verify(test_key.as_bytes(), &result, test_body.as_bytes()));
    }
}
