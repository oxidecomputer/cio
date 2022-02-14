#![allow(clippy::from_over_into)]
use std::{
    collections::BTreeMap,
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    str::from_utf8,
};

use anyhow::{anyhow, bail, Result};
use async_recursion::async_recursion;
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use comrak::{markdown_to_html, ComrakOptions};
use csv::ReaderBuilder;
use google_drive::traits::{DriveOps, FileOps};
use log::{info, warn};
use macros::db;
use regex::Regex;
use schemars::JsonSchema;
use sendgrid_api::{traits::MailOps, Client as SendGrid};
use serde::{Deserialize, Serialize};
use slack_chat_api::{FormattedMessage, MessageBlock, MessageBlockText, MessageBlockType, MessageType};

use crate::{
    airtable::AIRTABLE_RFD_TABLE,
    companies::Company,
    core::{GitHubPullRequest, UpdateAirtableRecord},
    db::Database,
    repos::FromUrl,
    schema::rfds as r_f_ds,
    schema::rfds,
    utils::{
        create_or_update_file_in_github_repo, decode_base64, decode_base64_to_string, get_file_content_from_repo,
        truncate, write_file,
    },
};

/// The data type for an RFD.
#[db {
    new_struct_name = "RFD",
    airtable_base = "roadmap",
    airtable_table = "AIRTABLE_RFD_TABLE",
    match_on = {
        "number" = "i32",
    }
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[table_name = "rfds"]
pub struct NewRFD {
    // TODO: remove this alias when we update https://github.com/oxidecomputer/rfd/blob/master/.helpers/rfd.csv
    // When you do this you need to update src/components/images.js in the rfd repo as well.
    // those are the only two things remaining that parse the CSV directly.
    #[serde(alias = "num")]
    pub number: i32,
    /// (generated) number_string is the long version of the number with leading zeros
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub number_string: String,
    pub title: String,
    /// (generated) name is a combination of number and title.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    pub state: String,
    /// link is the canonical link to the source.
    pub link: String,
    /// (generated) short_link is the generated link in the form of https://{number}.rfd.oxide.computer
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub short_link: String,
    /// (generated) rendered_link is the link to the rfd in the rendered html website in the form of
    /// https://rfd.shared.oxide.computer/rfd/{{number_string}}
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub rendered_link: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub discussion: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub authors: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub html: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub content: String,
    /// sha is the SHA of the last commit that modified the file
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sha: String,
    /// commit_date is the date of the last commit that modified the file
    #[serde(default = "Utc::now")]
    pub commit_date: DateTime<Utc>,
    /// milestones only exist in Airtable
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub milestones: Vec<String>,
    /// relevant_components only exist in Airtable
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relevant_components: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pdf_link_github: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pdf_link_google_drive: String,
    /// The section IDs of the RFD. This is set elsewhere.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rfd_sections_id: Vec<i32>,
    /// The CIO company ID.
    #[serde(default)]
    pub cio_company_id: i32,
}

impl NewRFD {
    /// Return a NewRFD from a parsed file on a specific GitHub branch.
    pub async fn new_from_github(
        company: &Company,
        github: &octorust::Client,
        owner: &str,
        repo: &str,
        branch: &str,
        file_path: &str,
        commit_date: DateTime<Utc>,
    ) -> Result<Self> {
        // Get the file from GitHub.
        let mut content = String::new();
        let mut link = String::new();
        let mut sha = String::new();
        if let Ok(f) = github.repos().get_content_file(owner, repo, file_path, branch).await {
            content = decode_base64_to_string(&f.content);
            link = f.html_url.to_string();
            sha = f.sha;
        }

        // Parse the RFD directory as an int.
        let (dir, _) = file_path.trim_start_matches("rfd/").split_once('/').unwrap();
        let number = dir.trim_start_matches('0').parse::<i32>()?;

        let number_string = NewRFD::generate_number_string(number);

        // Parse the RFD title from the contents.
        let title = NewRFD::get_title(&content)?;
        let name = NewRFD::generate_name(number, &title);

        // Parse the state from the contents.
        let state = NewRFD::get_state(&content)?;

        // Parse the discussion from the contents.
        let discussion = NewRFD::get_discussion(&content)?;

        Ok(NewRFD {
            number,
            number_string,
            title,
            name,
            state,
            link,
            short_link: Default::default(),
            rendered_link: Default::default(),
            discussion,
            authors: Default::default(),
            // We parse this below.
            html: Default::default(),
            content,
            sha,
            commit_date,
            // Only exists in Airtable,
            milestones: Default::default(),
            // Only exists in Airtable,
            relevant_components: Default::default(),
            pdf_link_github: Default::default(),
            pdf_link_google_drive: Default::default(),
            rfd_sections_id: Default::default(),
            cio_company_id: company.id,
        })
    }

    pub fn get_title(content: &str) -> Result<String> {
        let mut re = Regex::new(r"(?m)(RFD .*$)")?;
        match re.find(content) {
            Some(v) => {
                // TODO: find less horrible way to do this.
                let trimmed = v
                    .as_str()
                    .replace("RFD", "")
                    .replace("# ", "")
                    .replace("= ", " ")
                    .trim()
                    .to_string();

                let (_, s) = trimmed.split_once(' ').unwrap();

                // If the string is empty, it means there is no RFD in our
                // title.
                if s.is_empty() {}

                Ok(s.to_string())
            }
            None => {
                // There is no "RFD" in our title. This is the case for RFD 31.
                re = Regex::new(r"(?m)(^= .*$)")?;
                let c = re.find(content);
                if c.is_none() {
                    // If we couldn't find anything assume we have no title.
                    // This was related to this error in Sentry:
                    // https://sentry.io/organizations/oxide-computer-company/issues/2701636092/?project=-1
                    return Ok(String::new());
                }
                let results = c.unwrap();

                Ok(results
                    .as_str()
                    .replace("RFD", "")
                    .replace("# ", "")
                    .replace("= ", " ")
                    .trim()
                    .to_string())
            }
        }
    }

