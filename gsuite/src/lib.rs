/**
 * A rust library for interacting with the GSuite APIs.
 *
 * For more information, the GSuite Directory API is documented at [developers.google.com/admin-sdk/directory/v1/reference](https://developers.google.com/admin-sdk/directory/v1/reference) and the Google Groups settings API is documented at [developers.google.com/admin-sdk/groups-settings/v1/reference/groups](https://developers.google.com/admin-sdk/groups-settings/v1/reference/groups).
 */
use std::collections::HashMap;
use std::rc::Rc;

use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use reqwest::blocking::{Client, Request};
use reqwest::{header, Method, StatusCode, Url};
use serde::{Deserialize, Serialize};
use yup_oauth2::Token;

use cio::{BuildingConfig, ResourceConfig, UserConfig};

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

    token: Token,

    client: Rc<Client>,
}

impl GSuite {
    /// Create a new GSuite client struct. It takes a type that can convert into
    /// an &str (`String` or `Vec<u8>` for example). As long as the function is
    /// given a valid API Key and Secret your requests will work.
    pub fn new(customer: String, domain: String, token: Token) -> Self {
        let client = Client::builder().build();
        match client {
            Ok(c) => Self {
                customer: customer,
                domain: domain,
                token: token,
                client: Rc::new(c),
            },
            Err(e) => panic!("creating client failed: {:?}", e),
        }
    }

    /// Get the currently set authorization token.
    pub fn get_token(&self) -> &Token {
        &self.token
    }

    fn request<B>(
        &self,
        endpoint: &str,
        method: Method,
        path: String,
        body: B,
        query: Option<Vec<(&str, String)>>,
    ) -> Request
    where
        B: Serialize,
    {
        let base = Url::parse(endpoint).unwrap();
        let url = base.join(&path).unwrap();

        // Check if the token is expired and panic.
        if self.token.expired() {
            panic!("token is expired");
        }

        let bt = format!("Bearer {}", self.token.access_token);
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
        let request = rb.build().unwrap();

        return request;
    }

    /// List Google groups.
    pub fn list_groups(&self) -> Vec<Group> {
        // Build the request.
        let request = self.request(
            DIRECTORY_ENDPOINT,
            Method::GET,
            "groups".to_string(),
            {},
            Some(vec![
                ("customer", self.customer.to_string()),
                ("domain", self.domain.to_string()),
            ]),
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => panic!(
                "received response status: {:?}\nbody: {}",
                s,
                resp.text().unwrap()
            ),
        };

        // Try to deserialize the response.
        let value: Groups = resp.json().unwrap();

        return value.groups.unwrap();
    }

    /// Get the settings for a Google group.
    pub fn get_group_settings(&self, group_email: String) -> GroupSettings {
        // Build the request.
        let request = self.request(
            GROUPS_SETTINGS_ENDPOINT,
            Method::GET,
            group_email,
            {},
            Some(vec![("alt", "json".to_string())]),
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => panic!(
                "received response status: {:?}\nbody: {}",
                s,
                resp.text().unwrap()
            ),
        };

        // Try to deserialize the response.
        return resp.json().unwrap();
    }

    /// Update a Google group.
    pub fn update_group(&self, group: Group) {
        // Build the request.
        let request = self.request(
            DIRECTORY_ENDPOINT,
            Method::PUT,
            format!("groups/{}", group.clone().id.unwrap()),
            group,
            None,
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => panic!(
                "received response status: {:?}\nbody: {}",
                s,
                resp.text().unwrap()
            ),
        };
    }

    /// Update a Google group's settings.
    pub fn update_group_settings(&self, settings: GroupSettings) {
        // Build the request.
        let request = self.request(
            GROUPS_SETTINGS_ENDPOINT,
            Method::PUT,
            settings.clone().email.unwrap(),
            settings,
            Some(vec![("alt", "json".to_string())]),
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => panic!(
                "received response status: {:?}\nbody: {}",
                s,
                resp.text().unwrap()
            ),
        };
    }

    /// Create a google group.
    pub fn create_group(&self, group: Group) -> Group {
        // Build the request.
        let request = self.request(
            DIRECTORY_ENDPOINT,
            Method::POST,
            "groups".to_string(),
            group,
            None,
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => panic!(
                "received response status: {:?}\nbody: {}",
                s,
                resp.text().unwrap()
            ),
        };

        // Try to deserialize the response.
        return resp.json().unwrap();
    }

