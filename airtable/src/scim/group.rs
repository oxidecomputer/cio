pub struct AirtableScimGroupClient {
    inner: Inner,
}

impl AirtableScimGroupClient {
    fn singular_endpoint() -> &'static str {
        "https://airtable.com/scim/v2/Group"
    }

    fn plural_endpoint() -> &'static str {
        "https://airtable.com/scim/v2/Groups"
    }

    fn url(base: &str, path: Option<&str>) -> Result<Url, ScimError> {
        let url = Url::parse(base)?;

        if let Some(path) = path {
            Ok(url.join("/")?.join(path)?)
        } else {
            Ok(url)
        }
    }

    /// From: https://airtable.com/api/enterprise#scimGroupsList
    pub async fn list(&self) -> Result<ScimListResponse<ScimGroupIndex>, ScimError> {
        let req = self
            .inner
            .request(Method::GET, Self::url(Self::plural_endpoint(), None)?, None)?
            .body("")
            .build()?;
        let resp = self.inner.execute(req).await?;

        to_client_response(resp).await
    }

    /// From: https://airtable.com/api/enterprise#scimGroupsGetById
    pub async fn get<T: AsRef<str>>(&self, id: T) -> Result<Option<ScimGroup>, ScimError> {
        let req = self
            .inner
            .request(
                Method::GET,
                Self::url(Self::plural_endpoint(), Some(id.as_ref()))?,
                None,
            )?
            .body("")
            .build()?;
        let resp = self.inner.execute(req).await?;

        to_client_response(resp).await
    }

    /// From: https://airtable.com/api/enterprise#scimGroupCreate
    pub async fn create(&self, new_group: &ScimCreateGroup) -> Result<ScimWriteGroupResponse, ScimError> {
        let req = self
            .inner
            .request(Method::POST, Self::url(Self::singular_endpoint(), None)?, None)?
            .body(serde_json::to_string(new_group)?)
            .build()?;
        let resp = self.inner.execute(req).await?;

        to_client_response(resp).await
    }

    /// From: https://airtable.com/api/enterprise#scimGroupUpdate
    pub async fn update<T: AsRef<str>>(
        &self,
        id: T,
        group: &ScimUpdateGroup,
    ) -> Result<ScimWriteGroupResponse, ScimError> {
        let req = self
            .inner
            .request(
                Method::PUT,
                Self::url(Self::singular_endpoint(), Some(id.as_ref()))?,
                None,
            )?
            .body(serde_json::to_string(group)?)
            .build()?;
        let resp = self.inner.execute(req).await?;

        to_client_response(resp).await
    }

    // /// From: https://airtable.com/api/enterprise#scimGroupPatch
    // pub async fn patch<T: AsRef<str>>(&self, id: T, operation: ScimPatchOp) -> Result<ScimGroup, ScimError> {
    //     unimplemented!()
    // }

    /// From: https://airtable.com/api/enterprise#scimGroupDelete
    pub async fn delete<T: AsRef<str>>(&self, id: T) -> Result<(), ScimError> {
        let req = self
            .inner
            .request(
                Method::DELETE,
                Self::url(Self::plural_endpoint(), Some(id.as_ref()))?,
                None,
            )?
            .body("")
            .build()?;
        let resp = self.inner.execute(req).await?;

        // Delete does not return a body on success
        if resp.status() == StatusCode::OK {
            Ok(())
        } else {
            to_client_response(resp).await
        }
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, JsonSchema, Deserialize)]
pub struct ScimGroupIndex {
    pub schemas: Vec<String>,
    pub id: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
}

#[derive(Debug, PartialEq, Default, Clone, Serialize, JsonSchema, Deserialize)]
pub struct ScimGroup {
    pub schemas: Vec<String>,
    pub id: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    pub members: Vec<ScimGroupMember>,
}

#[derive(Debug, PartialEq, Default, Clone, Serialize, JsonSchema, Deserialize)]
pub struct ScimGroupMember {
    pub value: String,
}

#[derive(Debug, PartialEq, Default, Clone, Serialize, JsonSchema, Deserialize)]
pub struct ScimCreateGroup {
    pub schemas: Vec<String>,
    #[serde(rename = "displayName")]
    pub display_name: String,
}

#[derive(Debug, PartialEq, Default, Clone, Serialize, JsonSchema, Deserialize)]
pub struct ScimUpdateGroup {
    pub schemas: Option<Vec<String>>,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    pub members: Option<Vec<ScimGroupMember>>,
}

#[derive(Debug, PartialEq, Default, Clone, Serialize, JsonSchema, Deserialize)]
pub struct ScimWriteGroupResponse {
    pub schemas: Vec<String>,
    pub id: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
}
