use std::collections::BTreeMap;

use chrono::naive::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::airtable::core::User as AirtableUser;

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub users: BTreeMap<String, UserConfig>,
    pub groups: BTreeMap<String, GroupConfig>,

    pub buildings: BTreeMap<String, BuildingConfig>,
    pub resources: BTreeMap<String, ResourceConfig>,

    pub links: BTreeMap<String, LinkConfig>,

    pub labels: Vec<LabelConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UserConfig {
    pub first_name: String,
    pub last_name: String,
    pub username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aliases: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovery_email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovery_phone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gender: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub chat: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub github: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub twitter: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub groups: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_super_admin: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub building: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GroupConfig {
    pub name: String,
    pub description: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub aliases: Option<Vec<String>>,

    /// allow_external_members: Identifies whether members external to your
    /// organization can join the group. Possible values are:
    /// - true: G Suite users external to your organization can become
    /// members of this group.
    /// - false: Users not belonging to the organization are not allowed to
    /// become members of this group.
    pub allow_external_members: bool,

    /// allow_web_posting: Allows posting from web. Possible values are:
    /// - true: Allows any member to post to the group forum.
    /// - false: Members only use Gmail to communicate with the group.
    pub allow_web_posting: bool,

    /// is_archived: Allows the Group contents to be archived. Possible values
    /// are:
    /// - true: Archive messages sent to the group.
    /// - false: Do not keep an archive of messages sent to this group. If
    /// false, previously archived messages remain in the archive.
    pub is_archived: bool,

    /// who_can_discover_group: Specifies the set of users for whom this group
    /// is discoverable. Possible values are:
    /// - ANYONE_CAN_DISCOVER
    /// - ALL_IN_DOMAIN_CAN_DISCOVER
    /// - ALL_MEMBERS_CAN_DISCOVER
    pub who_can_discover_group: String,

    /// who_can_join: Permission to join group. Possible values are:
    /// - ANYONE_CAN_JOIN: Anyone in the account domain can join. This
    /// includes accounts with multiple domains.
    /// - ALL_IN_DOMAIN_CAN_JOIN: Any Internet user who is outside your
    /// domain can access your Google Groups service and view the list of
    /// groups in your Groups directory. Warning: Group owners can add
    /// external addresses, outside of the domain to their groups. They can
    /// also allow people outside your domain to join their groups. If you
    /// later disable this option, any external addresses already added to
    /// users' groups remain in those groups.
    /// - INVITED_CAN_JOIN: Candidates for membership can be invited to join.
    ///
    /// - CAN_REQUEST_TO_JOIN: Non members can request an invitation to join.
    pub who_can_join: String,

    /// who_can_moderate_members: Specifies who can manage members. Possible
    /// values are:
    /// - ALL_MEMBERS
    /// - OWNERS_AND_MANAGERS
    /// - OWNERS_ONLY
    /// - NONE
    pub who_can_moderate_members: String,

    /// who_can_post_message: Permissions to post messages. Possible values are:
    ///
    /// - NONE_CAN_POST: The group is disabled and archived. No one can post
    /// a message to this group.
    /// - When archiveOnly is false, updating who_can_post_message to
    /// NONE_CAN_POST, results in an error.
    /// - If archiveOnly is reverted from true to false, who_can_post_messages
    /// is set to ALL_MANAGERS_CAN_POST.
    /// - ALL_MANAGERS_CAN_POST: Managers, including group owners, can post
    /// messages.
    /// - ALL_MEMBERS_CAN_POST: Any group member can post a message.
    /// - ALL_OWNERS_CAN_POST: Only group owners can post a message.
    /// - ALL_IN_DOMAIN_CAN_POST: Anyone in the account can post a message.
    ///
    /// - ANYONE_CAN_POST: Any Internet user who outside your account can
    /// access your Google Groups service and post a message. Note: When
    /// who_can_post_message is set to ANYONE_CAN_POST, we recommend the
    /// messageModerationLevel be set to MODERATE_NON_MEMBERS to protect the
    /// group from possible spam.
    pub who_can_post_message: String,

    /// who_can_view_group: Permissions to view group messages. Possible values
    /// are:
    /// - ANYONE_CAN_VIEW: Any Internet user can view the group's messages.
    ///
    /// - ALL_IN_DOMAIN_CAN_VIEW: Anyone in your account can view this
    /// group's messages.
    /// - ALL_MEMBERS_CAN_VIEW: All group members can view the group's
    /// messages.
    /// - ALL_MANAGERS_CAN_VIEW: Any group manager can view this group's
    /// messages.
    pub who_can_view_group: String,

    /// who_can_view_membership: Permissions to view membership. Possible values
    /// are:
    /// - ALL_IN_DOMAIN_CAN_VIEW: Anyone in the account can view the group
    /// members list.
    /// If a group already has external members, those members can still send
    /// email to this group.
    ///
    /// - ALL_MEMBERS_CAN_VIEW: The group members can view the group members
    /// list.
    /// - ALL_MANAGERS_CAN_VIEW: The group managers can view group members
    /// list.
    pub who_can_view_membership: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BuildingConfig {
    pub name: String,
    pub description: String,
    pub address: String,
    pub city: String,
    pub state: String,
    pub zipcode: String,
    pub country: String,
    pub floors: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ResourceConfig {
    pub name: String,
    pub description: String,
    #[serde(rename = "type")]
    pub typev: String,
    pub building: String,
    pub capacity: i32,
    pub floor: String,
    pub section: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_zoom_room: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LinkConfig {
    /// name will not be used in config files.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub description: String,
    pub link: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aliases: Option<Vec<String>>,
    /// subdomain will not be used in config files.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subdomain: Option<String>,
    /// discussion will not be used in config files.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discussion: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LabelConfig {
    pub name: String,
    pub description: String,
    pub color: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SheetColumns {
    pub timestamp: usize,
    pub name: usize,
    pub email: usize,
    pub location: usize,
    pub phone: usize,
    pub github: usize,
    pub resume: usize,
    pub materials: usize,
    pub status: usize,
    pub received_application: usize,
}

#[derive(Debug, Clone)]
pub struct Applicant {
    pub submitted_time: NaiveDate,
    pub name: String,
    pub email: String,
    pub location: String,
    pub phone: String,
    pub github: String,
    pub resume: String,
    pub materials: String,
    pub status: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RFD {
    pub number: String,
    pub title: String,
    pub link: String,
    pub state: String,
    pub discussion: String,
}

/// The Airtable fields type for RFDs.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RFDFields {
    #[serde(rename = "Number")]
    pub number: i32,
    #[serde(rename = "State")]
    pub state: String,
    #[serde(rename = "Title")]
    pub title: String,
    // Never modify this, it is based on a function.
    #[serde(skip_serializing_if = "Option::is_none", rename = "Name")]
    pub name: Option<String>,
    // Never modify this, it is based on a function.
    #[serde(skip_serializing_if = "Option::is_none", rename = "Link")]
    pub link: Option<String>,
}

/// The Airtable fields type for discussion topics.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DiscussionFields {
    #[serde(rename = "Topic")]
    pub topic: String,
    #[serde(rename = "Submitter")]
    pub submitter: AirtableUser,
    #[serde(rename = "Priority")]
    pub priority: String,
    #[serde(rename = "Notes")]
    pub notes: String,
    // Never modify this, it is a linked record.
    #[serde(rename = "Associated meetings")]
    pub associated_meetings: Vec<String>,
}

/// The Airtable fields type for meetings.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MeetingFields {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(with = "meeting_date_format", rename = "Date")]
    pub date: NaiveDate,
    #[serde(rename = "Week")]
    pub week: String,
    #[serde(skip_serializing_if = "Option::is_none", rename = "Notes")]
    pub notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "Action items")]
    pub action_items: Option<String>,
    // Never modify this, it is a linked record.
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "Proposed discussion"
    )]
    pub proposed_discussion: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "Recording")]
    pub recording: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "Attendees")]
    pub attendees: Option<Vec<AirtableUser>>,
}

mod meeting_date_format {
    use chrono::naive::NaiveDate;
    use serde::{self, Deserialize, Deserializer, Serializer};

    const FORMAT: &'static str = "%Y-%m-%d";

    // The signature of a serialize_with function must follow the pattern:
    //
    //    fn serialize<S>(&T, S) -> Result<S::Ok, S::Error>
    //    where
    //        S: Serializer
    //
    // although it may also be generic over the input types T.
    pub fn serialize<S>(
        date: &NaiveDate,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = format!("{}", date.format(FORMAT));
        serializer.serialize_str(&s)
    }

    // The signature of a deserialize_with function must follow the pattern:
    //
    //    fn deserialize<'de, D>(D) -> Result<T, D::Error>
    //    where
    //        D: Deserializer<'de>
    //
    // although it may also be generic over the output types T.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<NaiveDate, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;

        Ok(NaiveDate::parse_from_str(&s, FORMAT).unwrap())
    }
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct ProductEmailData {
    pub date: String,
    pub topics: Vec<DiscussionFields>,
    pub last_meeting_reports_link: String,
    pub meeting_id: String,
    pub should_send: bool,
}
