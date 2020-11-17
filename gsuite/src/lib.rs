/*!
 * A rust library for interacting with the GSuite APIs.
 *
 * For more information, the GSuite Directory API is documented at
 * [developers.google.com/admin-sdk/directory/v1/reference](https://developers.google.com/admin-sdk/directory/v1/reference)
 * and the Google Groups settings API is documented at
 * [developers.google.com/admin-sdk/groups-settings/v1/reference/groups](https://developers.google.com/admin-sdk/groups-settings/v1/reference/groups).
 *
 * Example:
 *
 * ```
 * use std::env;
 *
 * use gsuite_api::GSuite;
 * use yup_oauth2::{read_service_account_key, ServiceAccountAuthenticator};
 *
 * async fn get_users() {
 *     // Get the GSuite credentials file.
 *     let gsuite_credential_file =
 *         env::var("GADMIN_CREDENTIAL_FILE").unwrap();
 *     let gsuite_subject = env::var("GADMIN_SUBJECT").unwrap();
 *     let gsuite_secret = read_service_account_key(gsuite_credential_file)
 *         .await
 *         .expect("failed to read gsuite credential file");
 *     let auth = ServiceAccountAuthenticator::builder(gsuite_secret)
 *         .subject(gsuite_subject.to_string())
 *         .build()
 *         .await
 *         .expect("failed to create authenticator");
 *
 *     // Add the scopes to the secret and get the token.
 *     let token = auth.token(&[
 *      "https://www.googleapis.com/auth/admin.directory.group",
 *      "https://www.googleapis.com/auth/admin.directory.resource.calendar",
 *      "https://www.googleapis.com/auth/admin.directory.user",
 *      "https://www.googleapis.com/auth/apps.groups.settings",
 *  ]).await.expect("failed to get token");
 *
 *     if token.as_str().is_empty() {
 *         panic!("empty token is not valid");
 *     }
 *
 *     // Initialize the GSuite client.
 *     let gsuite_client = GSuite::new("customer_id", "domain", token);
 *
 *     // List users.
 *     let users = gsuite_client.list_users().await;
 *
 *     // Iterate over the users.
 *     for user in users {
 *         println!("{:?}", user);
 *     }
 * }
 * ```
 */
use std::collections::HashMap;
use std::error;
use std::fmt;
use std::sync::Arc;

use chrono::offset::Utc;
use chrono::DateTime;
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use reqwest::{header, Client, Method, Request, StatusCode, Url};
use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize, Serializer};
use serde_json::value::Value;
use yup_oauth2::AccessToken;

#[macro_use]
extern crate serde_json;

use cio_api::configs::{BuildingConfig, ResourceConfig, UserConfig};

/// The endpoint for the GSuite Directory API.
const DIRECTORY_ENDPOINT: &str =
    "https://www.googleapis.com/admin/directory/v1/";

/// Endpoint for the Google Groups settings API.
const GROUPS_SETTINGS_ENDPOINT: &str =
    "https://www.googleapis.com/groups/v1/groups/";

/// Entrypoint for interacting with the GSuite APIs.
pub struct GSuite {
    customer: String,
    domain: String,

    token: AccessToken,

    client: Arc<Client>,
}

impl GSuite {
    /// Create a new GSuite client struct. It takes a type that can convert into
    /// an &str (`String` or `Vec<u8>` for example). As long as the function is
    /// given a valid API Key and Secret your requests will work.
    pub fn new(customer: &str, domain: &str, token: AccessToken) -> Self {
        let client = Client::builder().build().expect("creating client failed");
        Self {
            customer: customer.to_string(),
            domain: domain.to_string(),
            token,
            client: Arc::new(client),
        }
    }

    /// Get the currently set authorization token.
    pub fn get_token(&self) -> &AccessToken {
        &self.token
    }

    fn request<B>(
        &self,
        endpoint: &str,
        method: Method,
        path: &str,
        body: B,
        query: Option<&[(&str, &str)]>,
    ) -> Request
    where
        B: Serialize,
    {
        let base = Url::parse(endpoint).unwrap();
        let url = base.join(path).unwrap();

        // Check if the token is expired and panic.
        if self.token.is_expired() {
            panic!("token is expired");
        }

        let bt = format!("Bearer {}", self.token.as_str());
        let bearer = header::HeaderValue::from_str(&bt).unwrap();

        // Set the default headers.
        let mut headers = header::HeaderMap::new();
        headers.append(header::AUTHORIZATION, bearer);
        headers.append(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/json"),
        );
        headers.append(
            header::ACCEPT,
            header::HeaderValue::from_static("application/json"),
        );

        let mut rb = self.client.request(method.clone(), url).headers(headers);

        if let Some(val) = query {
            rb = rb.query(&val);
        }

        // Add the body, this is to ensure our GET and DELETE calls succeed.
        if method != Method::GET && method != Method::DELETE {
            rb = rb.json(&body);
        }

        // Build the request.
        rb.build().unwrap()
    }

