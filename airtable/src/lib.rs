/*!
 * A rust library for interacting with the Airtable API.
 *
 * For more information, the Airtable API is documented at [airtable.com/api](https://airtable.com/api).
 *
 * Example:
 *
 * ```
 * use airtable_api::{Airtable, Record};
 * use serde::{Deserialize, Serialize};
 *
 * async fn get_records() {
 *     // Initialize the Airtable client.
 *     let airtable = Airtable::new_from_env();
 *
 *     // Get the current records from a table.
 *     let mut records: Vec<Record<SomeFormat>> = airtable.list_records("Table Name", "Grid view", vec!["the", "fields", "you", "want", "to", "return"]).await.unwrap();
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
#![allow(clippy::field_reassign_with_default)]
use std::env;
use std::error;
use std::fmt;
use std::fmt::Debug;
use std::sync::Arc;

use chrono::offset::Utc;
use chrono::DateTime;
use reqwest::{header, Client, Method, Request, StatusCode, Url};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// Endpoint for the Airtable API.
const ENDPOINT: &str = "https://api.airtable.com/v0/";

/// Entrypoint for interacting with the Airtable API.
pub struct Airtable {
    key: String,
    base_id: String,
    enterprise_account_id: String,

    client: Arc<Client>,
}

/// Get the API key from the AIRTABLE_API_KEY env variable.
pub fn api_key_from_env() -> String {
    env::var("AIRTABLE_API_KEY").unwrap()
}

impl Airtable {
    /// Create a new Airtable client struct. It takes a type that can convert into
    /// an &str (`String` or `Vec<u8>` for example). As long as the function is
    /// given a valid API Key and Base ID your requests will work.
    /// You can leave the Enterprise Account ID empty if you are not using the
    /// Enterprise API features.
    pub fn new<K, B, E>(key: K, base_id: B, enterprise_account_id: E) -> Self
    where
        K: ToString,
        B: ToString,
        E: ToString,
    {
        let client = Client::builder().build();
        match client {
            Ok(c) => Self {
                key: key.to_string(),
                base_id: base_id.to_string(),
                enterprise_account_id: enterprise_account_id.to_string(),

                client: Arc::new(c),
            },
            Err(e) => panic!("creating client failed: {:?}", e),
        }
    }

    /// Create a new Airtable client struct from environment variables. It
    /// takes a type that can convert into
    /// an &str (`String` or `Vec<u8>` for example). As long as the function is
    /// given a valid API Key and Base ID your requests will work.
    pub fn new_from_env() -> Self {
        let base_id = env::var("AIRTABLE_BASE_ID").unwrap();
        let enterprise_account_id = env::var("AIRTABLE_ENTERPRISE_ACCOUNT_ID").unwrap_or_default();

        Airtable::new(api_key_from_env(), base_id, enterprise_account_id)
    }

    /// Get the currently set API key.
    pub fn get_key(&self) -> &str {
        &self.key
    }

    fn request<B>(&self, method: Method, path: String, body: B, query: Option<Vec<(&str, String)>>) -> Request
    where
        B: Serialize,
    {
        let base = Url::parse(ENDPOINT).unwrap();
        let url = base.join(&(self.base_id.to_string() + "/" + &path)).unwrap();

        let bt = format!("Bearer {}", self.key);
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

    /// List records in a table for a particular view.
    pub async fn list_records<T: DeserializeOwned>(&self, table: &str, view: &str, fields: Vec<&str>) -> Result<Vec<Record<T>>, APIError> {
        let mut params = vec![("pageSize", "100".to_string()), ("view", view.to_string())];
        for field in fields {
            params.push(("fields", field.to_string()));
        }

        // Build the request.
        let mut request = self.request(Method::GET, table.to_string(), (), Some(params));

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
        let mut r: APICall<T> = resp.json().await.unwrap();

        let mut records = r.records;

        let mut offset = r.offset;

        // Paginate if we should.
        // TODO: make this more DRY
        while !offset.is_empty() {
            request = self.request(
                Method::GET,
                table.to_string(),
                (),
                Some(vec![("pageSize", "100".to_string()), ("view", view.to_string()), ("offset", offset)]),
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

            records.append(&mut r.records);

            offset = r.offset;
        }

        Ok(records)
    }

    /// Get record from a table.
    pub async fn get_record<T: DeserializeOwned>(&self, table: &str, record_id: &str) -> Result<Record<T>, APIError> {
        // Build the request.
        let request = self.request(Method::GET, format!("{}/{}", table, record_id), (), None);

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
        let record: Record<T> = resp.json().await.unwrap();

        Ok(record)
    }

    /// Delete record from a table.
    pub async fn delete_record(&self, table: &str, record_id: &str) -> Result<(), APIError> {
        // Build the request.
        let request = self.request(Method::DELETE, table.to_string(), (), Some(vec![("records[]", record_id.to_string())]));

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

        Ok(())
    }

    /// Bulk create records in a table.
    ///
    /// Due to limitations on the Airtable API, you can only bulk create 10
    /// records at a time.
    pub async fn create_records<T: Serialize + DeserializeOwned>(&self, table: &str, records: Vec<Record<T>>) -> Result<Vec<Record<T>>, APIError> {
        // Build the request.
        let request = self.request(
            Method::POST,
            table.to_string(),
            APICall {
                records,
                offset: "".to_string(),
                typecast: Some(true),
            },
            None,
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
        let r: APICall<T> = resp.json().await.unwrap();

        Ok(r.records)
    }

    /// Bulk update records in a table.
    ///
    /// Due to limitations on the Airtable API, you can only bulk update 10
    /// records at a time.
    pub async fn update_records<T: Serialize + DeserializeOwned>(&self, table: &str, records: Vec<Record<T>>) -> Result<Vec<Record<T>>, APIError> {
        // Build the request.
        let request = self.request(
            Method::PATCH,
            table.to_string(),
            APICall {
                records,
                offset: "".to_string(),
                typecast: Some(true),
            },
            None,
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
        match resp.json::<APICall<T>>().await {
            Ok(v) => Ok(v.records),
            Err(_) => {
                // This might fail. On a faiture just return an empty vector.
                Ok(vec![])
            }
        }
    }

    /// Delete internal user by email.
    /// This is for an enterprise admin to do only.
    /// The user must be an internal user, meaning they have an email with the company domain.
    /// FROM: https://airtable.com/api/enterprise#enterpriseAccountUserDeleteUserByEmail
    pub async fn delete_internal_user_by_email(&self, email: &str) -> Result<(), APIError> {
        if self.enterprise_account_id.is_empty() {
            // Return an error early.
            return Err(APIError {
                status_code: StatusCode::OK,
                body: "An enterprise account id is required.".to_string(),
            });
        }

        // Build the request.
        let request = self.request(
            Method::DELETE,
            format!("meta/enterpriseAccounts/{}/users", self.enterprise_account_id),
            (),
            Some(vec![("email", email.to_string())]),
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
        let result: DeleteUserResponse = resp.json().await.unwrap();
        if !result.errors.is_empty() {
            return Err(APIError {
                status_code: StatusCode::OK,
                body: format!("{:?}", result),
            });
        }

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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct APICall<T> {
    /// If there are more records, the response will contain an
    /// offset. To fetch the next page of records, include offset
    /// in the next request's parameters.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub offset: String,
    /// The current page number of returned records.
    pub records: Vec<Record<T>>,
    /// The Airtable API will perform best-effort automatic data conversion
    /// from string values if the typecast parameter is passed in. Automatic
    /// conversion is disabled by default to ensure data integrity, but it may
    /// be helpful for integrating with 3rd party data sources.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub typecast: Option<bool>,
}

/// An Airtable record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record<T> {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    pub fields: T,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_time: Option<DateTime<Utc>>,
}

/// An airtable user.
#[derive(Debug, Default, Clone, Serialize, JsonSchema, Deserialize)]
pub struct User {
    pub id: String,
    pub email: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
}

/// The response returned from deleting a user.
/// FROM: https://airtable.com/api/enterprise#enterpriseAccountUserDeleteUserByEmail
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct DeleteUserResponse {
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "deletedUsers")]
    pub deleted_users: Vec<User>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<ErrorResponse>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "type")]
    pub type_: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub message: String,
}
