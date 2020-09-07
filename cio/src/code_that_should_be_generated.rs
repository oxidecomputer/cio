use chrono::offset::Utc;
use chrono::DateTime;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::schema::{
    applicants, auth_logins, buildings, conference_rooms, github_labels,
    groups, links, mailing_list_subscribers, rfds as r_f_ds, users,
};

#[derive(
    Debug,
    Queryable,
    Identifiable,
    Associations,
    PartialEq,
    Clone,
    JsonSchema,
    Deserialize,
    Serialize,
)]
pub struct Applicant {
    pub id: i32,
    pub name: String,
    pub role: String,
    pub sheet_id: String,
    pub status: String,
    pub submitted_time: DateTime<Utc>,
    pub email: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub phone: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub country_code: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub location: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub github: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub gitlab: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub linkedin: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub portfolio: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub website: String,
    pub resume: String,
    pub materials: String,
    #[serde(default)]
    pub sent_email_received: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub value_reflected: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub value_violated: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values_in_tension: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub resume_contents: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub materials_contents: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub work_samples: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub writing_samples: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub analysis_samples: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub presentation_samples: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub exploratory_samples: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub question_technically_challenging: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub question_proud_of: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub question_happiest: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub question_unhappiest: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub question_value_reflected: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub question_value_violated: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub question_values_in_tension: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub question_why_oxide: String,
}

#[derive(
    Debug,
    Queryable,
    Identifiable,
    Associations,
    PartialEq,
    Clone,
    JsonSchema,
    Deserialize,
    Serialize,
)]
pub struct AuthLogin {
    pub id: i32,
    pub user_id: String,
    pub name: String,
    pub nickname: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub username: String,
    pub email: String,
    pub email_verified: bool,
    pub picture: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub company: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub blog: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub phone: String,
    #[serde(default)]
    pub phone_verified: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub locale: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub login_provider: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_login: DateTime<Utc>,
    pub last_ip: String,
    pub logins_count: i32,
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
    PartialEq,
    Clone,
    JsonSchema,
    Deserialize,
    Serialize,
)]
pub struct MailingListSubscriber {
    pub id: i32,
    pub email: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub first_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub last_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub company: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub interest: String,
    #[serde(default)]
    pub wants_podcast_updates: bool,
    #[serde(default)]
    pub wants_newsletter: bool,
    #[serde(default)]
    pub wants_product_updates: bool,
    pub date_added: DateTime<Utc>,
    pub date_optin: DateTime<Utc>,
    pub date_last_changed: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub link_to_people: Vec<String>,
}

#[derive(
    Debug,
    Queryable,
    Identifiable,
    Associations,
    PartialEq,
    Clone,
    JsonSchema,
    Deserialize,
    Serialize,
)]
pub struct RFD {
    pub id: i32,
    pub number: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub number_string: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    pub state: String,
    pub link: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub short_link: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub rendered_link: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub discussion: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub authors: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub html: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub content: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sha: String,
    #[serde(default = "Utc::now")]
    pub commit_date: DateTime<Utc>,
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