    /// Update a Google group's aliases.
    pub fn update_group_aliases(
        &self,
        group_key: String,
        aliases: Vec<String>,
    ) {
        for alias in aliases {
            self.update_group_alias(group_key.to_string(), alias.to_string());
        }
    }

    /// Update an alias for a Google group.
    pub fn update_group_alias(&self, group_key: String, alias: String) {
        let mut a: HashMap<String, String> = HashMap::new();
        a.insert("alias".to_string(), alias);
        // Build the request.
        let request = self.request(
            DIRECTORY_ENDPOINT,
            Method::POST,
            format!("groups/{}/aliases", group_key.to_string()),
            a,
            None,
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                let body = resp.text().unwrap();

                if body.contains("duplicate") {
                    // Ignore the error because we don't care about if it is a duplicate.
                    return;
                }

                panic!(
                    "received response status: {:?}\nresponse body: {:?}",
                    s, body,
                );
            }
        };
    }

    /// Check if a user is a member of a Google group.
    pub fn group_has_member(&self, group_id: String, email: String) -> bool {
        // Build the request.
        let request = self.request(
            DIRECTORY_ENDPOINT,
            Method::GET,
            format!("groups/{}/hasMember/{}", group_id, email),
            {},
            None,
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => panic!(
                "received response status: {:?}\nbody: {}",
                s,
                resp.text().unwrap()
            ),
        };

        // Try to deserialize the response.
        let value: MembersHasMember = resp.json().unwrap();

        return value.is_member.unwrap();
    }

    /// Update a member of a Google group.
    pub fn group_update_member(
        &self,
        group_id: String,
        email: String,
        role: String,
    ) {
        let mut member: Member = Default::default();
        member.role = Some(role.to_string());
        member.email = Some(email.to_string());
        member.delivery_settings = Some("ALL_MAIL".to_string());

        // Build the request.
        let request = self.request(
            DIRECTORY_ENDPOINT,
            Method::PUT,
            format!("groups/{}/members/{}", group_id, email.to_string()),
            member,
            None,
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => panic!(
                "received response status: {:?}\nbody: {}",
                s,
                resp.text().unwrap()
            ),
        };
    }

    /// Add a user as a member of a Google group.
    pub fn group_insert_member(
        &self,
        group_id: String,
        email: String,
        role: String,
    ) {
        let mut member: Member = Default::default();
        member.role = Some(role.to_string());
        member.email = Some(email.to_string());
        member.delivery_settings = Some("ALL_MAIL".to_string());

        // Build the request.
        let request = self.request(
            DIRECTORY_ENDPOINT,
            Method::POST,
            format!("groups/{}/members", group_id),
            member,
            None,
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => panic!(
                "received response status: {:?}\nbody: {}",
                s,
                resp.text().unwrap()
            ),
        };
    }

    /// Remove a user as a member of a Google group.
    pub fn group_remove_member(&self, group_id: String, email: String) {
        // Build the request.
        let request = self.request(
            DIRECTORY_ENDPOINT,
            Method::DELETE,
            format!("groups/{}/members/{}", group_id, email),
            {},
            None,
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => panic!(
                "received response status: {:?}\nbody: {}",
                s,
                resp.text().unwrap()
            ),
        };
    }

    /// List users.
    pub fn list_users(&self) -> Vec<User> {
        // Build the request.
        let request = self.request(
            DIRECTORY_ENDPOINT,
            Method::GET,
            "users".to_string(),
            {},
            Some(vec![
                ("customer", self.customer.to_string()),
                ("domain", self.domain.to_string()),
                ("projection", "full".to_string()),
            ]),
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => panic!(
                "received response status: {:?}\nbody: {}",
                s,
                resp.text().unwrap()
            ),
        };

        // Try to deserialize the response.
        let value: Users = resp.json().unwrap();

        return value.users.unwrap();
    }

    /// Update a user.
    pub fn update_user(&self, user: User) {
        // Build the request.
        let request = self.request(
            DIRECTORY_ENDPOINT,
            Method::PUT,
            format!("users/{}", user.clone().id.unwrap()),
            user,
            None,
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => panic!(
                "received response status: {:?}\nbody: {}",
                s,
                resp.text().unwrap()
            ),
        };
    }

    /// Create a user.
    pub fn create_user(&self, user: User) -> User {
        // Build the request.
        let request = self.request(
            DIRECTORY_ENDPOINT,
            Method::POST,
            "users".to_string(),
            user,
            None,
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => panic!(
                "received response status: {:?}\nbody: {}",
                s,
                resp.text().unwrap()
            ),
        };

        // Try to deserialize the response.
        return resp.json().unwrap();
    }

    /// Update a user's aliases.
    pub fn update_user_aliases(&self, user_id: String, aliases: Vec<String>) {
        for alias in aliases {
            self.update_user_alias(user_id.to_string(), alias.to_string());
        }
    }

    /// Update an alias for a user.
    pub fn update_user_alias(&self, user_id: String, alias: String) {
        let mut a: HashMap<String, String> = HashMap::new();
        a.insert("alias".to_string(), alias);
        // Build the request.
        let request = self.request(
            DIRECTORY_ENDPOINT,
            Method::POST,
            format!("users/{}/aliases", user_id.to_string()),
            a,
            None,
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                let body = resp.text().unwrap();

                if body.contains("duplicate") {
                    // Ignore the error because we don't care about if it is a duplicate.
                    return;
                }

                panic!(
                    "received response status: {:?}\nresponse body: {:?}",
                    s, body,
                );
            }
        };
    }

    /// List calendar resources.
    pub fn list_calendar_resources(&self) -> Vec<CalendarResource> {
        // Build the request.
        let request = self.request(
            DIRECTORY_ENDPOINT,
            Method::GET,
            format!("customer/{}/resources/calendars", self.customer),
            {},
            None,
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => panic!(
                "received response status: {:?}\nbody: {}",
                s,
                resp.text().unwrap()
            ),
        };

        // Try to deserialize the response.
        let value: CalendarResources = resp.json().unwrap();

        return value.items.unwrap();
    }

    /// Update a calendar resource.
    pub fn update_calendar_resource(&self, resource: CalendarResource) {
        // Build the request.
        let request = self.request(
            DIRECTORY_ENDPOINT,
            Method::PUT,
            format!(
                "customer/{}/resources/calendars/{}",
                self.customer,
                resource.clone().id
            ),
            resource,
            None,
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => panic!(
                "received response status: {:?}\nbody: {}",
                s,
                resp.text().unwrap()
            ),
        };
    }

    /// Create a calendar resource.
    pub fn create_calendar_resource(&self, resource: CalendarResource) {
        // Build the request.
        let request = self.request(
            DIRECTORY_ENDPOINT,
            Method::POST,
            format!("customer/{}/resources/calendars", self.customer),
            resource,
            None,
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => panic!(
                "received response status: {:?}\nbody: {}",
                s,
                resp.text().unwrap()
            ),
        };
    }

    /// List buildings.
    pub fn list_buildings(&self) -> Vec<Building> {
        // Build the request.
        let request = self.request(
            DIRECTORY_ENDPOINT,
            Method::GET,
            format!("customer/{}/resources/buildings", self.customer),
            {},
            None,
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => panic!(
                "received response status: {:?}\nbody: {}",
                s,
                resp.text().unwrap()
            ),
        };

        // Try to deserialize the response.
        let value: Buildings = resp.json().unwrap();

        return value.buildings.unwrap();
    }

    /// Update a building.
    pub fn update_building(&self, building: Building) {
        // Build the request.
        let request = self.request(
            DIRECTORY_ENDPOINT,
            Method::PUT,
            format!(
                "customer/{}/resources/buildings/{}",
                self.customer,
                building.clone().id
            ),
            building,
            None,
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => panic!(
                "received response status: {:?}\nbody: {}",
                s,
                resp.text().unwrap()
            ),
        };
    }

    /// Create a building.
    pub fn create_building(&self, building: Building) {
        // Build the request.
        let request = self.request(
            DIRECTORY_ENDPOINT,
            Method::POST,
            format!("customer/{}/resources/buildings", self.customer),
            building,
            None,
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => panic!(
                "received response status: {:?}\nbody: {}",
                s,
                resp.text().unwrap()
            ),
        };
    }
}

