/*!
 * A rust library for interacting with the Gusto API.
 *
 * For more information, the Gusto API is documented at [docs.gusto.com](https://docs.gusto.com/).
 *
 * Example:
 *
 * ```
 * use gusto_api::Gusto;
 * use serde::{Deserialize, Serialize};
 *
 * async fn get_current_user() {
 *     // Initialize the Gusto client.
 *     let gusto = Gusto::new_from_env("", "", "");
 *
 *     // Get the current user.
 *     let current_user = gusto.current_user().await.unwrap();
 *
 *     println!("{:?}", current_user);
 * }
 * ```
 */
use std::collections::HashMap;
use std::env;
use std::error;
use std::fmt;
use std::fmt::Debug;
use std::sync::Arc;

use chrono::naive::NaiveDate;
use reqwest::{header, Client, Method, RequestBuilder, StatusCode, Url};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Endpoint for the Gusto API.
const ENDPOINT: &str = "https://api.gusto.com/";

/// Entrypoint for interacting with the Gusto API.
pub struct Gusto {
    token: String,
    // This expires in 101 days. It is hardcoded in the GitHub Actions secrets,
    // We might want something a bit better like storing it in the database.
    refresh_token: String,
    client_id: String,
    client_secret: String,
    redirect_uri: String,
    company_id: String,

    client: Arc<Client>,
}

impl Gusto {
    /// Create a new Gusto client struct. It takes a type that can convert into
    /// an &str (`String` or `Vec<u8>` for example). As long as the function is
    /// given a valid API key your requests will work.
    pub fn new<I, K, R, T, Q, C>(client_id: I, client_secret: K, redirect_uri: R, token: T, refresh_token: Q, company_id: C) -> Self
    where
        I: ToString,
        K: ToString,
        R: ToString,
        T: ToString,
        Q: ToString,
        C: ToString,
    {
        let client = Client::builder().build();
        match client {
            Ok(c) => {
                let g = Gusto {
                    client_id: client_id.to_string(),
                    client_secret: client_secret.to_string(),
                    redirect_uri: redirect_uri.to_string(),
                    token: token.to_string(),
                    refresh_token: refresh_token.to_string(),
                    company_id: company_id.to_string(),

                    client: Arc::new(c),
                };

                if g.token.is_empty() || g.refresh_token.is_empty() {
                    // This is super hacky and a work around since there is no way to
                    // auth without using the browser.
                    println!("gusto consent URL: {}", g.user_consent_url());
                }
                // We do not refresh the access token since we leave that up to the
                // user to do so they can re-save it to their database.

                g
            }
            Err(e) => panic!("creating client failed: {:?}", e),
        }
    }

    /// Create a new Gusto client struct from environment variables. It
    /// takes a type that can convert into
    /// an &str (`String` or `Vec<u8>` for example). As long as the function is
    /// given a valid API key and your requests will work.
    /// We pass in the token and refresh token to the client so if you are storing
    /// it in a database, you can get it first.
    pub fn new_from_env<T, R, C>(token: T, refresh_token: R, company_id: C) -> Self
    where
        T: ToString,
        R: ToString,
        C: ToString,
    {
        let client_id = env::var("GUSTO_CLIENT_ID").unwrap();
        let client_secret = env::var("GUSTO_CLIENT_SECRET").unwrap();
        let redirect_uri = env::var("GUSTO_REDIRECT_URI").unwrap();

        Gusto::new(client_id, client_secret, redirect_uri, token, refresh_token, company_id)
    }

    fn request<P>(&self, method: Method, path: P) -> RequestBuilder
    where
        P: ToString,
    {
        // Build the url.
        let base = Url::parse(ENDPOINT).unwrap();
        let mut p = path.to_string();
        // Make sure we have the leading "/".
        if !p.starts_with('/') {
            p = format!("/{}", p);
        }
        let url = base.join(&p).unwrap();

        let bt = format!("Bearer {}", self.token);
        let bearer = header::HeaderValue::from_str(&bt).unwrap();

        // Set the default headers.
        let mut headers = header::HeaderMap::new();
        headers.append(header::AUTHORIZATION, bearer);
        headers.append(header::CONTENT_TYPE, header::HeaderValue::from_static("application/json"));

        self.client.request(method, url).headers(headers)
    }

