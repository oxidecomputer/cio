use async_trait::async_trait;
use dropshot::{ApiEndpointBodyContentType, Extractor, ExtractorMetadata, HttpError, RequestContext, ServerContext};
use http::header::HeaderMap;
use std::sync::Arc;

pub struct Headers(pub HeaderMap);

#[async_trait]
impl Extractor for Headers {
    async fn from_request<Context: ServerContext>(rqctx: Arc<RequestContext<Context>>) -> Result<Headers, HttpError> {
        let request = rqctx.request.lock().await;
        Ok(Headers(request.headers().clone()))
    }

    fn metadata(_body_content_type: ApiEndpointBodyContentType) -> ExtractorMetadata {
        ExtractorMetadata {
            paginated: false,
            parameters: vec![],
        }
    }
}