/// Generate a random string that we can use as a temporary password for new users
/// when we set up their account.
pub fn generate_password() -> String {
    let rand_string: String =
        thread_rng().sample_iter(&Alphanumeric).take(30).collect();

    return rand_string;
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
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "includeInGlobalAddressList"
    )]
    pub include_in_global_address_list: Option<String>,
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
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct User {
    /// no description provided
    pub addresses: Option<String>,
    /// no description provided
    #[serde(skip_serializing_if = "Option::is_none", rename = "posixAccounts")]
    pub posix_accounts: Option<String>,
    /// no description provided
    pub phones: Option<Vec<UserPhone>>,
    /// no description provided
    pub locations: Option<Vec<UserLocation>>,
    /// Boolean indicating if the user is delegated admin (Read-only)
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "isDelegatedAdmin"
    )]
    pub is_delegated_admin: Option<bool>,
    /// ETag of the user's photo (Read-only)
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "thumbnailPhotoEtag"
    )]
    pub thumbnail_photo_etag: Option<String>,
    /// Indicates if user is suspended.
    pub suspended: Option<bool>,
    /// no description provided
    pub keywords: Option<String>,
    /// Kind of resource this is.
    pub kind: Option<String>,
    /// Unique identifier of User (Read-only)
    pub id: Option<String>,
    /// List of aliases (Read-only)
    pub aliases: Option<Vec<String>>,
    /// List of non editable aliases (Read-only)
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "nonEditableAliases"
    )]
    pub non_editable_aliases: Option<Vec<String>>,
    /// Indicates if user is archived.
    pub archived: Option<bool>,
    /// no description provided
    #[serde(skip_serializing_if = "Option::is_none", rename = "deletionTime")]
    pub deletion_time: Option<String>,
    /// Suspension reason if user is suspended (Read-only)
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "suspensionReason"
    )]
    pub suspension_reason: Option<String>,
    /// Photo Url of the user (Read-only)
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "thumbnailPhotoUrl"
    )]
    pub thumbnail_photo_url: Option<String>,
    /// Is enrolled in 2-step verification (Read-only)
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "isEnrolledIn2Sv"
    )]
    pub is_enrolled_in2_sv: Option<bool>,
    /// Boolean indicating if user is included in Global Address List
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "includeInGlobalAddressList"
    )]
    pub include_in_global_address_list: Option<bool>,
    /// no description provided
    pub relations: Option<String>,
    /// no description provided
    pub languages: Option<String>,
    /// Boolean indicating if the user is admin (Read-only)
    #[serde(skip_serializing_if = "Option::is_none", rename = "isAdmin")]
    pub is_admin: Option<bool>,
    /// ETag of the resource.
    pub etag: Option<String>,
    /// User's last login time. (Read-only)
    #[serde(skip_serializing_if = "Option::is_none", rename = "lastLoginTime")]
    pub last_login_time: Option<String>,
    /// OrgUnit of User
    #[serde(skip_serializing_if = "Option::is_none", rename = "orgUnitPath")]
    pub org_unit_path: Option<String>,
    /// Indicates if user has agreed to terms (Read-only)
    #[serde(skip_serializing_if = "Option::is_none", rename = "agreedToTerms")]
    pub agreed_to_terms: Option<bool>,
    /// no description provided
    #[serde(skip_serializing_if = "Option::is_none", rename = "externalIds")]
    pub external_ids: Option<String>,
    /// Boolean indicating if ip is whitelisted
    #[serde(skip_serializing_if = "Option::is_none", rename = "ipWhitelisted")]
    pub ip_whitelisted: Option<bool>,
    /// no description provided
    #[serde(skip_serializing_if = "Option::is_none", rename = "sshPublicKeys")]
    pub ssh_public_keys: Option<Vec<UserSSHKey>>,
    /// Custom fields of the user.
    #[serde(skip_serializing_if = "Option::is_none", rename = "customSchemas")]
    pub custom_schemas: Option<HashMap<String, UserCustomProperties>>,
    /// Is 2-step verification enforced (Read-only)
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "isEnforcedIn2Sv"
    )]
    pub is_enforced_in2_sv: Option<bool>,
    /// Is mailbox setup (Read-only)
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "isMailboxSetup"
    )]
    pub is_mailbox_setup: Option<bool>,
    /// User's password
    pub password: Option<String>,
    /// no description provided
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emails: Option<Vec<UserEmail>>,
    /// no description provided
    pub organizations: Option<String>,
    /// username of User
    #[serde(skip_serializing_if = "Option::is_none", rename = "primaryEmail")]
    pub primary_email: Option<String>,
    /// Hash function name for password. Supported are MD5, SHA-1 and crypt
    #[serde(skip_serializing_if = "Option::is_none", rename = "hashFunction")]
    pub hash_function: Option<String>,
    /// User's name
    pub name: Option<UserName>,
    /// no description provided
    pub gender: Option<UserGender>,
    /// no description provided
    pub notes: Option<String>,
    /// User's G Suite account creation time. (Read-only)
    #[serde(skip_serializing_if = "Option::is_none", rename = "creationTime")]
    pub creation_time: Option<String>,
    /// no description provided
    pub websites: Option<String>,
    /// Boolean indicating if the user should change password in next login
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "changePasswordAtNextLogin"
    )]
    pub change_password_at_next_login: Option<bool>,
    /// no description provided
    pub ims: Option<String>,
    /// CustomerId of User (Read-only)
    #[serde(skip_serializing_if = "Option::is_none", rename = "customerId")]
    pub customer_id: Option<String>,
    /// Recovery email of the user
    #[serde(skip_serializing_if = "Option::is_none", rename = "recoveryEmail")]
    pub recovery_email: Option<String>,
    /// Recovery phone of the user
    #[serde(skip_serializing_if = "Option::is_none", rename = "recoveryPhone")]
    pub recovery_phone: Option<String>,
}