    pub fn get_state(content: &str) -> Result<String> {
        let re = Regex::new(r"(?m)(state:.*$)")?;
        match re.find(content) {
            Some(v) => return Ok(v.as_str().replace("state:", "").trim().to_string()),
            None => Ok(Default::default()),
        }
    }

    pub fn get_discussion(content: &str) -> Result<String> {
        let re = Regex::new(r"(?m)(discussion:.*$)")?;
        match re.find(content) {
            Some(v) => {
                let d = v.as_str().replace("discussion:", "").trim().to_string();
                if !d.starts_with("http") {
                    return Ok(Default::default());
                }
                Ok(d)
            }
            None => Ok(Default::default()),
        }
    }

    pub fn generate_number_string(number: i32) -> String {
        // Add leading zeros to the number for the number_string.
        let mut number_string = number.to_string();
        while number_string.len() < 4 {
            number_string = format!("0{}", number_string);
        }

        number_string
    }

    pub fn generate_name(number: i32, title: &str) -> String {
        format!("RFD {} {}", number, title)
    }

    pub fn generate_short_link(number: i32) -> String {
        format!("https://{}.rfd.oxide.computer", number)
    }

    pub fn generate_rendered_link(number_string: &str) -> String {
        format!("https://rfd.shared.oxide.computer/rfd/{}", number_string)
    }

    pub fn get_authors(content: &str, is_markdown: bool) -> Result<String> {
        if is_markdown {
            // TODO: make work w asciidoc.
            let re = Regex::new(r"(?m)(^authors.*$)")?;
            match re.find(content) {
                Some(v) => return Ok(v.as_str().replace("authors:", "").trim().to_string()),
                None => return Ok(Default::default()),
            }
        }

        // We must have asciidoc content.
        // We want to find the line under the first "=" line (which is the title), authors is under
        // that.
        let re = Regex::new(r"(?m:^=.*$)[\n\r](?m)(.*$)")?;
        match re.find(content) {
            Some(v) => {
                let val = v.as_str().trim().to_string();
                let parts: Vec<&str> = val.split('\n').collect();
                if parts.len() < 2 {
                    Ok(Default::default())
                } else {
                    let mut authors = parts[1].to_string();
                    if authors == "{authors}" {
                        // Do the traditional check.
                        let re = Regex::new(r"(?m)(^:authors.*$)")?;
                        if let Some(v) = re.find(content) {
                            authors = v.as_str().replace(":authors:", "").trim().to_string();
                        }
                    }
                    Ok(authors)
                }
            }
            None => Ok(Default::default()),
        }
    }
}

/// Convert an RFD into Slack message.
impl From<NewRFD> for FormattedMessage {
    fn from(item: NewRFD) -> Self {
        let mut msg = format!(
            "{} (_*{}*_) <{}|github> <{}|rendered>",
            item.name, item.state, item.short_link, item.rendered_link
        );

        if !item.discussion.is_empty() {
            msg += &format!(" <{}|discussion>", item.discussion);
        }

        FormattedMessage {
            channel: Default::default(),
            attachments: Default::default(),
            blocks: vec![MessageBlock {
                block_type: MessageBlockType::Section,
                text: Some(MessageBlockText {
                    text_type: MessageType::Markdown,
                    text: msg,
                }),
                elements: Default::default(),
                accessory: Default::default(),
                block_id: Default::default(),
                fields: Default::default(),
            }],
        }
    }
}

impl From<RFD> for FormattedMessage {
    fn from(item: RFD) -> Self {
        let new: NewRFD = item.into();
        new.into()
    }
}

impl RFD {
    pub async fn get_html(
        &self,
        github: &octorust::Client,
        owner: &str,
        repo: &str,
        branch: &str,
        is_markdown: bool,
    ) -> Result<String> {
        let html: String = if is_markdown {
            // Parse the markdown.
            parse_markdown(&self.content)
        } else {
            // Parse the acsiidoc.
            self.parse_asciidoc(github, owner, repo, branch).await?
        };

        clean_rfd_html_links(&html, &self.number_string)
    }

    pub async fn parse_asciidoc(
        &self,
        github: &octorust::Client,
        owner: &str,
        repo: &str,
        branch: &str,
    ) -> Result<String> {
        let dir = format!("rfd/{}", self.number_string);

        // Create the temporary directory.
        let mut path = env::temp_dir();
        path.push("asciidoc-temp/");
        let pparent = path.clone();
        let parent = pparent.as_path().to_str().unwrap().trim_end_matches('/');
        path.push("contents.adoc");

        // Write the contents to a temporary file.
        write_file(&path, deunicode::deunicode(&self.content).as_bytes())?;

        // If the file contains inline images, we need to save those images locally.
        // TODO: we don't need to save all the images, only the inline ones, clean this up
        // eventually.
        if self.content.contains("[opts=inline]") {
            let images = get_images_in_branch(github, owner, repo, &dir, branch).await?;
            for image in images {
                // Save the image to our temporary directory.
                let image_path = format!("{}/{}", parent, image.path.replace(&dir, "").trim_start_matches('/'));

                write_file(&PathBuf::from(image_path), &decode_base64(&image.content))?;
            }
        }

        let cmd_output = Command::new("asciidoctor")
            .current_dir(parent)
            .args(&["-o", "-", "--no-header-footer", path.to_str().unwrap()])
            .output()?;

        let result = if cmd_output.status.success() {
            from_utf8(&cmd_output.stdout)?
        } else {
            bail!(
                "[rfds] running asciidoctor failed: {} {}",
                from_utf8(&cmd_output.stdout)?,
                from_utf8(&cmd_output.stderr)?
            );
        };

        // Delete the parent directory.
        let pdir = Path::new(parent);
        if pdir.exists() && pdir.is_dir() {
            fs::remove_dir_all(pdir)?;
        }

        Ok(result.to_string())
    }