    pub fn user_consent_url(&self) -> String {
        format!("{}oauth/authorize?client_id={}&response_type=code&redirect_uri={}", ENDPOINT, self.client_id, self.redirect_uri)
    }

    pub async fn refresh_access_token(&mut self) -> Result<AccessToken, APIError> {
        let mut headers = header::HeaderMap::new();
        headers.append(header::ACCEPT, header::HeaderValue::from_static("application/json"));

        let params = [
            ("grant_type", "refresh_token"),
            ("refresh_token", &self.refresh_token),
            ("client_id", &self.client_id),
            ("client_secret", &self.client_secret),
            ("redirect_uri", &self.redirect_uri),
        ];
        let client = reqwest::Client::new();
        let resp = client.post(&format!("{}oauth/token", ENDPOINT)).headers(headers).form(&params).send().await.unwrap();

        // Unwrap the response.
        let t: AccessToken = resp.json().await.unwrap();

        self.token = t.access_token.to_string();
        self.refresh_token = t.refresh_token.to_string();

        Ok(t)
    }

    pub async fn get_access_token(&mut self, code: &str) -> Result<AccessToken, APIError> {
        let mut headers = header::HeaderMap::new();
        headers.append(header::ACCEPT, header::HeaderValue::from_static("application/json"));

        let params = [
            ("grant_type", "authorization_code"),
            ("code", code),
            ("client_id", &self.client_id),
            ("client_secret", &self.client_secret),
            ("redirect_uri", &self.redirect_uri),
        ];
        let client = reqwest::Client::new();
        let resp = client.post(&format!("{}oauth/token", ENDPOINT)).headers(headers).form(&params).send().await.unwrap();

        // Unwrap the response.
        let t: AccessToken = resp.json().await.unwrap();

        self.token = t.access_token.to_string();
        self.refresh_token = t.refresh_token.to_string();

        Ok(t)
    }

    /// Get information about the current user.
    pub async fn current_user(&self) -> Result<CurrentUser, APIError> {
        // Build the request.
        let rb = self.request(Method::GET, "/v1/me");
        let request = rb.build().unwrap();

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

        // Try to deserialize the response.
        let result: CurrentUser = resp.json().await.unwrap();

        Ok(result)
    }

