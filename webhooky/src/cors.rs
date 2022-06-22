use dropshot::{HttpError, RequestContext, ServerContext};
use http::{header::HeaderValue, StatusCode};
use std::sync::Arc;

/// Given a request context and a list of valid origins cross origins, check that the incoming
/// request has specified one of those origins. If a valid origin has been specified then a
/// HeaderValue that should be added as the `Access-Control-Allow-Origin` header is returned.
/// If the origin header is missing, malformed, or not valid, a 403 Forbidden error is returned.
pub async fn get_cors_origin_header<C: ServerContext>(
    rqctx: Arc<RequestContext<C>>,
    allowed_origins: &[&'static str],
) -> Result<HeaderValue, HttpError> {
    let request = rqctx.request.lock().await;
    let incoming_headers = request.headers();

    let req_origin = incoming_headers
        .get("Origin")
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| HttpError::for_status(None, StatusCode::FORBIDDEN))?;

    match allowed_origins.iter().find(|o| *o == &req_origin) {
        Some(origin) => Ok(HeaderValue::from_static(origin)),
        None => Err(HttpError::for_status(None, StatusCode::FORBIDDEN)),
    }
}
