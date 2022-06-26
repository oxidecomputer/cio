use dropshot::{ApiEndpointBodyContentType, Extractor, ExtractorMetadata, HttpError, TypedBody, UntypedBody};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde_json;

pub mod bearer;
mod http;
pub mod query;
pub mod sig;

/// Trait that defines for a given type how to construct that type from a byte slice, as well
/// as how the type out to be described via an OpenAPI spec
pub trait FromBytes<E>: Send + Sync {
    fn from_bytes(bytes: &[u8]) -> Result<Self, E>
    where
        Self: Sized;
    fn metadata(body_content_type: ApiEndpointBodyContentType) -> ExtractorMetadata;
}

/// Provide an implementation of from_bytes for anything that can be deserialized from a JSON
/// payload. The JsonSchema bounds allows piggybacking on [`TypedBody`](dropshot::TypedBody) for generating OpenAPI data.
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

    fn metadata(body_content_type: ApiEndpointBodyContentType) -> ExtractorMetadata {
        TypedBody::<Self>::metadata(body_content_type)
    }
}

/// A type very similar to [`UntypedBody`](dropshot::UntypedBody), used for holding a body of arbitrary bytes
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
    fn metadata(body_content_type: ApiEndpointBodyContentType) -> ExtractorMetadata {
        UntypedBody::metadata(body_content_type)
    }
}
