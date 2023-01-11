use dropshot::{ApiEndpointBodyContentType, Extractor, ExtractorMetadata, HttpError, TypedBody};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;

pub mod bearer;
mod http;
pub mod query;
pub mod sig;

/// Trait that defines for a given type how to construct that type from a byte slice, as well
/// as how the type ought to be described via an OpenAPI spec
pub trait FromBytes<E>: Send + Sync {
    fn from_bytes(bytes: &[u8], body_content_type: &ApiEndpointBodyContentType) -> Result<Self, E>
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
    fn from_bytes(bytes: &[u8], body_content_type: &ApiEndpointBodyContentType) -> Result<Self, HttpError>
    where
        Self: Sized,
    {
        match body_content_type {
            ApiEndpointBodyContentType::Json => serde_json::from_slice(bytes)
                .map_err(|e| HttpError::for_bad_request(None, format!("Failed to parse body: {e}"))),
            ApiEndpointBodyContentType::UrlEncoded => serde_urlencoded::from_bytes(bytes)
                .map_err(|e| HttpError::for_bad_request(None, format!("Failed to parse body: {e}"))),
            _ => Err(HttpError::for_bad_request(None, "Unsupported content type".to_string())),
        }
    }

    fn metadata(body_content_type: ApiEndpointBodyContentType) -> ExtractorMetadata {
        TypedBody::<Self>::metadata(body_content_type)
    }
}
