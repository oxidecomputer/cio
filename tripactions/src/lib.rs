/*!
 * A rust library for interacting with the TripActions v3 API.
 *
 * For more information, the TripActions v1 API is documented at
 * https://app.tripactions.com/api/public/documentation/swagger-ui/index.html?configUrl=/api/public/documentation/api-docs/swagger-config
 *
 * Example:
 *
 * ```
 * use tripactions::TripActions;
 *
 * async fn get_bookings() {
 *     // Initialize the TripActions client.
 *     let tripactions = TripActions::new_from_env("");
 *
 *     let bookings = tripactions.get_bookings().await.unwrap();
 *
 *     println!("bookings: {:?}", bookings);
 * }
 * ```
 */
use std::env;
use std::error;
use std::fmt;
use std::sync::Arc;

use chrono::{DateTime, Duration, NaiveDate, Utc};
use reqwest::{header, Client, Method, Request, StatusCode, Url};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Endpoint for the TripActions API.
const ENDPOINT: &str = "https://api.tripactions.com/v1/";

const TOKEN_ENDPOINT: &str = "https://api.tripactions.com/ta-auth/oauth/token";

/// Entrypoint for interacting with the TripActions API.
pub struct TripActions {
    token: String,
    client_id: String,
    client_secret: String,

    client: Arc<Client>,
}

impl TripActions {
    /// Create a new TripActions client struct. It takes a type that can convert into
    /// an &str (`String` or `Vec<u8>` for example). As long as the function is
    /// given a valid API client ID and secret your requests will work.
    pub fn new<I, K, T>(client_id: I, client_secret: K, token: T) -> Self
    where
        I: ToString,
        K: ToString,
        T: ToString,
    {
        let client = Client::builder().build();
        match client {
            Ok(c) => TripActions {
                client_id: client_id.to_string(),
                client_secret: client_secret.to_string(),
                token: token.to_string(),

                client: Arc::new(c),
            },
            Err(e) => panic!("creating client failed: {:?}", e),
        }
    }

    /// Create a new TripActions client struct from environment variables. It
    /// takes a type that can convert into
    /// an &str (`String` or `Vec<u8>` for example). As long as the function is
    /// given a valid API key and your requests will work.
    /// We pass in the token and refresh token to the client so if you are storing
    /// it in a database, you can get it first.
    pub fn new_from_env<T>(token: T) -> Self
    where
        T: ToString,
    {
        let client_id = env::var("TRIPACTIONS_CLIENT_ID").unwrap();
        let client_secret = env::var("TRIPACTIONS_CLIENT_SECRET").unwrap();

        TripActions::new(client_id, client_secret, token)
    }