impl User {
    /// Update a user.
    pub fn update(
        mut self,
        user: UserConfig,
        domain: String,
        change_password: bool,
    ) -> User {
        // Set the settings for the user.
        self.name = Some(UserName {
            full_name: Some(format!(
                "{} {}",
                user.first_name.to_string(),
                user.last_name.to_string()
            )),
            given_name: Some(user.first_name.to_string()),
            family_name: Some(user.last_name.to_string()),
        });

        match user.clone().recovery_email {
            Some(val) => {
                // Set the recovery email for the user.
                self.recovery_email = Some(val.clone());

                // Check if we have a home email set for the user and update it.
                let mut has_home_email = false;
                match self.emails {
                    Some(mut emails) => {
                        for (index, email) in emails.iter().enumerate() {
                            match &email.typev {
                                Some(typev) => {
                                    if typev == "home" {
                                        // Update the set home email.
                                        emails[index].address = val.clone();
                                        // Break the loop early.
                                        has_home_email = true;
                                        break;
                                    }
                                }
                                None => (),
                            };
                        }

                        if !has_home_email {
                            // Set the home email for the user.
                            emails.push(UserEmail {
                                typev: Some("home".to_string()),
                                address: val.clone(),
                                primary: Some(false),
                            });
                        }

                        // Set the emails.
                        self.emails = Some(emails);
                    }
                    None => {
                        self.emails = Some(vec![UserEmail {
                            typev: Some("home".to_string()),
                            address: val.clone(),
                            primary: Some(false),
                        }]);
                    }
                }
            }
            None => self.recovery_email = None,
        }

        match user.clone().recovery_phone {
            Some(val) => {
                // Set the recovery phone for the user.
                self.recovery_phone = Some(val.clone());

                // Set the home phone for the user.
                self.phones = Some(vec![UserPhone {
                    typev: "home".to_string(),
                    value: val.clone(),
                    primary: true,
                }])
            }
            None => self.recovery_phone = None,
        }

        self.primary_email = Some(format!("{}@{}", user.username, domain));

        // Write the user aliases.
        let mut aliases: Vec<String> = Default::default();
        for alias in user.clone().aliases.unwrap() {
            aliases.push(format!("{}@{}", alias, domain));
        }
        self.aliases = Some(aliases);

        if change_password {
            // Since we are creating a new user, we want to change their password
            // at the next login.
            self.change_password_at_next_login = Some(true);
            // Generate a password for the user.
            let password = generate_password();
            self.password = Some(password.to_string());
        }

        match user.clone().gender {
            Some(val) => {
                let mut gender: UserGender = Default::default();
                gender.typev = val;
                self.gender = Some(gender);
            }
            None => self.gender = None,
        }

        match user.clone().building {
            Some(val) => {
                let mut location: UserLocation = Default::default();
                location.typev = "desk".to_string();
                location.building_id = Some(val);
                location.floor_name = Some("1".to_string());
                self.locations = Some(vec![location]);
            }
            None => self.locations = None,
        }

        let mut cs: HashMap<String, UserCustomProperties> = HashMap::new();
        match user.clone().github {
            Some(val) => {
                let mut gh: HashMap<String, String> = HashMap::new();
                gh.insert("GitHub_Username".to_string(), val.clone());
                cs.insert(
                    "Contact".to_string(),
                    UserCustomProperties(Some(gh)),
                );

                // Set their GitHub SSH Keys to their Google SSH Keys.
                let ssh_keys = get_github_user_public_ssh_keys(val.clone());
                self.ssh_public_keys = Some(ssh_keys);
            }
            None => (),
        }

        match user.clone().chat {
            Some(val) => {
                let mut chat: HashMap<String, String> = HashMap::new();
                chat.insert("Matrix_Chat_Username".to_string(), val.clone());
                cs.insert(
                    "Contact".to_string(),
                    UserCustomProperties(Some(chat)),
                );
            }
            None => (),
        }

        // Set the custom schemas.
        self.custom_schemas = Some(cs);

        return self;
    }
}

