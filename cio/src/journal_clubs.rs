use std::str::from_utf8;

use hubcaps::Github;

use crate::db::Database;
use crate::models::NewJournalClubMeeting;
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
    for (_, journal_club_meeting) in journal_club_meetings {
        db.upsert_journal_club_meeting(&journal_club_meeting);
    }
}

#[cfg(test)]
mod tests {
    use crate::journal_clubs::refresh_db_journal_club_meetings;
    use crate::utils::authenticate_github;

    #[tokio::test(threaded_scheduler)]
    async fn test_meetings() {
        let github = authenticate_github();
        refresh_db_journal_club_meetings(&github).await;
    }
}
