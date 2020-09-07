use std::collections::BTreeMap;
use std::str::from_utf8;

use csv::ReaderBuilder;
use hubcaps::Github;

use crate::db::Database;
use crate::models::NewRFD;
use crate::utils::github_org;

/// Get the RFDs from the rfd GitHub repo.
pub async fn get_rfds_from_repo(github: &Github) -> BTreeMap<i32, NewRFD> {
    // Get the contents of the .helpers/rfd.csv file.
    let rfd_csv_content = github
        .repo(github_org(), "rfd")
        .content()
        .file("/.helpers/rfd.csv")
        .await
        .expect("failed to get rfd csv content")
        .content;
    let rfd_csv_string = from_utf8(&rfd_csv_content).unwrap();

    // Create the csv reader.
    let mut csv_reader = ReaderBuilder::new()
        .delimiter(b',')
        .has_headers(true)
        .from_reader(rfd_csv_string.as_bytes());

    // Create the BTreeMap of RFDs.
    let mut rfds: BTreeMap<i32, NewRFD> = Default::default();
    for r in csv_reader.deserialize() {
        let mut rfd: NewRFD = r.unwrap();

        // Expand the fields in the RFD.
        rfd.expand();

        // Add this to our BTreeMap.
        rfds.insert(rfd.number, rfd);
    }

    rfds
}

// Sync the rfds with our database.
pub async fn refresh_db_rfds(github: &Github) {
    let rfds = get_rfds_from_repo(github).await;

    // Initialize our database.
    let db = Database::new();

    // Sync rfds.
    for (_, rfd) in rfds {
        db.upsert_rfd(&rfd);
    }
}

#[cfg(test)]
mod tests {
    use crate::rfds::refresh_db_rfds;
    use crate::utils::authenticate_github;

    #[tokio::test(threaded_scheduler)]
    async fn test_rfds() {
        let github = authenticate_github();
        refresh_db_rfds(&github).await;
    }
}
