use chrono::naive::NaiveDate;
use chrono::offset::Utc;
use chrono::DateTime;
use chrono_humanize::HumanTime;
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
    pub value_reflected: usize,
    pub value_violated: usize,
    pub value_in_tension_1: usize,
    pub value_in_tension_2: usize,
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
    pub gitlab: String,
    pub portfolio: String,
    pub website: String,
    pub linkedin: String,
    pub resume: String,
    pub materials: String,
    pub status: String,
    pub received_application: bool,
    pub role: String,
    pub sheet_id: String,
    pub value_reflected: String,
    pub value_violated: String,
    pub values_in_tension: Vec<String>,
}

impl Applicant {
    pub fn as_slack_msg(&self) -> Value {
        let mut color = "#805AD5";
        match self.role.as_str() {
            "Product Engineering and Design" => color = "#48D597",
            "Technical Program Management" => color = "#667EEA",
            _ => (),
        }

        let dur = self.submitted_time - Utc::now();
        let time = HumanTime::from(dur);

        let mut status_msg = format!("<https://docs.google.com/spreadsheets/d/{}|{}> Applicant | applied {}", self.sheet_id, self.role, time);
        if !self.status.is_empty() {
            status_msg += &format!(" | status: *{}*", self.status);
        }

        let mut values_msg = "".to_string();
        if !self.value_reflected.is_empty() {
            values_msg +=
                &format!("values reflected: *{}*", self.value_reflected);
        }
        if !self.value_violated.is_empty() {
            values_msg += &format!(" | violated: *{}*", self.value_violated);
        }
        for (k, tension) in self.values_in_tension.iter().enumerate() {
            if k == 0 {
                values_msg += &format!(" | in tension: *{}*", tension);
            } else {
                values_msg += &format!(" *& {}*", tension);
            }
        }
        if values_msg.is_empty() {
            values_msg = "values not yet populated".to_string();
        }

        let mut intro_msg =
            format!("*{}*  <mailto:{}|{}>", self.name, self.email, self.email,);
        if !self.location.is_empty() {
            intro_msg += &format!("  {}", self.location);
        }

        let mut info_msg = format!(
            "<{}|resume> | <{}|materials>",
            self.resume, self.materials,
        );
        if !self.phone.is_empty() {
            info_msg += &format!(" | <tel:{}|{}>", self.phone, self.phone);
        }
        if !self.github.is_empty() {
            info_msg += &format!(
                " | <https://github.com/{}|github:{}>",
                self.github.trim_start_matches('@'),
                self.github,
            );
        }
        if !self.gitlab.is_empty() {
            info_msg += &format!(
                " | <https://gitlab.com/{}|gitlab:{}>",
                self.gitlab.trim_start_matches('@'),
                self.gitlab,
            );
        }
        if !self.linkedin.is_empty() {
            info_msg += &format!(" | <{}|linkedin>", self.linkedin,);
        }
        if !self.portfolio.is_empty() {
            info_msg += &format!(" | <{}|portfolio>", self.portfolio,);
        }
        if !self.website.is_empty() {
            info_msg += &format!(" | <{}|website>", self.website,);
        }

        json!({
            "response_type": "in_channel",
            "attachments": [
                {
                    "color": color,
                    "blocks": [
                        {
                            "type": "section",
                            "text": {
                                "type": "mrkdwn",
                                "text": intro_msg
                            }
                        },
                        {
                            "type": "context",
                            "elements": [
                                {
                                    "type": "mrkdwn",
                                    "text": info_msg
                                }
                            ]
                        },
                        {
                            "type": "context",
                            "elements": [
                                {
                                    "type": "mrkdwn",
                                    "text": values_msg
                                }
                            ]
                        },
                        {
                            "type": "context",
                            "elements": [
                                {
                                    "type": "mrkdwn",
                                    "text": status_msg
                                }
                            ]
                        }
                    ]
                }
            ]
        })
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
    pub fn as_slack_msg(&self) -> Value {
        let mut color = "#ED64A6";
        if self.state == "closed" {
            color = "#ED8936";
        }

        let mut objects: Vec<Value> = Default::default();

        if !self.recording.is_empty() {
            objects.push(json!({
                "elements": [{
                    "text": format!("<{}|Meeting recording>", self.recording),
                    "type": "mrkdwn"
                }],
                "type": "context"
            }));
        }

        for p in self.papers.clone() {
            let mut title = p.title.to_string();
            if p.title == self.title {
                title = "Paper".to_string();
            }
            objects.push(json!({
                "elements": [{
                    "text": format!("<{}|{}>", p.link, title),
                    "type": "mrkdwn"
                }],
                "type": "context"
            }));
        }

        json!({
            "response_type": "in_channel",
             "attachments": [{
                    "blocks": [{
                    "text": {
                        "text": format!("<{}|*{}*>", self.issue, self.title),
                        "type": "mrkdwn"
                    },
                    "type": "section"
                },
                {
                    "elements": [{
                        "text": "<https://github.com/oxidecomputer/papers/blob/master/os/countering-ipc-threats-minix3.pdf|Countering IPC Threats in Multiserver Operating Systems>",
                        "type": "mrkdwn"
                    }],
                    "type": "context"
                },
                {
                    "elements": [{
                        "text": format!("<https://github.com/{}|@{}> | {} | status: *{}*",self.coordinator,self.coordinator,self.date.format("%m/%d/%Y"),self.state),
                        "type": "mrkdwn"
                    }],
                    "type": "context"
                }],
                "color": color
            }]
        })
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
        let mut msg = format!("RFD {} {} (_*{}*_) <https://{}.rfd.oxide.computer|github> <https://rfd.shared.oxide.computer/rfd/{}|rendered>", num, self.title, self.state, num, self.number);

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
