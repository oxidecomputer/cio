use airtable_api::User as AirtableUser;
use chrono::naive::NaiveDate;
use serde::{Deserialize, Serialize};

/// The data type for customer interactions.
/// This is inline with our Airtable workspace.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CustomerInteraction {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Company")]
    pub company: Vec<String>,
    #[serde(with = "meeting_date_format", rename = "Date")]
    pub date: NaiveDate,
    #[serde(rename = "Type")]
    pub meeting_type: String,
    #[serde(rename = "Phase")]
    pub phase: String,
    #[serde(rename = "People")]
    pub people: Vec<String>,
    #[serde(rename = "Oxide Folks")]
    pub oxide_folks: Vec<AirtableUser>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "Link to Notes")]
    pub notes_link: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "Notes")]
    pub notes: Option<String>,
}

/// The data type for discussion topics.
/// This is inline with our Airtable workspace.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DiscussionTopic {
    #[serde(rename = "Topic")]
    pub topic: String,
    #[serde(rename = "Submitter")]
    pub submitter: AirtableUser,
    #[serde(rename = "Priority")]
    pub priority: String,
    #[serde(skip_serializing_if = "Option::is_none", rename = "Notes")]
    pub notes: Option<String>,
    // Never modify this, it is a linked record.
    #[serde(rename = "Associated meetings")]
    pub associated_meetings: Vec<String>,
}

/// The data type for a meeting.
/// This is inline with our Airtable workspace for product huddle meetings, hardware
/// huddle meetings, etc.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Meeting {
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
    pub reminder_email_sent: bool,
}

/// Convert the date format `%Y-%m-%d` to a NaiveDate.
mod meeting_date_format {
    use chrono::naive::NaiveDate;
    use serde::{self, Deserialize, Deserializer, Serializer};

    const FORMAT: &str = "%Y-%m-%d";

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

/// The data type for sending reminders for meetings.
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct MeetingReminderEmailData {
    pub date: String,
    pub topics: Vec<DiscussionTopic>,
    pub last_meeting_reports_link: String,
    pub huddle_name: String,
    pub time: String,
    pub email: String,
}