    /// List Google groups.
    pub async fn list_groups(&self) -> Result<Vec<Group>, APIError> {
        // Build the request.
        let request = self.request(
            DIRECTORY_ENDPOINT,
            Method::GET,
            "groups",
            (),
            Some(&[("customer", &self.customer), ("domain", &self.domain)]),
        );

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                });
            }
        };

        // Try to deserialize the response.
        let value: Groups = resp.json().await.unwrap();

        Ok(value.groups.unwrap())
    }

    /// Get the settings for a Google group.
    pub async fn get_group_settings(
        &self,
        group_email: &str,
    ) -> Result<GroupSettings, APIError> {
        // Build the request.
        let request = self.request(
            GROUPS_SETTINGS_ENDPOINT,
            Method::GET,
            group_email,
            (),
            Some(&[("alt", "json")]),
        );

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                });
            }
        };

        // Try to deserialize the response.
        Ok(resp.json().await.unwrap())
    }

    /// Update a Google group.
    pub async fn update_group(&self, group: &Group) -> Result<(), APIError> {
        // Build the request.
        let request = self.request(
            DIRECTORY_ENDPOINT,
            Method::PUT,
            &format!("groups/{}", group.id.as_ref().unwrap()),
            group,
            None,
        );

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                });
            }
        };

        Ok(())
    }

    /// Update a Google group's settings.
    pub async fn update_group_settings(
        &self,
        settings: &GroupSettings,
    ) -> Result<(), APIError> {
        // Build the request.
        let request = self.request(
            GROUPS_SETTINGS_ENDPOINT,
            Method::PUT,
            settings.email.as_ref().unwrap(),
            settings,
            Some(&[("alt", "json")]),
        );

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                });
            }
        };

        Ok(())
    }

    /// Create a google group.
    pub async fn create_group(&self, group: &Group) -> Result<Group, APIError> {
        // Build the request.
        let request = self.request(
            DIRECTORY_ENDPOINT,
            Method::POST,
            "groups",
            group,
            None,
        );

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                });
            }
        };

        // Try to deserialize the response.
        Ok(resp.json().await.unwrap())
    }

    /// Update a Google group's aliases.
    pub async fn update_group_aliases<A>(&self, group_key: &str, aliases: A)
    where
        A: IntoIterator,
        A::Item: AsRef<str>,
    {
        for alias in aliases {
            self.update_group_alias(group_key, alias.as_ref())
                .await
                .unwrap();
        }
    }

    /// Update an alias for a Google group.
    pub async fn update_group_alias(
        &self,
        group_key: &str,
        alias: &str,
    ) -> Result<(), APIError> {
        let mut a: HashMap<&str, &str> = HashMap::new();
        a.insert("alias", alias);
        // Build the request.
        let request = self.request(
            DIRECTORY_ENDPOINT,
            Method::POST,
            &format!("groups/{}/aliases", group_key),
            a,
            None,
        );

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                let body = resp.text().await.unwrap();

                if body.contains("duplicate") {
                    // Ignore the error because we don't care about if it is a duplicate.
                    return Ok(());
                }

                return Err(APIError {
                    status_code: s,
                    body,
                });
            }
        };

        Ok(())
    }

    /// Check if a user is a member of a Google group.
    pub async fn group_has_member(
        &self,
        group_id: &str,
        email: &str,
    ) -> Result<bool, APIError> {
        // Build the request.
        let request = self.request(
            DIRECTORY_ENDPOINT,
            Method::GET,
            &format!("groups/{}/hasMember/{}", group_id, email),
            (),
            None,
        );

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                });
            }
        };

        // Try to deserialize the response.
        let value: MembersHasMember = resp.json().await.unwrap();

        Ok(value.is_member.unwrap())
    }

    /// Update a member of a Google group.
    pub async fn group_update_member(
        &self,
        group_id: &str,
        email: &str,
        role: &str,
    ) -> Result<(), APIError> {
        let mut member: Member = Default::default();
        member.role = Some(role.to_string());
        member.email = Some(email.to_string());
        member.delivery_settings = Some("ALL_MAIL".to_string());

        // Build the request.
        let request = self.request(
            DIRECTORY_ENDPOINT,
            Method::PUT,
            &format!("groups/{}/members/{}", group_id, email),
            member,
            None,
        );

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                });
            }
        };

        Ok(())
    }

    /// Add a user as a member of a Google group.
    pub async fn group_insert_member(
        &self,
        group_id: &str,
        email: &str,
        role: &str,
    ) -> Result<(), APIError> {
        let mut member: Member = Default::default();
        member.role = Some(role.to_string());
        member.email = Some(email.to_string());
        member.delivery_settings = Some("ALL_MAIL".to_string());

        // Build the request.
        let request = self.request(
            DIRECTORY_ENDPOINT,
            Method::POST,
            &format!("groups/{}/members", group_id),
            member,
            None,
        );

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                });
            }
        };

        Ok(())
    }

    /// Remove a user as a member of a Google group.
    pub async fn group_remove_member(
        &self,
        group_id: &str,
        email: &str,
    ) -> Result<(), APIError> {
        // Build the request.
        let request = self.request(
            DIRECTORY_ENDPOINT,
            Method::DELETE,
            &format!("groups/{}/members/{}", group_id, email),
            (),
            None,
        );

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                });
            }
        };

        Ok(())
    }

    /// List users.
    pub async fn list_users(&self) -> Result<Vec<User>, APIError> {
        // Build the request.
        let request = self.request(
            DIRECTORY_ENDPOINT,
            Method::GET,
            "users",
            (),
            Some(&[
                ("customer", &self.customer),
                ("domain", &self.domain),
                ("projection", "full"),
            ]),
        );

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                });
            }
        };

        // Try to deserialize the response.
        let value: Users = resp.json().await.unwrap();

        Ok(value.users)
    }

    /// Update a user.
    pub async fn update_user(&self, user: &User) -> Result<(), APIError> {
        // Build the request.
        let request = self.request(
            DIRECTORY_ENDPOINT,
            Method::PUT,
            &format!("users/{}", user.id),
            user,
            None,
        );

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                });
            }
        };

        Ok(())
    }

    /// Create a user.
    pub async fn create_user(&self, user: &User) -> Result<User, APIError> {
        // Build the request.
        let request =
            self.request(DIRECTORY_ENDPOINT, Method::POST, "users", user, None);

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                });
            }
        };

        // Try to deserialize the response.
        Ok(resp.json().await.unwrap())
    }

    /// Update a user's aliases.
    pub async fn update_user_aliases<A>(&self, user_id: &str, aliases: A)
    where
        A: IntoIterator,
        A::Item: AsRef<str>,
    {
        for alias in aliases {
            self.update_user_alias(user_id, alias.as_ref())
                .await
                .unwrap();
        }
    }

    /// Update an alias for a user.
    pub async fn update_user_alias(
        &self,
        user_id: &str,
        alias: &str,
    ) -> Result<(), APIError> {
        let mut a: HashMap<&str, &str> = HashMap::new();
        a.insert("alias", alias);
        // Build the request.
        let request = self.request(
            DIRECTORY_ENDPOINT,
            Method::POST,
            &format!("users/{}/aliases", user_id),
            a,
            None,
        );

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                let body = resp.text().await.unwrap();

                if body.contains("duplicate") {
                    // Ignore the error because we don't care about if it is a duplicate.
                    return Ok(());
                }

                return Err(APIError {
                    status_code: s,
                    body,
                });
            }
        };

        Ok(())
    }

    /// List calendar resources.
    pub async fn list_calendar_resources(
        &self,
    ) -> Result<Vec<CalendarResource>, APIError> {
        // Build the request.
        let request = self.request(
            DIRECTORY_ENDPOINT,
            Method::GET,
            &format!("customer/{}/resources/calendars", self.customer),
            (),
            None,
        );

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                });
            }
        };

        // Try to deserialize the response.
        let value: CalendarResources = resp.json().await.unwrap();

        Ok(value.items.unwrap())
    }

    /// Update a calendar resource.
    pub async fn update_calendar_resource(
        &self,
        resource: &CalendarResource,
    ) -> Result<(), APIError> {
        // Build the request.
        let request = self.request(
            DIRECTORY_ENDPOINT,
            Method::PUT,
            &format!(
                "customer/{}/resources/calendars/{}",
                self.customer, resource.id
            ),
            resource,
            None,
        );

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                });
            }
        };

        Ok(())
    }

    /// Create a calendar resource.
    pub async fn create_calendar_resource(
        &self,
        resource: &CalendarResource,
    ) -> Result<(), APIError> {
        // Build the request.
        let request = self.request(
            DIRECTORY_ENDPOINT,
            Method::POST,
            &format!("customer/{}/resources/calendars", self.customer),
            resource,
            None,
        );

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                });
            }
        };

        Ok(())
    }

    /// List buildings.
    pub async fn list_buildings(&self) -> Result<Vec<Building>, APIError> {
        // Build the request.
        let request = self.request(
            DIRECTORY_ENDPOINT,
            Method::GET,
            &format!("customer/{}/resources/buildings", self.customer),
            (),
            None,
        );

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                });
            }
        };

        // Try to deserialize the response.
        let value: Buildings = resp.json().await.unwrap();

        Ok(value.buildings.unwrap())
    }

    /// Update a building.
    pub async fn update_building(
        &self,
        building: &Building,
    ) -> Result<(), APIError> {
        // Build the request.
        let request = self.request(
            DIRECTORY_ENDPOINT,
            Method::PUT,
            &format!(
                "customer/{}/resources/buildings/{}",
                self.customer, building.id
            ),
            building,
            None,
        );

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                });
            }
        };

        Ok(())
    }

    /// Create a building.
    pub async fn create_building(
        &self,
        building: &Building,
    ) -> Result<(), APIError> {
        // Build the request.
        let request = self.request(
            DIRECTORY_ENDPOINT,
            Method::POST,
            &format!("customer/{}/resources/buildings", self.customer),
            building,
            None,
        );

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                });
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
        write!(
            f,
            "APIError: status code -> {}, body -> {}",
            self.status_code.to_string(),
            self.body
        )
    }
}

