#![allow(clippy::from_over_into)]
use std::str::from_utf8;

use async_trait::async_trait;
use chrono::NaiveDate;
use macros::db;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use slack_chat_api::{
    FormattedMessage, MessageBlock, MessageBlockText, MessageBlockType, MessageType,
};

use crate::{
    airtable::{AIRTABLE_JOURNAL_CLUB_MEETINGS_TABLE, AIRTABLE_JOURNAL_CLUB_PAPERS_TABLE},
    companies::Company,
    core::UpdateAirtableRecord,
    db::Database,
    schema::{journal_club_meetings, journal_club_papers},
    utils::get_file_content_from_repo,
};

/// The data type for a NewJournalClubMeeting.
#[db {
    new_struct_name = "JournalClubMeeting",
    airtable_base = "misc",
    airtable_table = "AIRTABLE_JOURNAL_CLUB_MEETINGS_TABLE",
    match_on = {
        "issue" = "String",
    },
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, Deserialize, Serialize)]
#[table_name = "journal_club_meetings"]
pub struct NewJournalClubMeeting {
    pub title: String,
    pub issue: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub papers: Vec<String>,
    #[serde(
        default = "crate::utils::default_date",
        deserialize_with = "crate::journal_clubs::meeting_date_format::deserialize",
        serialize_with = "crate::journal_clubs::meeting_date_format::serialize"
    )]
    pub issue_date: NaiveDate,
    #[serde(
        default = "crate::utils::default_date",
        deserialize_with = "crate::journal_clubs::meeting_date_format::deserialize",
        serialize_with = "crate::journal_clubs::meeting_date_format::serialize"
    )]
    pub meeting_date: NaiveDate,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub coordinator: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub state: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub recording: String,
    /// The CIO company ID.
    #[serde(default)]
    pub cio_company_id: i32,
}

impl JournalClubMeeting {
    /// Convert the journal club meeting into JSON as Slack message.
    pub fn as_slack_msg(&self) -> Value {
        let mut objects = vec![MessageBlock {
            block_type: MessageBlockType::Section,
            text: Some(MessageBlockText {
                text_type: MessageType::Markdown,
                text: format!("<{}|*{}*>", self.issue, self.title),
            }),
            elements: Default::default(),
            accessory: Default::default(),
            block_id: Default::default(),
            fields: Default::default(),
        }];

        let mut text = format!(
            "<https://github.com/{}|@{}> | issue date: {} | status: *{}*",
            self.coordinator,
            self.coordinator,
            self.issue_date.format("%m/%d/%Y"),
            self.state
        );
        let meeting_date = self.meeting_date.format("%m/%d/%Y").to_string();
        if meeting_date != *"01/01/1969" {
            text += &format!(" | meeting date: {}", meeting_date);
        }
        objects.push(MessageBlock {
            block_type: MessageBlockType::Context,
            elements: vec![MessageBlockText {
                text_type: MessageType::Markdown,
                text,
            }],
            text: Default::default(),
            accessory: Default::default(),
            block_id: Default::default(),
            fields: Default::default(),
        });

        if !self.recording.is_empty() {
            objects.push(MessageBlock {
                block_type: MessageBlockType::Context,
                elements: vec![MessageBlockText {
                    text_type: MessageType::Markdown,
                    text: format!("<{}|Meeting recording>", self.recording),
                }],
                text: Default::default(),
                accessory: Default::default(),
                block_id: Default::default(),
                fields: Default::default(),
            });
        }

        for paper in self.papers.clone() {
            let p: NewJournalClubPaper = serde_json::from_str(&paper).unwrap();

            let mut title = p.title.to_string();
            if p.title == self.title {
                title = "Paper".to_string();
            }
            objects.push(MessageBlock {
                block_type: MessageBlockType::Context,
                elements: vec![MessageBlockText {
                    text_type: MessageType::Markdown,
                    text: format!("<{}|{}>", p.link, title),
                }],
                text: Default::default(),
                accessory: Default::default(),
                block_id: Default::default(),
                fields: Default::default(),
            });
        }

        json!(FormattedMessage {
            channel: Default::default(),
            attachments: Default::default(),
            blocks: objects,
        })
    }
}

/// Implement updating the Airtable record for a JournalClubMeeting.
#[async_trait]
impl UpdateAirtableRecord<JournalClubMeeting> for JournalClubMeeting {
    async fn update_airtable_record(&mut self, record: JournalClubMeeting) {
        // Set the papers field, since it is pre-populated as table links.
        self.papers = record.papers;
    }
}

/// The data type for a NewJournalClubPaper.
#[db {
    new_struct_name = "JournalClubPaper",
    airtable_base = "misc",
    airtable_table = "AIRTABLE_JOURNAL_CLUB_PAPERS_TABLE",
    match_on = {
        "link" = "String",
    },
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, Deserialize, Serialize)]
#[table_name = "journal_club_papers"]
pub struct NewJournalClubPaper {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub link: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub meeting: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub link_to_meeting: Vec<String>,
    /// The CIO company ID.
    #[serde(default)]
    pub cio_company_id: i32,
}