    fn request<B>(&self, method: Method, path: &str, body: B, query: Option<Vec<(&str, String)>>) -> Request
    where
        B: Serialize,
    {
        let base = Url::parse(ENDPOINT).unwrap();
        let url = base.join(&path).unwrap();

        let bt = format!("Bearer {}", self.token);
        let bearer = header::HeaderValue::from_str(&bt).unwrap();

        // Set the default headers.
        let mut headers = header::HeaderMap::new();
        headers.append(header::AUTHORIZATION, bearer);
        headers.append(header::CONTENT_TYPE, header::HeaderValue::from_static("application/json"));

        let mut rb = self.client.request(method.clone(), url).headers(headers);

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

    /// Get all the bookings in the account.
    pub async fn get_bookings(&self) -> Result<Vec<Booking>, APIError> {
        let date_from = Utc::now().checked_sub_signed(Duration::weeks(52)).unwrap().timestamp();
        let date_to = Utc::now().timestamp();

        // Build the request.
        let mut request = self.request(
            Method::GET,
            "bookings",
            (),
            Some(vec![("createdFrom", format!("{}", date_from)), ("createdTo", format!("{}", date_to))]),
        );

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

        // Try to deserialize the response.
        let mut r: Bookings = resp.json().await.unwrap();

        let mut bookings = r.data;

        let mut page = r.page.current_page + 1;

        // Paginate if we should.
        // TODO: make this more DRY
        while page <= (r.page.total_pages - 1) {
            request = self.request(
                Method::GET,
                "bookings",
                (),
                Some(vec![("createdFrom", format!("{}", date_from)), ("createdTo", format!("{}", date_to)), ("page", format!("{}", page))]),
            );

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

            bookings.append(&mut r.data);

            page = r.page.current_page + 1;
        }

        Ok(bookings)
    }

    pub async fn get_access_token(&mut self) -> Result<AccessToken, APIError> {
        let mut headers = header::HeaderMap::new();
        headers.append(header::ACCEPT, header::HeaderValue::from_static("application/json"));

        let params = [("grant_type", "client_credentials"), ("client_id", &self.client_id), ("client_secret", &self.client_secret)];
        let client = reqwest::Client::new();
        let resp = client
            .post(TOKEN_ENDPOINT)
            .headers(headers)
            .form(&params)
            .basic_auth(&self.client_id, Some(&self.client_secret))
            .send()
            .await
            .unwrap();

        // Unwrap the response.
        let t: AccessToken = resp.json().await.unwrap();

        self.token = t.access_token.to_string();

        Ok(t)
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

#[derive(Debug, JsonSchema, Clone, Default, Serialize, Deserialize)]
pub struct AccessToken {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub access_token: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub token_type: String,
    #[serde(default)]
    pub expires_in: i64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub scope: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub refresh_token: String,
    #[serde(default)]
    pub refresh_token_expires_in: i64,
}

#[derive(Debug, JsonSchema, Clone, Default, Serialize, Deserialize)]
pub struct Bookings {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub data: Vec<Booking>,
    #[serde(default)]
    pub page: Page,
}

#[derive(Debug, JsonSchema, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Booking {
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub uuid: String,
    pub created: DateTime<Utc>,
    pub last_modified: DateTime<Utc>,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub pcc: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub booking_type: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub flight: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub booking_id: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub vendor: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub preferred_vendor: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub corporate_discount_used: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub cabin: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub booking_status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancelled_at: Option<DateTime<Utc>>,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub cancellation_reason: String,
    #[serde(default)]
    pub lead_time_in_days: i64,
    pub start_date: NaiveDate,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_date: Option<NaiveDate>,
    #[serde(default)]
    pub booking_duration: i64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub passengers: Vec<Passenger>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub segments: Vec<Segment>,
    #[serde(default)]
    pub booker: Booker,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trip_uuids: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub currency: String,
    #[serde(default)]
    pub currency_exhange_rate_from_usd: f64,
    #[serde(default)]
    pub optimal_price: f64,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub payment_schedule: String,
    #[serde(default)]
    pub base_price: f64,
    #[serde(default)]
    pub unitary_price: f64,
    pub saving: f64,
    #[serde(default)]
    pub saving_missed: f64,
    #[serde(default)]
    pub tax: f64,
    #[serde(default)]
    pub resort_fee: f64,
    #[serde(default)]
    pub trip_fee: f64,
    #[serde(default)]
    pub booking_fee: f64,
    #[serde(default)]
    pub vip_fee: f64,
    #[serde(default)]
    pub seats_fee: f64,
    #[serde(default)]
    pub extras_fees: f64,
    #[serde(default)]
    pub airline_credit_card_surcharge: f64,
    #[serde(default)]
    pub grand_total: f64,
    #[serde(default)]
    pub usd_grand_total: f64,
    #[serde(default)]
    pub vat: f64,
    #[serde(default)]
    pub exchange_amount: f64,
    #[serde(default)]
    pub exchange_fee: f64,
    #[serde(default)]
    pub net_charge: f64,
    #[serde(default)]
    pub gst: f64,
    #[serde(default)]
    pub hst: f64,
    #[serde(default)]
    pub qst: f64,
    #[serde(default)]
    pub travel_spend: f64,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub payment_method: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub name_on_credit_card: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub payment_method_used: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub payment_credit_card_type_name: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub company_payment_method: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub statement_description: String,
    #[serde(default)]
    pub expensed: bool,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub booking_method: String,
    #[serde(default)]
    pub out_of_policy: bool,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub out_of_policy_description: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub out_of_policy_violations: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub out_of_policy_violation_types: Option<Vec<String>>,
    #[serde(default)]
    pub trip_bucks_earned: f64,
    #[serde(default)]
    pub trip_bucks_earned_usd: f64,
    #[serde(default)]
    pub origin: Destination,
    #[serde(default)]
    pub destination: Destination,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub trip_length: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub trip_description: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub approver_reason: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub approver_email: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub approval_changed_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub etickets: Option<Vec<String>>,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub invoice: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub pdf: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub inventory: String,
    #[serde(default)]
    pub flight_miles: f64,
    #[serde(default)]
    pub train_miles: f64,
    #[serde(default)]
    pub carbon_emissions: f64,
    #[serde(default)]
    pub carbon_offset_cost: f64,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub fare_class: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub purpose: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub reason: String,
    #[serde(default)]
    pub cnr: Cnr,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seats: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom_fields: Vec<CustomField>,
}

#[derive(Debug, Default, JsonSchema, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Booker {
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub uuid: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub email: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub employeed_id: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub manager_uuid: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub manager_name: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub manager_email: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub department: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub cost_center: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub region: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub subsidiary: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub billable_entity: String,
}

#[derive(Debug, Default, JsonSchema, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cnr {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_price: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cnr_codes: Option<Vec<String>>,
}

#[derive(Debug, Default, JsonSchema, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomField {
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub value: String,
}

#[derive(Debug, Default, JsonSchema, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Destination {
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub country: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub state: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub city: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub airport_code: String,
}

#[derive(Debug, Default, JsonSchema, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Passenger {
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub traveler_type: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub status: String,
    #[serde(default)]
    pub person: Booker,
}

#[derive(Debug, JsonSchema, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    #[serde(default)]
    pub start_timestamp: i64,
    #[serde(default)]
    pub end_timestamp: i64,
    #[serde(default)]
    pub departure: Destination,
    #[serde(default)]
    pub arrival: Destination,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub provider_code: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub provider_name: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub flight_number: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub airline_alliance: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub hotel_chain: String,
}

#[derive(Debug, Default, JsonSchema, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    #[serde(default)]
    pub total_pages: i64,
    #[serde(default)]
    pub current_page: i64,
    #[serde(default)]
    pub page_size: i64,
    #[serde(default)]
    pub total_elements: i64,
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
