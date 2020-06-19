use chrono::naive::NaiveDate;
use chrono::offset::Utc;
use chrono::DateTime;
use chrono_humanize::HumanTime;
use serde::{Deserialize, Serialize};

use airtable::User as AirtableUser;

/// The data type for a Google Sheet Column, we use this when updating the
/// applications spreadsheet to mark that we have emailed someone.
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct SheetColumns {
    pub timestamp: usize,
    pub name: usize,
    pub email: usize,
    pub location: usize,
    pub phone: usize,
    pub github: usize,
    pub portfolio: usize,
    pub website: usize,
    pub linkedin: usize,
    pub resume: usize,
    pub materials: usize,
    pub status: usize,
    pub received_application: usize,
}

/// The data type for an applicant.
#[derive(Debug, Clone)]
pub struct Applicant {
    pub submitted_time: DateTime<Utc>,
    pub name: String,
    pub email: String,
    pub location: String,
    pub phone: String,
    pub country_code: String,
    pub github: String,
    pub portfolio: String,
    pub website: String,
    pub linkedin: String,
    pub resume: String,
    pub materials: String,
    pub status: String,
    pub received_application: bool,
    pub role: String,
    pub sheet_id: String,
}

impl Applicant {
    pub fn as_slack_msg(&self, include_time: bool) -> String {
        let mut emoji = ":floppy_disk:";
        match self.role.as_str() {
            "Product Engineering and Design" => emoji = ":iphone:",
            "Technical Program Manager" => emoji = ":pager:",
            _ => (),
        }

        let dur = self.submitted_time - Utc::now();
        let time = HumanTime::from(dur);

        let mut msg = format!(
            "{} <https://docs.google.com/spreadsheets/d/{}|{}>: *{}* <mailto:{}|{}>",
            emoji, self.sheet_id, self.role, self.name, self.email, self.email
        );

        if include_time {
            msg += &format!(" _*{}*_", time);
        }

        msg += &format!(
            "\n\t<{}|resume> | <{}|materials>",
            self.resume, self.materials,
        );

        if !self.location.is_empty() {
            msg += &format!(" | {}", self.location);
        }
        if !self.phone.is_empty() {
            msg += &format!(
                " | <tel:{}|:{}: {}>",
                self.phone, self.country_code, self.phone
            );
        }
        if !self.github.is_empty() {
            msg += &format!(
                " | <https://github.com/{}|github:{}>",
                self.github.trim_start_matches('@'),
                self.github,
            );
        }
        if !self.linkedin.is_empty() {
            msg += &format!(" | <{}|linkedin>", self.linkedin,);
        }
        if !self.portfolio.is_empty() {
            msg += &format!(" | <{}|portfolio>", self.portfolio,);
        }
        if !self.website.is_empty() {
            msg += &format!(" | <{}|website>", self.website,);
        }

        msg
    }
}

/// The data type for a Journal Club Meeting.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JournalClubMeeting {
    pub title: String,
    pub issue: String,
    pub papers: Vec<Paper>,
    pub date: NaiveDate,
    pub coordinator: String,
    pub state: String,
    pub recording: String,
}

impl JournalClubMeeting {
    pub fn as_slack_msg(&self) -> String {
        let emoji = ":blue_book:";

        let mut msg = format!(
            "{} <{}|*{}*> ({}) _{}_ <https://github.com/{}|@{}>",
            emoji,
            self.issue,
            self.title,
            self.state,
            self.date.format("%m/%d/%Y"),
            self.coordinator,
            self.coordinator,
        );

        if !self.recording.is_empty() {
            msg += &format!(" <{}|:vhs>", self.recording);
        }

        for p in self.papers.clone() {
            msg += &format!("\n\t• :page_facing_up: <{}|{}>", p.link, p.title,);
        }

        msg
    }
}

/// The data type for a paper.
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct Paper {
    pub title: String,
    pub link: String,
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

impl RFD {
    pub fn as_slack_msg(&self, num: i32) -> String {
        let mut msg = format!("RFD {} {} ({}) <https://{}.rfd.oxide.computer|github> <https://rfd.shared.oxide.computer/rfd/{}|rendered>", num, self.title, self.state, num, self.number);

        if !self.discussion.is_empty() {
            msg += &format!(" <{}|discussion>", self.discussion);
        }

        msg
    }
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

/// The data type for sending reminders for the product huddle meetings.
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct ProductEmailData {
    pub date: String,
    pub topics: Vec<DiscussionFields>,
    pub last_meeting_reports_link: String,
    pub meeting_id: String,
    pub should_send: bool,
}
