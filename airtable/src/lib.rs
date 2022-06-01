/*!
 * A rust library for interacting with the Airtable API.
 *
 * For more information, the Airtable API is documented at [airtable.com/api](https://airtable.com/api).
 *
 * Example:
 *
 * ```ignore
 * use airtable_api::{Airtable, Record};
 * use serde::{Deserialize, Serialize};
 *
 * async fn get_records() {
 *     // Initialize the Airtable client.
 *     let airtable = Airtable::new_from_env();
 *
 *     // Get the current records from a table.
 *     let mut records: Vec<Record<SomeFormat>> = airtable
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
#![allow(clippy::field_reassign_with_default)]
use std::{env, fmt, fmt::Debug};

use anyhow::{bail, Result};
use chrono::{offset::Utc, DateTime};
use reqwest::{header, Method, Request, StatusCode, Url};
use schemars::JsonSchema;
use serde::{
    de::{DeserializeOwned, MapAccess, SeqAccess, Visitor},
    Deserialize, Deserializer, Serialize,
};

/// Endpoint for the Airtable API.
const ENDPOINT: &str = "https://api.airtable.com/v0/";

/// Entrypoint for interacting with the Airtable API.
pub struct Airtable {
    key: String,
    base_id: String,
    enterprise_account_id: String,

    client: reqwest_middleware::ClientWithMiddleware,
}

/// Get the API key from the AIRTABLE_API_KEY env variable.
pub fn api_key_from_env() -> String {
    env::var("AIRTABLE_API_KEY").unwrap_or_default()
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
        let http = reqwest::Client::builder().build();
        match http {
            Ok(c) => {
                let retry_policy = reqwest_retry::policies::ExponentialBackoff::builder().build_with_max_retries(3);
                let client = reqwest_middleware::ClientBuilder::new(c)
                    // Trace HTTP requests. See the tracing crate to make use of these traces.
                    .with(reqwest_tracing::TracingMiddleware)
                    // Retry failed requests.
                    .with(reqwest_retry::RetryTransientMiddleware::new_with_policy(retry_policy))
                    .build();

                Self {
                    key: key.to_string(),
                    base_id: base_id.to_string(),
                    enterprise_account_id: enterprise_account_id.to_string(),

                    client,
                }
            }
            Err(e) => panic!("creating client failed: {:?}", e),
        }
    }

    /// Create a new Airtable client struct from environment variables. It
    /// takes a type that can convert into
    /// an &str (`String` or `Vec<u8>` for example). As long as the function is
    /// given a valid API Key and Base ID your requests will work.
    pub fn new_from_env() -> Self {
        let base_id = env::var("AIRTABLE_BASE_ID").unwrap_or_default();
        let enterprise_account_id = env::var("AIRTABLE_ENTERPRISE_ACCOUNT_ID").unwrap_or_default();

        Airtable::new(api_key_from_env(), base_id, enterprise_account_id)
    }

    /// Get the currently set API key.
    pub fn get_key(&self) -> &str {
        &self.key
    }

    fn request<B>(&self, method: Method, path: String, body: B, query: Option<Vec<(&str, String)>>) -> Result<Request>
    where
        B: Serialize,
    {
        let base = Url::parse(ENDPOINT)?;
        let url = base.join(&(self.base_id.to_string() + "/" + &path))?;

        let bt = format!("Bearer {}", self.key);
        let bearer = header::HeaderValue::from_str(&bt)?;

        // Set the default headers.
        let mut headers = header::HeaderMap::new();
        headers.append(header::AUTHORIZATION, bearer);
        headers.append(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/json"),
        );

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
        Ok(rb.build()?)
    }

    /// List records in a table for a particular view.
    pub async fn list_records<T: DeserializeOwned>(
        &self,
        table: &str,
        view: &str,
        fields: Vec<&str>,
    ) -> Result<Vec<Record<T>>> {
        let mut params = vec![("pageSize", "100".to_string()), ("view", view.to_string())];
        for field in fields {
            params.push(("fields[]", field.to_string()));
        }

        // Build the request.
        let mut request = self.request(Method::GET, table.to_string(), (), Some(params))?;

        let mut resp = self.client.execute(request).await?;
        match resp.status() {
            StatusCode::OK => (),
            s => {
                bail!("status code: {}, body: {}", s, resp.text().await?);
            }
        };

        // Try to deserialize the response.
        let mut r: APICall<T> = resp.json().await?;

        let mut records = r.records;

        let mut offset = r.offset;

        // Paginate if we should.
        // TODO: make this more DRY
        while !offset.is_empty() {
            request = self.request(
                Method::GET,
                table.to_string(),
                (),
                Some(vec![
                    ("pageSize", "100".to_string()),
                    ("view", view.to_string()),
                    ("offset", offset),
                ]),
            )?;

            resp = self.client.execute(request).await?;
            match resp.status() {
                StatusCode::OK => (),
                s => {
                    bail!("status code: {}, body: {}", s, resp.text().await?);
                }
            };

            // Try to deserialize the response.
            r = resp.json().await?;

            records.append(&mut r.records);

            offset = r.offset;
        }

        Ok(records)
    }

    /// Get record from a table.
    pub async fn get_record<T: DeserializeOwned>(&self, table: &str, record_id: &str) -> Result<Record<T>> {
        // Build the request.
        let request = self.request(Method::GET, format!("{}/{}", table, record_id), (), None)?;

        let resp = self.client.execute(request).await?;
        match resp.status() {
            StatusCode::OK => (),
            s => {
                bail!("status code: {}, body: {}", s, resp.text().await?);
            }
        };

        // Try to deserialize the response.
        let record: Record<T> = resp.json().await?;

        Ok(record)
    }

    /// Delete record from a table.
    pub async fn delete_record(&self, table: &str, record_id: &str) -> Result<()> {
        // Build the request.
        let request = self.request(
            Method::DELETE,
            table.to_string(),
            (),
            Some(vec![("records[]", record_id.to_string())]),
        )?;

        let resp = self.client.execute(request).await?;
        match resp.status() {
            StatusCode::OK => (),
            s => {
                bail!("status code: {}, body: {}", s, resp.text().await?);
            }
        };

        Ok(())
    }

    /// Bulk create records in a table.
    ///
    /// Due to limitations on the Airtable API, you can only bulk create 10
    /// records at a time.
    pub async fn create_records<T: Serialize + DeserializeOwned>(
        &self,
        table: &str,
        records: Vec<Record<T>>,
    ) -> Result<Vec<Record<T>>> {
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
        )?;

        let resp = self.client.execute(request).await?;
        match resp.status() {
            StatusCode::OK => (),
            s => {
                bail!("status code: {}, body: {}", s, resp.text().await?);
            }
        };

        // Try to deserialize the response.
        let r: APICall<T> = resp.json().await?;

        Ok(r.records)
    }

    /// Bulk update records in a table.
    ///
    /// Due to limitations on the Airtable API, you can only bulk update 10
    /// records at a time.
    pub async fn update_records<T: Serialize + DeserializeOwned>(
        &self,
        table: &str,
        records: Vec<Record<T>>,
    ) -> Result<Vec<Record<T>>> {
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
        )?;

        let resp = self.client.execute(request).await?;
        match resp.status() {
            StatusCode::OK => (),
            s => {
                bail!("status code: {}, body: {}", s, resp.text().await?);
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

    /// List users.
    /// This is for an enterprise admin to do only.
    /// FROM: https://airtable.com/api/enterprise
    pub async fn list_users(&self) -> Result<Vec<User>> {
        if self.enterprise_account_id.is_empty() {
            // Return an error early.
            bail!("An enterprise account id is required.");
        }

        // Build the request.
        let request = self.request(
            Method::GET,
            format!("v0/meta/enterpriseAccounts/{}/users", self.enterprise_account_id),
            (),
            Some(vec![("state", "provisioned".to_string())]),
        )?;

        let resp = self.client.execute(request).await?;
        match resp.status() {
            StatusCode::OK => (),
            s => {
                bail!("status code: {}, body: {}", s, resp.text().await?);
            }
        };

        // Try to deserialize the response.
        let result: UsersResponse = resp.json().await?;

        Ok(result.users)
    }

    /// Get an enterprise user.
    /// This is for an enterprise admin to do only.
    /// FROM: https://airtable.com/api/enterprise#enterpriseAccountUserGetInformationByEmail
    /// Permission level can be: owner | create | edit | comment | read
    pub async fn get_enterprise_user(&self, email: &str) -> Result<EnterpriseUser> {
        if self.enterprise_account_id.is_empty() {
            // Return an error early.
            bail!("An enterprise account id is required.");
        }

        // Build the request.
        let request = self.request(
            Method::GET,
            format!("v0/meta/enterpriseAccounts/{}/users", self.enterprise_account_id),
            (),
            Some(vec![
                ("email", email.to_string()),
                ("include", "collaborations".to_string()),
            ]),
        )?;

        let resp = self.client.execute(request).await?;
        match resp.status() {
            StatusCode::OK => (),
            s => {
                bail!("status code: {}, body: {}", s, resp.text().await?);
            }
        };

        let r: EnterpriseUsersResponse = resp.json().await?;

        if r.users.is_empty() {
            bail!("no user was returned");
        }

        Ok(r.users.get(0).unwrap().clone())
    }

    /// Add a collaborator to a workspace.
    /// This is for an enterprise admin to do only.
    /// FROM: https://airtable.com/api/enterprise#enterpriseWorkspaceAddCollaborator
    /// Permission level can be: owner | create | edit | comment | read
    pub async fn add_collaborator_to_workspace(
        &self,
        workspace_id: &str,
        user_id: &str,
        permission_level: &str,
    ) -> Result<()> {
        if self.enterprise_account_id.is_empty() {
            // Return an error early.
            bail!("An enterprise account id is required.");
        }

        // Build the request.
        let request = self.request(
            Method::POST,
            format!("v0/meta/workspaces/{}/collaborators", workspace_id),
            NewCollaborator {
                collaborators: vec![Collaborator {
                    user: User {
                        id: user_id.to_string(),
                        email: Default::default(),
                        name: Default::default(),
                    },
                    permission_level: permission_level.to_string(),
                }],
            },
            None,
        )?;

        let resp = self.client.execute(request).await?;
        match resp.status() {
            StatusCode::OK => (),
            s => {
                bail!("status code: {}, body: {}", s, resp.text().await?);
            }
        };

        Ok(())
    }

    /// Returns basic information on the workspace. Does not include deleted collaborators
    /// and only include outstanding invites.
    /// FROM: https://airtable.com/api/enterprise#enterpriseWorkspaceGetInformation
    pub async fn get_enterprise_workspace<const N: usize>(
        &self,
        workspace_id: &str,
        includes: Option<[WorkspaceIncludes; N]>,
    ) -> Result<Workspace> {
        if self.enterprise_account_id.is_empty() {
            // Return an error early.
            bail!("An enterprise account id is required.");
        }

        // Build the request.
        let request = self.request(
            Method::GET,
            format!("v0/meta/workspaces/{}?", workspace_id),
            (),
            includes.map(|includes| {
                includes
                    .map(|include| {
                        (
                            "include",
                            match include {
                                WorkspaceIncludes::Collaborators => "collaborators".to_string(),
                                WorkspaceIncludes::InviteLinks => "inviteLinks".to_string(),
                            },
                        )
                    })
                    .to_vec()
            }),
        )?;

        let resp = self.client.execute(request).await?;
        match resp.status() {
            StatusCode::OK => (),
            s => {
                bail!("status code: {}, body: {}", s, resp.text().await?);
            }
        };

        let r: Workspace = resp.json().await?;

        Ok(r)
    }

    /// Delete internal user by email.
    /// This is for an enterprise admin to do only.
    /// The user must be an internal user, meaning they have an email with the company domain.
    /// FROM: https://airtable.com/api/enterprise#enterpriseAccountUserDeleteUserByEmail
    pub async fn delete_internal_user_by_email(&self, email: &str) -> Result<()> {
        if self.enterprise_account_id.is_empty() {
            // Return an error early.
            bail!("An enterprise account id is required.");
        }

        // Build the request.
        let request = self.request(
            Method::DELETE,
            format!("v0/meta/enterpriseAccounts/{}/users", self.enterprise_account_id),
            (),
            Some(vec![("email", email.to_string())]),
        )?;

        let resp = self.client.execute(request).await?;
        match resp.status() {
            StatusCode::OK => (),
            s => {
                bail!("status code: {}, body: {}", s, resp.text().await?);
            }
        };

        // Try to deserialize the response.
        let result: DeleteUserResponse = resp.json().await?;
        if !result.errors.is_empty() {
            bail!("body: {:?}", result);
        }

        Ok(())
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
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
}

enum UserField {
    Id,
    Email,
    Name,
}

const USERFIELDS: &[&str] = &["id", "email", "name"];

impl<'de> Deserialize<'de> for UserField {
    fn deserialize<D>(deserializer: D) -> Result<UserField, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct UserFieldVisitor;

        impl<'de> Visitor<'de> for UserFieldVisitor {
            type Value = UserField;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("`id` `email` or `name`")
            }

            fn visit_str<E>(self, value: &str) -> Result<UserField, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "id" => Ok(UserField::Id),
                    "email" => Ok(UserField::Email),
                    "name" => Ok(UserField::Name),
                    _ => Err(serde::de::Error::unknown_field(value, USERFIELDS)),
                }
            }
        }

        deserializer.deserialize_identifier(UserFieldVisitor)
    }
}

struct UserVisitor;

impl<'de> Visitor<'de> for UserVisitor {
    type Value = User;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("struct User")
    }

    fn visit_seq<V>(self, mut seq: V) -> Result<User, V::Error>
    where
        V: SeqAccess<'de>,
    {
        let id = seq
            .next_element()?
            .ok_or_else(|| serde::de::Error::invalid_length(0, &self))?;
        let email = seq
            .next_element()?
            .ok_or_else(|| serde::de::Error::invalid_length(1, &self))?;
        let name = seq
            .next_element()?
            .ok_or_else(|| serde::de::Error::invalid_length(2, &self))?;
        Ok(User { id, email, name })
    }

    fn visit_map<V>(self, mut map: V) -> Result<User, V::Error>
    where
        V: MapAccess<'de>,
    {
        let mut id = None;
        let mut email = None;
        let mut name = None;
        while let Some(key) = map.next_key()? {
            match key {
                UserField::Id => {
                    if id.is_some() {
                        return Err(serde::de::Error::duplicate_field("id"));
                    }
                    id = Some(map.next_value()?);
                }
                UserField::Email => {
                    if email.is_some() {
                        return Err(serde::de::Error::duplicate_field("email"));
                    }
                    email = Some(map.next_value()?);
                }
                UserField::Name => {
                    if name.is_some() {
                        return Err(serde::de::Error::duplicate_field("name"));
                    }
                    name = Some(map.next_value()?);
                }
            }
        }
        let id = id.unwrap_or_default();
        let email = email.ok_or_else(|| serde::de::Error::missing_field("email"))?;
        let name = name.unwrap_or_default();
        Ok(User { id, email, name })
    }
}

struct UsersVisitor;

impl<'de> Visitor<'de> for UsersVisitor {
    type Value = Vec<User>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a very special vector")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut access: A) -> Result<Self::Value, A::Error> {
        let mut users: Vec<User> = Default::default();

        // While there are entries remaining in the input, add them
        // into our vector.
        while let Some(user) = access.next_element::<User>()? {
            users.push(user);
        }

        Ok(users)
    }
}

/// The response returned from listing users.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct UsersResponse {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub users: Vec<User>,
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

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct AttachmentShort {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Attachment {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub filename: String,
    #[serde(default)]
    pub size: i64,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "type")]
    pub type_: String,
    #[serde(default)]
    pub thumbnails: Thumbnails,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Thumbnails {
    #[serde(default)]
    pub small: Full,
    #[serde(default)]
    pub large: Full,
    #[serde(default)]
    pub full: Full,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Full {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    #[serde(default)]
    pub width: i64,
    #[serde(default)]
    pub height: i64,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct NewCollaborator {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub collaborators: Vec<Collaborator>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Collaborator {
    #[serde(default)]
    pub user: User,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "deserialize_null_string::deserialize",
        rename = "permissionLevel"
    )]
    pub permission_level: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct EnterpriseUsersResponse {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub users: Vec<EnterpriseUser>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnterpriseUser {
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "deserialize_null_string::deserialize"
    )]
    pub id: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "deserialize_null_string::deserialize"
    )]
    pub state: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "deserialize_null_string::deserialize"
    )]
    pub email: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "deserialize_null_string::deserialize"
    )]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "lastActivityTime")]
    pub last_activity_time: Option<DateTime<Utc>>,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "deserialize_null_string::deserialize",
        rename = "invitedToAirtableByUserId"
    )]
    pub invited_to_airtable_by_user_id: String,
    #[serde(rename = "createdTime")]
    pub created_time: DateTime<Utc>,
    #[serde(default)]
    pub collaborations: Collaborations,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Collaborations {
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "workspaceCollaborations")]
    pub workspace_collaborations: Vec<Collaboration>,
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "baseCollaborations")]
    pub base_collaborations: Vec<Collaboration>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collaboration {
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "deserialize_null_string::deserialize",
        rename = "baseId"
    )]
    pub base_id: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "deserialize_null_string::deserialize",
        rename = "permissionLevel"
    )]
    pub permission_level: String,
    #[serde(rename = "createdTime")]
    pub created_time: DateTime<Utc>,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "deserialize_null_string::deserialize",
        rename = "grantedByUserId"
    )]
    pub granted_by_user_id: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "deserialize_null_string::deserialize",
        rename = "workspaceId"
    )]
    pub workspace_id: String,
}

/// Optional include flags that can be passed to [get_enterprise_workspace] to control
/// fields are returned
pub enum WorkspaceIncludes {
    Collaborators,
    InviteLinks,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub created_time: Option<DateTime<Utc>>,
    #[serde(rename = "baseIds")]
    pub base_ids: Vec<String>,
    #[serde(rename = "individualCollaborators")]
    pub individual_collaborators: Option<WorkspaceCollaborators>,
    #[serde(rename = "baseCollaborators")]
    pub group_collaborators: Option<WorkspaceCollaborators>,
    pub invite_links: Option<InviteLinks>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceCollaborators {
    #[serde(rename = "workspaceCollaborators")]
    pub workspace_collaborators: Vec<WorkspaceCollaborator>,
    #[serde(rename = "baseCollaborators")]
    pub base_collaborators: Vec<BaseCollaborator>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceCollaborator {
    #[serde(rename = "userId")]
    pub user_id: String,
    pub email: String,
    #[serde(rename = "permissionLevel")]
    pub permission_level: String,
    #[serde(rename = "createdTime")]
    pub created_time: Option<DateTime<Utc>>,
    #[serde(rename = "grantedByUserId")]
    pub granted_by_user_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseCollaborator {
    #[serde(rename = "baseId")]
    pub base_id: String,
    #[serde(rename = "userId")]
    pub user_id: String,
    pub email: String,
    #[serde(rename = "permissionLevel")]
    pub permission_level: String,
    #[serde(rename = "createdTime")]
    pub created_time: Option<DateTime<Utc>>,
    #[serde(rename = "grantedByUserId")]
    pub granted_by_user_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InviteLinks {
    pub workspace_invite_links: Vec<WorkspaceInviteLink>,
    pub base_invite_links: Vec<BaseInviteLink>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceInviteLink {
    pub id: String,
    #[serde(rename = "type")]
    pub _type: String,
    #[serde(rename = "invitedEmail")]
    pub invited_email: String,
    #[serde(rename = "restrictedToEmailDomains")]
    pub restricted_to_email_domains: Vec<String>,
    #[serde(rename = "createdTime")]
    pub created_time: Option<DateTime<Utc>>,
    #[serde(rename = "permissionLevel")]
    pub permission_level: String,
    #[serde(rename = "referredByUserId")]
    pub referred_by_user_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseInviteLink {
    pub id: String,
    #[serde(rename = "baseId")]
    pub base_id: String,
    #[serde(rename = "type")]
    pub _type: String,
    #[serde(rename = "invitedEmail")]
    pub invited_email: String,
    #[serde(rename = "restrictedToEmailDomains")]
    pub restricted_to_email_domains: Vec<String>,
    #[serde(rename = "createdTime")]
    pub created_time: Option<DateTime<Utc>>,
    #[serde(rename = "permissionLevel")]
    pub permission_level: String,
    #[serde(rename = "referredByUserId")]
    pub referred_by_user_id: String,
}

struct AttachmentsVisitor;

impl<'de> Visitor<'de> for AttachmentsVisitor {
    type Value = Vec<Attachment>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a very special vector")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut access: A) -> Result<Self::Value, A::Error> {
        let mut attachments: Vec<Attachment> = Default::default();

        // While there are entries remaining in the input, add them
        // into our vector.
        while let Some(attachment) = access.next_element::<Attachment>()? {
            attachments.push(attachment);
        }

        Ok(attachments)
    }
}

pub mod user_format_as_array_of_strings {
    use serde::{self, ser::SerializeSeq, Deserializer, Serializer};

    use super::{User, UsersVisitor};

    // The signature of a serialize_with function must follow the pattern:
    //
    //    fn serialize<S>(&T, S) -> Result<S::Ok, S::Error>
    //    where
    //        S: Serializer
    //
    // although it may also be generic over the input types T.
    pub fn serialize<S>(array: &[String], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Make our array of Airtable user objects.
        let mut seq = serializer.serialize_seq(Some(array.len())).unwrap();
        for e in array {
            seq.serialize_element(&User {
                id: Default::default(),
                email: e.to_string(),
                name: Default::default(),
            })
            .unwrap();
        }
        seq.end()
    }

    // The signature of a deserialize_with function must follow the pattern:
    //
    //    fn deserialize<'de, D>(D) -> Result<T, D::Error>
    //    where
    //        D: Deserializer<'de>
    //
    // although it may also be generic over the output types T.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let airtable_users = deserializer.deserialize_seq(UsersVisitor {}).unwrap();

        let mut users: Vec<String> = Default::default();
        for a in airtable_users {
            users.push(a.email.to_string());
        }

        Ok(users)
    }
}

pub mod user_format_as_string {
    use serde::{self, ser::SerializeStruct, Deserializer, Serializer};

    use super::{UserVisitor, USERFIELDS};

    // The signature of a serialize_with function must follow the pattern:
    //
    //    fn serialize<S>(&T, S) -> Result<S::Ok, S::Error>
    //    where
    //        S: Serializer
    //
    // although it may also be generic over the input types T.
    pub fn serialize<S>(email: &str, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("User", 1)?;
        state.serialize_field("email", &email)?;
        state.end()
    }

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
        let user = deserializer
            .deserialize_struct("User", USERFIELDS, UserVisitor)
            .unwrap();
        Ok(user.email)
    }
}

pub mod attachment_format_as_array_of_strings {
    use serde::{self, ser::SerializeSeq, Deserializer, Serializer};

    use super::{AttachmentShort, AttachmentsVisitor};

    // The signature of a serialize_with function must follow the pattern:
    //
    //    fn serialize<S>(&T, S) -> Result<S::Ok, S::Error>
    //    where
    //        S: Serializer
    //
    // although it may also be generic over the input types T.
    pub fn serialize<S>(array: &[String], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Make our array of Airtable attachment objects.
        let mut seq = serializer.serialize_seq(Some(array.len())).unwrap();
        for e in array {
            let mut attachment: AttachmentShort = Default::default();
            attachment.url = e.to_string();
            seq.serialize_element(&attachment).unwrap();
        }
        seq.end()
    }

    // The signature of a deserialize_with function must follow the pattern:
    //
    //    fn deserialize<'de, D>(D) -> Result<T, D::Error>
    //    where
    //        D: Deserializer<'de>
    //
    // although it may also be generic over the output types T.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let airtable_attachments = deserializer.deserialize_seq(AttachmentsVisitor {}).unwrap();

        let mut attachments: Vec<String> = Default::default();
        for a in airtable_attachments {
            attachments.push(a.url.to_string());
        }

        Ok(attachments)
    }
}

pub mod attachment_format_as_string {
    use serde::{self, ser::SerializeSeq, Deserializer, Serializer};

    use super::{Attachment, AttachmentsVisitor};

    // The signature of a serialize_with function must follow the pattern:
    //
    //    fn serialize<S>(&T, S) -> Result<S::Ok, S::Error>
    //    where
    //        S: Serializer
    //
    // although it may also be generic over the input types T.
    pub fn serialize<S>(url: &str, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Make our array of Airtable attachment objects.
        let mut seq = serializer.serialize_seq(Some(1)).unwrap();
        let mut attachment: Attachment = Default::default();
        attachment.url = url.to_string();
        seq.serialize_element(&attachment).unwrap();
        seq.end()
    }

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
        let airtable_attachments = deserializer.deserialize_seq(AttachmentsVisitor {}).unwrap();
        let mut url = String::new();
        if !airtable_attachments.is_empty() {
            url = airtable_attachments[0].url.to_string();
        }
        Ok(url)
    }
}

/// An airtable barcode.
#[derive(Debug, Default, Clone, Serialize, JsonSchema, Deserialize)]
pub struct Barcode {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "type")]
    pub type_: String,
}

struct BarcodeVisitor;

impl<'de> Visitor<'de> for BarcodeVisitor {
    type Value = Barcode;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("struct Barcode")
    }

    fn visit_seq<V>(self, mut seq: V) -> Result<Barcode, V::Error>
    where
        V: SeqAccess<'de>,
    {
        let text = seq
            .next_element()?
            .ok_or_else(|| serde::de::Error::invalid_length(0, &self))?;
        let type_ = seq
            .next_element()?
            .ok_or_else(|| serde::de::Error::invalid_length(1, &self))?;
        Ok(Barcode { text, type_ })
    }

    fn visit_map<V>(self, mut map: V) -> Result<Barcode, V::Error>
    where
        V: MapAccess<'de>,
    {
        let mut text = None;
        let mut type_ = None;
        while let Some(key) = map.next_key()? {
            match key {
                BarcodeField::Text => {
                    if text.is_some() {
                        return Err(serde::de::Error::duplicate_field("text"));
                    }
                    text = Some(map.next_value()?);
                }
                BarcodeField::Type => {
                    if type_.is_some() {
                        return Err(serde::de::Error::duplicate_field("type"));
                    }
                    type_ = Some(map.next_value()?);
                }
            }
        }
        let text = text.ok_or_else(|| serde::de::Error::missing_field("text"))?;
        let type_ = type_.ok_or_else(|| serde::de::Error::missing_field("type"))?;
        Ok(Barcode { text, type_ })
    }
}

enum BarcodeField {
    Text,
    Type,
}

const BARCODEFIELDS: &[&str] = &["text", "type"];

impl<'de> Deserialize<'de> for BarcodeField {
    fn deserialize<D>(deserializer: D) -> Result<BarcodeField, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct BarcodeFieldVisitor;

        impl<'de> Visitor<'de> for BarcodeFieldVisitor {
            type Value = BarcodeField;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("`text` or `type`")
            }

            fn visit_str<E>(self, value: &str) -> Result<BarcodeField, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "text" => Ok(BarcodeField::Text),
                    "type" => Ok(BarcodeField::Type),
                    _ => Err(serde::de::Error::unknown_field(value, BARCODEFIELDS)),
                }
            }
        }

        deserializer.deserialize_identifier(BarcodeFieldVisitor)
    }
}

pub mod barcode_format_as_string {
    use serde::{self, ser::SerializeStruct, Deserializer, Serializer};

    use super::{BarcodeVisitor, BARCODEFIELDS};

    // The signature of a serialize_with function must follow the pattern:
    //
    //    fn serialize<S>(&T, S) -> Result<S::Ok, S::Error>
    //    where
    //        S: Serializer
    //
    // although it may also be generic over the input types T.
    pub fn serialize<S>(text: &str, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("Barcode", 1)?;
        state.serialize_field("text", &text)?;
        // This needs to be code39 or upce.
        state.serialize_field("type", "code39")?;
        state.end()
    }

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
        let barcode = deserializer
            .deserialize_struct("Barcode", BARCODEFIELDS, BarcodeVisitor)
            .unwrap();
        Ok(barcode.text)
    }
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