impl fmt::Debug for APIError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "APIError: status code -> {}, body -> {}",
            self.status_code.to_string(),
            self.body
        )
    }
}

// This is important for other errors to wrap this one.
impl error::Error for APIError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        // Generic error, underlying cause isn't tracked.
        None
    }
}

/// Generate a random string that we can use as a temporary password for new users
/// when we set up their account.
pub fn generate_password() -> String {
    thread_rng().sample_iter(&Alphanumeric).take(30).collect()
}

/// A Google group.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct Group {
    /// List of non editable aliases (Read-only)
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "nonEditableAliases"
    )]
    pub non_editable_aliases: Option<Vec<String>>,
    /// Kind of resource this is.
    pub kind: Option<String>,
    /// Description of the group
    pub description: Option<String>,
    /// Is the group created by admin (Read-only) *
    #[serde(skip_serializing_if = "Option::is_none", rename = "adminCreated")]
    pub admin_created: Option<bool>,
    /// Group direct members count
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "directMembersCount"
    )]
    pub direct_members_count: Option<String>,
    /// Email of Group
    pub email: Option<String>,
    /// ETag of the resource.
    pub etag: Option<String>,
    /// List of aliases (Read-only)
    pub aliases: Option<Vec<String>>,
    /// Unique identifier of Group (Read-only)
    pub id: Option<String>,
    /// Group name
    pub name: Option<String>,
}