/// Implement updating the Airtable record for a JournalClubPaper.
#[async_trait]
impl UpdateAirtableRecord<JournalClubPaper> for JournalClubPaper {
    async fn update_airtable_record(&mut self, _record: JournalClubPaper) {
        // Get the current journal club meetings in Airtable so we can link to it.
        // TODO: make this more dry so we do not call it every single damn time.
        let db = Database::new();
        let journal_club_meetings =
            JournalClubMeetings::get_from_airtable(&db, self.cio_company_id).await;

        // Iterate over the journal_club_meetings and see if we find a match.
        for (_id, meeting_record) in journal_club_meetings {
            if meeting_record.fields.issue == self.meeting {
                // Set the link_to_meeting to the right meeting.
                self.link_to_meeting = vec![meeting_record.id];
                // Break the loop and return early.
                break;
            }
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Meeting {
    pub title: String,
    pub issue: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub papers: Vec<NewJournalClubPaper>,
    #[serde(
        default = "crate::utils::default_date",
        deserialize_with = "meeting_date_format::deserialize",
        serialize_with = "meeting_date_format::serialize"
    )]
    pub issue_date: NaiveDate,
    #[serde(
        default = "crate::utils::default_date",
        deserialize_with = "meeting_date_format::deserialize",
        serialize_with = "meeting_date_format::serialize"
    )]
    pub meeting_date: NaiveDate,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub coordinator: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub state: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub recording: String,
}

pub mod meeting_date_format {
    use chrono::NaiveDate;
    use serde::{self, Deserialize, Deserializer, Serializer};

    const FORMAT: &str = "%m/%d/%Y";

    // The signature of a serialize_with function must follow the pattern:
    //
    //    fn serialize<S>(&T, S) -> Result<S::Ok, S::Error>
    //    where
    //        S: Serializer
    //
    // although it may also be generic over the input types T.
    pub fn serialize<S>(date: &NaiveDate, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut s = format!("{}", date.format(FORMAT));
        if *date == crate::utils::default_date() {
            s = "".to_string();
        }
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
        let s = String::deserialize(deserializer).unwrap_or_default();
        Ok(NaiveDate::parse_from_str(&s, FORMAT).unwrap_or_else(|_| crate::utils::default_date()))
    }
}

impl Meeting {
    pub fn to_model(&self, company: &Company) -> NewJournalClubMeeting {
        let mut papers: Vec<String> = Default::default();
        for p in &self.papers {
            let paper = serde_json::to_string_pretty(&p).unwrap();
            papers.push(paper);
        }

        NewJournalClubMeeting {
            title: self.title.to_string(),
            issue: self.issue.to_string(),
            papers,
            issue_date: self.issue_date,
            meeting_date: self.meeting_date,
            coordinator: self.coordinator.to_string(),
            state: self.state.to_string(),
            recording: self.recording.to_string(),
            cio_company_id: company.id,
        }
    }
}

/// Get the journal club meetings from the papers GitHub repo.
pub async fn get_meetings_from_repo(github: &octorust::Client, company: &Company) -> Vec<Meeting> {
    let owner = &company.github_org;
    let repo = "papers";

    // Get the contents of the .helpers/meetings.json file.
    let (meetings_json_content, _) = get_file_content_from_repo(
        github,
        owner,
        repo,
        // branch, empty means default
        "",
        "/.helpers/meetings.json",
    )
    .await;
    let meetings_json_string = from_utf8(&meetings_json_content).unwrap();

    // Parse the meetings from the json string.
    let meetings: Vec<Meeting> = serde_json::from_str(meetings_json_string).unwrap();

    meetings
}

// Sync the journal_club_meetings with our database.
pub async fn refresh_db_journal_club_meetings(
    db: &Database,
    github: &octorust::Client,
    company: &Company,
) {
    let journal_club_meetings = get_meetings_from_repo(github, company).await;

    // Sync journal_club_meetings.
    for journal_club_meeting in journal_club_meetings {
        journal_club_meeting.to_model(company).upsert(db).await;

        // Upsert the papers.
        for mut journal_club_paper in journal_club_meeting.papers {
            journal_club_paper.meeting = journal_club_meeting.issue.to_string();
            journal_club_paper.cio_company_id = company.id;
            journal_club_paper.upsert(db).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        companies::Company,
        db::Database,
        journal_clubs::{refresh_db_journal_club_meetings, JournalClubMeetings, JournalClubPapers},
    };

    #[ignore]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_cron_journal_club_meetings_and_papers() {
        let db = Database::new();

        // Get the company id for Oxide.
        // TODO: split this out per company.
        let oxide = Company::get_from_db(&db, "Oxide".to_string()).unwrap();

        let github = oxide.authenticate_github();

        refresh_db_journal_club_meetings(&db, &github, &oxide).await;

        JournalClubPapers::get_from_db(&db, oxide.id)
            .update_airtable(&db)
            .await;
        JournalClubMeetings::get_from_db(&db, oxide.id)
            .update_airtable(&db)
            .await;
    }
}
