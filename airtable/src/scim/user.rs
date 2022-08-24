
pub struct AirtableScimUserClient {
    inner: Inner,
}

impl AirtableScimUserClient {
    fn base_endpoint() -> &'static str {
        "https://airtable.com/scim/v2/Users"
    }

    fn url(base: &str, path: Option<&str>) -> Result<Url, ScimError> {
        let url = Url::parse(base)?;

        if let Some(path) = path {
            Ok(url.join("/")?.join(path)?)
        } else {
            Ok(url)
        }
    }

    /// From: https://airtable.com/api/enterprise#scimUsersGet
    pub async fn list(&self) -> Result<ScimListResponse<ScimUser>, ScimError> {
        let req = self
            .inner
            .request(Method::GET, Self::url(Self::base_endpoint(), None)?, None)?
            .body("")
            .build()?;
        let resp = self.inner.execute(req).await?;

        to_client_response(resp).await
    }

    /// From: https://airtable.com/api/enterprise#scimUsersGetById
    pub async fn get<T: AsRef<str>>(&self, id: T) -> Result<Option<ScimUser>, ScimError> {
        let req = self
            .inner
            .request(Method::GET, Self::url(Self::base_endpoint(), Some(id.as_ref()))?, None)?
            .body("")
            .build()?;
        let resp = self.inner.execute(req).await?;

        to_client_response(resp).await
    }

    /// From: https://airtable.com/api/enterprise#scimUserCreate
    pub async fn create(&self, new_user: &ScimCreateUser) -> Result<ScimUser, ScimError> {
        let req = self
            .inner
            .request(Method::POST, Self::url(Self::base_endpoint(), None)?, None)?
            .body(serde_json::to_string(new_user)?)
            .build()?;
        let resp = self.inner.execute(req).await?;

        to_client_response(resp).await
    }

    /// From: https://airtable.com/api/enterprise#scimUserUpdate
    pub async fn update<T: AsRef<str>>(&self, id: T, user: &ScimUpdateUser) -> Result<ScimUser, ScimError> {
        let req = self
            .inner
            .request(Method::PUT, Self::url(Self::base_endpoint(), Some(id.as_ref()))?, None)?
            .body(serde_json::to_string(user)?)
            .build()?;
        let resp = self.inner.execute(req).await?;

        to_client_response(resp).await
    }

    // /// From: https://airtable.com/api/enterprise#scimUserPatch
    // pub async fn patch<T: AsRef<str>>(&self, id: T, operation: ScimPatchOp) -> Result<ScimUser, ScimError> {
    //     unimplemented!()
    // }
}

#[derive(Debug, PartialEq, Clone, Serialize, JsonSchema, Deserialize)]
pub struct ScimUser {
    pub schemas: Vec<String>,
    pub id: String,
    #[serde(rename = "userName")]
    pub username: String,
    pub name: ScimName,
    pub active: bool,
    pub meta: ScimUserMeta,
    pub emails: Vec<ScimUserEmail>,
    #[serde(flatten)]
    pub extensions: HashMap<String, HashMap<String, Value>>,
}

#[derive(Debug, PartialEq, Clone, Serialize, JsonSchema, Deserialize)]
pub struct ScimName {
    #[serde(rename = "familyName")]
    pub family_name: String,
    #[serde(rename = "givenName")]
    pub given_name: String,
}

#[derive(Debug, PartialEq, Clone, Serialize, JsonSchema, Deserialize)]
pub struct ScimUserMeta {
    pub created: DateTime<Utc>,
    #[serde(rename = "resourceType")]
    pub resource_type: String,
    pub location: String,
}

#[derive(Debug, PartialEq, Clone, Serialize, JsonSchema, Deserialize)]
pub struct ScimUserEmail {
    pub value: String,
}

#[derive(Debug, PartialEq, Clone, Serialize, JsonSchema, Deserialize)]
pub struct ScimCreateUser {
    schemas: Vec<String>,
    #[serde(rename = "userName")]
    user_name: String,
    name: ScimName,
    /// The title field is available in create and update requests, but it is not returned in
    /// retrieval responses
    /// See: https://airtable.com/api/enterprise#scimUserFieldTypes
    title: String,
    #[serde(flatten)]
    extensions: HashMap<String, HashMap<String, Value>>,
}

#[derive(Debug, PartialEq, Clone, Serialize, JsonSchema, Deserialize)]
pub struct ScimUpdateUser {
    schemas: Option<Vec<String>>,
    #[serde(rename = "userName")]
    user_name: Option<String>,
    name: Option<ScimName>,
    /// The title field is available in create and update requests, but it is not returned in
    /// retrieval responses
    /// See: https://airtable.com/api/enterprise#scimUserFieldTypes
    title: Option<String>,
    active: Option<bool>,
    #[serde(flatten)]
    extensions: Option<HashMap<String, HashMap<String, Value>>>,
}
