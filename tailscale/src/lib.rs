/*!
 * A rust library for interacting with the Tailscale API.
 *
 * For more information, the Tailscale API is still in beta. Once the docs are
 * online, we need to link to them.
 *
 * Example:
 *
 * ```
 * use serde::{Deserialize, Serialize};
 * use tailscale::Tailscale;
 *
 * async fn get_devices() {
 *     // Initialize the Tailscale client.
 *     let tailscale = Tailscale::new_from_env();
 *
 *     // List the devices.
 *     let devices = tailscale.list_devices().await.unwrap();
 *
 *     println!("{:?}", devices);
 * }
 * ```
 */
#![allow(clippy::field_reassign_with_default)]
use std::env;
use std::error;
use std::fmt;
use std::sync::Arc;

use reqwest::{header, Client, Method, Request, StatusCode, Url};
use serde::Serialize;

/// Endpoint for the Tailscale API.
const ENDPOINT: &str = "https://api.tailscale.com/api/v2/";

/// Entrypoint for interacting with the Tailscale API.
pub struct Tailscale {
    key: String,

    client: Arc<Client>,
}

impl Tailscale {
    /// Create a new Tailscale client struct. It takes a type that can convert into
    /// an &str (`String` or `Vec<u8>` for example). As long as the function is
    /// given a valid API key your requests will work.
    pub fn new<K>(key: K) -> Self
    where
        K: ToString,
    {
        let client = Client::builder().build();
        match client {
            Ok(c) => Self {
                key: key.to_string(),

                client: Arc::new(c),
            },
            Err(e) => panic!("creating client failed: {:?}", e),
        }
    }

    /// Create a new Tailscale client struct from environment variables. It
    /// takes a type that can convert into
    /// an &str (`String` or `Vec<u8>` for example). As long as the function is
    /// given a valid API key and your requests will work.
    pub fn new_from_env() -> Self {
        let key = env::var("TAILSCALE_API_KEY").unwrap();

        Tailscale::new(key)
    }

    fn request<B>(&self, method: Method, path: &str, body: B, query: Option<Vec<(&str, String)>>) -> Request
    where
        B: Serialize,
    {
        let base = Url::parse(ENDPOINT).unwrap();
        let url = base.join(path).unwrap();

        // Set the default headers.
        let mut headers = header::HeaderMap::new();
        headers.append(header::CONTENT_TYPE, header::HeaderValue::from_static("application/json"));

        let mut rb = self.client.request(method.clone(), url).headers(headers).basic_auth(&self.key, Some(""));

        match query {
            None => (),
            Some(val) => {
                rb = rb.query(&val);
            }
        }

        // Add the body, this is to ensure our GET and DELETE calls succeed.
        if method != Method::GET && method != Method::DELETE {
            rb = rb.json(&body);
        }

        // Build the request.
        rb.build().unwrap()
    }

    /// List devices.
    /// A maximum date range of 90 days is permitted. Provided dates should be ISO 8601 UTC dates.
    pub async fn list_devices(&self) -> Result<serde_json::Value, APIError> {
        // Build the request.
        // TODO: paginate.
        let request = self.request(Method::GET, "devices", (), None);

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                })
            }
        };

        Ok(resp.json().await.unwrap())
    }
}

/// Error type returned by our library.
pub struct APIError {
    pub status_code: StatusCode,
    pub body: String,
}

impl fmt::Display for APIError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "APIError: status code -> {}, body -> {}", self.status_code.to_string(), self.body)
    }
}

impl fmt::Debug for APIError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "APIError: status code -> {}, body -> {}", self.status_code.to_string(), self.body)
    }
}

// This is important for other errors to wrap this one.
impl error::Error for APIError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        // Generic error, underlying cause isn't tracked.
        None
    }
}
