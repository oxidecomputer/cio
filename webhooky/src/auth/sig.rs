use async_trait::async_trait;
use digest::KeyInit;
use dropshot::{Extractor, ExtractorMetadata, HttpError, RequestContext, ServerContext, UntypedBody};
use hmac::Mac;
use std::{borrow::Cow, marker::PhantomData, sync::Arc};

use crate::http::{internal_error, unauthorized};

// listen_checkr_background_update_webhooks
// listen_docusign_envelope_update_webhooks
// listen_emails_incoming_sendgrid_parse_webhooks
// listen_github_webhooks
// listen_mailchimp_mailing_list_webhooks
// listen_mailchimp_rack_line_webhooks
// listen_shippo_tracking_update_webhooks
// listen_slack_commands_webhooks

#[derive(Debug)]
pub struct HmacVerifiedBody<T> {
    audit: HmacVerifiedBodyAudit<T>,
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

#[derive(Debug)]
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

    async fn content<'a, 'b, Context: ServerContext>(
        _rqctx: &'a Arc<RequestContext<Context>>,
        body: &'b UntypedBody,
    ) -> anyhow::Result<Cow<'b, [u8]>> {
        Ok(Cow::Borrowed(body.as_bytes()))
    }
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

        log::debug!("Computed hmac audit result {}", audit.verified);

        if audit.verified {
            Ok(HmacVerifiedBody { audit })
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
        let content = T::content(&rqctx, &body).await.map_err(|_| internal_error())?;
        let key = T::key(&rqctx).await.map_err(|_| internal_error())?;

        let verified = if let Ok(signature) = T::signature(&rqctx).await {
            if let Ok(mut mac) = <T::Algo as Mac>::new_from_slice(&*key) {
                mac.update(&*content);
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