/// Return a user's public ssh key's from GitHub by their GitHub handle.
fn get_github_user_public_ssh_keys(handle: String) -> Vec<UserSSHKey> {
    let body = reqwest::blocking::get(
        &format!("https://github.com/{}.keys", handle).to_string(),
    )
    .unwrap()
    .text()
    .unwrap();

    let k: Vec<&str> = body.split("\n").collect();
    let mut keys: Vec<UserSSHKey> = Default::default();
    for key in k {
        if key.trim().len() > 0 {
            keys.push(UserSSHKey {
                key: key.trim().to_string(),
                expiration_time_usec: None,
            });
        }
    }

    return keys;
}

/// A user's email.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct UserEmail {
    #[serde(skip_serializing_if = "Option::is_none", rename = "type")]
    pub typev: Option<String>,
    pub address: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary: Option<bool>,
}

/// A user's phone.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct UserPhone {
    #[serde(rename = "type")]
    pub typev: String,
    pub value: String,
    pub primary: bool,
}

/// A user's name.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct UserName {
    /// Full Name
    #[serde(skip_serializing_if = "Option::is_none", rename = "fullName")]
    pub full_name: Option<String>,
    /// First Name
    #[serde(skip_serializing_if = "Option::is_none", rename = "givenName")]
    pub given_name: Option<String>,
    /// Last Name
    #[serde(skip_serializing_if = "Option::is_none", rename = "familyName")]
    pub family_name: Option<String>,
}