    /// Get a changelog for the RFD.
    pub async fn get_weekly_changelog(
        &self,
        github: &octorust::Client,
        since: DateTime<Utc>,
        company: &Company,
    ) -> Result<String> {
        let owner = &company.github_org;
        let repo = "rfd";
        let r = github.repos().get(owner, repo).await?;
        let mut changelog = String::new();

        let mut branch = self.number_string.to_string();
        if self.link.contains(&format!("/{}/", r.default_branch)) {
            branch = r.default_branch.to_string();
        }

        // Get the commits from the last seven days to the file.
        let commits = match github
            .repos()
            .list_all_commits(
                owner,
                repo,
                &branch,
                &format!("/rfd/{}/", self.number_string),
                "",
                Some(since),
                None,
            )
            .await
        {
            Ok(v) => v,
            Err(_) => {
                // Ignore the error and create an empty list.
                vec![]
            }
        };

        for commit in commits {
            let message: Vec<&str> = commit.commit.message.lines().collect();
            if !message.is_empty() {
                if let Some(author) = commit.author {
                    changelog += &format!(
                        "\t- \"{}\" by @{}\n\t\thttps://github.com/oxidecomputer/rfd/commit/{}\n",
                        message[0], author.login, commit.sha
                    );
                } else {
                    changelog += &format!(
                        "\t- \"{}\"\n\t\thttps://github.com/oxidecomputer/rfd/commit/{}\n",
                        message[0], commit.sha
                    );
                }
            }
        }

        Ok(changelog)
    }

    /// Get the filename for the PDF of the RFD.
    pub fn get_pdf_filename(&self) -> String {
        format!(
            "RFD {} {}.pdf",
            self.number_string,
            self.title.replace('/', "-").replace('\'', "").replace(':', "").trim()
        )
    }

    /// Update an RFDs state.
    pub fn update_state(&mut self, state: &str, is_markdown: bool) -> Result<()> {
        self.content = update_state(&self.content, state, is_markdown)?;
        self.state = state.to_string();

        Ok(())
    }

    /// Update an RFDs discussion link.
    pub fn update_discussion(&mut self, link: &str, is_markdown: bool) -> Result<()> {
        self.content = update_discussion_link(&self.content, link, is_markdown)?;
        self.discussion = link.to_string();

        Ok(())
    }

    /// Convert the RFD content to a PDF and upload the PDF to the /pdfs folder of the RFD
    /// repository.
    pub async fn convert_and_upload_pdf(
        &mut self,
        db: &Database,
        github: &octorust::Client,
        company: &Company,
    ) -> Result<()> {
        // Initialize the Google Drive client.
        // We do this here so we know the token is not expired.
        let drive_client = company.authenticate_google_drive(db).await?;

        // Get the rfd repo client.
        let owner = &company.github_org;
        let rfd_repo = "rfd";
        let repo = github.repos().get(owner, rfd_repo).await?;

        let mut path = env::temp_dir();
        path.push(format!("pdfcontents{}.adoc", self.number_string));

        let rfd_content = self.content.to_string();

        // Write the contents to a temporary file.
        let mut file = fs::File::create(path.clone())?;
        file.write_all(rfd_content.as_bytes())?;

        let file_name = self.get_pdf_filename();
        let rfd_path = format!("/pdfs/{}", file_name);

        let mut branch = self.number_string.to_string();
        if self.link.contains(&format!("/{}/", repo.default_branch)) {
            branch = repo.default_branch.to_string();
        }

        // Create the dir where to save images.
        let temp_dir = env::temp_dir();
        let temp_dir_str = temp_dir.to_str().unwrap();

        // We need to save the images locally as well.
        // This ensures that
        let old_dir = format!("rfd/{}", self.number_string);
        let images = get_images_in_branch(github, owner, rfd_repo, &old_dir, &branch).await?;
        for image in images {
            // Save the image to our temporary directory.
            let image_path = format!(
                "{}/{}",
                temp_dir_str.trim_end_matches('/'),
                image.path.replace(&old_dir, "").trim_start_matches('/')
            );

            write_file(&PathBuf::from(image_path), &decode_base64(&image.content))?;
        }

        let cmd_output = Command::new("asciidoctor-pdf")
            .current_dir(env::temp_dir())
            .args(&["-o", "-", "-a", "source-highlighter=rouge", path.to_str().unwrap()])
            .output()?;

        if !cmd_output.status.success() {
            bail!(
                "running asciidoctor failed: {} {}",
                from_utf8(&cmd_output.stdout)?,
                from_utf8(&cmd_output.stderr)?
            );
        }

        // Create or update the file in the github repository.
        create_or_update_file_in_github_repo(
            github,
            owner,
            rfd_repo,
            &repo.default_branch,
            &rfd_path,
            cmd_output.stdout.clone(),
        )
        .await?;

        // Figure out where our directory is.
        // It should be in the shared drive : "Automated Documents"/"rfds"
        let shared_drive = drive_client.drives().get_by_name("Automated Documents").await?;
        let drive_id = shared_drive.id.to_string();

        // Get the directory by the name.
        let parent_id = drive_client.files().create_folder(&drive_id, "", "rfds").await?;

        // Create or update the file in the google_drive.
        let drive_file = drive_client
            .files()
            .create_or_update(&drive_id, &parent_id, &file_name, "application/pdf", &cmd_output.stdout)
            .await?;
        self.pdf_link_google_drive = format!("https://drive.google.com/open?id={}", drive_file.id);

        // Delete our temporary file.
        if path.exists() && !path.is_dir() {
            fs::remove_file(path)?;
        }

        Ok(())
    }

