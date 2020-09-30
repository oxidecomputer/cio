use std::collections::BTreeMap;
use std::str::from_utf8;

use airtable_api::{Airtable, Record};
use hubcaps::Github;

use crate::airtable::{
    airtable_api_key, AIRTABLE_BASE_ID_MISC, AIRTABLE_GRID_VIEW,
    AIRTABLE_JOURNAL_CLUB_MEETINGS_TABLE,
};
use crate::db::Database;
use crate::models::{JournalClubMeeting, NewJournalClubMeeting};
use crate::utils::github_org;

/// Get the journal club meetings from the papers GitHub repo.
pub async fn get_meetings_from_repo(
    github: &Github,
) -> Vec<NewJournalClubMeeting> {
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
    let meetings: Vec<NewJournalClubMeeting> =
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
        db.upsert_journal_club_meeting(&journal_club_meeting);
    }
}

pub async fn refresh_airtable_journal_club_meetings() {
    // Initialize the Airtable client.
    let airtable = Airtable::new(airtable_api_key(), AIRTABLE_BASE_ID_MISC);

    let records = airtable
        .list_records(
            AIRTABLE_JOURNAL_CLUB_MEETINGS_TABLE,
            AIRTABLE_GRID_VIEW,
            vec![],
        )
        .await
        .unwrap();

    let mut airtable_journal_club_meetings: BTreeMap<
        i32,
        (Record, JournalClubMeeting),
    > = Default::default();
    for record in records {
        let fields: JournalClubMeeting =
            serde_json::from_value(record.fields.clone()).unwrap();

        airtable_journal_club_meetings.insert(fields.id, (record, fields));
    }

    // Initialize our database.
    let db = Database::new();
    let journal_club_meetings = db.get_journal_club_meetings();

    let mut updated: i32 = 0;
    for journal_club_meeting in journal_club_meetings {
        // See if we have it in our fields.
        match airtable_journal_club_meetings.get(&journal_club_meeting.id) {
            Some((r, _in_airtable_fields)) => {
                let mut record = r.clone();

                record.fields = json!(journal_club_meeting);

                airtable
                    .update_records(
                        AIRTABLE_JOURNAL_CLUB_MEETINGS_TABLE,
                        vec![record.clone()],
                    )
                    .await
                    .unwrap();

                updated += 1;
            }
            None => {
                // Create the record.
                journal_club_meeting.push_to_airtable().await;
            }
        }
    }

    println!("updated {} journal_club_meetings", updated);
}

#[cfg(test)]
mod tests {
    use crate::journal_clubs::{
        refresh_airtable_journal_club_meetings,
        refresh_db_journal_club_meetings,
    };
    use crate::utils::authenticate_github;

    #[tokio::test(threaded_scheduler)]
    async fn test_journal_club_meetings() {
        let github = authenticate_github();
        refresh_db_journal_club_meetings(&github).await;
    }

    #[tokio::test(threaded_scheduler)]
    async fn test_journal_club_meetings_airtable() {
        refresh_airtable_journal_club_meetings().await;
    }
}
