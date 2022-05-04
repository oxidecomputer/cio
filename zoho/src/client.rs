use std::{
    env,
    fmt::Display,
    marker::PhantomData,
    sync::{Arc, RwLock},
};

use anyhow::{anyhow, Result};
use reqwest::{header, Client, Method, Request, StatusCode, Url};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

const TOKEN_ENDPOINT: &str = "https://accounts.zoho.com";
const CRM_ENDPOINT: &str = "https://www.zohoapis.com/crm/v2/";

struct ZohoClient {
    // Access tokens only last one hour and need frequent refreshing
    access_token: RwLock<String>,
    refresh_token: Option<String>,
    client_id: Option<String>,
    client_secret: Option<String>,
    client: Client,
}

#[derive(Clone)]
pub struct Zoho {
    inner: Arc<ZohoClient>,
}

impl Zoho {
    /// Creates a new Zoho API client with a given access_token and optional refresh_token.
    ///
    /// If all three of the refresh token, client id, and client secret parameters are provided,
    /// then the client can run token refreshes. If they are not provided, then refresh attempts
    /// will be fail.
    pub fn new<T, U, V, W>(
        access_token: T,
        refresh_token: Option<U>,
        client_id: Option<V>,
        client_secret: Option<W>,
    ) -> Self
    where
        T: ToString,
        U: ToString,
        V: ToString,
        W: ToString,
    {
        Self {
            inner: Arc::new(ZohoClient {
                access_token: RwLock::new(access_token.to_string()),
                refresh_token: refresh_token.map(|rt| rt.to_string()),
                client_id: client_id.map(|ci| ci.to_string()),
                client_secret: client_secret.map(|cs| cs.to_string()),
                client: Client::builder()
                    .build()
                    .expect("Failed to construct HTTP client for Zoho"),
            }),
        }
    }

    /// Creates a new Zoho API client with parameters take from environment variables.
    ///
    /// [Required]
    /// ZOHO_ACCESS_TOKEN - OAuth access token
    ///
    /// [Optional]
    /// ZOHO_REFRESH_TOKEN - OAuth refresh token
    /// ZOHO_CLIENT_ID - A "Self Client" client id generated by Zoho
    /// ZOHO_CLIENT_SECRET - A "Self Client" client secret generated by Zoho
    ///
    /// If all three of the refresh token, client id, and client secret parameters are provided,
    /// then the client can run token refreshes. If they are not provided, then refresh attempts
    /// will be fail.
    pub fn new_from_env() -> Self {
        Self::new(
            env::var("ZOHO_ACCESS_TOKEN")
                .expect("Unable to construct a Zoho client from the environment without ZOHO_ACCESS_TOKEN set"),
            env::var("ZOHO_REFRESH_TOKEN").ok(),
            env::var("ZOHO_CLIENT_ID").ok(),
            env::var("ZOHO_CLIENT_SECRET").ok(),
        )
    }

    /// Creates a new Zoho API client with keys take from environment variables. Access tokens
    /// are provided by the caller.
    ///
    /// [Optional]
    /// ZOHO_CLIENT_ID - A "Self Client" client id generated by Zoho
    /// ZOHO_CLIENT_SECRET - A "Self Client" client secret generated by Zoho
    ///
    /// If all three of the refresh token, client id, and client secret parameters are provided,
    /// then the client can run token refreshes. If they are not provided, then refresh attempts
    /// will be fail.
    pub fn new_with_keys_from_env<T, U>(access_token: T, refresh_token: Option<U>) -> Self
    where
        T: ToString,
        U: ToString,
    {
        Self::new(
            access_token,
            refresh_token,
            env::var("ZOHO_CLIENT_ID").ok(),
            env::var("ZOHO_CLIENT_SECRET").ok(),
        )
    }

    /// Attempts to refresh the currently stored access token. This will return an error if
    /// any one of refresh_token, client_id, or client_secret are not set.
    pub async fn refresh_access_token(&self) -> Result<AccessTokenRefreshResponse> {
        self.inner.refresh_access_token().await
    }

