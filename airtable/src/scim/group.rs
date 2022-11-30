use reqwest::{Method, Url};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{to_client_response, ScimClientError, ScimListResponse};
use crate::Inner;

/// A client for making requests to the Airtable Enterprise SCIM Group endpoints. An [AirtableScimGroupClient]
/// can be retrieved an from [AirtableScimClient][crate::AirtableScimClient]. Supports listing, reading, creating,
/// updating, and deleting groups as defined by the Airtable SCIM Groups API. Patching groups is not currently
/// supported.
pub struct AirtableScimGroupClient {
    inner: Inner,
}

impl AirtableScimGroupClient {
    pub(super) fn new(inner: Inner) -> Self {
        Self { inner }
    }

    fn base_endpoint() -> &'static str {
        "https://airtable.com/scim/v2/Groups"
    }

    fn url(base: &str, path: Option<&str>) -> Result<Url, ScimClientError> {
        if let Some(path) = path {
            Ok(Url::parse(&(base.to_string() + "/" + path))?)
        } else {
            Ok(Url::parse(base)?)
        }
    }

    /// List groups as [SCIM Group](https://datatracker.ietf.org/doc/html/rfc7643#section-4.2) objects.
    ///
    /// From: <https://airtable.com/api/enterprise#scimGroupsList>
    pub async fn list(
        &self,
        filter: Option<&ScimListGroupOptions>,
    ) -> Result<ScimListResponse<ScimGroupIndex>, ScimClientError> {
        let query_args = filter.map(|options| options.to_query_args());

        let req = self
            .inner
            .request(Method::GET, Self::url(Self::base_endpoint(), None)?, query_args)?
            .body("")
            .build()?;
        let resp = self.inner.execute(req).await?;

        to_client_response(resp).await
    }

    /// Get a single group as a [SCIM Group](https://datatracker.ietf.org/doc/html/rfc7643#section-4.2) object.
    ///
    /// From: <https://airtable.com/api/enterprise#scimGroupsGetById>
    pub async fn get<T: AsRef<str>>(&self, id: T) -> Result<Option<ScimGroup>, ScimClientError> {
        let req = self
            .inner
            .request(
                Method::GET,
                Self::url(Self::base_endpoint(), Some(id.as_ref()))?,
                None,
            )?
            .body("")
            .build()?;
        let resp = self.inner.execute(req).await?;

        to_client_response(resp).await
    }

    /// Create a new group from a [SCIM Group](https://datatracker.ietf.org/doc/html/rfc7643#section-4.2) object.
    /// The supplied display name must not currently be in use.
    ///
    /// From: <https://airtable.com/api/enterprise#scimGroupCreate>
    pub async fn create(&self, new_group: &ScimCreateGroup) -> Result<ScimWriteGroupResponse, ScimClientError> {
        let req = self
            .inner
            .request(Method::POST, Self::url(Self::base_endpoint(), None)?, None)?
            .json(new_group)
            .build()?;
        let resp = self.inner.execute(req).await?;

        to_client_response(resp).await
    }

    /// Replace a group with a new a [SCIM Group](https://datatracker.ietf.org/doc/html/rfc7643#section-4.2) object.
    /// Display name and member list are both optional. If a display name is supplied it must be an unused group name.
    /// If a member list is supplied it will replace the existing list in entirety.
    ///
    /// From: <https://airtable.com/api/enterprise#scimGroupUpdate>
    pub async fn update<T: AsRef<str>>(
        &self,
        id: T,
        group: &ScimUpdateGroup,
    ) -> Result<ScimWriteGroupResponse, ScimClientError> {
        let req = self
            .inner
            .request(
                Method::PUT,
                Self::url(Self::base_endpoint(), Some(id.as_ref()))?,
                None,
            )?
            .json(group)
            .build()?;
        let resp = self.inner.execute(req).await?;

        to_client_response(resp).await
    }

    // /// From: <https://airtable.com/api/enterprise#scimGroupPatch>
    // pub async fn patch<T: AsRef<str>>(&self, id: T, operation: ScimPatchOp) -> Result<ScimGroup, ScimClientError> {
    //     unimplemented!()
    // }

    /// Delete a group
    ///
    /// From: <https://airtable.com/api/enterprise#scimGroupDelete>
    pub async fn delete<T: AsRef<str>>(&self, id: T) -> Result<(), ScimClientError> {
        let req = self
            .inner
            .request(
                Method::DELETE,
                Self::url(Self::base_endpoint(), Some(id.as_ref()))?,
                None,
            )?
            .body("")
            .build()?;
        let resp = self.inner.execute(req).await?;

        // Delete does not return a body on success
        if resp.status().is_success() {
            Ok(())
        } else {
            to_client_response(resp).await
        }
    }
}

