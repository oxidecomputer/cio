use async_trait::async_trait;
use dropshot::{ApiEndpointBodyContentType, ExtensionMode, SharedExtractor, ExtractorMetadata, HttpError, RequestContext, ServerContext};
use http::header::HeaderMap;

pub struct Headers(pub HeaderMap);

#[async_trait]
impl SharedExtractor for Headers {
    async fn from_request<Context: ServerContext>(rqctx: &RequestContext<Context>) -> Result<Headers, HttpError> {
        let request = &rqctx.request;
        Ok(Headers(request.headers().clone()))
    }

    fn metadata(_body_content_type: ApiEndpointBodyContentType) -> ExtractorMetadata {
        ExtractorMetadata {
            extension_mode: ExtensionMode::None,
            parameters: vec![],
        }
    }
}
