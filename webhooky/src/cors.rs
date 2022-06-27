use dropshot::{HttpError, RequestContext, ServerContext};
use http::{header::HeaderValue, StatusCode};
use std::{collections::HashSet, sync::Arc};

#[derive(Debug)]
pub enum CorsFailure {
    InvalidValue(String),
    Missing,
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

/// Given a request context and a list of valid  origins, check that the incoming
/// request has specified one of those origins. If a valid origin has been specified then a
/// [`http::header::HeaderValue`] that should be added as the `Access-Control-Allow-Origin` header is returned.
/// If the origin header is missing, malformed, or not valid, a CORS error report is returned.
pub async fn get_cors_origin_header<C: ServerContext>(
    rqctx: Arc<RequestContext<C>>,
    allowed_origins: &[&'static str],
) -> Result<HeaderValue, CorsError> {
    get_cors_header(rqctx, "Origin", allowed_origins).await
}

/// Given a request context and a list of valid headers, checks that the incoming
/// request has specified a list of headers to be checked and that all headers including in
/// that list are allowed. If all of the requested headers are allowed, then a [`http::header::HeaderValue`]
/// that should be used as the `Access-Control-Allow-Headers` header is returned. If the request
/// headers is missing, malformed, or not valid a CORS error report is returned.
pub async fn get_cors_headers_header<C: ServerContext>(
    rqctx: Arc<RequestContext<C>>,
    allowed_headers: &[&'static str],
) -> Result<HeaderValue, CorsError> {
    get_cors_header(rqctx, "Access-Control-Request-Headers", allowed_headers).await
}

/// Constructs a header value to use in conjunction with a Access-Control-Allow-Methods header
pub async fn get_cors_method_header(allowed_methods: &[http::Method]) -> Result<HeaderValue, CorsError> {
    // This should never fail has we know that [`http::Method`] converts to valid str values and
    // joining those values with , remains valid
    Ok(HeaderValue::from_str(
        &allowed_methods
            .iter()
            .map(|m| m.as_str())
            .collect::<Vec<&str>>()
            .join(", "),
    )
    .expect("Converting method to str generated invalid string"))
}

pub async fn get_cors_header<C: ServerContext>(
    rqctx: Arc<RequestContext<C>>,
    header_name: &str,
    allowed: &[&'static str],
) -> Result<HeaderValue, CorsError> {
    let request = rqctx.request.lock().await;
    let incoming_headers = request.headers();

    let req_value = incoming_headers
        .get(header_name)
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| CorsError {
            failures: vec![CorsFailure::Missing],
        })?;

    // Split the header value on ", " to handle headers that pass in multiple values in a single
    // header like Access-Control-Request-Headers
    let req_values: HashSet<&str> = req_value.split(", ").collect();
    let allowed_values: HashSet<&str> = allowed.iter().map(|s| *s).collect();

    let diff: HashSet<&str> = req_values.difference(&allowed_values).map(|s| *s).collect();

    if diff.len() == 0 {
        // This should never panic as we are reusing the str value that was taken from a HeaderValue
        // on the request
        Ok(HeaderValue::from_str(req_value).expect("Rejoining passed in header values failed"))
    } else {
        Err(CorsError {
            failures: diff
                .into_iter()
                .map(|v| CorsFailure::InvalidValue(v.to_string()))
                .collect::<Vec<CorsFailure>>(),
        })
    }
}