    /// Update the pull request information for an RFD.
    pub async fn update_pull_request(
        &self,
        github: &octorust::Client,
        company: &Company,
        pull_request: &GitHubPullRequest,
    ) -> Result<()> {
        let owner = company.github_org.to_string();
        let repo = "rfd";

        // Let's make sure the title of the pull request is what it should be.
        // The pull request title should be equal to the name of the pull request.
        if self.name != pull_request.title {
            // Get the current set of settings for the pull request.
            // We do this because we want to keep the current state for body.
            let pull = github.pulls().get(&owner, repo, pull_request.number).await?;

            // Update the title of the pull request.
            match github
                .pulls()
                .update(
                    &owner,
                    repo,
                    pull_request.number,
                    &octorust::types::PullsUpdateRequest {
                        title: self.name.to_string(),
                        body: pull.body.to_string(),
                        base: "".to_string(),
                        maintainer_can_modify: None,
                        state: None,
                    },
                )
                .await
            {
                Ok(_) => (),
                Err(e) => {
                    return Err(anyhow!(
                        "unable to update title of pull request from `{}` to `{}` for pr#{}: {}",
                        pull_request.title,
                        self.name,
                        pull_request.number,
                        e,
                    ));
                }
            }
        }

        // Update the labels for the pull request.
        let mut labels: Vec<String> = Default::default();
        if self.state == "discussion" {
            labels.push(":thought_balloon: discussion".to_string());
        } else if self.state == "ideation" {
            labels.push(":hatching_chick: ideation".to_string());
        }
        github
            .issues()
            .add_labels(
                &owner,
                repo,
                pull_request.number,
                &octorust::types::IssuesAddLabelsRequestOneOf::StringVector(labels),
            )
            .await?;

        Ok(())
    }

    /// Trigger updating the search index for the RFD.
    pub async fn update_search_index(&self) -> Result<()> {
        let client = reqwest::Client::new();
        let req = client.put(&format!("https://rfd.shared.oxide.computer/api/search/{}", self.number));
        req.send().await?;

        Ok(())
    }

    /// Expand the fields in the RFD.
    /// This will get the content, html, sha, commit_date as well as fill in all generated fields.
    pub async fn expand(&mut self, github: &octorust::Client, company: &Company) -> Result<()> {
        let owner = &company.github_org;
        let repo = "rfd";
        let r = github.repos().get(owner, repo).await?;

        // Trim the title.
        self.title = self.title.trim().to_string();

        // Add leading zeros to the number for the number_string.
        self.number_string = NewRFD::generate_number_string(self.number);

        // Set the full name.
        self.name = NewRFD::generate_name(self.number, &self.title);

        // Set the short_link.
        self.short_link = NewRFD::generate_short_link(self.number);
        // Set the rendered_link.
        self.rendered_link = NewRFD::generate_rendered_link(&self.number_string);

        let mut branch = self.number_string.to_string();
        if self.link.contains(&format!("/{}/", r.default_branch)) {
            branch = r.default_branch.to_string();
        }

        // Get the RFD contents from the branch.
        let rfd_dir = format!("/rfd/{}", self.number_string);
        let (rfd_content, is_markdown, sha) =
            get_rfd_contents_from_repo(github, owner, repo, &branch, &rfd_dir, company).await?;
        self.content = rfd_content;
        self.sha = sha;

        // Get the commit date.
        if let Ok(commits) = github
            .repos()
            .list_commits(owner, repo, &branch, &rfd_dir, "", None, None, 0, 0)
            .await
        {
            let commit = commits.get(0).unwrap();
            self.commit_date = commit.commit.committer.as_ref().unwrap().date.parse()?;
        }

        // Parse the HTML.
        self.html = self.get_html(github, owner, repo, &branch, is_markdown).await?;

        self.authors = NewRFD::get_authors(&self.content, is_markdown)?;

        // Set the pdf link
        let file_name = self.get_pdf_filename();
        let rfd_path = format!("/pdfs/{}", file_name);
        self.pdf_link_github = format!("https://github.com/{}/rfd/blob/master{}", company.github_org, rfd_path);

        self.cio_company_id = company.id;

        Ok(())
    }
}

/// Implement updating the Airtable record for an RFD.
#[async_trait]
impl UpdateAirtableRecord<RFD> for RFD {
    async fn update_airtable_record(&mut self, record: RFD) -> Result<()> {
        // Set the Link to People from the original so it stays intact.
        self.milestones = record.milestones.clone();
        self.relevant_components = record.relevant_components;
        // Airtable can only hold 100,000 chars. IDK which one is that long but LOL
        // https://community.airtable.com/t/what-is-the-long-text-character-limit/1780
        self.content = truncate(&self.content, 100000);
        self.html = truncate(&self.html, 100000);
        // Ignore the rfd_sections_id as this won't parse in Airtable.
        self.rfd_sections_id = vec![];

        Ok(())
    }
}

/// Get the RFDs from the rfd GitHub repo.
pub async fn get_rfds_from_repo(github: &octorust::Client, company: &Company) -> Result<BTreeMap<i32, NewRFD>> {
    let owner = &company.github_org;
    let repo = "rfd";
    let r = github.repos().get(owner, repo).await?;

    // Get the contents of the .helpers/rfd.csv file.
    let (rfd_csv_content, _) =
        get_file_content_from_repo(github, owner, repo, &r.default_branch, "/.helpers/rfd.csv").await?;
    let rfd_csv_string = from_utf8(&rfd_csv_content)?;

    // Create the csv reader.
    let mut csv_reader = ReaderBuilder::new()
        .delimiter(b',')
        .has_headers(true)
        .from_reader(rfd_csv_string.as_bytes());

    // Create the BTreeMap of RFDs.
    let mut rfds: BTreeMap<i32, NewRFD> = Default::default();
    for r in csv_reader.deserialize() {
        let mut rfd: NewRFD = r?;

        // TODO: this whole thing is a mess jessfraz needs to cleanup
        rfd.number_string = NewRFD::generate_number_string(rfd.number);
        rfd.name = NewRFD::generate_name(rfd.number, &rfd.title);
        rfd.cio_company_id = company.id;

        // Add this to our BTreeMap.
        rfds.insert(rfd.number, rfd);
    }

    Ok(rfds)
}

