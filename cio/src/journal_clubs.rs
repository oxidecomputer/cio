use std::str::from_utf8;

use hubcaps::Github;

use crate::models::JournalClubMeeting;
use crate::utils::github_org;

/// Get the journal club meetings from the papers GitHub repo.
pub async fn get_meetings_from_repo(
    github: &Github,
) -> Vec<JournalClubMeeting> {
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
    let meetings: Vec<JournalClubMeeting> =
        serde_json::from_str(meetings_json_string).unwrap();

    meetings
}

#[cfg(test)]
mod tests {
    use crate::journal_clubs::get_meetings_from_repo;
    use crate::utils::authenticate_github;

    #[tokio::test(threaded_scheduler)]
    async fn test_meetings() {
        let github = authenticate_github();
        let meetings = get_meetings_from_repo(&github).await;
        println!("{:?}", meetings);
    }
}
