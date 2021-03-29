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
        let mut request = self.request(Method::GET, "candidates", (), None);

        let mut resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                })
            }
        };

        let mut r: CandidatesResponse = resp.json().await.unwrap();
        let mut candidates = r.candidates;

        let mut next_href = r.next_href;

        // Paginate if we should.
        // TODO: make this more DRY
        while !next_href.is_empty() {
            request = self.request(Method::GET, next_href.trim_start_matches(ENDPOINT), (), None);

            resp = self.client.execute(request).await.unwrap();
            match resp.status() {
                StatusCode::OK => (),
                s => {
                    return Err(APIError {
                        status_code: s,
                        body: resp.text().await.unwrap(),
                    })
                }
            };

            // Try to deserialize the response.
            r = resp.json().await.unwrap();

            candidates.append(&mut r.candidates);

            next_href = r.next_href;
        }

        Ok(candidates)
    }

    /// Create a new candidate.
    pub async fn create_candidate(&self, email: &str) -> Result<Candidate, APIError> {
        // Build the request.
        let request = self.request(Method::POST, "candidates", (), Some(vec![("email", email.to_string())]));

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

    /// Get a report.
    pub async fn get_report(&self, id: &str) -> Result<Report, APIError> {
        // Build the request.
        let request = self.request(Method::GET, &format!("reports/{}", id), (), None);

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

    /// List invitations.
    pub async fn list_invitations(&self) -> Result<Vec<Invitation>, APIError> {
        // Build the request.
        // TODO: paginate.
        let request = self.request(Method::GET, "invitations", (), None);

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

        let r: InvitationsResponse = resp.json().await.unwrap();

        Ok(r.invitations)
    }

    /// Create a new invitation.
    pub async fn create_invitation(&self, candidate_id: &str, package: &str) -> Result<Invitation, APIError> {
        // Build the request.
        let request = self.request(
            Method::POST,
            "invitations",
            (),
            Some(vec![("candidate_id", candidate_id.to_string()), ("package", package.to_string())]),
        );

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
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub object: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub uri: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub first_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub middle_name: String,
    pub no_middle_name: bool,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub last_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub mother_maiden_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub email: String,
    #[serde(default)]
    pub phone: Option<i64>,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub zipcode: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub dob: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub ssn: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub driver_license_number: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub driver_license_state: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub previous_driver_license_number: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub previous_driver_license_state: String,
    #[serde(default)]
    pub copy_requested: bool,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub custom_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub report_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub geo_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub adjudication: String,
    #[serde(default)]
    pub metadata: Metadata,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Metadata {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub object: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub uri: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub status: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub result: String,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub revised_at: Option<DateTime<Utc>>,
    pub upgraded_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub turnaround_time: Option<i64>,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub adjudication: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub package: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub source: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub candidate_id: String,
    #[serde(default)]
    pub drug_screening: DrugScreening,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub ssn_trace_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub arrest_search_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub drug_screening_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub facis_search_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub federal_criminal_search_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub global_watchlist_search_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub sex_offender_search_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub national_criminal_search_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub county_criminal_search_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub personal_reference_verification_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub professional_reference_verification_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub motor_vehicle_report_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub professional_license_verification_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub state_criminal_searches: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub document_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub geo_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub program_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidate_story_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub estimated_completion_time: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DrugScreening {
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub status: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub result: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub disposition: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub mro_notes: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub analytes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<String>,
    pub screening_pass_expires_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub appointment_id: String,
}

/// The data type for an API response.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct InvitationsResponse {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub object: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub next_href: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub previous_href: String,
    #[serde(default)]
    pub count: i64,
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "data")]
    pub invitations: Vec<Invitation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invitation {
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub object: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub uri: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub invitation_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub deleted_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub package: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub candidate_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub report_id: String,
}

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