/// Try to get the markdown or asciidoc contents from the repo.
pub async fn get_rfd_contents_from_repo(
    github: &octorust::Client,
    _owner: &str,
    _repo: &str,
    branch: &str,
    dir: &str,
    company: &Company,
) -> Result<(String, bool, String)> {
    let owner = &company.github_org;
    let repo = "rfd";
    let r = github.repos().get(owner, repo).await?;
    let mut is_markdown = false;
    let decoded: String;
    let sha: String;

    // Get the contents of the file.
    let path = format!("{}/README.adoc", dir);
    match github.repos().get_content_file(owner, repo, &path, branch).await {
        Ok(f) => {
            decoded = decode_base64_to_string(&f.content);
            sha = f.sha;
        }
        Err(e) => {
            info!(
                "getting file contents for {} failed: {}, trying markdown instead...",
                path, e
            );

            // Try to get the markdown instead.
            is_markdown = true;
            let f = github
                .repos()
                .get_content_file(owner, repo, &format!("{}/README.md", dir), branch)
                .await?;

            decoded = decode_base64_to_string(&f.content);
            sha = f.sha;
        }
    }

    // Get all the images in the branch and make sure they are in the images directory on master.
    let images = get_images_in_branch(github, owner, repo, dir, branch).await?;
    for image in images {
        let new_path = image.path.replace("rfd/", "src/public/static/images/");

        // Make sure we have this file in the static images dir on the master branch.
        create_or_update_file_in_github_repo(
            github,
            owner,
            repo,
            &r.default_branch,
            &new_path,
            decode_base64(&image.content),
        )
        .await?;
    }

    Ok((deunicode::deunicode(&decoded), is_markdown, sha))
}

// Get all the images in a specific directory of a GitHub branch.
#[async_recursion]
pub async fn get_images_in_branch(
    github: &octorust::Client,
    owner: &str,
    repo: &str,
    dir: &str,
    branch: &str,
) -> Result<Vec<octorust::types::ContentFile>> {
    let mut files: Vec<octorust::types::ContentFile> = Default::default();

    // Get all the images in the branch and make sure they are in the images directory on master.
    let resp = github.repos().get_content_vec_entries(owner, repo, dir, branch).await?;
    for file in resp {
        if file.type_ == "dir" {
            let path = file.path.trim_end_matches('/');
            // We have a directory. We need to get the file contents recursively.
            let mut fs = get_images_in_branch(github, owner, repo, path, branch).await?;
            files.append(&mut fs);
            continue;
        }

        if is_image(&file.name) {
            // Get the contents of the image.
            match github.repos().get_content_file(owner, repo, &file.path, branch).await {
                Ok(f) => {
                    // Push the file to our vector.
                    files.push(f);
                }
                Err(e) => {
                    // TODO: better match on errors
                    if e.to_string().contains("too large") {
                        // The file is too big for us to get it's contents through this API.
                        // The error suggests we use the Git Data API but we need the file sha for
                        // that.
                        // We have the sha we can see if the files match using the
                        // Git Data API.
                        let blob = github.git().get_blob(owner, repo, &file.sha).await?;

                        // Push the new file.
                        files.push(octorust::types::ContentFile {
                            type_: Default::default(),
                            encoding: Default::default(),
                            submodule_git_url: Default::default(),
                            target: Default::default(),
                            size: blob.size,
                            name: file.name,
                            path: file.path,
                            content: blob.content,
                            sha: file.sha,
                            url: file.url,
                            git_url: file.git_url,
                            html_url: file.html_url,
                            download_url: file.download_url,
                            links: file.links,
                        });

                        continue;
                    }

                    bail!("[rfd] getting file contents for {} failed: {}", file.path, e);
                }
            }
        }
    }

    Ok(files)
}

pub fn parse_markdown(content: &str) -> String {
    markdown_to_html(content, &ComrakOptions::default())
}

/// Return if the file is an image.
pub fn is_image(file: &str) -> bool {
    file.ends_with(".svg") || file.ends_with(".png") || file.ends_with(".jpg") || file.ends_with(".jpeg")
}