/// A Google group's settings.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct GroupSettings {
    /// Permission to ban users. Possible values are: NONE OWNERS_ONLY OWNERS_AND_MANAGERS ALL_MEMBERS
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "whoCanBanUsers"
    )]
    pub who_can_ban_users: Option<String>,
    /// Permission for content assistants. Possible values are: Possible values are: NONE OWNERS_ONLY MANAGERS_ONLY OWNERS_AND_MANAGERS ALL_MEMBERS
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "whoCanAssistContent"
    )]
    pub who_can_assist_content: Option<String>,
    /// Are external members allowed to join the group.
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "allowExternalMembers"
    )]
    pub allow_external_members: Option<String>,
    /// Permission to enter free form tags for topics in a forum. Possible values are: NONE OWNERS_ONLY MANAGERS_ONLY OWNERS_AND_MANAGERS ALL_MEMBERS
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "whoCanEnterFreeFormTags"
    )]
    pub who_can_enter_free_form_tags: Option<String>,
    /// Permission to approve pending messages in the moderation queue. Possible values are: NONE OWNERS_ONLY OWNERS_AND_MANAGERS ALL_MEMBERS
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "whoCanApproveMessages"
    )]
    pub who_can_approve_messages: Option<String>,
    /// Permission to mark a topic as a duplicate of another topic. Possible values are: NONE OWNERS_ONLY MANAGERS_ONLY OWNERS_AND_MANAGERS ALL_MEMBERS
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "whoCanMarkDuplicate"
    )]
    pub who_can_mark_duplicate: Option<String>,
    /// Permissions to join the group. Possible values are: ANYONE_CAN_JOIN ALL_IN_DOMAIN_CAN_JOIN INVITED_CAN_JOIN CAN_REQUEST_TO_JOIN
    #[serde(skip_serializing_if = "Option::is_none", rename = "whoCanJoin")]
    pub who_can_join: Option<String>,
    /// Permission to change tags and categories. Possible values are: NONE OWNERS_ONLY MANAGERS_ONLY OWNERS_AND_MANAGERS ALL_MEMBERS
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "whoCanModifyTagsAndCategories"
    )]
    pub who_can_modify_tags_and_categories: Option<String>,
    /// Permission to mark a topic as not needing a response. Possible values are: NONE OWNERS_ONLY MANAGERS_ONLY OWNERS_AND_MANAGERS ALL_MEMBERS
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "whoCanMarkNoResponseNeeded"
    )]
    pub who_can_mark_no_response_needed: Option<String>,
    /// Permission to unmark any post from a favorite reply. Possible values are: NONE OWNERS_ONLY MANAGERS_ONLY OWNERS_AND_MANAGERS ALL_MEMBERS
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "whoCanUnmarkFavoriteReplyOnAnyTopic"
    )]
    pub who_can_unmark_favorite_reply_on_any_topic: Option<String>,
    /// Permission for content moderation. Possible values are: NONE OWNERS_ONLY OWNERS_AND_MANAGERS ALL_MEMBERS
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "whoCanModerateContent"
    )]
    pub who_can_moderate_content: Option<String>,
    /// Primary language for the group.
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "primaryLanguage"
    )]
    pub primary_language: Option<String>,
    /// Permission to mark a post for a topic they started as a favorite reply. Possible values are: NONE OWNERS_ONLY MANAGERS_ONLY OWNERS_AND_MANAGERS ALL_MEMBERS
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "whoCanMarkFavoriteReplyOnOwnTopic"
    )]
    pub who_can_mark_favorite_reply_on_own_topic: Option<String>,
    /// Permissions to view membership. Possible values are: ALL_IN_DOMAIN_CAN_VIEW ALL_MEMBERS_CAN_VIEW ALL_MANAGERS_CAN_VIEW ALL_OWNERS_CAN_VIEW
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "whoCanViewMembership"
    )]
    pub who_can_view_membership: Option<String>,
    /// If favorite replies should be displayed above other replies.
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "favoriteRepliesOnTop"
    )]
    pub favorite_replies_on_top: Option<String>,
    /// Permission to mark any other user's post as a favorite reply. Possible values are: NONE OWNERS_ONLY MANAGERS_ONLY OWNERS_AND_MANAGERS ALL_MEMBERS
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "whoCanMarkFavoriteReplyOnAnyTopic"
    )]
    pub who_can_mark_favorite_reply_on_any_topic: Option<String>,
    /// Whether to include custom footer.
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "includeCustomFooter"
    )]
    pub include_custom_footer: Option<String>,
    /// Permission to move topics out of the group or forum. Possible values are: NONE OWNERS_ONLY OWNERS_AND_MANAGERS ALL_MEMBERS
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "whoCanMoveTopicsOut"
    )]
    pub who_can_move_topics_out: Option<String>,
    /// Default message deny notification message
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "defaultMessageDenyNotificationText"
    )]
    pub default_message_deny_notification_text: Option<String>,
    /// If this groups should be included in global address list or not.
    #[serde(default, rename = "includeInGlobalAddressList")]
    pub include_in_global_address_list: bool,
    /// If the group is archive only
    #[serde(skip_serializing_if = "Option::is_none", rename = "archiveOnly")]
    pub archive_only: Option<String>,
    /// Permission to delete topics. Possible values are: NONE OWNERS_ONLY OWNERS_AND_MANAGERS ALL_MEMBERS
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "whoCanDeleteTopics"
    )]
    pub who_can_delete_topics: Option<String>,
    /// Permission to delete replies to topics. Possible values are: NONE OWNERS_ONLY OWNERS_AND_MANAGERS ALL_MEMBERS
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "whoCanDeleteAnyPost"
    )]
    pub who_can_delete_any_post: Option<String>,
    /// If the contents of the group are archived.
    #[serde(skip_serializing_if = "Option::is_none", rename = "isArchived")]
    pub is_archived: Option<String>,
    /// Can members post using the group email address.
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "membersCanPostAsTheGroup"
    )]
    pub members_can_post_as_the_group: Option<String>,
    /// Permission to make topics appear at the top of the topic list. Possible values are: NONE OWNERS_ONLY MANAGERS_ONLY OWNERS_AND_MANAGERS ALL_MEMBERS
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "whoCanMakeTopicsSticky"
    )]
    pub who_can_make_topics_sticky: Option<String>,
    /// If any of the settings that will be merged have custom roles which is anything other than owners, managers, or group scopes.
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "customRolesEnabledForSettingsToBeMerged"
    )]
    pub custom_roles_enabled_for_settings_to_be_merged: Option<String>,
    /// Email id of the group
    pub email: Option<String>,
    /// Permission for who can discover the group. Possible values are: ALL_MEMBERS_CAN_DISCOVER ALL_IN_DOMAIN_CAN_DISCOVER ANYONE_CAN_DISCOVER
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "whoCanDiscoverGroup"
    )]
    pub who_can_discover_group: Option<String>,
    /// Permission to modify members (change member roles). Possible values are: NONE OWNERS_ONLY OWNERS_AND_MANAGERS ALL_MEMBERS
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "whoCanModifyMembers"
    )]
    pub who_can_modify_members: Option<String>,
    /// Moderation level for messages. Possible values are: MODERATE_ALL_MESSAGES MODERATE_NON_MEMBERS MODERATE_NEW_MEMBERS MODERATE_NONE
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "messageModerationLevel"
    )]
    pub message_moderation_level: Option<String>,
    /// Description of the group
    pub description: Option<String>,
    /// Permission to unassign any topic in a forum. Possible values are: NONE OWNERS_ONLY MANAGERS_ONLY OWNERS_AND_MANAGERS ALL_MEMBERS
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "whoCanUnassignTopic"
    )]
    pub who_can_unassign_topic: Option<String>,
    /// Whome should the default reply to a message go to. Possible values are: REPLY_TO_CUSTOM REPLY_TO_SENDER REPLY_TO_LIST REPLY_TO_OWNER REPLY_TO_IGNORE REPLY_TO_MANAGERS
    #[serde(skip_serializing_if = "Option::is_none", rename = "replyTo")]
    pub reply_to: Option<String>,
    /// Default email to which reply to any message should go.
    #[serde(skip_serializing_if = "Option::is_none", rename = "customReplyTo")]
    pub custom_reply_to: Option<String>,
    /// Should the member be notified if his message is denied by owner.
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "sendMessageDenyNotification"
    )]
    pub send_message_deny_notification: Option<String>,
    /// If a primary Collab Inbox feature is enabled.
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "enableCollaborativeInbox"
    )]
    pub enable_collaborative_inbox: Option<String>,
    /// Permission to contact owner of the group via web UI. Possible values are: ANYONE_CAN_CONTACT ALL_IN_DOMAIN_CAN_CONTACT ALL_MEMBERS_CAN_CONTACT ALL_MANAGERS_CAN_CONTACT
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "whoCanContactOwner"
    )]
    pub who_can_contact_owner: Option<String>,
    /// Default message display font. Possible values are: DEFAULT_FONT FIXED_WIDTH_FONT
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "messageDisplayFont"
    )]
    pub message_display_font: Option<String>,
    /// Permission to leave the group. Possible values are: ALL_MANAGERS_CAN_LEAVE ALL_OWNERS_CAN_LEAVE ALL_MEMBERS_CAN_LEAVE NONE_CAN_LEAVE
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "whoCanLeaveGroup"
    )]
    pub who_can_leave_group: Option<String>,
    /// Permissions to add members. Possible values are: ALL_MANAGERS_CAN_ADD ALL_OWNERS_CAN_ADD ALL_MEMBERS_CAN_ADD NONE_CAN_ADD
    #[serde(skip_serializing_if = "Option::is_none", rename = "whoCanAdd")]
    pub who_can_add: Option<String>,
    /// Permissions to post messages to the group. Possible values are: NONE_CAN_POST ALL_MANAGERS_CAN_POST ALL_MEMBERS_CAN_POST ALL_OWNERS_CAN_POST ALL_IN_DOMAIN_CAN_POST ANYONE_CAN_POST
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "whoCanPostMessage"
    )]
    pub who_can_post_message: Option<String>,
    /// Permission to move topics into the group or forum. Possible values are: NONE OWNERS_ONLY OWNERS_AND_MANAGERS ALL_MEMBERS
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "whoCanMoveTopicsIn"
    )]
    pub who_can_move_topics_in: Option<String>,
    /// Permission to take topics in a forum. Possible values are: NONE OWNERS_ONLY MANAGERS_ONLY OWNERS_AND_MANAGERS ALL_MEMBERS
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "whoCanTakeTopics"
    )]
    pub who_can_take_topics: Option<String>,
    /// Name of the Group
    pub name: Option<String>,
    /// The type of the resource.
    pub kind: Option<String>,
    /// Maximum message size allowed.
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "maxMessageBytes"
    )]
    pub max_message_bytes: Option<i32>,
    /// Permissions to invite members. Possible values are: ALL_MEMBERS_CAN_INVITE ALL_MANAGERS_CAN_INVITE ALL_OWNERS_CAN_INVITE NONE_CAN_INVITE
    #[serde(skip_serializing_if = "Option::is_none", rename = "whoCanInvite")]
    pub who_can_invite: Option<String>,
    /// Permission to approve members. Possible values are: ALL_OWNERS_CAN_APPROVE ALL_MANAGERS_CAN_APPROVE ALL_MEMBERS_CAN_APPROVE NONE_CAN_APPROVE
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "whoCanApproveMembers"
    )]
    pub who_can_approve_members: Option<String>,
    /// Moderation level for messages detected as spam. Possible values are: ALLOW MODERATE SILENTLY_MODERATE REJECT
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "spamModerationLevel"
    )]
    pub spam_moderation_level: Option<String>,
    /// If posting from web is allowed.
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "allowWebPosting"
    )]
    pub allow_web_posting: Option<String>,
    /// Permission for membership moderation. Possible values are: NONE OWNERS_ONLY OWNERS_AND_MANAGERS ALL_MEMBERS
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "whoCanModerateMembers"
    )]
    pub who_can_moderate_members: Option<String>,
    /// Permission to add references to a topic. Possible values are: NONE OWNERS_ONLY MANAGERS_ONLY OWNERS_AND_MANAGERS ALL_MEMBERS
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "whoCanAddReferences"
    )]
    pub who_can_add_references: Option<String>,
    /// Permissions to view group. Possible values are: ANYONE_CAN_VIEW ALL_IN_DOMAIN_CAN_VIEW ALL_MEMBERS_CAN_VIEW ALL_MANAGERS_CAN_VIEW ALL_OWNERS_CAN_VIEW
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "whoCanViewGroup"
    )]
    pub who_can_view_group: Option<String>,
    /// Is the group listed in groups directory
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "showInGroupGSuite"
    )]
    pub show_in_group_directory: Option<String>,
    /// Permission to post announcements, a special topic type. Possible values are: NONE OWNERS_ONLY OWNERS_AND_MANAGERS ALL_MEMBERS
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "whoCanPostAnnouncements"
    )]
    pub who_can_post_announcements: Option<String>,
    /// Permission to lock topics. Possible values are: NONE OWNERS_ONLY OWNERS_AND_MANAGERS ALL_MEMBERS
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "whoCanLockTopics"
    )]
    pub who_can_lock_topics: Option<String>,
    /// Permission to assign topics in a forum to another user. Possible values are: NONE OWNERS_ONLY MANAGERS_ONLY OWNERS_AND_MANAGERS ALL_MEMBERS
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "whoCanAssignTopics"
    )]
    pub who_can_assign_topics: Option<String>,
    /// Custom footer text.
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "customFooterText"
    )]
    pub custom_footer_text: Option<String>,
    /// Is google allowed to contact admins.
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "allowGoogleCommunication"
    )]
    pub allow_google_communication: Option<String>,
    /// Permission to hide posts by reporting them as abuse. Possible values are: NONE OWNERS_ONLY MANAGERS_ONLY OWNERS_AND_MANAGERS ALL_MEMBERS
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "whoCanHideAbuse"
    )]
    pub who_can_hide_abuse: Option<String>,
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
struct Groups {
    /// Token used to access next page of this result.
    #[serde(skip_serializing_if = "Option::is_none", rename = "nextPageToken")]
    pub next_page_token: Option<String>,
    /// Kind of resource this is.
    pub kind: Option<String>,
    /// ETag of the resource.
    pub etag: Option<String>,
    /// List of group objects.
    pub groups: Option<Vec<Group>>,
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
struct MembersHasMember {
    /// Identifies whether the given user is a member of the group. Membership can be direct or nested.
    #[serde(skip_serializing_if = "Option::is_none", rename = "isMember")]
    pub is_member: Option<bool>,
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
struct Members {
    /// Token used to access next page of this result.
    #[serde(skip_serializing_if = "Option::is_none", rename = "nextPageToken")]
    pub next_page_token: Option<String>,
    /// Kind of resource this is.
    pub kind: Option<String>,
    /// ETag of the resource.
    pub etag: Option<String>,
    /// List of member objects.
    pub members: Option<Vec<Member>>,
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
struct Member {
    /// Status of member (Immutable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Kind of resource this is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Delivery settings of member
    pub delivery_settings: Option<String>,
    /// Email of member (Read-only)
    pub email: Option<String>,
    /// ETag of the resource.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
    /// Role of member
    pub role: Option<String>,
    /// Type of member (Immutable)
    #[serde(skip_serializing_if = "Option::is_none", rename = "type")]
    pub type_: Option<String>,
    /// Unique identifier of customer member (Read-only) Unique identifier of group (Read-only) Unique identifier of member (Read-only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

/// A user.
/// FROM: https://developers.google.com/admin-sdk/directory/v1/reference/users#resource
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct User {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub addresses: Vec<UserAddress>,
    /// Indicates if user has agreed to terms (read-only)
    #[serde(skip_serializing_if = "Option::is_none", rename = "agreedToTerms")]
    pub agreed_to_terms: Option<bool>,
    /// List of aliases (read-only)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    /// Indicates if user is archived (read-only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived: Option<bool>,
    /// Boolean indicating if the user should change password in next login
    #[serde(default, rename = "changePasswordAtNextLogin")]
    pub change_password_at_next_login: bool,
    /// User's G Suite account creation time (read-only)
    #[serde(skip_serializing_if = "Option::is_none", rename = "creationTime")]
    pub creation_time: Option<DateTime<Utc>>,
    /// Custom fields of the user
    #[serde(
        default,
        skip_serializing_if = "HashMap::is_empty",
        rename = "customSchemas"
    )]
    pub custom_schemas: HashMap<String, UserCustomProperties>,
    /// CustomerId of User (read-only)
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "customerId"
    )]
    pub customer_id: String,
    /// User's G Suite account deletion time (read-only)
    #[serde(skip_serializing_if = "Option::is_none", rename = "deletionTime")]
    pub deletion_time: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emails: Vec<UserEmail>,
    /// ETag of the resource (read-only)
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub etag: String,
    /// A list of external IDs for the user, such as an employee or network ID.
    /// The maximum allowed data size for this field is 2Kb.
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        rename = "externalIds"
    )]
    pub external_ids: Vec<UserExternalId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gender: Option<UserGender>,
    /// Hash function name for password. Supported are MD5, SHA-1 and crypt
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "hashFunction"
    )]
    pub hash_function: String,
    /// Unique identifier of User (read-only)
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    /// The user's Instant Messenger (IM) accounts. A user account can have
    /// multiple ims properties. But, only one of these ims properties can be
    /// the primary IM contact. The maximum allowed data size for this field is
    /// 2Kb.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ims: Vec<UserInstantMessenger>,
    /// Boolean indicating if user is included in Global Address List
    #[serde(default, rename = "includeInGlobalAddressList")]
    pub include_in_global_address_list: bool,
    /// Boolean indicating if ip is whitelisted
    #[serde(default, rename = "ipWhitelisted")]
    pub ip_whitelisted: bool,
    /// Boolean indicating if the user is admin (read-only)
    #[serde(skip_serializing_if = "Option::is_none", rename = "isAdmin")]
    pub is_admin: Option<bool>,
    /// Boolean indicating if the user is delegated admin (read-only)
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "isDelegatedAdmin"
    )]
    pub is_delegated_admin: Option<bool>,
    /// Is 2-step verification enforced (read-only)
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "isEnforcedIn2Sv"
    )]
    pub is_enforced_in2_sv: Option<bool>,
    /// Is enrolled in 2-step verification (read-only)
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "isEnrolledIn2Sv"
    )]
    pub is_enrolled_in2_sv: Option<bool>,
    /// Is mailbox setup (read-only)
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "isMailboxSetup"
    )]
    pub is_mailbox_setup: Option<bool>,
    /// The user's keywords. The maximum allowed data size for this field is 1Kb.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<UserKeyword>,
    /// Kind of resource this is (read-only)
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub languages: Vec<UserLanguage>,
    /// User's last login time (read-only)
    #[serde(skip_serializing_if = "Option::is_none", rename = "lastLoginTime")]
    pub last_login_time: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub locations: Vec<UserLocation>,
    /// User's name
    pub name: UserName,
    /// List of non editable aliases (read-only)
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        rename = "nonEditableAliases"
    )]
    pub non_editable_aliases: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<UserNotes>,
    /// OrgUnit of User
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "orgUnitPath"
    )]
    pub org_unit_path: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub organizations: Vec<Organization>,
    /// User's password
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub password: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub phones: Vec<UserPhone>,
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        rename = "posixAccounts"
    )]
    pub posix_accounts: Vec<UserPosixAccount>,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "primaryEmail"
    )]
    pub primary_email: String,
    /// Recovery email of the user
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "recoveryEmail"
    )]
    pub recovery_email: String,
    /// Recovery phone of the user
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "recoveryPhone"
    )]
    pub recovery_phone: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relations: Vec<UserRelation>,
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        rename = "sshPublicKeys"
    )]
    pub ssh_public_keys: Vec<UserSSHKey>,
    /// Indicates if user is suspended
    #[serde(default)]
    pub suspended: bool,
    /// Suspension reason if user is suspended (read-only)
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "suspensionReason"
    )]
    pub suspension_reason: String,
    /// ETag of the user's photo (read-only)
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "thumbnailPhotoEtag"
    )]
    pub thumbnail_photo_etag: String,
    /// Photo Url of the user (read-only)
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "thumbnailPhotoUrl"
    )]
    pub thumbnail_photo_url: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub websites: Vec<UserWebsite>,
}

