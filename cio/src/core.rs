use airtable_api::User as AirtableUser;
use async_trait::async_trait;
use chrono::naive::NaiveDate;
use gusto_api::date_format;
use serde::{Deserialize, Serialize};

/// Define the trait for doing logic in updating Airtable.
#[async_trait]
pub trait UpdateAirtableRecord<T> {
    async fn update_airtable_record(&mut self, _: T);
}

/// The data type for customer interactions.
/// This is inline with our Airtable workspace.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CustomerInteraction {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Company")]
    pub company: Vec<String>,
    #[serde(with = "date_format", rename = "Date")]
    pub date: NaiveDate,
    #[serde(rename = "Type")]
    pub meeting_type: String,
    #[serde(rename = "Phase")]
    pub phase: String,
    #[serde(default, rename = "People")]
    pub people: Vec<String>,
    #[serde(default, rename = "Oxide Folks")]
    pub oxide_folks: Vec<AirtableUser>,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "Link to Notes")]
    pub notes_link: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "Notes")]
    pub notes: String,
}

/// The data type for discussion topics.
/// This is inline with our Airtable workspace.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DiscussionTopic {
    #[serde(rename = "Topic")]
    pub topic: String,
    #[serde(default, rename = "Submitter")]
    pub submitter: AirtableUser,
    #[serde(rename = "Priority", skip_serializing_if = "String::is_empty", default)]
    pub priority: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "Notes")]
    pub notes: String,
    // Never modify this, it is a linked record.
    #[serde(rename = "Associated meetings")]
    pub associated_meetings: Vec<String>,
}

/// The data type for a meeting.
/// This is inline with our Airtable workspace for product huddle meetings, hardware
/// huddle meetings, etc.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Meeting {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(serialize_with = "gusto_api::date_format::serialize", deserialize_with = "gusto_api::date_format::deserialize")]
    pub date: NaiveDate,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub week: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub action_items: String,
    // Never modify this, it is a linked record.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proposed_discussion: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub recording: String,
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        serialize_with = "airtable_api::user_format_as_array_of_strings::serialize",
        deserialize_with = "airtable_api::user_format_as_array_of_strings::deserialize"
    )]
    pub attendees: Vec<String>,
    #[serde(default)]
    pub reminder_email_sent: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub calendar_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub calendar_event_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub calendar_event_link: String,
    #[serde(default)]
    pub cancelled: bool,
}

/// The data type for sending reminders for meetings.
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct MeetingReminderEmailData {
    pub date: String,
    pub topics: Vec<DiscussionTopic>,
    pub huddle_name: String,
    pub time: String,
    pub email: String,
}
