use async_trait::async_trait;
use dropshot::{
    ApiEndpointBodyContentType, ExtensionMode, ExtractorMetadata, HttpError, RequestContext, ServerContext,
    SharedExtractor,
};
use http::header::HeaderMap;

pub struct Headers(pub HeaderMap);

#[async_trait]
impl SharedExtractor for Headers {
    async fn from_request<Context: ServerContext>(rqctx: &RequestContext<Context>) -> Result<Headers, HttpError> {
        Ok(Headers(rqctx.request.headers().clone()))
    }

    fn metadata(_body_content_type: ApiEndpointBodyContentType) -> ExtractorMetadata {
        ExtractorMetadata {
            extension_mode: ExtensionMode::None,
            parameters: vec![],
        }
    }
}

pub fn unauthorized() -> HttpError {
    HttpError::for_client_error(None, http::StatusCode::UNAUTHORIZED, "".to_string())
}

pub fn internal_error() -> HttpError {
    HttpError::for_internal_error("".to_string())
}