    /// Fetches a list of available modules and their metadata
    /// https://www.zoho.com/crm/developer/docs/api/v2/module-meta.html
    pub async fn modules(&self) -> Result<GetModulesResponse> {
        let request = self
            .inner
            .request(CRM_ENDPOINT, &Method::GET, "settings/modules", &(), None);
        let resp = self.inner.client.execute(request).await?;

        match resp.status() {
            StatusCode::OK => Ok(resp.json().await?),
            StatusCode::NO_CONTENT => Ok(GetModulesResponse { modules: vec![] }),
            s => Err(anyhow!("status code: {}, body: {}", s, resp.text().await?)),
        }
    }

    /// Fetches a list of available fields and this metadata for a given module
    /// https://www.zoho.com/crm/developer/docs/api/v2/field-meta.html
    pub async fn fields<T>(&self, module_name: T) -> Result<GetFieldsResponse>
    where
        T: ToString,
    {
        let params = vec![("module", module_name.to_string())];
        let request = self
            .inner
            .request(CRM_ENDPOINT, &Method::GET, "settings/fields", &(), Some(params));
        let resp = self.inner.client.execute(request).await?;

        match resp.status() {
            StatusCode::OK => Ok(resp.json().await?),
            StatusCode::NO_CONTENT => Ok(GetFieldsResponse { fields: vec![] }),
            s => Err(anyhow!("status code: {}, body: {}", s, resp.text().await?)),
        }
    }

    /// Constructs a [ModuleClient] for performing CRUD operations on a specific Module type.
    ///
    /// use zoho::modules::Accounts;
    ///
    /// let client = Zoho::new_from_env();
    /// let account_client = client.module_client::<Accounts>();
    pub fn module_client<M>(&self) -> ModuleClient<M>
    where
        M: RecordsModule + DeserializeOwned,
        M::Input: Serialize,
    {
        ModuleClient {
            client: self.inner.clone(),
            _marker: PhantomData,
        }
    }
}

impl ZohoClient {
    // Constructs a request for sending to the Zoho API
    fn request<B, P>(
        &self,
        base: &str,
        method: &Method,
        path: P,
        body: &B,
        query: Option<Vec<(&str, String)>>,
    ) -> Request
    where
        B: Serialize,
        P: AsRef<str>,
    {
        let base = Url::parse(base).expect("Failed to parse Zoho client ENDPOINT");
        let url = base.join(path.as_ref()).expect("Failed to construct Zoho endpoint url");

        // Unwrapping as we want to panic if the access_token is considered poisoned
        let auth_token = format!("Zoho-oauthtoken {}", self.access_token.read().unwrap());
        let auth = header::HeaderValue::from_str(&auth_token).expect("Failed to construct Zoho auth header value");

        // Set the default headers.
        let mut headers = header::HeaderMap::new();
        headers.append(header::AUTHORIZATION, auth);

        // We send json to the Zoho API, but the API documentation does not specify a required
        // Content-Type header. It is uncertain if it is actually necessary.
        headers.append(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/json"),
        );

        let mut request = self.client.request(method.clone(), url).headers(headers);

        // Add the body, this is to ensure our GET and DELETE calls succeed.
        if method != Method::GET && method != Method::DELETE {
            request = request.json(body);
        }

        if let Some(query) = query {
            request = request.query(&query);
        }

        // Build the request.
        request.build().expect("Failed to construct Zoho request")
    }

    /// Refreshes the internal access token stored in this client and returns back the refresh
    /// token response sent by Zoho. This will return an error if any one of refresh_token,
    /// client_id, or client_secret are not set.
    async fn refresh_access_token(&self) -> Result<AccessTokenRefreshResponse> {
        if let (Some(refresh_token), Some(client_id), Some(client_secret)) =
            (&self.refresh_token, &self.client_id, &self.client_secret)
        {
            let params = vec![
                ("refresh_token", refresh_token.to_string()),
                ("client_id", client_id.to_string()),
                ("client_secret", client_secret.to_string()),
                ("grant_type", "refresh_token".to_string()),
            ];

            let request = self.request(TOKEN_ENDPOINT, &Method::POST, "oauth/v2/token", &(), Some(params));
            let response = self.client.execute(request).await?;
            let new_token: AccessTokenRefreshResponse = response.json().await?;

            {
                let mut token = self
                    .access_token
                    .write()
                    .expect("Failed to gain write access to Zoho access token");
                *token = new_token.access_token.clone();
            }

            Ok(new_token)
        } else {
            Err(anyhow!(
                "Unable to refresh access token without refresh token, client id, and client secret"
            ))
        }
    }
}