impl User {
    /// Update a user.
    pub async fn update(
        mut self,
        user: &UserConfig,
        domain: &str,
        change_password: bool,
    ) -> User {
        // TODO(cbiffle): use &mut self instead of consume-and-return
        // Set the settings for the user.
        self.name = UserName {
            full_name: format!("{} {}", user.first_name, user.last_name),
            given_name: user.first_name.to_string(),
            family_name: user.last_name.to_string(),
        };

        if !user.recovery_email.is_empty() {
            // Set the recovery email for the user.
            self.recovery_email = user.recovery_email.to_string();

            // Check if we have a home email set for the user and update it.
            let mut has_home_email = false;
            for (index, email) in self.emails.iter().enumerate() {
                if email.typev == "home" {
                    // Update the set home email.
                    self.emails[index].address =
                        user.recovery_email.to_string();
                    // Break the loop early.
                    has_home_email = true;
                    break;
                }
            }

            if !has_home_email {
                // Set the home email for the user.
                self.emails.push(UserEmail {
                    custom_type: "".to_string(),
                    typev: "home".to_string(),
                    address: user.recovery_email.to_string(),
                    primary: false,
                });
            }
        }

        if !user.recovery_phone.is_empty() {
            // Set the recovery phone for the user.
            self.recovery_phone = user.recovery_phone.to_string();

            // Set the home phone for the user.
            self.phones = vec![UserPhone {
                custom_type: "".to_string(),
                typev: "home".to_string(),
                value: user.recovery_phone.to_string(),
                primary: true,
            }];
        }

        self.primary_email = format!("{}@{}", user.username, domain);

        if change_password {
            // Since we are creating a new user, we want to change their password
            // at the next login.
            self.change_password_at_next_login = true;
            // Generate a password for the user.
            let password = generate_password();
            self.password = password;
        }

        // Set the user's address if we have one.
        if !user.home_address_street_1.is_empty() {
            let mut street_address = user.home_address_street_1.to_string();
            if !user.home_address_street_2.is_empty() {
                street_address = format!(
                    "{}\n{}",
                    user.home_address_street_1, user.home_address_street_2,
                );
            }
            let formatted = format!(
                "{}\n{} {}, {}, {}",
                street_address,
                user.home_address_city,
                user.home_address_state,
                user.home_address_zip,
                user.home_address_country
            );

            self.addresses = vec![UserAddress {
                country: user.home_address_country.to_string(),
                // TODO: fix this when we have an employee from another country.
                country_code: "US".to_string(),
                custom_type: "".to_string(),
                extended_address: "".to_string(),
                formatted,
                locality: user.home_address_city.to_string(),
                po_box: "".to_string(),
                postal_code: user.home_address_zip.to_string(),
                primary: true,
                region: user.home_address_state.to_string(),
                // Indicates if the user-supplied address was formatted. Formatted addresses are
                // not currently supported.
                // FROM: https://developers.google.com/admin-sdk/directory/v1/reference/users#resource
                // TODO: figure out when this is supported and what it means
                source_is_structured: false,
                street_address,
                typev: "home".to_string(),
            }];
        }

        // Include the user in the global address list
        self.include_in_global_address_list = true;

        if !user.gender.is_empty() {
            self.gender = Some(UserGender {
                address_me_as: "".to_string(),
                custom_gender: "".to_string(),
                typev: user.gender.to_string(),
            });
        }

        if !user.building.is_empty() {
            self.locations = vec![UserLocation {
                area: "".to_string(),
                building_id: user.building.to_string(),
                custom_type: "".to_string(),
                desk_code: "".to_string(),
                floor_name: "1".to_string(),
                floor_section: "".to_string(),
                typev: "desk".to_string(),
            }];
        }

        // Set their GitHub SSH Keys to their Google SSH Keys.
        for k in &user.public_ssh_keys {
            self.ssh_public_keys.push(UserSSHKey {
                key: k.to_string(),
                expiration_time_usec: None,
                // fingerprint is a read-only property so make sure it is empty
                fingerprint: "".to_string(),
            });
        }

        // Set the IM field for matrix.
        // TODO: once we migrate to slack update or add to this.
        if !user.chat.is_empty() {
            self.ims = vec![UserInstantMessenger {
                custom_protocol: "matrix".to_string(),
                custom_type: "".to_string(),
                im: user.chat.to_string(),
                primary: true,
                protocol: "custom_protocol".to_string(),
                typev: "work".to_string(),
            }];
        }

        // Set the custom schemas.
        self.custom_schemas = HashMap::new();
        let mut contact: HashMap<String, Value> = HashMap::new();
        contact.insert("Start_Date".to_string(), json!(user.start_date));

        // Set the GitHub username.
        if !user.github.is_empty() {
            contact.insert(
                "GitHub_Username".to_string(),
                json!(user.github.to_string()),
            );
        }
        self.custom_schemas
            .insert("Contact".to_string(), UserCustomProperties(Some(contact)));

        // Get the AWS Role information.
        if !user.aws_role.is_empty() {
            let mut aws_role: HashMap<String, Value> = HashMap::new();
            let mut aws_type: HashMap<String, String> = HashMap::new();
            aws_type.insert("type".to_string(), "work".to_string());
            aws_type.insert("value".to_string(), user.aws_role.to_string());
            aws_role.insert("Role".to_string(), json!(vec![aws_type]));
            self.custom_schemas.insert(
                "Amazon_Web_Services".to_string(),
                UserCustomProperties(Some(aws_role)),
            );
        }

        self
    }
}