/// A user's ssh key.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct UserSSHKey {
    pub key: String,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "expirationTimeUsec"
    )]
    pub expiration_time_usec: Option<i128>,
}

/// Custom properties for a user.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct UserCustomProperties(pub Option<HashMap<String, String>>);

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
struct Users {
    /// Token used to access next page of this result.
    #[serde(skip_serializing_if = "Option::is_none", rename = "nextPageToken")]
    pub next_page_token: Option<String>,
    /// Kind of resource this is.
    pub kind: Option<String>,
    /// ETag of the resource.
    pub etag: Option<String>,
    /// Event that triggered this response (only used in case of Push Response)
    pub trigger_event: Option<String>,
    /// List of user objects.
    pub users: Option<Vec<User>>,
}

/// A user's location.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct UserLocation {
    #[serde(rename = "type")]
    pub typev: String,
    pub area: String,
    /// Unique ID for the building a resource is located in.
    #[serde(skip_serializing_if = "Option::is_none", rename = "buildingId")]
    pub building_id: Option<String>,
    /// Name of the floor a resource is located on.
    #[serde(skip_serializing_if = "Option::is_none", rename = "floorName")]
    pub floor_name: Option<String>,
}

/// A user's gender.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct UserGender {
    #[serde(rename = "type")]
    pub typev: String,
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
    /// no description provided
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
        resource: ResourceConfig,
        id: String,
    ) -> CalendarResource {
        self.id = id;
        self.typev = Some(resource.typev.to_string());
        self.name = resource.name.to_string();
        self.building_id = Some(resource.building.to_string());
        self.description = Some(resource.description.to_string());
        self.user_visible_description = Some(resource.description.to_string());
        self.capacity = Some(resource.capacity);
        self.floor_name = Some(resource.floor.to_string());
        self.floor_section = Some(resource.section.to_string());
        self.category = Some("CONFERENCE_ROOM".to_string());

        return self;
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
    pub fn update(mut self, building: BuildingConfig, id: String) -> Building {
        self.id = id;
        self.name = building.name.to_string();
        self.description = Some(building.description.to_string());
        self.address = Some(BuildingAddress {
            address_lines: Some(vec![building.address.to_string()]),
            locality: Some(building.city.to_string()),
            administrative_area: Some(building.state.to_string()),
            postal_code: Some(building.zipcode.to_string()),
            region_code: Some(building.country.to_string()),
            language_code: Some("en".to_string()),
            sublocality: None,
        });
        self.floor_names = Some(building.clone().floors);

        return self;
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
