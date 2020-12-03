use std::str::from_utf8;

use chrono::NaiveDate;
use hubcaps::Github;
use serde::{Deserialize, Serialize};

use crate::db::Database;
use crate::models::{NewJournalClubMeeting, NewJournalClubPaper};
use crate::utils::github_org;

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

mod meeting_date_format {
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
    pub fn serialize<S>(
        date: &NaiveDate,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
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
        let s = String::deserialize(deserializer).unwrap();
        Ok(NaiveDate::parse_from_str(&s, FORMAT)
            .unwrap_or(crate::utils::default_date()))
    }
}

impl Meeting {
    pub fn to_model(&self) -> NewJournalClubMeeting {
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
        }
    }
}

/// Get the journal club meetings from the papers GitHub repo.
pub async fn get_meetings_from_repo(github: &Github) -> Vec<Meeting> {
    // Get the contents of the .helpers/meetings.csv file.
    let meetings_csv_content = github
        .repo(github_org(), "papers")
        .content()
        .file("/.helpers/meetings.json", "master")
        .await
        .expect("failed to get meetings csv content")
        .content;
    let meetings_json_string = from_utf8(&meetings_csv_content).unwrap();

    // Parse the meetings from the json string.
    let meetings: Vec<Meeting> =
        serde_json::from_str(meetings_json_string).unwrap();

    meetings
}

// Sync the journal_club_meetings with our database.
pub async fn refresh_db_journal_club_meetings(github: &Github) {
    let journal_club_meetings = get_meetings_from_repo(github).await;

    // Initialize our database.
    let db = Database::new();

    // Sync journal_club_meetings.
    for journal_club_meeting in journal_club_meetings {
        db.upsert_journal_club_meeting(&journal_club_meeting.to_model());

        // Upsert the papers.
        for mut journal_club_paper in journal_club_meeting.papers {
            journal_club_paper.meeting = journal_club_meeting.issue.to_string();
            db.upsert_journal_club_paper(&journal_club_paper);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::db::Database;
    use crate::journal_clubs::refresh_db_journal_club_meetings;
    use crate::models::{JournalClubMeetings, JournalClubPapers};
    use crate::utils::authenticate_github;

    #[tokio::test(threaded_scheduler)]
    async fn test_cron_journal_club_meetings() {
        let github = authenticate_github();
        refresh_db_journal_club_meetings(&github).await;
    }

    #[tokio::test(threaded_scheduler)]
    async fn test_cron_journal_club_meetings_airtable() {
        // Initialize our database.
        let db = Database::new();

        let journal_club_meetings = db.get_journal_club_meetings();
        // Update journal club meetings in airtable.
        JournalClubMeetings(journal_club_meetings)
            .update_airtable()
            .await;
    }

    #[tokio::test(threaded_scheduler)]
    async fn test_cron_journal_club_papers_airtable() {
        // Initialize our database.
        let db = Database::new();

        let journal_club_papers = db.get_journal_club_papers();
        // Update journal club papers in airtable.
        JournalClubPapers(journal_club_papers)
            .update_airtable()
            .await;
    }
}