pub fn clean_rfd_html_links(content: &str, num: &str) -> Result<String> {
    let mut cleaned = content
        .replace(r#"href="\#"#, &format!(r#"href="/rfd/{}#"#, num))
        .replace("href=\"#", &format!("href=\"/rfd/{}#", num))
        .replace(r#"img src=""#, &format!(r#"img src="/static/images/{}/"#, num))
        .replace(r#"object data=""#, &format!(r#"object data="/static/images/{}/"#, num))
        .replace(
            r#"object type="image/svg+xml" data=""#,
            &format!(r#"object type="image/svg+xml" data="/static/images/{}/"#, num),
        );

    let mut re = Regex::new(r"https://(?P<num>[0-9]).rfd.oxide.computer")?;
    cleaned = re
        .replace_all(&cleaned, "https://rfd.shared.oxide.computer/rfd/000$num")
        .to_string();
    re = Regex::new(r"https://(?P<num>[0-9][0-9]).rfd.oxide.computer")?;
    cleaned = re
        .replace_all(&cleaned, "https://rfd.shared.oxide.computer/rfd/00$num")
        .to_string();
    re = Regex::new(r"https://(?P<num>[0-9][0-9][0-9]).rfd.oxide.computer")?;
    cleaned = re
        .replace_all(&cleaned, "https://rfd.shared.oxide.computer/rfd/0$num")
        .to_string();
    re = Regex::new(r"https://(?P<num>[0-9][0-9][0-9][0-9]).rfd.oxide.computer")?;
    cleaned = re
        .replace_all(&cleaned, "https://rfd.shared.oxide.computer/rfd/$num")
        .to_string();

    Ok(cleaned
        .replace("link:", &format!("link:https://{}.rfd.oxide.computer/", num))
        .replace(&format!("link:https://{}.rfd.oxide.computer/http", num), "link:http"))
}

pub fn update_discussion_link(content: &str, link: &str, is_markdown: bool) -> Result<String> {
    // TODO: there is probably a better way to do these regexes.
    let mut re = Regex::new(r"(?m)(:discussion:.*$)")?;
    // Asciidoc starts with a colon.
    let mut pre = ":";
    if is_markdown {
        // Markdown does not start with a colon.
        pre = "";
        re = Regex::new(r"(?m)(discussion:.*$)")?;
    }

    let replacement = if let Some(v) = re.find(content) {
        v.as_str().to_string()
    } else {
        String::new()
    };

    Ok(content.replacen(&replacement, &format!("{}discussion: {}", pre, link.trim()), 1))
}

pub fn update_state(content: &str, state: &str, is_markdown: bool) -> Result<String> {
    // TODO: there is probably a better way to do these regexes.
    let mut re = Regex::new(r"(?m)(:state:.*$)")?;
    // Asciidoc starts with a colon.
    let mut pre = ":";
    if is_markdown {
        // Markdown does not start with a colon.
        pre = "";
        re = Regex::new(r"(?m)(state:.*$)")?;
    }

    let replacement = if let Some(v) = re.find(content) {
        v.as_str().to_string()
    } else {
        String::new()
    };

    Ok(content.replacen(&replacement, &format!("{}state: {}", pre, state.trim()), 1))
}

// Sync the rfds with our database.
pub async fn refresh_db_rfds(db: &Database, company: &Company) -> Result<()> {
    // Authenticate GitHub.
    let github = company.authenticate_github()?;

    // Check if the repo exists, if not exit early.
    if let Err(e) = github.repos().get(&company.github_org, "rfd").await {
        if e.to_string().contains("404") {
            return Ok(());
        } else {
            bail!("checking for rfd repo failed: {}", e);
        }
    }

    let rfds = get_rfds_from_repo(&github, company).await?;

    // Sync rfds.
    for (_, mut rfd) in rfds {
        // Check if we already have an existing RFD.
        if let Some(existing) = RFD::get_from_db(db, rfd.number) {
            // Set the rfd_sections_id so we don't overwrite it.
            rfd.rfd_sections_id = existing.rfd_sections_id;
        }

        let mut new_rfd = rfd.upsert(db).await?;

        // Expand the fields in the RFD.
        new_rfd.expand(&github, company).await?;

        // Make and update the PDF versions.
        if let Err(err) = new_rfd.convert_and_upload_pdf(db, &github, company).await {
            warn!(
                "failed to convert and upload PDF for RFD {}: {}",
                new_rfd.number_string, err
            );
        }

        // Update the RFD again.
        // We do this so the expand functions are only one place.
        new_rfd.update(db).await?;

        // Now that the database is updated, update the search index.
        new_rfd.update_search_index().await?;
    }

    // Update rfds in airtable.
    RFDs::get_from_db(db, company.id)?.update_airtable(db).await?;

    Ok(())
}

pub async fn cleanup_rfd_pdfs(db: &Database, company: &Company) -> Result<()> {
    // Get all the rfds from the database.
    let rfds = RFDs::get_from_db(db, company.id)?;
    let github = company.authenticate_github()?;

    // Check if the repo exists, if not exit early.
    if let Err(e) = github.repos().get(&company.github_org, "rfd").await {
        if e.to_string().contains("404") {
            return Ok(());
        } else {
            bail!("checking for rfd repo failed: {}", e);
        }
    }

    // Get all the PDF files.
    let files = github
        .repos()
        .get_content_vec_entries(
            &company.github_org,
            "rfd",
            "/pdfs/",
            "", // leaving the branch blank gives us the default branch
        )
        .await?;

    let mut github_pdf_files: BTreeMap<String, String> = Default::default();
    for file in files {
        // We will store these in github_pdf_files as <{name}, {sha}>. So we can more easily delete
        // them.
        github_pdf_files.insert(file.name.to_string(), file.sha.to_string());
    }

    let drive_client = company.authenticate_google_drive(db).await?;

    // Figure out where our directory is.
    // It should be in the shared drive : "Automated Documents"/"rfds"
    let shared_drive = drive_client.drives().get_by_name("Automated Documents").await?;
    let drive_id = shared_drive.id.to_string();

    // Get the directory by the name.
    let parent_id = drive_client.files().create_folder(&drive_id, "", "rfds").await?;

    // Iterate over the RFD and cleanup any PDFs with the wrong name.
    for rfd in rfds {
        let pdf_file_name = rfd.get_pdf_filename();

        // First let's do Google Drive.
        // Search for files with that rfd number string.
        let drive_files = drive_client
            .files()
            .list_all(
                "drive",                                                                           // corpa
                &drive_id,                                                                         // drive id
                true,  // include items from all drives
                "",    // include permissions for view
                false, // include team drive items
                "",    // order by
                &format!("name contains '{}' and '{}' in parents", &rfd.number_string, parent_id), // query
                "",    // spaces
                true,  // supports all drives
                false, // supports team drives
                "",    // team drive id
            )
            .await?;
        // Iterate over the files and if the name does not equal our name, then nuke it.
        for df in drive_files {
            if df.name == pdf_file_name {
                info!("keeping Google Drive PDF of RFD `{}`: {}", rfd.number_string, df.name);
                continue;
            }

            info!("deleting Google Drive PDF of RFD `{}`: {}", rfd.number_string, df.name);
            // Delete the file from our drive.
            drive_client.files().delete(&df.id, true, true).await?;
        }

        // Now let's do GitHub.
        // Iterate over our github_pdf_files and delete any that do not match.
        for (gf_name, sha) in github_pdf_files.clone() {
            if gf_name == pdf_file_name {
                info!("keeping GitHub PDF of RFD `{}`: {}", rfd.number_string, gf_name);
                // Remove it from our btree map.
                github_pdf_files.remove(&gf_name);
                continue;
            }

            if gf_name.contains(&rfd.number_string) {
                // Remove it from GitHub.
                info!("deleting GitHub PDF of RFD `{}`: {}", rfd.number_string, gf_name);
                github
                    .repos()
                    .delete_file(
                        &company.github_org,
                        "rfd",
                        &format!("pdfs/{}", gf_name),
                        &octorust::types::ReposDeleteFileRequest {
                            message: format!(
                                "Deleting file content {} programatically\n\nThis is done from \
                                 the cio repo cio::cleanup_rfd_pdfs function.",
                                gf_name
                            ),
                            sha: sha.to_string(),
                            committer: None,
                            author: None,
                            branch: "".to_string(),
                        },
                    )
                    .await?;

                // Remove it from our btree map.
                github_pdf_files.remove(&gf_name);
            }
        }
    }

    Ok(())
}

/// Create a changelog email for the RFDs.
pub async fn send_rfd_changelog(db: &Database, company: &Company) -> Result<()> {
    let rfds = RFDs::get_from_db(db, company.id)?;

    if rfds.0.is_empty() {
        // Return early.
        return Ok(());
    }

    let github = company.authenticate_github()?;
    let seven_days_ago = Utc::now() - Duration::days(7);
    let week_format = format!(
        "from {} to {}",
        seven_days_ago.format("%m-%d-%Y"),
        Utc::now().format("%m-%d-%Y")
    );

    let mut changelog = format!("Changes to RFDs for the week {}:\n", week_format);

    // Iterate over the RFDs.
    for rfd in rfds {
        let changes = rfd.get_weekly_changelog(&github, seven_days_ago, company).await?;
        if !changes.is_empty() {
            changelog += &format!("\n{} {}\n{}", rfd.name, rfd.short_link, changes);
        }
    }

    // Initialize the SendGrid clVient.
    let sendgrid_client = SendGrid::new_from_env();

    // Send the message.
    sendgrid_client
        .mail_send()
        .send_plain_text(
            &format!("RFD changelog for the week from {}", week_format),
            &changelog,
            &[format!("all@{}", company.gsuite_domain)],
            &[],
            &[],
            &format!("rfds@{}", company.gsuite_domain),
        )
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{
        companies::Company,
        db::Database,
        rfds::{clean_rfd_html_links, send_rfd_changelog, update_discussion_link, update_state, NewRFD},
    };

    #[ignore]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_monday_cron_rfds_changelog() {
        crate::utils::setup_logger();

        // Initialize our database.
        let db = Database::new();

        // Get the company id for Oxide.
        // TODO: split this out per company.
        let oxide = Company::get_from_db(&db, "Oxide".to_string()).unwrap();

        send_rfd_changelog(&db, &oxide).await.unwrap();
    }

    #[test]
    fn test_clean_rfd_html_links() {
        crate::utils::setup_logger();

        let content = r#"https://3.rfd.oxide.computer
        https://41.rfd.oxide.computer
        https://543.rfd.oxide.computer#-some-link
        https://3245.rfd.oxide.computer/things
        https://3265.rfd.oxide.computer/things
        <img src="things.png" \>
        <a href="\#_principles">
        <object data="thing.svg">
        <object type="image/svg+xml" data="thing.svg">
        <a href="\#things" \>
        link:thing.html[Our thing]
        link:http://example.com[our example]"#;

        let cleaned = clean_rfd_html_links(content, "0032").unwrap();

        let expected = r#"https://rfd.shared.oxide.computer/rfd/0003
        https://rfd.shared.oxide.computer/rfd/0041
        https://rfd.shared.oxide.computer/rfd/0543#-some-link
        https://rfd.shared.oxide.computer/rfd/3245/things
        https://rfd.shared.oxide.computer/rfd/3265/things
        <img src="/static/images/0032/things.png" \>
        <a href="/rfd/0032#_principles">
        <object data="/static/images/0032/thing.svg">
        <object type="image/svg+xml" data="/static/images/0032/thing.svg">
        <a href="/rfd/0032#things" \>
        link:https://0032.rfd.oxide.computer/thing.html[Our thing]
        link:http://example.com[our example]"#;

        assert_eq!(expected, cleaned);
    }

    #[test]
    fn test_get_authors() {
        crate::utils::setup_logger();

        let mut content = r#"sdfsdf
sdfsdf
authors: things, joe
dsfsdf
sdf
authors: nope"#;
        let mut authors = NewRFD::get_authors(content, true).unwrap();
        let mut expected = "things, joe".to_string();
        assert_eq!(expected, authors);

        content = r#"sdfsdf
= sdfgsdfgsdfg
things, joe
dsfsdf
sdf
:authors: nope"#;
        authors = NewRFD::get_authors(content, true).unwrap();
        expected = "".to_string();
        assert_eq!(expected, authors);

        content = r#"sdfsdf
= sdfgsdfgsdfg
things <things@email.com>, joe <joe@email.com>
dsfsdf
sdf
authors: nope"#;
        authors = NewRFD::get_authors(content, false).unwrap();
        expected = r#"things <things@email.com>, joe <joe@email.com>"#.to_string();
        assert_eq!(expected, authors);

        content = r#":authors: Jess <jess@thing.com>

= sdfgsdfgsdfg
{authors}
dsfsdf
sdf"#;
        authors = NewRFD::get_authors(content, false).unwrap();
        expected = r#"Jess <jess@thing.com>"#.to_string();
        assert_eq!(expected, authors);
    }

    #[test]
    fn test_get_state() {
        crate::utils::setup_logger();

        let mut content = r#"sdfsdf
sdfsdf
state: discussion
dsfsdf
sdf
authors: nope"#;
        let mut state = NewRFD::get_state(content).unwrap();
        let mut expected = "discussion".to_string();
        assert_eq!(expected, state);

        content = r#"sdfsdf
= sdfgsdfgsdfg
:state: prediscussion
dsfsdf
sdf
:state: nope"#;
        state = NewRFD::get_state(content).unwrap();
        expected = "prediscussion".to_string();
        assert_eq!(expected, state);
    }

    #[test]
    fn test_get_discussion() {
        crate::utils::setup_logger();

        let mut content = r#"sdfsdf
sdfsdf
discussion: https://github.com/oxidecomputer/rfd/pulls/1
dsfsdf
sdf
authors: nope"#;
        let mut discussion = NewRFD::get_discussion(content).unwrap();
        let expected = "https://github.com/oxidecomputer/rfd/pulls/1".to_string();
        assert_eq!(expected, discussion);

        content = r#"sdfsdf
= sdfgsdfgsdfg
:discussion: https://github.com/oxidecomputer/rfd/pulls/1
dsfsdf
sdf
:discussion: nope"#;
        discussion = NewRFD::get_discussion(content).unwrap();
        assert_eq!(expected, discussion);
    }

    #[test]
    fn test_update_discussion_link() {
        crate::utils::setup_logger();

        let link = "https://github.com/oxidecomputer/rfd/pulls/2019";
        let mut content = r#"sdfsdf
sdfsdf
discussion:   https://github.com/oxidecomputer/rfd/pulls/1
dsfsdf
sdf
authors: nope"#;
        let mut result = update_discussion_link(content, link, true).unwrap();
        let mut expected = r#"sdfsdf
sdfsdf
discussion: https://github.com/oxidecomputer/rfd/pulls/2019
dsfsdf
sdf
authors: nope"#;
        assert_eq!(expected, result);

        content = r#"sdfsdf
= sdfgsd
discussion: fgsdfg
:discussion: https://github.com/oxidecomputer/rfd/pulls/1
dsfsdf
sdf
:discussion: nope"#;
        result = update_discussion_link(content, link, false).unwrap();
        expected = r#"sdfsdf
= sdfgsd
discussion: fgsdfg
:discussion: https://github.com/oxidecomputer/rfd/pulls/2019
dsfsdf
sdf
:discussion: nope"#;
        assert_eq!(expected, result);

        content = r#"sdfsdf
= sdfgsd
discussion: fgsdfg
:discussion:
dsfsdf
sdf
:discussion: nope"#;
        result = update_discussion_link(content, link, false).unwrap();
        expected = r#"sdfsdf
= sdfgsd
discussion: fgsdfg
:discussion: https://github.com/oxidecomputer/rfd/pulls/2019
dsfsdf
sdf
:discussion: nope"#;
        assert_eq!(expected, result);
    }

    #[test]
    fn test_update_state() {
        crate::utils::setup_logger();

        let state = "discussion";
        let mut content = r#"sdfsdf
sdfsdf
state:   sdfsdfsdf
dsfsdf
sdf
authors: nope"#;
        let mut result = update_state(content, state, true).unwrap();
        let mut expected = r#"sdfsdf
sdfsdf
state: discussion
dsfsdf
sdf
authors: nope"#;
        assert_eq!(expected, result);

        content = r#"sdfsdf
= sdfgsd
state: fgsdfg
:state: prediscussion
dsfsdf
sdf
:state: nope"#;
        result = update_state(content, state, false).unwrap();
        expected = r#"sdfsdf
= sdfgsd
state: fgsdfg
:state: discussion
dsfsdf
sdf
:state: nope"#;
        assert_eq!(expected, result);

        content = r#"sdfsdf
= sdfgsd
state: fgsdfg
:state:
dsfsdf
sdf
:state: nope"#;
        result = update_state(content, state, false).unwrap();
        expected = r#"sdfsdf
= sdfgsd
state: fgsdfg
:state: discussion
dsfsdf
sdf
:state: nope"#;
        assert_eq!(expected, result);
    }

    #[test]
    fn test_get_title() {
        crate::utils::setup_logger();

        let mut content = r#"things
# RFD 43 Identity and Access Management (IAM)
sdfsdf
title: https://github.com/oxidecomputer/rfd/pulls/1
dsfsdf
sdf
authors: nope"#;
        let mut title = NewRFD::get_title(content).unwrap();
        let expected = "Identity and Access Management (IAM)".to_string();
        assert_eq!(expected, title);

        content = r#"sdfsdf
= RFD 43 Identity and Access Management (IAM)
:title: https://github.com/oxidecomputer/rfd/pulls/1
dsfsdf
= RFD 53 Bye
sdf
:title: nope"#;
        title = NewRFD::get_title(content).unwrap();
        assert_eq!(expected, title);

        // Add a test to show what happens for rfd 31 where there is no "RFD" in
        // the title.
        content = r#"sdfsdf
= Identity and Access Management (IAM)
:title: https://github.com/oxidecomputer/rfd/pulls/1
dsfsdf
sdf
:title: nope"#;
        title = NewRFD::get_title(content).unwrap();
        assert_eq!(expected, title);
    }
}
