use std::str::from_utf8;

use chrono::NaiveDate;
use csv::ReaderBuilder;
use hubcaps::Github;

use crate::models::{JournalClubMeeting, JournalClubPaper};
use crate::utils::github_org;

/// Get the journal club meetings from the papers GitHub repo.
pub async fn get_meetings_from_repo(
    github: &Github,
) -> Vec<JournalClubMeeting> {
    // Get the contents of the .helpers/meetings.csv file.
    let meetings_csv_content = github
        .repo(github_org(), "papers")
        .content()
        .file("/.helpers/meetings.csv")
        .await
        .expect("failed to get meetings csv content")
        .content;
    let meetings_csv_string = from_utf8(&meetings_csv_content).unwrap();

    // Create the csv reader.
    let mut csv_reader = ReaderBuilder::new()
        .delimiter(b',')
        .has_headers(true)
        .from_reader(meetings_csv_string.as_bytes());

    // Create the BTreeMap of Meetings.
    let mut meetings: Vec<JournalClubMeeting> = Default::default();
    // TODO: deserialize this in a real way.
    for r in csv_reader.records() {
        let record = r.unwrap();

        // Parse the date.
        let date = NaiveDate::parse_from_str(&record[5], "%m/%d/%Y").unwrap();

        // Parse the papers.
        let mut papers: Vec<JournalClubPaper> = Default::default();
        let papers_parts = record[2].trim().split(") [");
        for p in papers_parts {
            // Parse the markdown for the papers.
            let start_title = p.find('[').unwrap_or(0);
            let end_title = p.find(']').unwrap_or_else(|| p.len());
            let title = p[start_title..end_title]
                .trim_start_matches('[')
                .trim_end_matches(']')
                .to_string();

            let start_link = p.find('(').unwrap_or(0);
            let end_link = p.find(')').unwrap_or_else(|| p.len());
            let link = p[start_link..end_link]
                .trim_start_matches('(')
                .trim_end_matches(')')
                .to_string();

            papers.push(JournalClubPaper { title, link });
        }

        let meeting = JournalClubMeeting {
            title: record[0].to_string(),
            issue: record[1].to_string(),
            papers,
            coordinator: record[3].to_string(),
            state: record[4].to_string(),
            date,
            recording: record[6].to_string(),
        };

        // Add this to our Vec.
        meetings.push(meeting);
    }

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
