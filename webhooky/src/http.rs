use async_trait::async_trait;
use dropshot::{Extractor, ExtractorMetadata, HttpError, RequestContext, ServerContext};
use http::header::HeaderMap;
use std::sync::Arc;

pub struct Headers(pub HeaderMap);

#[async_trait]
impl Extractor for Headers {
    async fn from_request<Context: ServerContext>(rqctx: Arc<RequestContext<Context>>) -> Result<Headers, HttpError> {
        let request = rqctx.request.lock().await;
        Ok(Headers(request.headers().clone()))
    }

    fn metadata() -> ExtractorMetadata {
        ExtractorMetadata {
            paginated: false,
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
