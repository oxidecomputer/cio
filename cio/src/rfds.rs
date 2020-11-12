use airtable_api::{Airtable, Record};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::{stderr, stdout, Write};
use std::process::Command;
use std::str::from_utf8;

use comrak::{markdown_to_html, ComrakOptions};
use csv::ReaderBuilder;
use google_drive::GoogleDrive;
use hubcaps::Github;
use regex::Regex;

use crate::airtable::{
    airtable_api_key, AIRTABLE_BASE_ID_RACK_ROADMAP, AIRTABLE_GRID_VIEW,
    AIRTABLE_RFD_TABLE,
};
use crate::db::Database;
use crate::models::{NewRFD, RFD};
use crate::utils::{get_gsuite_token, github_org};

/// Get the RFDs from the rfd GitHub repo.
pub async fn get_rfds_from_repo(github: &Github) -> BTreeMap<i32, NewRFD> {
    // Get the contents of the .helpers/rfd.csv file.
    let rfd_csv_content = github
        .repo(github_org(), "rfd")
        .content()
        .file("/.helpers/rfd.csv", "master")
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
        rfd.expand(github).await;

        // Add this to our BTreeMap.
        rfds.insert(rfd.number, rfd);
    }

    rfds
}

/// Try to get the markdown or asciidoc contents from the repo.
pub async fn get_rfd_contents_from_repo(
    github: &Github,
    branch: &str,
    dir: &str,
) -> (String, bool, String) {
    let repo_contents = github.repo(github_org(), "rfd").content();
    let mut is_markdown = false;
    let decoded: String;
    let sha: String;

    // Get the contents of the file.
    let path = format!("{}/README.adoc", dir);
    match repo_contents.file(&path, branch).await {
        Ok(contents) => {
            decoded = from_utf8(&contents.content).unwrap().trim().to_string();
            sha = contents.sha;
        }
        Err(e) => {
            println!("[rfd] getting file contents for {} failed: {}, trying markdown instead...", path, e);

            // Try to get the markdown instead.
            is_markdown = true;
            let contents = repo_contents
                .file(&format!("{}/README.md", dir), branch)
                .await
                .unwrap();

            decoded = from_utf8(&contents.content).unwrap().trim().to_string();
            sha = contents.sha;
        }
    }

    (decoded, is_markdown, sha)
}

pub fn parse_markdown(content: &str) -> String {
    markdown_to_html(content, &ComrakOptions::default())
}

pub fn parse_asciidoc(content: &str) -> String {
    let mut path = env::temp_dir();
    path.push("contents.adoc");

    // Write the contents to a temporary file.
    let mut file = fs::File::create(path.clone()).unwrap();
    file.write_all(content.as_bytes()).unwrap();

    let cmd_output = Command::new("asciidoctor")
        .args(&["-o", "-", "--no-header-footer", path.to_str().unwrap()])
        .output()
        .unwrap();

    let result = if cmd_output.status.success() {
        from_utf8(&cmd_output.stdout).unwrap()
    } else {
        println!("[rfds] running asciidoctor failed:");
        stdout().write_all(&cmd_output.stdout).unwrap();
        stderr().write_all(&cmd_output.stderr).unwrap();

        Default::default()
    };

    // Delete our temporary file.
    if path.exists() && !path.is_dir() {
        fs::remove_file(path).unwrap();
    }

    result.to_string()
}

