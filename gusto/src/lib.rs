/*!
 * A rust library for interacting with the Gusto API.
 *
 * For more information, the Gusto API is documented at [docs.gusto.com](https://docs.gusto.com/).
 *
 * Example:
 *
 * ```
 * use gusto_api::{Gusto, Record};
 * use serde::{Deserialize, Serialize};
 *
 * async fn get_records() {
 *     // Initialize the Gusto client.
 *     let gusto = Gusto::new_from_env();
 *
 *     // Get the current records from a table.
 *     let mut records: Vec<Record<SomeFormat>> = gusto
 *         .list_records(
 *             "Table Name",
 *             "Grid view",
 *             vec!["the", "fields", "you", "want", "to", "return"],
 *         )
 *         .await
 *         .unwrap();
 *
 *     // Iterate over the records.
 *     for (i, record) in records.clone().iter().enumerate() {
 *         println!("{} - {:?}", i, record);
 *     }
 * }
 *
 * #[derive(Debug, Clone, Serialize, Deserialize)]
 * pub struct SomeFormat {
 *     pub x: bool,
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
use reqwest::{header, Client, Method, Request, StatusCode, Url};
use serde::{Deserialize, Serialize};

/// Endpoint for the Gusto API.
const ENDPOINT: &str = "https://api.gusto.com";

/// Entrypoint for interacting with the Gusto API.
pub struct Gusto {
    key: String,
    base_id: String,

    client: Arc<Client>,
}

impl Gusto {
    /// Create a new Gusto client struct. It takes a type that can convert into
    /// an &str (`String` or `Vec<u8>` for example). As long as the function is
    /// given a valid API Key and Base ID your requests will work.
    pub fn new<K, B>(key: K, base_id: B) -> Self
    where
        K: ToString,
        B: ToString,
    {
        let client = Client::builder().build();
        match client {
            Ok(c) => Self {
                key: key.to_string(),
                base_id: base_id.to_string(),

                client: Arc::new(c),
            },
            Err(e) => panic!("creating client failed: {:?}", e),
        }
    }

    /// Create a new Gusto client struct from environment variables. It
    /// takes a type that can convert into
    /// an &str (`String` or `Vec<u8>` for example). As long as the function is
    /// given a valid API Key and Base ID your requests will work.
    pub fn new_from_env() -> Self {
        let key = env::var("GUSTO_API_KEY").unwrap();
        let base_id = env::var("GUSTO_BASE_ID").unwrap();

        Gusto::new(key, base_id)
    }

    /// Get the currently set API key.
    pub fn get_key(&self) -> &str {
        &self.key
    }

    fn request<P, B>(
        &self,
        method: Method,
        path: P,
        body: B,
        query: Vec<(&str, String)>,
    ) -> Request
    where
        P: ToString,
        B: Serialize,
    {
        // Build the url.
        let base = Url::parse(ENDPOINT).unwrap();
        let mut p = path.to_string();
        // Make sure we have the leading "/".
        if !p.starts_with('/') {
            p = format!("/{}", p);
        }
        let url = base.join(&p).unwrap();

        let bt = format!("Bearer {}", self.key);
        let bearer = header::HeaderValue::from_str(&bt).unwrap();

        // Set the default headers.
        let mut headers = header::HeaderMap::new();
        headers.append(header::AUTHORIZATION, bearer);
        headers.append(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/json"),
        );

        let mut rb = self.client.request(method.clone(), url).headers(headers);

        if !query.is_empty() {
            rb = rb.query(&query);
        }

        // Add the body, this is to ensure our GET and DELETE calls succeed.
        if method != Method::GET && method != Method::DELETE {
            rb = rb.json(&body);
        }

        // Build the request.
        rb.build().unwrap()
    }

    /// List employees by company id.
    pub async fn current_user(&self) -> Result<CurrentUser, APIError> {
        // Build the request.
        let request = self.request(Method::GET, "/v1/me", (), vec![]);

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

    /// Get information about the current user.
    pub async fn list_employees_by_company_id(
        &self,
        company_id: &u64,
    ) -> Result<Vec<Employee>, APIError> {
        // Build the request.
        let request = self.request(
            Method::GET,
            &format!("/v1/companies/{}/employees", company_id),
            (),
            vec![],
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

/// An employee.
/// FROM: https://docs.gusto.com/v1/employees
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Employee {
    #[serde(default)]
    pub id: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub first_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub middle_initial: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub last_name: String,
    #[serde(default)]
    pub company_id: u64,
    #[serde(default)]
    pub manager_id: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub department: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ssn: String,
    // In the format YYYY-MM-DD.
    #[serde(with = "date_format")]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    #[serde(default)]
    pub id: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
    #[serde(default)]
    pub employee_id: u64,
    #[serde(default)]
    pub location_id: u64,
    pub location: Location,
    // In the format YYYY-MM-DD.
    #[serde(with = "date_format")]
    pub hire_date: NaiveDate,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(default)]
    pub primary: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub rate: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub payment_unit: String,
    #[serde(default)]
    pub current_compensation_id: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub compensations: Vec<Compensation>,
}

/// A location.
/// FROM: https://docs.gusto.com/v1/locations
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Location {
    #[serde(default)]
    pub id: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
    #[serde(default)]
    pub company_id: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub phone_number: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub street_1: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub street_2: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub city: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub state: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub zip: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub country: String,
    #[serde(default)]
    pub active: bool,
}

/// A compensation.
/// FROM: https://docs.gusto.com/v1/compensations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Compensation {
    #[serde(default)]
    pub id: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
    #[serde(default)]
    pub job_id: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub rate: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub payment_unit: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub flsa_status: String,
    // In the format YYYY-MM-DD.
    #[serde(with = "date_format")]
    pub effective_date: NaiveDate,
}

/// An address.
/// FROM: https://docs.gusto.com/v1/employee_home_address
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Address {
    #[serde(default)]
    pub id: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
    #[serde(default)]
    pub employee_id: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub street_1: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub street_2: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub city: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub state: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub zip: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub country: String,
    #[serde(default)]
    pub active: bool,
}

/// A garnishment.
/// FROM: https://docs.gusto.com/v1/garnishments
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Garnishment {
    #[serde(default)]
    pub id: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
    #[serde(default)]
    pub employee_id: u64,
    #[serde(default)]
    pub active: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub amount: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default)]
    pub court_ordered: bool,
    #[serde(default)]
    pub times: u32,
    #[serde(default)]
    pub recurring: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub annual_maximum: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub deduct_as_percentage: String,
}

/// Paid time off.
/// FROM: https://docs.gusto.com/v1/paid_time_off
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct PaidTimeOff {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub accrual_unit: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub accrual_period: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub accrual_rate: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub accrual_balance: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub maximum_accrual_balance: String,
    #[serde(default)]
    pub paid_at_termination: bool,
}

/// Termination.
/// FROM: https://docs.gusto.com/v1/terminations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Termination {
    #[serde(default)]
    pub id: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
    #[serde(default)]
    pub employee_id: u64,
    #[serde(default)]
    pub active: bool,
    // In the format YYYY-MM-DD.
    #[serde(with = "date_format")]
    pub effective_date: NaiveDate,
    #[serde(default)]
    pub run_termination_payroll: bool,
}

/// Current user.
/// FROM: https://docs.gusto.com/v1/current_user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentUser {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email: String,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub roles: HashMap<String, Role>,
}

/// A role.
/// FROM: https://docs.gusto.com/v1/current_user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Role {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub companies: Vec<Company>,
}

/// A company.
/// FROM: https://docs.gusto.com/v1/companies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Company {
    #[serde(default)]
    pub id: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub trade_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ein: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub entity_type: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub company_status: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub location: Vec<Location>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub compensations: HashMap<String, Compensation>,
    pub primary_signatory: Employee,
    pub primary_payroll_admin: Employee,
}

/// Convert the date format `%Y-%m-%d` to a NaiveDate.
pub mod date_format {
    use chrono::naive::NaiveDate;
    use serde::{self, Deserialize, Deserializer, Serializer};

    const FORMAT: &str = "%Y-%m-%d";

    // The signature of a serialize_with function must follow the pattern:
    //
    //    fn serialize<S>(&T, S) -> Result<S::Ok, S::Error>
    //    where
    //        S: Serializer
    //
    // although it may also be generic over the input types T.
    pub fn serialize<S>(
        date: &NaiveDate,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = format!("{}", date.format(FORMAT));
        serializer.serialize_str(&s)
    }

    // The signature of a deserialize_with function must follow the pattern:
    //
    //    fn deserialize<'de, D>(D) -> Result<T, D::Error>
    //    where
    //        D: Deserializer<'de>
    //
    // although it may also be generic over the output types T.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<NaiveDate, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;

        Ok(NaiveDate::parse_from_str(&s, FORMAT).unwrap())
    }
}