/// A user's address.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct UserAddress {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub country: String,
    /// The country code. Uses the ISO 3166-1 standard.
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "countryCode"
    )]
    pub country_code: String,
    /// If the value of type is custom, this property contains the custom type string.
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "customType"
    )]
    pub custom_type: String,
    /// For extended addresses, such as an address that includes a sub-region.
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "extendedAddress"
    )]
    pub extended_address: String,
    /// A full and unstructured postal address. This is not synced with the structured address fields.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub formatted: String,
    /// The town or city of the address.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub locality: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "poBox"
    )]
    pub po_box: String,
    /// The ZIP or postal code, if applicable.
    #[serde(
        default,
        rename = "postalCode",
        skip_serializing_if = "String::is_empty"
    )]
    pub postal_code: String,
    #[serde(default)]
    pub primary: bool,
    /// The abbreviated province or state.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub region: String,
    /// Indicates if the user-supplied address was formatted. Formatted addresses are not currently supported.
    #[serde(default, rename = "sourceIsStructured")]
    pub source_is_structured: bool,
    /// The street address, such as 1600 Amphitheatre Parkway.
    /// Whitespace within the string is ignored; however, newlines are significant.
    #[serde(
        default,
        rename = "street_address",
        skip_serializing_if = "String::is_empty"
    )]
    pub street_address: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "type"
    )]
    pub typev: String,
}

