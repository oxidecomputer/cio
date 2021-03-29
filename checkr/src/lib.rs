/*!
 * A rust library for interacting with the Checker API.
 *
 * For more information, the Checker API is still is doumented here:
 * https://docs.checkr.com
 *
 * Example:
 *
 * ```
 * use checkr::Checkr;
 * use serde::{Deserialize, Serialize};
 *
 * async fn get_candidates() {
 *     // Initialize the Checker client.
 *     let checkr = Checkr::new_from_env();
 *
 *     // List the candidates.
 *     let candidates = checkr.list_candidates().await.unwrap();
 *
 *     println!("{:?}", candidates);
 * }
 * ```
 */
#![allow(clippy::field_reassign_with_default)]
use std::env;
use std::error;
use std::fmt;
use std::sync::Arc;

use chrono::offset::Utc;
use chrono::DateTime;
use reqwest::{header, Client, Method, Request, StatusCode, Url};
use serde::{Deserialize, Serialize};

/// Endpoint for the Checker API.
const ENDPOINT: &str = "https://api.checkr.com/v1/";

/// Entrypoint for interacting with the Checker API.
pub struct Checkr {
    key: String,

    client: Arc<Client>,
}

impl Checkr {
    /// Create a new Checker client struct. It takes a type that can convert into
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

    /// Create a new Checker client struct from environment variables. It
    /// takes a type that can convert into
    /// an &str (`String` or `Vec<u8>` for example). As long as the function is
    /// given a valid API key and domain and your requests will work.
    pub fn new_from_env() -> Self {
        let key = env::var("CHECKR_API_KEY").unwrap();

        Checkr::new(key)
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

    /// List candidates.
    pub async fn list_candidates(&self) -> Result<Vec<Candidate>, APIError> {
        // Build the request.
        // TODO: paginate.
        let request = self.request(Method::GET, "candidates", (), None);

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

        let r: CandidatesResponse = resp.json().await.unwrap();

        Ok(r.candidates)
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

/// The data type for an API response.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CandidatesResponse {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub object: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub next_href: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub previous_href: String,
    #[serde(default)]
    pub count: i64,
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "data")]
    pub candidates: Vec<Candidate>,
}

/// The data type for a candidate.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Candidate {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub object: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub uri: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub first_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub middle_name: String,
    #[serde(default)]
    pub no_middle_name: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub last_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub mother_maiden_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email: String,
    #[serde(default)]
    pub phone: i64,
    #[serde(default)]
    pub zipcode: i64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub dob: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ssn: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub driver_license_number: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub driver_license_state: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub previous_driver_license_number: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub previous_driver_license_state: String,
    #[serde(default)]
    pub copy_requested: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub custom_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub report_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub geo_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub adjudication: String,
    pub metadata: Metadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metadata {}

pub mod deserialize_null_string {
    use serde::{self, Deserialize, Deserializer};

    // The signature of a deserialize_with function must follow the pattern:
    //
    //    fn deserialize<'de, D>(D) -> Result<T, D::Error>
    //    where
    //        D: Deserializer<'de>
    //
    // although it may also be generic over the output types T.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<String, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer).unwrap_or_default();

        Ok(s)
    }
}
