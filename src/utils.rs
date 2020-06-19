use chrono::naive::NaiveDate;
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::str::from_utf8;

use clap::ArgMatches;
use csv::ReaderBuilder;
use hubcaps::http_cache::FileBasedCache;
use hubcaps::{Credentials, Github};
use log::info;
use reqwest::Client;
use yup_oauth2::{
    read_service_account_key, AccessToken, ServiceAccountAuthenticator,
};

use crate::core::{JournalClubMeeting, Paper, RFD};

use cio::Config;

/// Write a file.
pub fn write_file(file: PathBuf, contents: String) {
    // create each directory.
    fs::create_dir_all(file.parent().unwrap()).unwrap();

    // Write to the file.
    let mut f = fs::File::create(file.clone()).unwrap();
    f.write_all(contents.as_bytes()).unwrap();

    info!("wrote file: {}", file.to_str().unwrap());
}

/// Read and decode the config from the files that are passed on the command line.
pub fn read_config_from_files(cli_matches: &ArgMatches) -> Config {
    let files: Vec<String>;
    match cli_matches.values_of("file") {
        None => panic!("no configuration files specified"),
        Some(val) => {
            files = val.map(|s| s.to_string()).collect();
        }
    };

    let mut contents = String::from("");
    for file in files.iter() {
        info!("decoding {}", file);

        // Read the file.
        let body = fs::read_to_string(file).expect("reading the file failed");

        // Append the body of the file to the rest of the contents.
        contents.push_str(&body);
    }

    // Decode the contents.
    let config: Config = toml::from_str(&contents).unwrap();

    config
}

/// Get a GSuite token.
pub async fn get_gsuite_token() -> AccessToken {
    // Get the GSuite credentials file.
    let gsuite_credential_file = env::var("GADMIN_CREDENTIAL_FILE").unwrap();
    let gsuite_subject = env::var("GADMIN_SUBJECT").unwrap();
    let gsuite_secret = read_service_account_key(gsuite_credential_file)
        .await
        .expect("failed to read gsuite credential file");
    let auth = ServiceAccountAuthenticator::builder(gsuite_secret)
        .subject(gsuite_subject.to_string())
        .build()
        .await
        .expect("failed to create authenticator");

    // Add the scopes to the secret and get the token.
    let token = auth
        .token(&[
            "https://www.googleapis.com/auth/admin.directory.group",
            "https://www.googleapis.com/auth/admin.directory.resource.calendar",
            "https://www.googleapis.com/auth/admin.directory.user",
            "https://www.googleapis.com/auth/apps.groups.settings",
            "https://www.googleapis.com/auth/spreadsheets",
            "https://www.googleapis.com/auth/drive",
        ])
        .await
        .expect("failed to get token");

    if token.as_str().is_empty() {
        panic!("empty token is not valid");
    }

    token
}

/// Authenticate with GitHub.
pub fn authenticate_github() -> Github {
    // Initialize the github client.
    let github_token = env::var("GITHUB_TOKEN").unwrap();
    // Get the current working directory.
    let curdir = env::current_dir().unwrap();
    // Create the HTTP cache.
    let http_cache =
        Box::new(FileBasedCache::new(curdir.join(".cache/github")));
    Github::custom(
        "https://api.github.com",
        concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")),
        Credentials::Token(github_token),
        Client::builder().build().unwrap(),
        http_cache,
    )
}

/// Get the Journal Club meetings from the papers GitHub repo.
pub async fn get_journal_club_meetings_from_repo(
    github: &Github,
) -> Vec<JournalClubMeeting> {
    let github_org = env::var("GITHUB_ORG").unwrap();

    // Get the contents of the .helpers/meetings.csv file.
    let meetings_csv_content = github
        .repo(github_org, "papers")
        .content()
        .file(".helpers/meetings.csv")
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
    for r in csv_reader.records() {
        let record = r.unwrap();

        // Parse the date.
        let date = NaiveDate::parse_from_str(&record[5], "%m/%d/%Y").unwrap();

        // Parse the papers.
        let mut papers: Vec<Paper> = Default::default();
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

            papers.push(Paper { title, link });
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

/// The warning for files that we automatically generate so folks don't edit them
/// all willy nilly.
pub static TEMPLATE_WARNING: &str =
    "# THIS FILE HAS BEEN GENERATED BY THE CONFIGS REPO
# AND SHOULD NEVER BE EDITED BY HAND!!
# Instead change the link in configs/links.toml

";