/// A user's email.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct UserEmail {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub address: String,
    /// If the value of type is custom, this property contains the custom type string.
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "customType"
    )]
    pub custom_type: String,
    #[serde(default)]
    pub primary: bool,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "type"
    )]
    pub typev: String,
}

/// A user's external id.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct UserExternalId {
    /// If the value of type is custom, this property contains the custom type string.
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "customType"
    )]
    pub custom_type: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "type"
    )]
    pub typev: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub value: String,
}

/// A user's gender.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct UserGender {
    #[serde(
        default,
        rename = "addressMeAs",
        skip_serializing_if = "String::is_empty"
    )]
    pub address_me_as: String,
    #[serde(
        default,
        rename = "customGender",
        skip_serializing_if = "String::is_empty"
    )]
    pub custom_gender: String,
    #[serde(
        default,
        rename = "type",
        skip_serializing_if = "String::is_empty"
    )]
    pub typev: String,
}

/// A user's instant messanger.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct UserInstantMessenger {
    /// If the protocol value is custom_protocol, this property holds the custom protocol's string.
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "customProtocol"
    )]
    pub custom_protocol: String,
    /// If the value of type is custom, this property contains the custom type string.
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "customType"
    )]
    pub custom_type: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub im: String,
    pub primary: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub protocol: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "type"
    )]
    pub typev: String,
}

/// A user's keyword.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct UserKeyword {
    /// If the value of type is custom, this property contains the custom type string.
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "customType"
    )]
    pub custom_type: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "type"
    )]
    pub typev: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub value: String,
}

/// A user's language.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct UserLanguage {
    /// Other language. A user can provide their own language name if there is no corresponding
    /// Google III language code. If this is set, LanguageCode can't be set
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "customLanguage"
    )]
    pub custom_language: String,
    /// Language Code. Should be used for storing Google III LanguageCode string representation for language. Illegal values cause SchemaException.
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "languageCode"
    )]
    pub language_code: String,
}

/// A user's location.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct UserLocation {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub area: String,
    /// Unique ID for the building a resource is located in.
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "buildingId"
    )]
    pub building_id: String,
    /// If the value of type is custom, this property contains the custom type string.
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "customType"
    )]
    pub custom_type: String,
    /// Most specific textual code of individual desk location.
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "deskCode"
    )]
    pub desk_code: String,
    /// Name of the floor a resource is located on.
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "floorName"
    )]
    pub floor_name: String,
    /// Floor section. More specific location within the floor. For example, if a floor is divided into sections "A", "B", and "C", this field would identify one of those values.
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "floorSection"
    )]
    pub floor_section: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "type"
    )]
    pub typev: String,
}

/// A user's name.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct UserName {
    /// Last name
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "familyName"
    )]
    pub family_name: String,
    /// Full name
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "fullName"
    )]
    pub full_name: String,
    /// First name
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "givenName"
    )]
    pub given_name: String,
}

/// A user's notes.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct UserNotes {
    #[serde(
        default,
        rename = "contentType",
        skip_serializing_if = "String::is_empty"
    )]
    pub content_type: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub value: String,
}

/// An organization
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct Organization {
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "costCenter"
    )]
    pub cost_center: String,
    /// If the value of type is custom, this property contains the custom type string.
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "customType"
    )]
    pub custom_type: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub department: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub domain: String,
    #[serde(default, rename = "fullTimeEquivalent")]
    pub full_time_equivalent: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub location: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    pub primary: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub symbol: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "type"
    )]
    pub typev: String,
}

/// A user's phone.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct UserPhone {
    /// If the value of type is custom, this property contains the custom type string.
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "customType"
    )]
    pub custom_type: String,
    pub primary: bool,
    #[serde(
        default,
        rename = "type",
        skip_serializing_if = "String::is_empty"
    )]
    pub typev: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub value: String,
}

/// A user's posix account.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct UserPosixAccount {
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "accountId"
    )]
    pub account_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub gecos: String,
    #[serde(default)]
    pub gid: isize,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "homeDirectory"
    )]
    pub home_directory: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "operatingSystemType"
    )]
    pub operating_system_type: String,
    #[serde(default)]
    pub primary: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub shell: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "systemId"
    )]
    pub system_id: String,
    #[serde(default)]
    pub uid: isize,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub username: String,
}

/// A user's relation.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct UserRelation {
    /// If the value of type is custom, this property contains the custom type string.
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "customType"
    )]
    pub custom_type: String,
    #[serde(
        default,
        rename = "type",
        skip_serializing_if = "String::is_empty"
    )]
    pub typev: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub value: String,
}

/// A user's ssh key.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct UserSSHKey {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub key: String,
    /// An expiration time in microseconds since epoch.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "expirationTimeUsec"
    )]
    pub expiration_time_usec: Option<i128>,
    /// A SHA-256 fingerprint of the SSH public key (read-only)
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub fingerprint: String,
}

/// A user's website.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct UserWebsite {
    /// If the value of type is custom, this property contains the custom type string.
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "customType"
    )]
    pub custom_type: String,
    #[serde(default, alias = "is_group_admin")]
    pub primary: bool,
    #[serde(
        default,
        rename = "type",
        skip_serializing_if = "String::is_empty"
    )]
    pub typev: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub value: String,
}

/// Custom properties for a user.
#[derive(Default, Clone, Debug, Deserialize)]
pub struct UserCustomProperties(pub Option<HashMap<String, Value>>);

impl Serialize for UserCustomProperties {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let ucp = self.0.as_ref().unwrap();
        let mut map = s.serialize_map(Some(ucp.len())).unwrap();
        for (k, v) in ucp {
            if v.is_string() {
                map.serialize_entry(&k, v.as_str().unwrap()).unwrap();
            } else if v.is_array() {
                let val: Vec<HashMap<String, String>> =
                    serde_json::from_str(&v.to_string()).unwrap();
                map.serialize_entry(&k, &val).unwrap();
            }
        }
        map.end()
    }
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
struct Users {
    /// Token used to access next page of this result.
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "nextPageToken"
    )]
    pub next_page_token: String,
    /// Kind of resource this is.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub kind: String,
    /// ETag of the resource.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub etag: String,
    /// Event that triggered this response (only used in case of Push Response)
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub trigger_event: String,
    /// List of user objects.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub users: Vec<User>,
}