/// Refreshed access tokens last for one hour
/// https://www.zoho.com/crm/developer/docs/api/v2/refresh.html
#[derive(Debug, Deserialize)]
pub struct AccessTokenRefreshResponse {
    pub access_token: String,
    pub expires_in: u32,
    pub api_domain: String,
    pub token_type: String,
}

/// https://www.zoho.com/crm/developer/docs/api/v2/module-meta.html
#[derive(Debug, Deserialize)]
pub struct GetModulesResponse {
    pub modules: Vec<Module>,
}

/// https://www.zoho.com/crm/developer/docs/api/v2/module-meta.html
#[derive(Debug, Deserialize)]
pub struct Module {
    pub id: String,
    pub singular_label: String,
    pub api_supported: bool,
    pub api_name: String,
    pub module_name: String,
}

/// https://www.zoho.com/crm/developer/docs/api/v2/field-meta.html
#[derive(Debug, Deserialize)]
pub struct GetFieldsResponse {
    pub fields: Vec<Field>,
}

/// https://www.zoho.com/crm/developer/docs/api/v2/field-meta.html
#[derive(Debug, Deserialize)]
pub struct Field {
    pub api_name: String,
    pub json_type: String,
}

/// Attempts to parse the Zoho provided type for an object in to a "stringy" Rust type
/// This is used in generated Module structs to provide better types for struct fields
/// https://www.zoho.com/crm/developer/docs/api/v2/field-meta.html
impl Field {
    pub fn json_type(&self) -> &str {
        match self.json_type.as_str() {
            "string" => "String",
            "integer" => "i64",
            "double" => "f64",
            "boolean" => "bool",
            "jsonobject" => "serde_json::Value",
            "jsonarray" => "Vec<serde_json::Value>",
            _ => {
                // Unknown types are mapped to serde_json::Value so they can at be deserialized
                "serde_json::Value"
            }
        }
    }
}

/// The defining trait for a Zoho module. All Module structs must implement this trait.
/// [ModuleClient] requires that this trait be implemented. The `api_path` method should
/// return the url safe string name of this Module for use in constructing api endpoint strings.
/// The associated `Input` type defines the type to use when performing Insert, Update, or Upsert
/// operations.
pub trait RecordsModule {
    type Input;
    fn api_path() -> &'static str;
}

/// A client for interaction with Records in Zoho. Provides basic CRUD operations for a specific
/// Module type. The associated Input struct encodes the fields that are required when performing
/// updates.
pub struct ModuleClient<M> {
    client: Arc<ZohoClient>,
    _marker: PhantomData<M>,
}

