/*!
 * A rust library for interacting with the Ramp v3 API.
 *
 * For more information, the Ramp v1 API is documented at [ramp.stoplight.io](https://ramp.stoplight.io/docs/ramp-developer/docs).
 *
 * Example:
 *
 * ```
 * use ramp_api::Ramp;
 *
 * async fn get_transactions() {
 *     // Initialize the Ramp client.
 *     let ramp = Ramp::new_from_env("", "");
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

use chrono::{DateTime, NaiveDate, Utc};
use reqwest::{header, Client, Method, Request, StatusCode, Url};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Endpoint for the Ramp API.
const ENDPOINT: &str = "https://api.ramp.com/developer/v1/";

const TOKEN_ENDPOINT: &str = "https://api.ramp.com/v1/public/customer/token";

/// Entrypoint for interacting with the Ramp API.
pub struct Ramp {
    token: String,
    // This expires in 101 days. It is hardcoded in the GitHub Actions secrets,
    // We might want something a bit better like storing it in the database.
    refresh_token: String,
    client_id: String,
    client_secret: String,
    redirect_uri: String,

    client: Arc<Client>,
}

impl Ramp {
    /// Create a new Ramp client struct. It takes a type that can convert into
    /// an &str (`String` or `Vec<u8>` for example). As long as the function is
    /// given a valid API client ID and secret your requests will work.
    pub fn new<I, K, R, T, Q>(client_id: I, client_secret: K, redirect_uri: R, token: T, refresh_token: Q) -> Self
    where
        I: ToString,
        K: ToString,
        R: ToString,
        T: ToString,
        Q: ToString,
    {
        let client = Client::builder().build();
        match client {
            Ok(c) => {
                let g = Ramp {
                    client_id: client_id.to_string(),
                    client_secret: client_secret.to_string(),
                    redirect_uri: redirect_uri.to_string(),
                    token: token.to_string(),
                    refresh_token: refresh_token.to_string(),

                    client: Arc::new(c),
                };

                if g.token.is_empty() || g.refresh_token.is_empty() {
                    // This is super hacky and a work around since there is no way to
                    // auth without using the browser.
                    println!("ramp consent URL: {}", g.user_consent_url());
                }
                // We do not refresh the access token since we leave that up to the
                // user to do so they can re-save it to their database.

                g
            }
            Err(e) => panic!("creating client failed: {:?}", e),
        }
    }

    /// Create a new Ramp client struct from environment variables. It
    /// takes a type that can convert into
    /// an &str (`String` or `Vec<u8>` for example). As long as the function is
    /// given a valid API key and your requests will work.
    /// We pass in the token and refresh token to the client so if you are storing
    /// it in a database, you can get it first.
    pub fn new_from_env<T, R>(token: T, refresh_token: R) -> Self
    where
        T: ToString,
        R: ToString,
    {
        let client_id = env::var("RAMP_CLIENT_ID").unwrap();
        let client_secret = env::var("RAMP_CLIENT_SECRET").unwrap();
        let redirect_uri = env::var("RAMP_REDIRECT_URI").unwrap();

        Ramp::new(client_id, client_secret, redirect_uri, token, refresh_token)
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

    pub fn user_consent_url(&self) -> String {
        let state = uuid::Uuid::new_v4();
        format!(
            "https://app.ramp.com/v1/authorize?client_id={}&response_type=code&redirect_uri={}&state={}&scope={}",
            self.client_id, self.redirect_uri, state, "transactions:read users:read users:write receipts:read cards:read departments:read reimbursements:read"
        )
    }

    pub async fn refresh_access_token(&mut self) -> Result<AccessToken, APIError> {
        let mut headers = header::HeaderMap::new();
        headers.append(header::ACCEPT, header::HeaderValue::from_static("application/json"));

        let params = [("grant_type", "refresh_token"), ("refresh_token", &self.refresh_token), ("redirect_uri", &self.redirect_uri)];

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

    pub async fn get_access_token(&mut self, code: &str, state: &str) -> Result<AccessToken, APIError> {
        let mut headers = header::HeaderMap::new();
        headers.append(header::ACCEPT, header::HeaderValue::from_static("application/json"));

        let params = [
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", &self.redirect_uri),
            ("client_id", &self.client_id),
            ("client_secret", &self.client_secret),
            ("state", &state),
        ];
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
        self.refresh_token = t.refresh_token.to_string();

        Ok(t)
    }

    /// List all the reimbursements.
    pub async fn list_reimbursements(&self) -> Result<Vec<Reimbursement>, APIError> {
        // Build the request.
        let mut request = self.request(Method::GET, "reimbursements", (), None);

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
        let mut r: Reimbursements = resp.json().await.unwrap();

        let mut reimbursements = r.data;

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

            request = self.request(Method::GET, "reimbursements", (), Some(new_pairs));

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

            reimbursements.append(&mut r.data);

            if !r.page.next.is_empty() && r.page.next != page {
                page = r.page.next;
            } else {
                page = "".to_string();
            }
        }

        Ok(reimbursements)
    }

    /// List all the departments.
    pub async fn list_departments(&self) -> Result<Vec<Department>, APIError> {
        // Build the request.
        let mut request = self.request(Method::GET, "departments", (), None);

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
        let mut r: Departments = resp.json().await.unwrap();

        let mut departments = r.data;

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

            request = self.request(Method::GET, "departments", (), Some(new_pairs));

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

            departments.append(&mut r.data);

            if !r.page.next.is_empty() && r.page.next != page {
                page = r.page.next;
            } else {
                page = "".to_string();
            }
        }

        Ok(departments)
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
    pub async fn invite_new_user(&self, user: &User) -> Result<User, APIError> {
        // Build the request.
        let request = self.request(Method::POST, "users/deferred", user, None);

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
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

    /// Update a user.
    pub async fn update_user(&self, id: &str, user: &UpdateUser) -> Result<(), APIError> {
        // Build the request.
        let request = self.request(Method::PATCH, &format!("users/{}", id), user, None);

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
        Ok(())
    }

    /// Get the status of a deferred card.
    pub async fn get_deferred_card_status(&self, card_id: &str) -> Result<CardStatus, APIError> {
        // Build the request.
        let request = self.request(Method::GET, &format!("cards/deferred/status/{}", card_id), (), None);

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
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

    /// Create a virtual card.
    pub async fn create_virtual_card(&self, card: &VirtualCard) -> Result<DeferredCard, APIError> {
        // Build the request.
        let request = self.request(Method::POST, "cards/deferred/virtual", card, None);

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
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

    /// Create a physical card.
    pub async fn create_physical_card(&self, card: &PhysicalCard) -> Result<DeferredCard, APIError> {
        // Build the request.
        let request = self.request(Method::POST, "cards/deferred/physical", card, None);

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
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

    /// Get a user.
    pub async fn get_user(&self, id: &str) -> Result<User, APIError> {
        // Build the request.
        let request = self.request(Method::GET, &format!("users/{}", id), (), None);

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
        Ok(resp.json().await.unwrap())
    }

    /// List cards for a user.
    pub async fn list_cards_for_user(&self, user_id: &str) -> Result<Vec<Card>, APIError> {
        // Build the request.
        let request = self.request(Method::GET, "cards", (), Some(vec![("user_id".to_string(), user_id.to_string())]));

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
        let r: Cards = resp.json().await.unwrap();
        Ok(r.data)
    }

    /// Get a receipt.
    pub async fn get_receipt(&self, id: &str) -> Result<Receipt, APIError> {
        // Build the request.
        let request = self.request(Method::GET, &format!("receipts/{}", id), (), None);

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
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub scope: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub refresh_token: String,
    #[serde(default)]
    pub refresh_token_expires_in: i64,
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
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub card_id: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub id: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub merchant_id: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub merchant_name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub receipts: Vec<String>,
    #[serde(default)]
    pub sk_category_id: i64,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub sk_category_name: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub state: String,
    #[serde(deserialize_with = "ramp_date_format::deserialize")]
    pub user_transaction_time: DateTime<Utc>,
}

#[derive(Debug, JsonSchema, Clone, Serialize, Deserialize)]
pub struct Reimbursement {
    #[serde(default)]
    pub amount: f64,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub user_id: String,
    #[serde(deserialize_with = "ramp_date_format::deserialize")]
    pub created_at: DateTime<Utc>,
    pub transaction_date: NaiveDate,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub id: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub currency: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub merchant: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub receipts: Vec<String>,
}

#[derive(Debug, Default, JsonSchema, Clone, Serialize, Deserialize)]
pub struct CardHolder {
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub department_id: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub department_name: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub first_name: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub last_name: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub location_id: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub location_name: String,
}

#[derive(Debug, Default, JsonSchema, Clone, Serialize, Deserialize)]
pub struct Page {
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
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
pub struct Departments {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub data: Vec<Department>,
    #[serde(default)]
    pub page: Page,
}

#[derive(Debug, Default, JsonSchema, Clone, Serialize, Deserialize)]
pub struct Reimbursements {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub data: Vec<Reimbursement>,
    #[serde(default)]
    pub page: Page,
}

#[derive(Debug, Default, JsonSchema, Clone, Serialize, Deserialize)]
pub struct Cards {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub data: Vec<Card>,
    #[serde(default)]
    pub page: Page,
}

#[derive(Debug, Default, JsonSchema, Clone, Serialize, Deserialize)]
pub struct Department {
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub id: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub name: String,
}

#[derive(Debug, Default, JsonSchema, Clone, Serialize, Deserialize)]
pub struct CardStatus {
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub id: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub status: String,
    #[serde(default)]
    pub data: CardStatusData,
}

#[derive(Debug, Default, JsonSchema, Clone, Serialize, Deserialize)]
pub struct CardStatusData {
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub id: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub misc: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub card_id: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub error: String,
}

#[derive(Debug, Default, JsonSchema, Clone, Serialize, Deserialize)]
pub struct DeferredCard {
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub id: String,
}

#[derive(Debug, Default, JsonSchema, Clone, Serialize, Deserialize)]
pub struct VirtualCard {
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub display_name: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub user_id: String,
    #[serde(default)]
    pub spending_restrictions: SpendingRestrictions,
}

#[derive(Debug, Default, JsonSchema, Clone, Serialize, Deserialize)]
pub struct PhysicalCard {
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub display_name: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub user_id: String,
    #[serde(default)]
    pub spending_restrictions: SpendingRestrictions,
    #[serde(default)]
    pub fulfillment: Fulfillment,
}

#[derive(Debug, Default, JsonSchema, Clone, Serialize, Deserialize)]
pub struct Card {
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub id: String,
    #[serde(default)]
    pub is_physical: bool,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub display_name: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub last_four: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub cardholder_id: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub cardholder_name: String,
    #[serde(default)]
    pub fulfillment: Fulfillment,
    #[serde(default)]
    pub spending_restrictions: SpendingRestrictions,
}

#[derive(Debug, Default, JsonSchema, Clone, Serialize, Deserialize)]
pub struct Fulfillment {}

#[derive(Debug, Default, JsonSchema, Clone, Serialize, Deserialize)]
pub struct SpendingRestrictions {
    #[serde(default)]
    pub amount: i64,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub interval: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock_date: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<i64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocked_categories: Vec<i64>,
    #[serde(default)]
    pub transaction_amount_limit: i64,
}

#[derive(Debug, Default, JsonSchema, Clone, Serialize, Deserialize)]
pub struct UpdateUser {
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub department_id: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub location_id: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub direct_manager_id: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub role: String,
}

#[derive(Debug, Default, JsonSchema, Clone, Serialize, Deserialize)]
pub struct User {
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub business_id: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub department_id: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub email: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub first_name: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub id: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub last_name: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub location_id: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub manager_id: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub direct_manager_id: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub phone: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub role: String,
}

#[derive(Debug, JsonSchema, Clone, Serialize, Deserialize)]
pub struct Receipt {
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub id: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub transaction_id: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub user_id: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub receipt_url: String,
    #[serde(deserialize_with = "ramp_date_format::deserialize")]
    pub created_at: DateTime<Utc>,
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

mod ramp_date_format {
    use chrono::{DateTime, TimeZone, Utc};
    use serde::{self, Deserialize, Deserializer};

    // The date format Ramp returns looks like this: "2021-04-24T01:03:21"
    const FORMAT: &str = "%Y-%m-%dT%H:%M:%S%:z";

    // The signature of a deserialize_with function must follow the pattern:
    //
    //    fn deserialize<'de, D>(D) -> Result<T, D::Error>
    //    where
    //        D: Deserializer<'de>
    //
    // although it may also be generic over the output types T.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut s = String::deserialize(deserializer).unwrap();
        match Utc.datetime_from_str(&s, "%+") {
            Ok(t) => Ok(t),
            Err(_) => {
                s = format!("{}+00:00", s);
                // Try both ways to parse the date.
                match Utc.datetime_from_str(&s, FORMAT) {
                    Ok(r) => Ok(r),
                    Err(_) => Utc.datetime_from_str(&s, "%+").map_err(serde::de::Error::custom),
                }
            }
        }
    }
}
