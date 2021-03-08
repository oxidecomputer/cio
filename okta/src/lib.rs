/*!
 * A rust library for interacting with the Okta API.
 *
 * For more information, the Okta API is documented at
 * [developer.okta.com](https://developer.okta.com/docs/reference/).
 *
 * Example:
 *
 * ```
 * use okta::Okta;
 * use serde::{Deserialize, Serialize};
 *
 * async fn get_current_user() {
 *     // Initialize the Okta client.
 *     let okta = Okta::new_from_env();
 *
 *     // List users.
 *     let users = okta.list_users().await.unwrap();
 *
 *     println!("{:?}", users);
 * }
 * ```
 */
use std::env;
use std::error;
use std::fmt;
use std::fmt::Debug;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use reqwest::{header, Client, Method, RequestBuilder, StatusCode, Url};
use serde::{Deserialize, Serialize};

/// Entrypoint for interacting with the Okta API.
pub struct Okta {
    key: String,
    domain: String,

    client: Arc<Client>,
}

impl Okta {
    /// Create a new Okta client struct. It takes a type that can convert into
    /// an &str (`String` or `Vec<u8>` for example). As long as the function is
    /// given a valid API Key your requests will work.
    pub fn new<K, D>(key: K, domain: D) -> Self
    where
        K: ToString,
        D: ToString,
    {
        let client = Client::builder().build();
        match client {
            Ok(c) => Self {
                key: key.to_string(),
                domain: domain.to_string(),

                client: Arc::new(c),
            },
            Err(e) => panic!("creating client failed: {:?}", e),
        }
    }

    /// Create a new Okta client struct from environment variables. It
    /// takes a type that can convert into
    /// an &str (`String` or `Vec<u8>` for example). As long as the function is
    /// given a valid API your requests will work.
    pub fn new_from_env() -> Self {
        let key = env::var("OKTA_API_TOKEN").unwrap();
        let domain = env::var("OKTA_DOMAIN").unwrap();

        Okta::new(key, domain)
    }

    /// Get the currently set API key.
    pub fn get_key(&self) -> &str {
        &self.key
    }

    fn request<P, B>(&self, method: Method, path: P, body: B) -> RequestBuilder
    where
        P: ToString,
        B: Serialize,
    {
        let endpoint = format!("https://{}", self.domain.trim_start_matches("https://").trim_start_matches("https://").trim_end_matches('/'));

        // Build the url.
        let base = Url::parse(&endpoint).unwrap();
        let mut p = path.to_string();
        // Make sure we have the leading "/".
        if !p.starts_with('/') {
            p = format!("/{}", p);
        }
        let url = base.join(&p).unwrap();

        let bt = format!("SSWS {}", self.key);
        let bearer = header::HeaderValue::from_str(&bt).unwrap();

        // Set the default headers.
        let mut headers = header::HeaderMap::new();
        headers.append(header::AUTHORIZATION, bearer);
        headers.append(header::CONTENT_TYPE, header::HeaderValue::from_static("application/json"));

        let mut rb = self.client.request(method.clone(), url).headers(headers);

        if method != Method::GET && method != Method::DELETE {
            rb = rb.json(&body);
        }

        rb
    }

    /// List users.
    pub async fn list_users(&self) -> Result<Vec<User>, APIError> {
        // Build the request.
        // TODO: paginate.
        let rb = self.request(Method::GET, "/api/v1/users?limit=200", ());
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
        let result: Vec<User> = resp.json().await.unwrap();

        Ok(result)
    }

    /// Create a user.
    pub async fn create_user(&self, profile: Profile) -> Result<User, APIError> {
        // Build the request.
        let rb = self.request(Method::POST, "/api/v1/users?activate=false", NewUser { profile });
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
        let result: User = resp.json().await.unwrap();

        Ok(result)
    }

    /// Get a user by their email.
    pub async fn get_user(&self, email: &str) -> Result<User, APIError> {
        // Build the request.
        let rb = self.request(Method::GET, format!("/api/v1/users/{}", email), ());
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
        let result: User = resp.json().await.unwrap();

        Ok(result)
    }

    /// Update a user.
    pub async fn update_user(&self, profile: Profile) -> Result<User, APIError> {
        // First we need to get the user to get their user_id.
        let user = self.get_user(&profile.login).await.unwrap();

        // Build the request.
        let rb = self.request(Method::PUT, format!("/api/v1/users/{}", user.id), NewUser { profile });
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
        let result: User = resp.json().await.unwrap();

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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct User {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    status: String,
    created: DateTime<Utc>,
    activated: Option<DateTime<Utc>>,
    #[serde(rename = "statusChanged")]
    status_changed: Option<DateTime<Utc>>,
    #[serde(rename = "lastLogin")]
    last_login: Option<DateTime<Utc>>,
    #[serde(rename = "lastUpdated")]
    last_updated: DateTime<Utc>,
    #[serde(rename = "passwordChanged")]
    password_changed: Option<DateTime<Utc>>,
    profile: Profile,
    credentials: Credentials,
    #[serde(default, rename = "_links")]
    links: Links,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct NewUser {
    profile: Profile,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct Credentials {
    password: Password,
    recovery_question: RecoveryQuestion,
    provider: Provider,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct Password {}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct Provider {
    #[serde(rename = "type", skip_serializing_if = "String::is_empty")]
    provider_type: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    name: String,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct RecoveryQuestion {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    question: String,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct Links {
    #[serde(rename = "resetPassword")]
    reset_password: ChangePassword,
    #[serde(rename = "resetFactors")]
    reset_factors: ChangePassword,
    #[serde(rename = "expirePassword")]
    expire_password: ChangePassword,
    #[serde(rename = "forgotPassword")]
    forgot_password: ChangePassword,
    #[serde(rename = "changeRecoveryQuestion")]
    change_recovery_question: ChangePassword,
    deactivate: ChangePassword,
    #[serde(rename = "changePassword")]
    change_password: ChangePassword,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct ChangePassword {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    href: String,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct Profile {
    #[serde(rename = "firstName", skip_serializing_if = "String::is_empty")]
    first_name: String,
    #[serde(rename = "lastName", skip_serializing_if = "String::is_empty")]
    last_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    email: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    login: String,
    #[serde(rename = "mobilePhone", skip_serializing_if = "String::is_empty")]
    mobile_phone: String,
}
