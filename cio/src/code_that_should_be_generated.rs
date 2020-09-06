use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::schema::{
    buildings, conference_rooms, github_labels, groups, links, users,
};

#[derive(
    Debug,
    Queryable,
    Identifiable,
    Associations,
    Default,
    PartialEq,
    Clone,
    JsonSchema,
    Deserialize,
    Serialize,
)]
pub struct Building {
    pub id: i32,
    pub name: String,
    pub description: String,
    pub address: String,
    pub city: String,
    pub state: String,
    pub zipcode: String,
    pub country: String,
    pub floors: Vec<String>,
}

#[derive(
    Debug,
    Queryable,
    Identifiable,
    Associations,
    Default,
    PartialEq,
    Clone,
    JsonSchema,
    Deserialize,
    Serialize,
)]
pub struct ConferenceRoom {
    pub id: i32,
    pub name: String,
    pub description: String,
    #[serde(rename = "type")]
    pub typev: String,
    pub building: String,
    pub capacity: i32,
    pub floor: String,
    pub section: String,
}

#[derive(
    Debug,
    Queryable,
    Identifiable,
    Associations,
    Default,
    PartialEq,
    Clone,
    JsonSchema,
    Deserialize,
    Serialize,
)]
pub struct GithubLabel {
    pub id: i32,
    pub name: String,
    pub description: String,
    pub color: String,
}

#[derive(
    Debug,
    Queryable,
    Identifiable,
    Associations,
    Default,
    PartialEq,
    Clone,
    JsonSchema,
    Deserialize,
    Serialize,
)]
pub struct Group {
    pub id: i32,
    pub name: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    pub allow_external_members: bool,
    pub allow_web_posting: bool,
    pub is_archived: bool,
    pub who_can_discover_group: String,
    pub who_can_join: String,
    pub who_can_moderate_members: String,
    pub who_can_post_message: String,
    pub who_can_view_group: String,
    pub who_can_view_membership: String,
}

#[derive(
    Debug,
    Queryable,
    Identifiable,
    Associations,
    Default,
    PartialEq,
    Clone,
    JsonSchema,
    Deserialize,
    Serialize,
)]
pub struct Link {
    pub id: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    pub description: String,
    pub link: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
}

#[derive(
    Debug,
    Queryable,
    Identifiable,
    Associations,
    Default,
    PartialEq,
    Clone,
    JsonSchema,
    Deserialize,
    Serialize,
)]
pub struct User {
    pub id: i32,
    pub first_name: String,
    pub last_name: String,
    pub username: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub recovery_email: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub recovery_phone: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub gender: String,

    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub chat: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub github: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub twitter: String,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<String>,

    #[serde(default)]
    pub is_super_admin: bool,

    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub building: String,
}