/// A calendar resource.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct CalendarResource {
    /// The type of the resource. For calendar resources, the value is admin#directory#resources#calendars#CalendarResource.
    pub kind: Option<String>,
    /// Capacity of a resource, number of seats in a room.
    pub capacity: Option<i32>,
    /// The type of the calendar resource, intended for non-room resources.
    #[serde(skip_serializing_if = "Option::is_none", rename = "resourceType")]
    pub typev: Option<String>,
    /// Description of the resource, visible only to admins.
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "resourceDescription"
    )]
    pub description: Option<String>,
    /// The read-only auto-generated name of the calendar resource which includes metadata about the resource such as building name, floor, capacity, etc. For example, "NYC-2-Training Room 1A (16)".
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "generatedResourceName"
    )]
    pub generated_resource_name: Option<String>,
    /// ETag of the resource.
    pub etags: Option<String>,
    /// The category of the calendar resource. Either CONFERENCE_ROOM or OTHER. Legacy data is set to CATEGORY_UNKNOWN.
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "resourceCategory"
    )]
    pub category: Option<String>,
    /// The read-only email for the calendar resource. Generated as part of creating a new calendar resource.
    #[serde(skip_serializing_if = "Option::is_none", rename = "resourceEmail")]
    pub email: Option<String>,
    /// The name of the calendar resource. For example, "Training Room 1A".
    #[serde(rename = "resourceName")]
    pub name: String,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "featureInstances"
    )]
    pub feature_instances: Option<Vec<CalendarFeatures>>,
    /// Name of the section within a floor a resource is located in.
    #[serde(skip_serializing_if = "Option::is_none", rename = "floorSection")]
    pub floor_section: Option<String>,
    /// The unique ID for the calendar resource.
    #[serde(rename = "resourceId")]
    pub id: String,
    /// Unique ID for the building a resource is located in.
    #[serde(skip_serializing_if = "Option::is_none", rename = "buildingId")]
    pub building_id: Option<String>,
    /// Name of the floor a resource is located on.
    #[serde(skip_serializing_if = "Option::is_none", rename = "floorName")]
    pub floor_name: Option<String>,
    /// Description of the resource, visible to users and admins.
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "userVisibleDescription"
    )]
    pub user_visible_description: Option<String>,
}

impl CalendarResource {
    /// Update a calendar resource.
    pub fn update(
        mut self,
        resource: &ResourceConfig,
        id: &str,
    ) -> CalendarResource {
        // TODO(cbiffle): the consume-and-return self pattern here complicates
        // things; use &mut self
        self.id = id.to_string();
        self.typev = Some(resource.typev.clone());
        self.name = resource.name.clone();
        self.building_id = Some(resource.building.clone());
        self.description = Some(resource.description.clone());
        self.user_visible_description = Some(resource.description.clone());
        self.capacity = Some(resource.capacity);
        self.floor_name = Some(resource.floor.clone());
        self.floor_section = Some(resource.section.clone());
        self.category = Some("CONFERENCE_ROOM".to_string());

        self
    }
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
struct CalendarResources {
    /// The continuation token, used to page through large result sets. Provide this value in a subsequent request to return the next page of results.
    #[serde(rename = "nextPageToken")]
    pub next_page_token: Option<String>,
    /// The CalendarResources in this page of results.
    pub items: Option<Vec<CalendarResource>>,
    /// Identifies this as a collection of CalendarResources. This is always admin#directory#resources#calendars#calendarResourcesList.
    pub kind: Option<String>,
    /// ETag of the resource.
    pub etag: Option<String>,
}

/// A feature of a calendar.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct CalendarFeature {
    /// The continuation token, used to page through large result sets. Provide this value in a subsequent request to return the next page of results.
    pub name: Option<String>,
    /// Identifies this as a collection of CalendarFeatures. This is always admin#directory#resources#calendars#calendarFeaturesList.
    pub kind: Option<String>,
    /// ETag of the resource.
    pub etags: Option<String>,
}

/// A calendar's features.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct CalendarFeatures {
    pub feature: Option<CalendarFeature>,
}

/// A building.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct Building {
    /// Kind of resource this is.
    pub kind: Option<String>,
    /// The building name as seen by users in Calendar. Must be unique for the customer. For example, "NYC-CHEL". The maximum length is 100 characters.
    #[serde(rename = "buildingName")]
    pub name: String,
    /// The geographic coordinates of the center of the building, expressed as latitude and longitude in decimal degrees.
    pub coordinates: Option<BuildingCoordinates>,
    /// ETag of the resource.
    pub etags: Option<String>,
    /// The postal address of the building. See PostalAddress for details. Note that only a single address line and region code are required.
    pub address: Option<BuildingAddress>,
    /// The display names for all floors in this building. The floors are expected to be sorted in ascending order, from lowest floor to highest floor. For example, ["B2", "B1", "L", "1", "2", "2M", "3", "PH"] Must contain at least one entry.
    #[serde(rename = "floorNames")]
    pub floor_names: Option<Vec<String>>,
    /// Unique identifier for the building. The maximum length is 100 characters.
    #[serde(rename = "buildingId")]
    pub id: String,
    /// A brief description of the building. For example, "Chelsea Market".
    pub description: Option<String>,
}

impl Building {
    /// Update a building.
    pub fn update(mut self, building: &BuildingConfig, id: &str) -> Building {
        // TOOD(cbiffle): use &mut self instead of consume-and-return
        self.id = id.to_string();
        self.name = building.name.clone();
        self.description = Some(building.description.clone());
        self.address = Some(BuildingAddress {
            address_lines: Some(vec![building.address.clone()]),
            locality: Some(building.city.clone()),
            administrative_area: Some(building.state.clone()),
            postal_code: Some(building.zipcode.clone()),
            region_code: Some(building.country.clone()),
            language_code: Some("en".to_string()),
            sublocality: None,
        });
        self.floor_names = Some(building.floors.clone());

        self
    }
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
struct Buildings {
    /// The continuation token, used to page through large result sets. Provide this value in a subsequent request to return the next page of results.
    #[serde(rename = "nextPageToken")]
    pub next_page_token: Option<String>,
    /// The Buildings in this page of results.
    pub buildings: Option<Vec<Building>>,
    /// ETag of the resource.
    pub etag: Option<String>,
    /// Kind of resource this is.
    pub kind: Option<String>,
}

/// A building's coordinates.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct BuildingCoordinates {
    /// Latitude in decimal degrees.
    pub latitude: Option<f64>,
    /// Longitude in decimal degrees.
    pub longitude: Option<f64>,
}

/// A building's address.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct BuildingAddress {
    /// Optional. BCP-47 language code of the contents of this address (if known).
    #[serde(rename = "languageCode")]
    pub language_code: Option<String>,
    /// Optional. Highest administrative subdivision which is used for postal addresses of a country or region.
    #[serde(rename = "administrativeArea")]
    pub administrative_area: Option<String>,
    /// Required. CLDR region code of the country/region of the address.
    #[serde(rename = "regionCode")]
    pub region_code: Option<String>,
    /// Optional. Generally refers to the city/town portion of the address. Examples: US city, IT comune, UK post town. In regions of the world where localities are not well defined or do not fit into this structure well, leave locality empty and use addressLines.
    pub locality: Option<String>,
    /// Optional. Postal code of the address.
    #[serde(rename = "postalCode")]
    pub postal_code: Option<String>,
    /// Optional. Sublocality of the address.
    pub sublocality: Option<String>,
    /// Unstructured address lines describing the lower levels of an address.
    #[serde(rename = "addressLines")]
    pub address_lines: Option<Vec<String>>,
}
