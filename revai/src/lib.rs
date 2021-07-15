/*!
 * A rust library for interacting with the RevAI API.
 *
 * For more information, you can check out their documentation at:
 * https://www.rev.ai/docs
 *
 * Example:
 *
 * ```
 * use revai::RevAI;
 * use serde::{Deserialize, Serialize};
 *
 * async fn geocode() {
 *     // Initialize the RevAI client.
 *     let revai = RevAI::new_from_env();
 *
 *     let transcript = revai.get_transcript("some_id").await.unwrap();
 *
 *     println!("{}", transcript);
 * }
 * ```
 */
#![allow(clippy::field_reassign_with_default)]
use std::{env, error, fmt, sync::Arc};

use bytes::Bytes;
use chrono::{offset::Utc, DateTime};
use reqwest::{header, multipart::Form, Client, Method, Request, StatusCode, Url};
use serde::{Deserialize, Serialize};

/// Endpoint for the RevAI API.
const ENDPOINT: &str = "https://api.rev.ai/speechtotext/v1/";

/// Entrypoint for interacting with the RevAI API.
pub struct RevAI {
    key: String,

    client: Arc<Client>,
}

impl RevAI {
    /// Create a new RevAI client struct. It takes a type that can convert into
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

    /// Create a new RevAI client struct from environment variables. It
    /// takes a type that can convert into
    /// an &str (`String` or `Vec<u8>` for example). As long as the function is
    /// given a valid API key and your requests will work.
    pub fn new_from_env() -> Self {
        let key = env::var("REVAI_API_KEY").unwrap();

        RevAI::new(key)
    }

    fn request(
        &self,
        method: Method,
        path: &str,
        form: Option<Form>,
        query: Option<Vec<(&str, String)>>,
    ) -> Request {
        let base = Url::parse(ENDPOINT).unwrap();
        let url = base.join(path).unwrap();

        // Set the default headers.
        let mut headers = header::HeaderMap::new();
        if method != Method::POST {
            headers.append(
                header::CONTENT_TYPE,
                header::HeaderValue::from_static("application/json"),
            );
        }
        if path.ends_with("/transcript") {
            // Get the plain text transcript
            headers.append(
                header::ACCEPT,
                header::HeaderValue::from_static("text/plain"),
            );
        } else {
            headers.append(
                header::ACCEPT,
                header::HeaderValue::from_static("application/json"),
            );
        }

        let mut rb = self
            .client
            .request(method, url)
            .headers(headers)
            .bearer_auth(&self.key);

        match query {
            None => (),
            Some(val) => {
                rb = rb.query(&val);
            }
        }

        // Add the body, this is to ensure our GET and DELETE calls succeed.
        if let Some(f) = form {
            rb = rb.multipart(f);
        }

        // Build the request.
        rb.build().unwrap()
    }

    /// Create a job.
    pub async fn create_job(&self, bytes: Bytes) -> Result<Job, APIError> {
        let form = Form::new()
            .part(
                "media",
                reqwest::multipart::Part::bytes(bytes.to_vec())
                    .mime_str("video/mp4")
                    .unwrap()
                    .file_name("testing.mp4"),
            )
            .text("options", "{}");
        // Build the request.
        let request = self.request(Method::POST, "jobs", Some(form), None);

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

    /// Get a transcript from a job ID.
    pub async fn get_transcript(&self, id: &str) -> Result<String, APIError> {
        // Build the request.
        let request = self.request(Method::GET, &format!("jobs/{}/transcript", id), None, None);

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

        Ok(resp.text().await.unwrap())
    }
}

/// Error type returned by our library.
pub struct APIError {
    pub status_code: StatusCode,
    pub body: String,
}

impl fmt::Display for APIError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "APIError: status code -> {}, body -> {}",
            self.status_code.to_string(),
            self.body
        )
    }
}

impl fmt::Debug for APIError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "APIError: status code -> {}, body -> {}",
            self.status_code.to_string(),
            self.body
        )
    }
}

// This is important for other errors to wrap this one.
impl error::Error for APIError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        // Generic error, underlying cause isn't tracked.
        None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub status: String,
    pub created_on: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub type_: String,
    #[serde(default)]
    pub delete_after_seconds: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobOptions {
    pub skip_diarization: bool,
    pub skip_punctuation: bool,
    pub remove_disfluencies: bool,
    pub filter_profanity: bool,
    pub speaker_channels_count: i64,
    pub metadata: String,
    pub callback_url: String,
    pub custom_vocabulary_id: String,
    pub language: String,
    pub delete_after_seconds: i64,
}
