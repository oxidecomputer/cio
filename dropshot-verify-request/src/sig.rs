use async_trait::async_trait;
use digest::KeyInit;
use dropshot::{
    ApiEndpointBodyContentType, ExclusiveExtractor, ExtractorMetadata, HttpError, RequestContext, ServerContext,
    UntypedBody,
};
use hmac::Mac;
use std::{borrow::Cow, marker::PhantomData};

use crate::{
    http::{internal_error, unauthorized},
    FromBytes,
};

/// A request body that has been verified by an HMAC verifier `T`.
#[derive(Debug)]
pub struct HmacVerifiedBody<T, BodyType> {
    audit: HmacVerifiedBodyAudit<T, BodyType>,
}

impl<T, BodyType> HmacVerifiedBody<T, BodyType>
where
    BodyType: FromBytes<HttpError>,
{
    /// Attempts to deserialize the request body into the specified `BodyType`. Returns a
    /// [`BAD_REQUEST`](http::status::StatusCode::BAD_REQUEST) [`HttpError`](dropshot::HttpError) if the deserialization of `BodyType` fails
    #[allow(dead_code)]
    pub fn into_inner(self) -> Result<BodyType, HttpError> {
        self.audit.into_inner()
    }
}

/// A request body that performs the HMAC verification specified by the verifier `T`, but does not
/// fail extraction when verification fails. The [`HmacVerifiedBodyAudit`] can be queried to determine
/// if verification failed.
#[derive(Debug)]
pub struct HmacVerifiedBodyAudit<T, BodyType> {
    body: UntypedBody,
    _body_type: PhantomData<BodyType>,
    content_type: ApiEndpointBodyContentType,
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
    /// [`BAD_REQUEST`](http::status::StatusCode::BAD_REQUEST) [`HttpError`](dropshot::HttpError) if the deserialization of `BodyType` fails.
    pub fn into_inner(self) -> Result<BodyType, HttpError> {
        BodyType::from_bytes(self.body.as_bytes(), &self.content_type)
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
    async fn key<Context: ServerContext>(rqctx: &RequestContext<Context>) -> anyhow::Result<Vec<u8>>;

    /// Provides the signature that should be tested.
    async fn signature<Context: ServerContext>(rqctx: &RequestContext<Context>) -> anyhow::Result<Vec<u8>>;

    /// Provides the content that should be signed. By default this provides the request body content.
    async fn content<'a, 'b, Context: ServerContext>(
        _rqctx: &'a RequestContext<Context>,
        body: &'b UntypedBody,
    ) -> anyhow::Result<Cow<'b, [u8]>> {
        Ok(Cow::Borrowed(body.as_bytes()))
    }
}

/// Extracting an [`HmacVerifiedBody`] will return an [`UNAUTHORIZED`](http::status::StatusCode::UNAUTHORIZED) [`HttpError`](dropshot::HttpError) if verification fails.
/// An [`INTERNAL_SERVER_ERROR`](http::status::StatusCode::INTERNAL_SERVER_ERROR) will be returned if verification can not be performed due to a
/// the verifier `T` failing to supply a key or content,
#[async_trait]
impl<T, BodyType> ExclusiveExtractor for HmacVerifiedBody<T, BodyType>
where
    T: HmacSignatureVerifier + Send + Sync,
    BodyType: FromBytes<HttpError>,
{
    async fn from_request<Context: ServerContext>(
        rqctx: &RequestContext<Context>,
        request: hyper::Request<hyper::Body>,
    ) -> Result<HmacVerifiedBody<T, BodyType>, HttpError> {
        let audit = HmacVerifiedBodyAudit::<T, BodyType>::from_request(rqctx, request).await?;

        log::debug!("Computed HMAC audit result {}", audit.verified);

        if audit.verified() {
            Ok(HmacVerifiedBody { audit })
        } else {
            Err(unauthorized())
        }
    }

    fn metadata(body_content_type: ApiEndpointBodyContentType) -> ExtractorMetadata {
        HmacVerifiedBodyAudit::<T, BodyType>::metadata(body_content_type)
    }
}

/// An [`INTERNAL_SERVER_ERROR`](http::status::StatusCode::INTERNAL_SERVER_ERROR) will be returned if verification can not be performed due to
/// the verifier `T` failing to supply a key or content,
#[async_trait]
impl<T, BodyType> ExclusiveExtractor for HmacVerifiedBodyAudit<T, BodyType>
where
    T: HmacSignatureVerifier + Send + Sync,
    BodyType: FromBytes<HttpError>,
{
    async fn from_request<Context: ServerContext>(
        rqctx: &RequestContext<Context>,
        request: hyper::Request<hyper::Body>,
    ) -> Result<HmacVerifiedBodyAudit<T, BodyType>, HttpError> {
        let body = UntypedBody::from_request(rqctx, request).await?;
        let content = T::content(rqctx, &body).await.map_err(|_| internal_error())?;
        let key = T::key(rqctx).await.map_err(|_| internal_error())?;
        let req_uri = rqctx.request.uri().clone();

        let signature = T::signature(rqctx).await;
        let mac = <T::Algo as Mac>::new_from_slice(&key);

        let verified = match (signature, mac) {
            (Ok(signature), Ok(mut mac)) => {
                mac.update(&content);
                let verified = mac.verify_slice(&signature).is_ok();

                if !verified {
                    log::info!(
                        "Failed to verify signature. req_id: {} uri: {} sig: {:?} body: {:?}",
                        rqctx.request_id,
                        req_uri,
                        signature,
                        body.as_bytes()
                    );
                } else {
                    log::info!(
                        "Successfully verified signature. req_id: {} uri: {}",
                        rqctx.request_id,
                        req_uri
                    );
                }

                verified
            }
            (signature_res, mac_res) => {
                log::info!(
                    "Unable to test signature. req_id: {} uri: {} sig: {:?} mac_err: {:?}",
                    rqctx.request_id,
                    req_uri,
                    signature_res,
                    mac_res.err()
                );
                false
            }
        };

        Ok(HmacVerifiedBodyAudit {
            body,
            _body_type: PhantomData,
            content_type: rqctx.body_content_type.clone(),
            verified,
            _verifier: PhantomData,
        })
    }

    fn metadata(body_content_type: ApiEndpointBodyContentType) -> ExtractorMetadata {
        // The HMAC extractor is a wrapper around an inner type that does not perform any
        // alterations on the body content. Therefore we can use the metadata of the inner
        // type, as that is what we expect users to submit
        BodyType::metadata(body_content_type)
    }
}
