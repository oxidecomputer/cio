use async_trait::async_trait;
use digest::KeyInit;
use dropshot::{Extractor, ExtractorMetadata, HttpError, RequestContext, ServerContext, TypedBody, UntypedBody};
use hmac::Mac;
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use std::{borrow::Cow, marker::PhantomData, sync::Arc};

use crate::http::{internal_error, unauthorized};

/// Trait that defines for a given type how to construct that type from a byte slice, as well
/// as how the type out to be described via an OpenAPI spec
pub trait FromBytes<E>: Send + Sync {
    fn from_bytes(bytes: &[u8]) -> Result<Self, E>
    where
        Self: Sized;
    fn metadata() -> ExtractorMetadata;
}

/// Provide an implementation of from_bytes for anything that can be deserialized from a JSON
/// payload. The JsonSchema bounds allows us to additionally piggyback on [dropshot::TypedBody]
/// for generating OpenAPI data.
impl<T> FromBytes<HttpError> for T
where
    T: DeserializeOwned + JsonSchema + Send + Sync + 'static,
{
    fn from_bytes(bytes: &[u8]) -> Result<Self, HttpError>
    where
        Self: Sized,
    {
        serde_json::from_slice(bytes)
            .map_err(|e| HttpError::for_bad_request(None, format!("Failed to parse body: {}", e)))
    }

    fn metadata() -> ExtractorMetadata {
        TypedBody::<Self>::metadata()
    }
}

/// A type very similar to [dropshot::UntypedBody], used for holding a body of arbitrary bytes
pub struct RawBody {
    inner: Vec<u8>,
}

impl RawBody {
    pub fn as_str(&self) -> Result<&str, HttpError> {
        std::str::from_utf8(&self.inner).map_err(|_| HttpError::for_bad_request(None, format!("Failed to read body")))
    }

    pub fn to_string(&self) -> Result<String, HttpError> {
        self.as_str().map(|s| s.to_string())
    }
}

impl FromBytes<HttpError> for RawBody {
    fn from_bytes(bytes: &[u8]) -> Result<Self, HttpError>
    where
        Self: Sized,
    {
        Ok(RawBody { inner: bytes.to_vec() })
    }

    /// In terms of the OpenAPI spec, a RawBody is the same as an UntypedBody
    fn metadata() -> ExtractorMetadata {
        UntypedBody::metadata()
    }
}

/// A request body that has been verified by an HMAC verifier T. Extracting an [HMACVerifiedBody]
/// will return an UNAUTHORIZED error if verification fails. An INTERNAL_SERVER_ERROR will be
/// returned if verification can not be performed due to a the verifier T failing to supply a key
/// or content,
#[derive(Debug)]
pub struct HmacVerifiedBody<T, BodyType> {
    audit: HmacVerifiedBodyAudit<T, BodyType>,
}

impl<T, BodyType> HmacVerifiedBody<T, BodyType>
where
    BodyType: FromBytes<HttpError>,
{
    /// Attempts to deserialize the request body into the specified `BodyType`. Returns a
    /// BAD_REQUEST [dropshot::HttpError] if the deserialization of `BodyType` fails
    #[allow(dead_code)]
    pub fn into_inner(self) -> Result<BodyType, HttpError> {
        self.audit.into_inner()
    }
}

/// A request body that performs the HMAC verification specified by the verifier T, but does not
/// fail extraction when verification fails. The extracted [HmacVerifiedBodyAudit] can be queried
/// to determine if verification failed. An INTERNAL_SERVER_ERROR will be returned if verification
/// can not be performed due to a the verifier T failing to supply a key or content,
#[derive(Debug)]
pub struct HmacVerifiedBodyAudit<T, BodyType> {
    body: UntypedBody,
    _body_type: PhantomData<BodyType>,
    verified: bool,
    _verifier: PhantomData<T>,
}

impl<T, BodyType> HmacVerifiedBodyAudit<T, BodyType>
where
    BodyType: FromBytes<HttpError>,
{
    /// Returns that status of if this body passed verification
    pub fn verified(&self) -> bool {
        self.verified
    }

    /// Attempts to deserialize the request body into the specified `BodyType`. Returns a
    /// BAD_REQUEST HttpError if the deserialization of `BodyType` fails.
    pub fn into_inner(self) -> Result<BodyType, HttpError> {
        BodyType::from_bytes(self.body.as_bytes())
            .map_err(|e| HttpError::for_bad_request(None, format!("Failed to parse body: {}", e)))
    }
}

/// A trait to be used to implement various HMAC verification strategies. By default a strategy
/// must implement two functions, one to provide the secret to the verifier, and one to extract
/// the signature to check from a request. Additionally, a strategy can implement a custom function
/// for extracting the materials from a request that should be signed.
#[async_trait]
pub trait HmacSignatureVerifier {
    type Algo: Mac + KeyInit;

    /// Provides the key to be used in signature verification.
    async fn key<'a, Context: ServerContext>(rqctx: &'a Arc<RequestContext<Context>>) -> anyhow::Result<Cow<'a, [u8]>>;

    /// Provides the signature that should be tested.
    async fn signature<'a, Context: ServerContext>(
        rqctx: &'a Arc<RequestContext<Context>>,
    ) -> anyhow::Result<Cow<'a, [u8]>>;

    /// Provides the content that should be signed. By default this provides the request body content.
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
    BodyType: FromBytes<HttpError>,
{
    async fn from_request<Context: ServerContext>(
        rqctx: Arc<RequestContext<Context>>,
    ) -> Result<HmacVerifiedBody<T, BodyType>, HttpError> {
        let audit = HmacVerifiedBodyAudit::<T, BodyType>::from_request(rqctx.clone()).await?;

        log::debug!("Computed hmac audit result {}", audit.verified);

        if audit.verified() {
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
    BodyType: FromBytes<HttpError>,
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
        // The HMAC extractor is a wrapper around an inner type that does not perform any
        // alterations on the body content. Therefore we can use the metadata of the inner
        // type, as that is what we expect users to submit
        BodyType::metadata()
    }
}