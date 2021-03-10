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
        let endpoint = format!(
            "https://{}.okta.com",
            self.domain
                .trim_start_matches("https://")
                .trim_start_matches("https://")
                .trim_end_matches('/')
                .trim_end_matches(".okta.com")
                .trim_end_matches('/')
        );

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
        let rb = self.request(Method::POST, "/api/v1/users?activate=true", NewUser { profile });
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

    /// List groups.
    pub async fn list_groups(&self, query: &str) -> Result<Vec<Group>, APIError> {
        // Build the request.
        // TODO: paginate.
        let mut q = "".to_string();
        if !query.is_empty() {
            q = format!("&q={}", query);
        }
        let rb = self.request(Method::GET, &format!("/api/v1/groups?limit=200{}", q), ());
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
        let result: Vec<Group> = resp.json().await.unwrap();

        Ok(result)
    }

    /// Create a group.
    pub async fn create_group(&self, profile: GroupProfile) -> Result<Group, APIError> {
        // Build the request.
        let rb = self.request(Method::POST, "/api/v1/groups", NewGroup { profile });
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
        let result: Group = resp.json().await.unwrap();

        Ok(result)
    }

    /// Get a group by its name.
    pub async fn get_group(&self, name: &str) -> Result<Group, APIError> {
        let groups = self.list_groups(name).await.unwrap();
        for group in groups {
            if group.profile.name == name {
                return Ok(group);
            }
        }

        Err(APIError {
            status_code: StatusCode::NOT_FOUND,
            body: format!("Could not find group with name: {}", name),
        })
    }

    /// Update a group.
    pub async fn update_group(&self, profile: GroupProfile) -> Result<Group, APIError> {
        // First we need to get the group to get its group_id.
        let group = self.get_group(&profile.name).await.unwrap();

        // Build the request.
        let rb = self.request(Method::PUT, format!("/api/v1/groups/{}", group.id), NewGroup { profile });
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
        let result: Group = resp.json().await.unwrap();

        Ok(result)
    }

    /// Add user to a group.
    pub async fn add_user_to_group(&self, group_id: &str, user: &str) -> Result<(), APIError> {
        // First we need to get the user to get their user_id.
        let u = self.get_user(user).await.unwrap();

        // Build the request.
        let rb = self.request(Method::PUT, format!("/api/v1/groups/{}/users/{}", group_id, u.id), ());
        let request = rb.build().unwrap();

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::NO_CONTENT => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                })
            }
        };

        Ok(())
    }

    /// Delete a user from a group.
    pub async fn delete_user_from_group(&self, group_id: &str, user: &str) -> Result<(), APIError> {
        // First we need to get the user to get their user_id.
        let u = self.get_user(user).await.unwrap();

        // Build the request.
        let rb = self.request(Method::DELETE, format!("/api/v1/groups/{}/users/{}", group_id, u.id), ());
        let request = rb.build().unwrap();

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::NO_CONTENT => (),
            StatusCode::FORBIDDEN => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                })
            }
        };

        Ok(())
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
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub status: String,
    pub created: DateTime<Utc>,
    pub activated: Option<DateTime<Utc>>,
    #[serde(rename = "statusChanged")]
    pub status_changed: Option<DateTime<Utc>>,
    #[serde(rename = "lastLogin")]
    pub last_login: Option<DateTime<Utc>>,
    #[serde(rename = "lastUpdated")]
    pub last_updated: DateTime<Utc>,
    #[serde(rename = "passwordChanged")]
    pub password_changed: Option<DateTime<Utc>>,
    pub profile: Profile,
    #[serde(default)]
    pub credentials: Credentials,
    #[serde(default, rename = "_links")]
    pub links: Links,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct NewUser {
    pub profile: Profile,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct Credentials {
    #[serde(default)]
    pub password: Password,
    #[serde(default)]
    pub recovery_question: RecoveryQuestion,
    #[serde(default)]
    pub provider: Provider,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct Password {}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct Provider {
    #[serde(rename = "type", skip_serializing_if = "String::is_empty")]
    pub provider_type: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct RecoveryQuestion {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub question: String,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct Links {
    #[serde(default, rename = "resetPassword")]
    pub reset_password: ChangePassword,
    #[serde(default, rename = "resetFactors")]
    pub reset_factors: ChangePassword,
    #[serde(default, rename = "expirePassword")]
    pub expire_password: ChangePassword,
    #[serde(default, rename = "forgotPassword")]
    pub forgot_password: ChangePassword,
    #[serde(default, rename = "changeRecoveryQuestion")]
    pub change_recovery_question: ChangePassword,
    #[serde(default)]
    pub deactivate: ChangePassword,
    #[serde(default, rename = "changePassword")]
    pub change_password: ChangePassword,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub logo: Vec<Logo>,
    #[serde(default)]
    pub users: ChangePassword,
    #[serde(default)]
    pub apps: ChangePassword,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct ChangePassword {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub href: String,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct Profile {
    #[serde(default, rename = "firstName", skip_serializing_if = "String::is_empty")]
    pub first_name: String,
    #[serde(default, rename = "lastName", skip_serializing_if = "String::is_empty")]
    pub last_name: String,
    #[serde(default, rename = "displayName", deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub display_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub login: String,
    #[serde(default, rename = "primaryPhone", deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub primary_phone: String,
    #[serde(default, rename = "mobilePhone", deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub mobile_phone: String,
    #[serde(default, rename = "streetAddress", deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub street_address: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub city: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub state: String,
    #[serde(default, rename = "zipCode", deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub zip_code: String,
    #[serde(default, rename = "countryCode", deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub country_code: String,
    #[serde(default, rename = "secondEmail", deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub second_email: String,
    #[serde(default, rename = "githubUsername", deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub github_username: String,
    #[serde(default, rename = "matrixUsername", deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub matrix_username: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    pub created: DateTime<Utc>,
    #[serde(rename = "lastUpdated")]
    pub last_updated: DateTime<Utc>,
    #[serde(rename = "lastMembershipUpdated")]
    pub last_membership_updated: Option<DateTime<Utc>>,
    #[serde(rename = "objectClass", default, skip_serializing_if = "Vec::is_empty")]
    pub object_class: Vec<String>,
    #[serde(default, rename = "type", skip_serializing_if = "String::is_empty")]
    pub group_type: String,
    #[serde(default)]
    pub profile: GroupProfile,
    #[serde(default, rename = "_links")]
    pub links: Links,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct NewGroup {
    #[serde(default)]
    pub profile: GroupProfile,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Logo {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub href: String,
    #[serde(rename = "type", default, skip_serializing_if = "String::is_empty")]
    pub logo_type: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct GroupProfile {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
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