    /// List all employees.
    pub async fn list_employees(&self) -> Result<Vec<Employee>, APIError> {
        // Build the request.
        let rb = self.request(Method::GET, &format!("/v1/companies/{}/employees", self.company_id));
        let request = rb.build().unwrap();

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

        // Try to deserialize the response.
        let result: Vec<Employee> = resp.json().await.unwrap();

        Ok(result)
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

/// An employee.
/// FROM: https://docs.gusto.com/v1/employees
#[derive(Debug, Clone, JsonSchema, Serialize, Deserialize)]
pub struct Employee {
    #[serde(default)]
    pub id: u64,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub version: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub first_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub middle_initial: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub last_name: String,
    #[serde(default)]
    pub company_id: u64,
    #[serde(default)]
    pub manager_id: u64,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub department: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub email: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub ssn: String,
    // In the format YYYY-MM-DD.
    pub date_of_birth: NaiveDate,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub jobs: Vec<Job>,
    pub home_address: Address,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub garnishments: Vec<Garnishment>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub eligible_paid_time_off: Vec<PaidTimeOff>,
    #[serde(default)]
    pub onboarded: bool,
    #[serde(default)]
    pub terminated: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub terminations: Vec<Termination>,
}

/// A job.
/// FROM: https://docs.gusto.com/v1/jobs
#[derive(Debug, Clone, JsonSchema, Serialize, Deserialize)]
pub struct Job {
    #[serde(default)]
    pub id: u64,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub version: String,
    #[serde(default)]
    pub employee_id: u64,
    #[serde(default)]
    pub location_id: u64,
    pub location: Location,
    // In the format YYYY-MM-DD.
    pub hire_date: NaiveDate,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub title: String,
    #[serde(default)]
    pub primary: bool,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub rate: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub payment_unit: String,
    #[serde(default)]
    pub current_compensation_id: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub compensations: Vec<Compensation>,
}

/// A location.
/// FROM: https://docs.gusto.com/v1/locations
#[derive(Debug, Default, JsonSchema, Clone, Serialize, Deserialize)]
pub struct Location {
    #[serde(default)]
    pub id: u64,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub version: String,
    #[serde(default)]
    pub company_id: u64,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub phone_number: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub street_1: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub street_2: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub city: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub state: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub zip: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub country: String,
    #[serde(default)]
    pub active: bool,
}

/// A compensation.
/// FROM: https://docs.gusto.com/v1/compensations
#[derive(Debug, Clone, JsonSchema, Serialize, Deserialize)]
pub struct Compensation {
    #[serde(default)]
    pub id: u64,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub version: String,
    #[serde(default)]
    pub job_id: u64,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub rate: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub payment_unit: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub flsa_status: String,
    // In the format YYYY-MM-DD.
    pub effective_date: NaiveDate,
}

/// An address.
/// FROM: https://docs.gusto.com/v1/employee_home_address
#[derive(Debug, Default, JsonSchema, Clone, Serialize, Deserialize)]
pub struct Address {
    #[serde(default)]
    pub id: u64,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub version: String,
    #[serde(default)]
    pub employee_id: u64,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub street_1: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub street_2: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub city: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub state: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub zip: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub country: String,
    #[serde(default)]
    pub active: bool,
}

/// A garnishment.
/// FROM: https://docs.gusto.com/v1/garnishments
#[derive(Debug, Default, JsonSchema, Clone, Serialize, Deserialize)]
pub struct Garnishment {
    #[serde(default)]
    pub id: u64,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub version: String,
    #[serde(default)]
    pub employee_id: u64,
    #[serde(default)]
    pub active: bool,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub amount: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub description: String,
    #[serde(default)]
    pub court_ordered: bool,
    #[serde(default)]
    pub times: u32,
    #[serde(default)]
    pub recurring: bool,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub annual_maximum: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub deduct_as_percentage: String,
}

/// Paid time off.
/// FROM: https://docs.gusto.com/v1/paid_time_off
#[derive(Debug, Default, JsonSchema, Clone, Serialize, Deserialize)]
pub struct PaidTimeOff {
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub accrual_unit: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub accrual_period: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub accrual_rate: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub accrual_balance: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub maximum_accrual_balance: String,
    #[serde(default)]
    pub paid_at_termination: bool,
}

/// Termination.
/// FROM: https://docs.gusto.com/v1/terminations
#[derive(Debug, Clone, JsonSchema, Serialize, Deserialize)]
pub struct Termination {
    #[serde(default)]
    pub id: u64,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub version: String,
    #[serde(default)]
    pub employee_id: u64,
    #[serde(default)]
    pub active: bool,
    // In the format YYYY-MM-DD.
    pub effective_date: NaiveDate,
    #[serde(default)]
    pub run_termination_payroll: bool,
}

/// Current user.
/// FROM: https://docs.gusto.com/v1/current_user
#[derive(Debug, Clone, JsonSchema, Serialize, Deserialize)]
pub struct CurrentUser {
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub email: String,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub roles: HashMap<String, Role>,
}

/// A role.
/// FROM: https://docs.gusto.com/v1/current_user
#[derive(Debug, Clone, JsonSchema, Serialize, Deserialize)]
pub struct Role {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub companies: Vec<Company>,
}

/// A company.
/// FROM: https://docs.gusto.com/v1/companies
#[derive(Debug, Clone, JsonSchema, Serialize, Deserialize)]
pub struct Company {
    #[serde(default)]
    pub id: u64,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub trade_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub ein: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub entity_type: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub company_status: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub location: Vec<Location>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub compensations: HashMap<String, Compensation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_signatory: Option<Employee>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_payroll_admin: Option<Employee>,
}

#[derive(Debug, JsonSchema, Clone, Default, Serialize, Deserialize)]
pub struct AccessToken {
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub access_token: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub token_type: String,
    #[serde(default)]
    pub expires_in: i64,
    #[serde(default)]
    pub x_refresh_token_expires_in: i64,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub refresh_token: String,
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
