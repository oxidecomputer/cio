/*!
 * A rust library for interacting with the Ramp v3 API.
 *
 * For more information, the Ramp v1 API is documented at [ramp.stoplight.io](https://ramp.stoplight.io/docs/ramp-developer/docs).
 *
 * Example:
 *
 * ```
 * use ramp::Ramp;
 *
 * async fn get_transactions() {
 *     // Initialize the Ramp client.
 *     let ramp = Ramp::new_from_env().await;
 *
 *     let transactions = ramp.get_transactions().await.unwrap();
 *
 *     println!("transactions: {:?}", transactions);
 * }
 * ```
 */
use std::borrow::Cow;
use std::env;
use std::error;
use std::fmt;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use reqwest::{header, Client, Method, Request, StatusCode, Url};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Endpoint for the Ramp API.
const ENDPOINT: &str = "https://api.ramp.com/developer/v1/";

const TOKEN_ENDPOINT: &str = "https://api.ramp.com/v1/public/customer/token";

/// Entrypoint for interacting with the Ramp API.
pub struct Ramp {
    client_id: String,
    client_secret: String,
    token: String,

    client: Arc<Client>,
}

impl Ramp {
    /// Create a new Ramp client struct. It takes a type that can convert into
    /// an &str (`String` or `Vec<u8>` for example). As long as the function is
    /// given a valid API client ID and secret your requests will work.
    pub async fn new<K, S>(client_id: K, client_secret: S) -> Self
    where
        K: ToString,
        S: ToString,
    {
        let client = Client::builder().build();
        match client {
            Ok(c) => {
                let mut ramp = Self {
                    client_id: client_id.to_string(),
                    client_secret: client_secret.to_string(),
                    token: "".to_string(),

                    client: Arc::new(c),
                };

                // Let's get the token.
                ramp.get_token().await.unwrap();

                ramp
            }
            Err(e) => panic!("creating client failed: {:?}", e),
        }
    }

    /// Create a new Ramp client struct from environment variables. It
    /// takes a type that can convert into
    /// an &str (`String` or `Vec<u8>` for example). As long as the function is
    /// given a valid API Key your requests will work.
    pub async fn new_from_env() -> Self {
        let client_id = env::var("RAMP_CLIENT_ID").unwrap();
        let client_secret = env::var("RAMP_CLIENT_SECRET").unwrap();

        Ramp::new(client_id, client_secret).await
    }

    // Sets the token for requests.
    async fn get_token(&mut self) -> Result<(), APIError> {
        let client = reqwest::Client::new();

        let params = [("grant_type", "client_credentials"), ("scope", "transactions:read users:read users:write receipts:read cards:read")];
        let resp = client.post(TOKEN_ENDPOINT).form(&params).basic_auth(&self.client_id, Some(&self.client_secret)).send().await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                })
            }
        };

        let at: AccessToken = resp.json().await.unwrap();
        self.token = at.access_token;

        Ok(())
    }

    fn request<B>(&self, method: Method, path: &str, body: B, query: Option<Vec<(String, String)>>) -> Request
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

    /// Get all the transactions in the account.
    pub async fn get_transactions(&self) -> Result<Vec<Transaction>, APIError> {
        // Build the request.
        let mut request = self.request(Method::GET, "transactions", (), None);

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
        let mut r: Transactions = resp.json().await.unwrap();

        let mut transactions = r.data;

        let mut page = r.page.next;

        // Paginate if we should.
        // TODO: make this more DRY
        while !page.is_empty() {
            let url = Url::parse(&page).unwrap();
            let pairs: Vec<(Cow<'_, str>, Cow<'_, str>)> = url.query_pairs().collect();
            let mut new_pairs: Vec<(String, String)> = Vec::new();
            for (a, b) in pairs {
                let sa = a.into_owned();
                let sb = b.into_owned();
                new_pairs.push((sa, sb));
            }
            println!("pairs: {:?}", new_pairs);

            request = self.request(Method::GET, "transactions", (), Some(new_pairs));

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

            transactions.append(&mut r.data);

            if !r.page.next.is_empty() && r.page.next != page {
                page = r.page.next;
            } else {
                page = "".to_string();
            }
        }

        Ok(transactions)
    }

    /// List all the users.
    pub async fn list_users(&self) -> Result<Vec<User>, APIError> {
        // Build the request.
        let mut request = self.request(Method::GET, "users", (), None);

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
        let mut r: Users = resp.json().await.unwrap();

        let mut users = r.data;

        let mut page = r.page.next;

        // Paginate if we should.
        // TODO: make this more DRY
        while !page.is_empty() {
            let url = Url::parse(&page).unwrap();
            let pairs: Vec<(Cow<'_, str>, Cow<'_, str>)> = url.query_pairs().collect();
            let mut new_pairs: Vec<(String, String)> = Vec::new();
            for (a, b) in pairs {
                let sa = a.into_owned();
                let sb = b.into_owned();
                new_pairs.push((sa, sb));
            }
            println!("pairs: {:?}", new_pairs);

            request = self.request(Method::GET, "users", (), Some(new_pairs));

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

            users.append(&mut r.data);

            if !r.page.next.is_empty() && r.page.next != page {
                page = r.page.next;
            } else {
                page = "".to_string();
            }
        }

        Ok(users)
    }

    /// Invite a new user.
    pub async fn invite_new_user(&self) -> Result<User, APIError> {
        // Build the request.
        let request = self.request(Method::POST, "users/deferred", (), None);

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::CREATED => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                })
            }
        };

        // Try to deserialize the response.
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

#[derive(Debug, JsonSchema, Clone, Default, Serialize, Deserialize)]
pub struct AccessToken {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub access_token: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub token_type: String,
    #[serde(default)]
    pub expires_in: i64,
}

#[derive(Debug, JsonSchema, Clone, Default, Serialize, Deserialize)]
pub struct Transactions {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub data: Vec<Transaction>,
    #[serde(default)]
    pub page: Page,
}

#[derive(Debug, JsonSchema, Clone, Serialize, Deserialize)]
pub struct Transaction {
    #[serde(default)]
    pub amount: f64,
    #[serde(default)]
    pub card_holder: CardHolder,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub card_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub merchant_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub merchant_name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub receipts: Vec<String>,
    #[serde(default)]
    pub sk_category_id: i64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sk_category_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub state: String,
    // TODO: Parse this as a DateTime<Utc>
    pub user_transaction_time: String,
}

#[derive(Debug, Default, JsonSchema, Clone, Serialize, Deserialize)]
pub struct CardHolder {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub department_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub department_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub first_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub last_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub location_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub location_name: String,
}

#[derive(Debug, Default, JsonSchema, Clone, Serialize, Deserialize)]
pub struct Page {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub next: String,
}

#[derive(Debug, Default, JsonSchema, Clone, Serialize, Deserialize)]
pub struct Users {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub data: Vec<User>,
    #[serde(default)]
    pub page: Page,
}

#[derive(Debug, Default, JsonSchema, Clone, Serialize, Deserialize)]
pub struct User {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub business_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub department_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub first_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub last_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub location_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub manager_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub phone: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub role: String,
}