pub fn clean_rfd_html_links(content: &str, num: &str) -> String {
    let mut cleaned = content
        .replace(r#"href="\#"#, &format!(r#"href="/rfd/{}#"#, num))
        .replace("href=\"#", &format!("href=\"/rfd/{}#", num))
        .replace(
            r#"img src=""#,
            &format!(r#"img src="/static/images/{}/"#, num),
        )
        .replace(
            r#"object data=""#,
            &format!(r#"object data="/static/images/{}/"#, num),
        )
        .replace(
            r#"object type="image/svg+xml" data=""#,
            &format!(
                r#"object type="image/svg+xml" data="/static/images/{}/"#,
                num
            ),
        );

    let mut re =
        Regex::new(r"https://(?P<num>[0-9]).rfd.oxide.computer").unwrap();
    cleaned = re
        .replace_all(&cleaned, "https://rfd.shared.oxide.computer/rfd/000$num")
        .to_string();
    re = Regex::new(r"https://(?P<num>[0-9][0-9]).rfd.oxide.computer").unwrap();
    cleaned = re
        .replace_all(&cleaned, "https://rfd.shared.oxide.computer/rfd/00$num")
        .to_string();
    re = Regex::new(r"https://(?P<num>[0-9][0-9][0-9]).rfd.oxide.computer")
        .unwrap();
    cleaned = re
        .replace_all(&cleaned, "https://rfd.shared.oxide.computer/rfd/0$num")
        .to_string();
    re =
        Regex::new(r"https://(?P<num>[0-9][0-9][0-9][0-9]).rfd.oxide.computer")
            .unwrap();
    cleaned = re
        .replace_all(&cleaned, "https://rfd.shared.oxide.computer/rfd/$num")
        .to_string();

    cleaned
}

pub fn get_authors(content: &str, is_markdown: bool) -> String {
    if is_markdown {
        // TODO: make work w asciidoc.
        let re = Regex::new(r"(?m)(^authors.*$)").unwrap();
        match re.find(&content) {
            Some(v) => {
                return v.as_str().replace("authors:", "").trim().to_string()
            }
            None => return Default::default(),
        }
    }

    // We must have asciidoc content.
    // We want to find the line under the first "=" line (which is the title), authors is under
    // that.
    let re = Regex::new(r"(?m:^=.*$)[\n\r](?m)(.*$)").unwrap();
    match re.find(&content) {
        Some(v) => {
            let val = v.as_str().trim().to_string();
            let parts: Vec<&str> = val.split('\n').collect();
            if parts.len() < 2 {
                Default::default()
            } else {
                parts[1].to_string()
            }
        }
        None => Default::default(),
    }
}

pub async fn refresh_airtable_rfds() {
    // Initialize the Airtable client.
    let airtable =
        Airtable::new(airtable_api_key(), AIRTABLE_BASE_ID_RACK_ROADMAP);

    let records: Vec<Record<RFD>> = airtable
        .list_records(AIRTABLE_RFD_TABLE, AIRTABLE_GRID_VIEW, vec![])
        .await
        .unwrap();

    let mut airtable_rfds: BTreeMap<i32, Record<RFD>> = Default::default();
    for record in records {
        airtable_rfds.insert(record.fields.id, record);
    }

    // Initialize our database.
    let db = Database::new();
    let rfds = db.get_rfds();

    let mut updated: i32 = 0;
    for mut rfd in rfds {
        // See if we have it in our fields.
        match airtable_rfds.get(&rfd.id) {
            Some(r) => {
                let mut record = r.clone();

                // Set the Link to People from the original so it stays intact.
                rfd.milestones = r.fields.milestones.clone();
                rfd.relevant_components = r.fields.relevant_components.clone();
                // Airtable can only hold 100,000 chars. IDK which one is that long but LOL
                // https://community.airtable.com/t/what-is-the-long-text-character-limit/1780
                rfd.content = truncate(&rfd.content, 100000);
                rfd.html = truncate(&rfd.html, 100000);

                record.fields = rfd;

                airtable
                    .update_records(AIRTABLE_RFD_TABLE, vec![record.clone()])
                    .await
                    .unwrap();

                updated += 1;
            }
            None => {
                // Create the record.
                rfd.push_to_airtable().await;
            }
        }
    }

    println!("updated {} rfds", updated);
}

fn truncate(s: &str, max_chars: usize) -> String {
    match s.char_indices().nth(max_chars) {
        None => s.to_string(),
        Some((idx, _)) => s[..idx].to_string(),
    }
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

// Sync the rfds from the database as rendered PDFs in GitHub.
pub async fn refresh_rfd_pdfs(github: &Github) {
    // Get gsuite token.
    let token = get_gsuite_token().await;

    // Initialize the Google Drive client.
    let drive_client = GoogleDrive::new(token);

    // Figure out where our directory is.
    // It should be in the shared drive : "Automated Documents"/"rfds"
    let drive = drive_client
        .get_drive_by_name("Automated Documents")
        .await
        .unwrap();
    let drive_id = drive.id;

    // Get the directory by the name.
    let dir = drive_client
        .get_file_by_name(&drive_id, "rfds")
        .await
        .unwrap();

    // Initialize our database.
    let db = Database::new();

    let rfds = db.get_rfds();

    // Sync rfds.
    for rfd in rfds {
        rfd.convert_and_upload_pdf(
            github,
            &drive_client,
            &drive_id,
            &dir.get(0).unwrap().id,
        )
        .await;
    }
}

#[cfg(test)]
mod tests {
    use crate::rfds::{
        clean_rfd_html_links, get_authors, refresh_airtable_rfds,
        refresh_db_rfds, refresh_rfd_pdfs,
    };
    use crate::utils::authenticate_github;

    #[tokio::test(threaded_scheduler)]
    async fn test_rfds() {
        let github = authenticate_github();
        refresh_db_rfds(&github).await;
    }

    #[test]
    fn test_clean_rfd_html_links() {
        let content = r#"https://3.rfd.oxide.computer
        https://41.rfd.oxide.computer
        https://543.rfd.oxide.computer#-some-link
        https://3245.rfd.oxide.computer/things
        https://3265.rfd.oxide.computer/things
        <img src="things.png" \>
        <a href="\#_principles">
        <object data="thing.svg">
        <object type="image/svg+xml" data="thing.svg">
        <a href="\#things" \>"#;

        let cleaned = clean_rfd_html_links(&content, "0032");

        let expected = r#"https://rfd.shared.oxide.computer/rfd/0003
        https://rfd.shared.oxide.computer/rfd/0041
        https://rfd.shared.oxide.computer/rfd/0543#-some-link
        https://rfd.shared.oxide.computer/rfd/3245/things
        https://rfd.shared.oxide.computer/rfd/3265/things
        <img src="/static/images/0032/things.png" \>
        <a href="/rfd/0032#_principles">
        <object data="/static/images/0032/thing.svg">
        <object type="image/svg+xml" data="/static/images/0032/thing.svg">
        <a href="/rfd/0032#things" \>"#;

        assert_eq!(expected, cleaned);
    }

    #[test]
    fn test_get_authors() {
        let mut content = r#"sdfsdf
sdfsdf
authors: things, joe
dsfsdf
sdf
authors: nope"#;
        let mut authors = get_authors(&content, true);
        let mut expected = "things, joe".to_string();
        assert_eq!(expected, authors);

        content = r#"sdfsdf
= sdfgsdfgsdfg
things, joe
dsfsdf
sdf
:authors: nope"#;
        authors = get_authors(&content, true);
        expected = "".to_string();
        assert_eq!(expected, authors);

        content = r#"sdfsdf
= sdfgsdfgsdfg
things <things@email.com>, joe <joe@email.com>
dsfsdf
sdf
authors: nope"#;
        authors = get_authors(&content, false);
        expected =
            r#"things <things@email.com>, joe <joe@email.com>"#.to_string();
        assert_eq!(expected, authors);
    }

    #[tokio::test(threaded_scheduler)]
    async fn test_rfds_airtable() {
        refresh_airtable_rfds().await;
    }

    #[tokio::test(threaded_scheduler)]
    async fn test_rfd_pdfs() {
        let github = authenticate_github();
        refresh_rfd_pdfs(&github).await;
    }
}