/// Options for controlling the groups that are returned from a list request
#[derive(Debug, PartialEq, Clone, Serialize, JsonSchema, Deserialize)]
pub struct ScimListGroupOptions {
    pub start_index: Option<u32>,
    pub count: Option<u32>,
    pub filter: Option<ScimListGroupFilter>,
}

/// Filters the groups returned in a list request by their displayName
#[derive(Debug, PartialEq, Clone, Serialize, JsonSchema, Deserialize)]
pub struct ScimListGroupFilter {
    pub display_name: Option<String>,
}

impl ScimListGroupOptions {
    pub fn to_query_args(&self) -> Vec<(&str, String)> {
        let mut args = vec![];

        if let Some(start_index) = self.start_index {
            args.push(("startIndex", start_index.to_string()));
        }

        if let Some(count) = self.count {
            args.push(("count", count.to_string()));
        }

        if let Some(filter) = &self.filter {
            let mut filter_conditions = String::default();

            if let Some(display_name) = &filter.display_name {
                filter_conditions.push_str(&format!(r#"displayName+eq+"{}""#, display_name));
            }

            if !filter_conditions.is_empty() {
                args.push(("fitler", filter_conditions));
            }
        }

        args
    }
}

/// A SCIM group
///
/// See: <https://airtable.com/api/enterprise#scimGroupFieldTypes>
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

/// A partial SCIM group that does not contain membership data. Partial groups are returned from
/// the list group endpoints
#[derive(Debug, PartialEq, Clone, Serialize, JsonSchema, Deserialize)]
pub struct ScimGroupIndex {
    pub schemas: Vec<String>,
    pub id: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
}

impl From<ScimGroup> for ScimGroupIndex {
    fn from(group: ScimGroup) -> Self {
        ScimGroupIndex {
            schemas: group.schemas,
            id: group.id,
            display_name: group.display_name,
        }
    }
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

#[cfg(test)]
mod tests {
    use reqwest::Url;
    use super::{AirtableScimGroupClient, ScimListGroupFilter, ScimListGroupOptions};

    #[test]
    fn test_url_construction() {
        assert_eq!(
            Url::parse("https://airtable.com/scim/v2/Groups").unwrap(),
            AirtableScimGroupClient::url(AirtableScimGroupClient::base_endpoint(), None).unwrap(),
        );

        assert_eq!(
            Url::parse("https://airtable.com/scim/v2/Groups/a_group_id").unwrap(),
            AirtableScimGroupClient::url(AirtableScimGroupClient::base_endpoint(), Some("a_group_id")).unwrap(),
        );
    }

    #[test]
    fn test_serialize_list_options() {
        let options = ScimListGroupOptions {
            start_index: Some(5),
            count: Some(10),
            filter: Some(ScimListGroupFilter {
                display_name: Some("Example Group".to_string()),
            }),
        };

        let expected = vec![
            ("startIndex", "5".to_string()),
            ("count", "10".to_string()),
            ("fitler", r#"displayName+eq+"Example Group""#.to_string()),
        ];

        assert_eq!(expected, options.to_query_args());
    }
}
