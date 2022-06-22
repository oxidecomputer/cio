use async_trait::async_trait;
use digest::KeyInit;
use dropshot::{Extractor, ExtractorMetadata, HttpError, RequestContext, ServerContext, UntypedBody, TypedBody};
use hmac::Mac;
use std::{borrow::Cow, marker::PhantomData, sync::Arc};

use crate::http::{internal_error, unauthorized};

pub trait BodyTypeAlias: serde::de::DeserializeOwned + Send + Sync + schemars::JsonSchema {}
impl<T> BodyTypeAlias for T where T: serde::de::DeserializeOwned + Send + Sync + schemars::JsonSchema {}

#[derive(Debug)]
pub struct HmacVerifiedBody<T, BodyType> {
    audit: HmacVerifiedBodyAudit<T, BodyType>,
}

impl<T, BodyType> HmacVerifiedBody<T, BodyType>
where
    BodyType: BodyTypeAlias,
{
    #[allow(dead_code)]
    pub fn into_inner_raw(self) -> UntypedBody {
        self.audit.into_inner_raw()
    }

    pub fn into_inner(self) -> Result<BodyType, HttpError>
    {
        self.audit.into_inner()
    }
}

#[derive(Debug)]
pub struct HmacVerifiedBodyAudit<T, BodyType> {
    body: UntypedBody,
    _body_type: PhantomData<BodyType>,
    verified: bool,
    _verifier: PhantomData<T>,
}

impl<T, BodyType> HmacVerifiedBodyAudit<T, BodyType>
where
    BodyType: BodyTypeAlias,
{
    #[allow(dead_code)]
    pub fn into_inner_raw(self) -> UntypedBody {
        self.body
    }

    pub fn into_inner(self) -> Result<BodyType, HttpError>
    {
        serde_json::from_slice::<BodyType>(self.body.as_bytes())
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
impl<T, BodyType> Extractor for HmacVerifiedBody<T, BodyType>
where
    T: HmacSignatureVerifier + Send + Sync,
    BodyType: BodyTypeAlias
{
    async fn from_request<Context: ServerContext>(
        rqctx: Arc<RequestContext<Context>>,
    ) -> Result<HmacVerifiedBody<T, BodyType>, HttpError> {
        let audit = HmacVerifiedBodyAudit::<T, BodyType>::from_request(rqctx.clone()).await?;

        log::debug!("Computed hmac audit result {}", audit.verified);

        if audit.verified {
            Ok(HmacVerifiedBody { audit })
        } else {
            Err(unauthorized())
        }
    }

    fn metadata() -> ExtractorMetadata {
        HmacVerifiedBodyAudit::<T, BodyType>::metadata()
    }
}

#[async_trait]
impl<T, BodyType> Extractor for HmacVerifiedBodyAudit<T, BodyType>
where
    T: HmacSignatureVerifier + Send + Sync,
    BodyType: BodyTypeAlias
{
    async fn from_request<Context: ServerContext>(
        rqctx: Arc<RequestContext<Context>>,
    ) -> Result<HmacVerifiedBodyAudit<T, BodyType>, HttpError> {
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
            _body_type: PhantomData,
            verified,
            _verifier: PhantomData,
        })
    }

    fn metadata() -> ExtractorMetadata {

        // The HMAC extractor is a wrapper around an inner type that does not imposed any
        // alterations of the body content. Therefore we can use the metadata of the inner
        // type, as that is what we expect users to submit
        let body = TypedBody::<BodyType>::metadata();

        ExtractorMetadata {
            paginated: false,
            parameters: vec![body]
        }
    }
}