impl<M> ModuleClient<M>
where
    M: RecordsModule + DeserializeOwned,
    M::Input: Serialize,
{
    /// https://www.zoho.com/crm/developer/docs/api/v2/get-records.html
    pub async fn all(&self, params: GetModuleRecordsParams) -> Result<GetModuleRecordsResponse<M>> {
        let path = M::api_path();
        let request = self
            .client
            .request(CRM_ENDPOINT, &Method::GET, path, &(), Some(params.into()));

        let response = self.client.client.execute(request).await?;

        match response.status() {
            StatusCode::OK => Ok(response.json().await?),
            StatusCode::NO_CONTENT => Ok(GetModuleRecordsResponse {
                data: vec![],
                info: ModuleRecordsPagination {
                    call: None,
                    per_page: 0,
                    count: 0,
                    page: 1,
                    email: None,
                    more_records: false,
                },
            }),
            s => Err(anyhow!("status code: {}, body: {}", s, response.text().await?)),
        }
    }

    /// https://www.zoho.com/crm/developer/docs/api/v2/get-records.html
    pub async fn get<S>(&self, id: S, params: GetModuleRecordsParams) -> Result<GetModuleRecordsResponse<M>>
    where
        S: AsRef<str>,
    {
        let path = [M::api_path(), id.as_ref()].join("/");
        let request = self
            .client
            .request(CRM_ENDPOINT, &Method::GET, path, &(), Some(params.into()));

        let response = self.client.client.execute(request).await?;

        match response.status() {
            StatusCode::OK => Ok(response.json().await?),
            StatusCode::NO_CONTENT => Ok(GetModuleRecordsResponse {
                data: vec![],
                info: ModuleRecordsPagination {
                    call: None,
                    per_page: 0,
                    count: 0,
                    page: 1,
                    email: None,
                    more_records: false,
                },
            }),
            s => Err(anyhow!("status code: {}, body: {}", s, response.text().await?)),
        }
    }

    /// https://www.zoho.com/crm/developer/docs/api/v2/insert-records.html
    pub async fn insert(&self, input: Vec<M::Input>, trigger: Option<Vec<String>>) -> Result<ModuleUpdateResponse> {
        let path = M::api_path();
        let request = self.client.request(
            CRM_ENDPOINT,
            &Method::POST,
            path,
            &ModuleUpdateRequest { data: input, trigger },
            None,
        );

        let response = self.client.client.execute(request).await?;
        let data: ModuleUpdateResponse = response.json().await?;

        Ok(data)
    }

    /// https://www.zoho.com/crm/developer/docs/api/v2/update-records.html
    pub async fn update(&self, update: Vec<M::Input>, trigger: Option<Vec<String>>) -> Result<ModuleUpdateResponse> {
        let path = M::api_path();
        let request = self.client.request(
            CRM_ENDPOINT,
            &Method::PUT,
            path,
            &ModuleUpdateRequest { data: update, trigger },
            None,
        );

        let response = self.client.client.execute(request).await?;
        let data: ModuleUpdateResponse = response.json().await?;

        Ok(data)
    }

    /// https://www.zoho.com/crm/developer/docs/api/v2/upsert-records.html
    pub async fn upsert(
        &self,
        update: Vec<M::Input>,
        trigger: Option<Vec<String>>,
        duplicate_check_fields: Option<Vec<String>>,
    ) -> Result<ModuleUpdateResponse> {
        let path = M::api_path();
        let request = self.client.request(
            CRM_ENDPOINT,
            &Method::PUT,
            path,
            &ModuleUpsertRequest {
                data: update,
                trigger,
                duplicate_check_fields,
            },
            None,
        );

        let response = self.client.client.execute(request).await?;
        let data: ModuleUpdateResponse = response.json().await?;

        Ok(data)
    }

    /// https://www.zoho.com/crm/developer/docs/api/v2/delete-records.html
    pub async fn delete(&self, ids: Vec<String>, wf_trigger: bool) -> Result<ModuleDeleteResponse> {
        let path = M::api_path();
        let params = vec![("ids", ids.join(",")), ("wf_trigger", wf_trigger.to_string())];
        let request = self
            .client
            .request(CRM_ENDPOINT, &Method::DELETE, path, &(), Some(params));

        let response = self.client.client.execute(request).await?;
        let data: ModuleDeleteResponse = response.json().await?;

        Ok(data)
    }

    // TODO: https://www.zoho.com/crm/developer/docs/api/v2/get-deleted-records.html
    // TODO: https://www.zoho.com/crm/developer/docs/api/v2/search-records.html
}

// impl ModuleClient<Leads> {
// TODO: https://www.zoho.com/crm/developer/docs/api/v2/convert-lead.html
// }

#[derive(Default)]
pub struct GetModuleRecordsParams {
    pub fields: Option<Vec<String>>,
    pub ids: Option<Vec<String>>,
    pub sort_order: Option<ModuleSortOrder>,
    pub sort_by: Option<String>,
    pub converted: Option<ModuleConvertedFlag>,
    pub approved: Option<ModuleApprovedFlag>,
    pub page: Option<u32>,
    pub per_page: Option<u32>,
    pub cvid: Option<String>,
    pub territory_id: Option<String>,
    pub include_child: Option<bool>,
}

