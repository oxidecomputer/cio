use chrono::naive::NaiveDate;
use serde::{Deserialize, Serialize};

use airtable::User as AirtableUser;

/// The data type for a Google Sheet Column, we use this when updating the
/// applications spreadsheet to mark that we have emailed someone.
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

/// The data type for an applicant.
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

/// The data type for an RFD.
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
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
    #[serde(skip_serializing_if = "Option::is_none", rename = "Notes")]
    pub notes: Option<String>,
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

/// Convert the date format `%Y-%m-%d` to a NaiveDate.
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

/// The data type for sending reminders for the product huddle meetings.
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct ProductEmailData {
    pub date: String,
    pub topics: Vec<DiscussionFields>,
    pub last_meeting_reports_link: String,
    pub meeting_id: String,
    pub should_send: bool,
}
