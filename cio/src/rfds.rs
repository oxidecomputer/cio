use std::collections::BTreeMap;
use std::env;
use std::str::from_utf8;

use csv::ReaderBuilder;
use hubcaps::Github;
use serde::{Deserialize, Serialize};

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

/// Get the RFDs from the rfd GitHub repo.
pub async fn get_rfds_from_repo(github: &Github) -> BTreeMap<i32, RFD> {
    let github_org = env::var("GITHUB_ORG").unwrap();

    // Get the contents of the .helpers/rfd.csv file.
    let rfd_csv_content = github
        .repo(github_org, "rfd")
        .content()
        .file(".helpers/rfd.csv")
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
    let mut rfds: BTreeMap<i32, RFD> = Default::default();
    for r in csv_reader.records() {
        let record = r.unwrap();
        // Add this to our BTreeMap.
        rfds.insert(
            record[0].to_string().parse::<i32>().unwrap(),
            RFD {
                number: record[0].to_string(),
                title: record[1].to_string(),
                link: record[2].to_string(),
                state: record[3].to_string(),
                discussion: record[4].to_string(),
            },
        );
    }

    rfds
}