pub enum ModuleSortOrder {
    Asc,
    Desc,
}

impl Display for ModuleSortOrder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Asc => write!(f, "asc"),
            Self::Desc => write!(f, "desc"),
        }
    }
}

pub enum ModuleConvertedFlag {
    True,
    False,
    Both,
}

impl Display for ModuleConvertedFlag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::True => write!(f, "true"),
            Self::False => write!(f, "false"),
            Self::Both => write!(f, "both"),
        }
    }
}

pub enum ModuleApprovedFlag {
    True,
    False,
    Both,
}

impl Display for ModuleApprovedFlag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::True => write!(f, "true"),
            Self::False => write!(f, "false"),
            Self::Both => write!(f, "both"),
        }
    }
}

impl From<GetModuleRecordsParams> for Vec<(&str, String)> {
    fn from(get_params: GetModuleRecordsParams) -> Self {
        let mut params = vec![];

        if let Some(fields) = get_params.fields {
            params.push(("fields", fields.join(",")));
        }

        if let Some(ids) = get_params.ids {
            params.push(("ids", ids.join(",")));
        }

        if let Some(sort_order) = get_params.sort_order {
            params.push(("sort_order", sort_order.to_string()));
        }

        if let Some(sort_by) = get_params.sort_by {
            params.push(("sort_by", sort_by));
        }

        if let Some(converted) = get_params.converted {
            params.push(("converted", converted.to_string()));
        }

        if let Some(approved) = get_params.approved {
            params.push(("approved", approved.to_string()));
        }

        if let Some(page) = get_params.page {
            params.push(("page", page.to_string()));
        }

        if let Some(per_page) = get_params.per_page {
            params.push(("per_page", per_page.to_string()));
        }

        if let Some(cvid) = get_params.cvid {
            params.push(("cvid", cvid));
        }

        if let Some(territory_id) = get_params.territory_id {
            params.push(("territory_id", territory_id));
        }

        if let Some(include_child) = get_params.include_child {
            params.push(("include_child", include_child.to_string()));
        }

        params
    }
}

#[derive(Debug, Deserialize)]
pub struct GetModuleRecordsResponse<M> {
    pub data: Vec<M>,
    pub info: ModuleRecordsPagination,
}

#[derive(Debug, Deserialize)]
pub struct ModuleRecordsPagination {
    pub call: Option<bool>,
    pub per_page: u32,
    pub count: u32,
    pub page: u32,
    pub email: Option<bool>,
    pub more_records: bool,
}

#[derive(Debug, Serialize)]
pub struct ModuleUpdateRequest<M> {
    pub data: Vec<M>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct ModuleUpdateResponse {
    pub data: Vec<ModuleUpdateResponseEntry>,
}

#[derive(Debug, Deserialize)]
pub struct ModuleUpdateResponseEntry {
    pub code: String,
    pub details: ModuleUpdateResponseEntryDetails,
}

#[derive(Debug, Deserialize)]
pub struct ModuleUpdateResponseEntryDetails {
    #[serde(alias = "Modified_Time")]
    pub modifiend_time: String,
    #[serde(alias = "Modified_Time")]
    pub modified_by: ModuleUpdateResponseEntryModified,
    #[serde(alias = "Created_Time")]
    pub created_time: String,
    pub id: String,
    #[serde(alias = "Created_By")]
    pub created_by: ModuleUpdateResponseEntryModified,
    pub message: String,
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct ModuleUpdateResponseEntryModified {
    pub name: String,
    pub id: String,
}

#[derive(Debug, Serialize)]
pub struct ModuleUpsertRequest<M> {
    pub data: Vec<M>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duplicate_check_fields: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct ModuleDeleteResponse {
    pub data: Vec<ModuleDeleteResponseEntry>,
}

#[derive(Debug, Deserialize)]
pub struct ModuleDeleteResponseEntry {
    pub code: String,
    pub details: ModuleDeleteResponseDetails,
    pub message: String,
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct ModuleDeleteResponseDetails {
    pub id: String,
}
