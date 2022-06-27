use dropshot::{HttpError, RequestContext, ServerContext};
use http::{header::HeaderValue, StatusCode};
use std::sync::Arc;

#[derive(Debug)]
pub enum CorsFailure {
    InvalidOrigin(String),
    OriginMissing,
}

#[derive(Debug)]
pub struct CorsError {
    pub failures: Vec<CorsFailure>,
}

impl From<CorsError> for HttpError {
    fn from(_: CorsError) -> Self {
        // Currently all CORS errors collapse to a Forbidden response, they do not report
        // on what the expected values are
        HttpError::for_status(None, StatusCode::FORBIDDEN)
    }
}

/// Given a request context and a list of valid origins cross origins, check that the incoming
/// request has specified one of those origins. If a valid origin has been specified then a
/// HeaderValue that should be added as the `Access-Control-Allow-Origin` header is returned.
/// If the origin header is missing, malformed, or not valid, a CORS error report is returned.
pub async fn get_cors_origin_header<C: ServerContext>(
    rqctx: Arc<RequestContext<C>>,
    allowed_origins: &[&'static str],
) -> Result<HeaderValue, CorsError> {
    let request = rqctx.request.lock().await;
    let incoming_headers = request.headers();

    let req_origin = incoming_headers
        .get("Origin")
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| CorsError {
            failures: vec![CorsFailure::OriginMissing],
        })?;

    match allowed_origins.iter().find(|o| *o == &req_origin) {
        Some(origin) => Ok(HeaderValue::from_static(origin)),
        None => Err(CorsError {
            failures: vec![CorsFailure::InvalidOrigin(req_origin.to_string())],
        }),
    }
}
