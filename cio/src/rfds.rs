#![allow(clippy::from_over_into)]
use std::{
    collections::BTreeMap,
    env, fs,
    io::{stderr, stdout, Write},
    path::{Path, PathBuf},
    process::Command,
    str::from_utf8,
};

use async_recursion::async_recursion;
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use comrak::{markdown_to_html, ComrakOptions};
use csv::ReaderBuilder;
use google_drive::GoogleDrive;
use macros::db;
use regex::Regex;
use schemars::JsonSchema;
use sendgrid_api::SendGrid;
use serde::{Deserialize, Serialize};

use crate::{
    airtable::AIRTABLE_RFD_TABLE,
    companies::Company,
    core::UpdateAirtableRecord,
    db::Database,
    schema::{rfds as r_f_ds, rfds},
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
    ) -> Self {
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
        let number = dir.trim_start_matches('0').parse::<i32>().unwrap();

        let number_string = NewRFD::generate_number_string(number);

        // Parse the RFD title from the contents.
        let title = NewRFD::get_title(&content);
        let name = NewRFD::generate_name(number, &title);

        // Parse the state from the contents.
        let state = NewRFD::get_state(&content);

        // Parse the discussion from the contents.
        let discussion = NewRFD::get_discussion(&content);

        NewRFD {
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
            cio_company_id: company.id,
        }
    }

    pub fn get_title(content: &str) -> String {
        let mut re = Regex::new(r"(?m)(RFD .*$)").unwrap();
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

                s.to_string()
            }
            None => {
                // There is no "RFD" in our title. This is the case for RFD 31.
                re = Regex::new(r"(?m)(^= .*$)").unwrap();
                let results = re.find(content).unwrap();
                results
                    .as_str()
                    .replace("RFD", "")
                    .replace("# ", "")
                    .replace("= ", " ")
                    .trim()
                    .to_string()
            }
        }
    }

    pub fn get_state(content: &str) -> String {
        let re = Regex::new(r"(?m)(state:.*$)").unwrap();
        match re.find(content) {
            Some(v) => return v.as_str().replace("state:", "").trim().to_string(),
            None => Default::default(),
        }
    }

    pub fn get_discussion(content: &str) -> String {
        let re = Regex::new(r"(?m)(discussion:.*$)").unwrap();
        match re.find(content) {
            Some(v) => return v.as_str().replace("discussion:", "").trim().to_string(),
            None => Default::default(),
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

    pub fn get_authors(content: &str, is_markdown: bool) -> String {
        if is_markdown {
            // TODO: make work w asciidoc.
            let re = Regex::new(r"(?m)(^authors.*$)").unwrap();
            match re.find(content) {
                Some(v) => return v.as_str().replace("authors:", "").trim().to_string(),
                None => Default::default(),
            }
        }

        // We must have asciidoc content.
        // We want to find the line under the first "=" line (which is the title), authors is under
        // that.
        let re = Regex::new(r"(?m:^=.*$)[\n\r](?m)(.*$)").unwrap();
        match re.find(content) {
            Some(v) => {
                let val = v.as_str().trim().to_string();
                let parts: Vec<&str> = val.split('\n').collect();
                if parts.len() < 2 {
                    Default::default()
                } else {
                    let mut authors = parts[1].to_string();
                    if authors == "{authors}" {
                        // Do the traditional check.
                        let re = Regex::new(r"(?m)(^:authors.*$)").unwrap();
                        if let Some(v) = re.find(content) {
                            authors = v.as_str().replace(":authors:", "").trim().to_string();
                        }
                    }
                    authors
                }
            }
            None => Default::default(),
        }
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
    ) -> String {
        let html: String;
        if is_markdown {
            // Parse the markdown.
            html = parse_markdown(&self.content);
        } else {
            // Parse the acsiidoc.
            html = self.parse_asciidoc(github, owner, repo, branch).await;
        }

        clean_rfd_html_links(&html, &self.number_string)
    }

    pub async fn parse_asciidoc(&self, github: &octorust::Client, owner: &str, repo: &str, branch: &str) -> String {
        let dir = format!("rfd/{}", self.number_string);

        // Create the temporary directory.
        let mut path = env::temp_dir();
        path.push("asciidoc-temp/");
        let pparent = path.clone();
        let parent = pparent.as_path().to_str().unwrap().trim_end_matches('/');
        path.push("contents.adoc");

        // Write the contents to a temporary file.
        write_file(&path, deunicode::deunicode(&self.content).as_bytes());

        // If the file contains inline images, we need to save those images locally.
        // TODO: we don't need to save all the images, only the inline ones, clean this up
        // eventually.
        if self.content.contains("[opts=inline]") {
            let images = get_images_in_branch(github, owner, repo, &dir, branch).await;
            for image in images {
                // Save the image to our temporary directory.
                let image_path = format!("{}/{}", parent, image.path.replace(&dir, "").trim_start_matches('/'));

                write_file(&PathBuf::from(image_path), &decode_base64(&image.content));
            }
        }

        let cmd_output = Command::new("asciidoctor")
            .current_dir(parent)
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

        // Delete the parent directory.
        let pdir = Path::new(parent);
        if pdir.exists() && pdir.is_dir() {
            fs::remove_dir_all(pdir).unwrap();
        }

        result.to_string()
    }

    /// Convert an RFD into JSON as Slack message.
    // TODO: make this include more fields
    pub fn as_slack_msg(&self) -> String {
        let mut msg = format!(
            "{} (_*{}*_) <{}|github> <{}|rendered>",
            self.name, self.state, self.short_link, self.rendered_link
        );

        if !self.discussion.is_empty() {
            msg += &format!(" <{}|discussion>", self.discussion);
        }

        msg
    }

    /// Get a changelog for the RFD.
    pub async fn get_weekly_changelog(
        &self,
        github: &octorust::Client,
        since: DateTime<Utc>,
        company: &Company,
    ) -> String {
        let owner = &company.github_org;
        let repo = "rfd";
        let r = github.repos().get(owner, repo).await.unwrap();
        let mut changelog = String::new();

        let mut branch = self.number_string.to_string();
        if self.link.contains(&format!("/{}/", r.default_branch)) {
            branch = r.default_branch.to_string();
        }

        // Get the commits from the last seven days to the file.
        let commits = github
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
            .unwrap();

        for commit in commits {
            let message: Vec<&str> = commit.commit.message.lines().collect();
            if !message.is_empty() {
                changelog += &format!(
                    "\t- \"{}\" by @{}\n\t\thttps://github.com/oxidecomputer/rfd/commit/{}\n",
                    message[0],
                    commit.author.unwrap().login,
                    commit.sha
                );
            }
        }

        changelog
    }

    /// Get the filename for the PDF of the RFD.
    pub fn get_pdf_filename(&self) -> String {
        format!(
            "RFD {} {}.pdf",
            self.number_string,
            self.title.replace("/", "-").replace("'", "").replace(":", "").trim()
        )
    }

    /// Update an RFDs state.
    pub fn update_state(&mut self, state: &str, is_markdown: bool) {
        self.content = update_state(&self.content, state, is_markdown);
        self.state = state.to_string();
    }

    /// Update an RFDs discussion link.
    pub fn update_discussion(&mut self, link: &str, is_markdown: bool) {
        self.content = update_discussion_link(&self.content, link, is_markdown);
        self.discussion = link.to_string();
    }

    /// Convert the RFD content to a PDF and upload the PDF to the /pdfs folder of the RFD
    /// repository.
    pub async fn convert_and_upload_pdf(
        &mut self,
        github: &octorust::Client,
        drive_client: &GoogleDrive,
        company: &Company,
    ) {
        // Get the rfd repo client.
        let owner = &company.github_org;
        let rfd_repo = "rfd";
        let repo = github.repos().get(owner, rfd_repo).await.unwrap();

        let mut path = env::temp_dir();
        path.push(format!("pdfcontents{}.adoc", self.number_string));

        let rfd_content = self.content.to_string();

        // Write the contents to a temporary file.
        let mut file = fs::File::create(path.clone()).unwrap();
        file.write_all(rfd_content.as_bytes()).unwrap();

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
        let images = get_images_in_branch(github, owner, rfd_repo, &old_dir, &branch).await;
        for image in images {
            // Save the image to our temporary directory.
            let image_path = format!(
                "{}/{}",
                temp_dir_str.trim_end_matches('/'),
                image.path.replace(&old_dir, "").trim_start_matches('/')
            );

            write_file(&PathBuf::from(image_path), &decode_base64(&image.content));
        }

        let cmd_output = Command::new("asciidoctor-pdf")
            .current_dir(env::temp_dir())
            .args(&["-o", "-", "-a", "source-highlighter=rouge", path.to_str().unwrap()])
            .output()
            .unwrap();

        if !cmd_output.status.success() {
            println!("[rfdpdf] running asciidoctor failed:");
            stdout().write_all(&cmd_output.stdout).unwrap();
            stderr().write_all(&cmd_output.stderr).unwrap();
            return;
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
        .await;

        // Figure out where our directory is.
        // It should be in the shared drive : "Automated Documents"/"rfds"
        let shared_drive = drive_client.get_drive_by_name("Automated Documents").await.unwrap();
        let drive_id = shared_drive.id.to_string();

        // Get the directory by the name.
        let drive_rfd_dir = drive_client.get_file_by_name(&drive_id, "rfds").await.unwrap();
        let parent_id = drive_rfd_dir.get(0).unwrap().id.to_string();

        // Create or update the file in the google_drive.
        let drive_file = drive_client
            .create_or_update_file(&drive_id, &parent_id, &file_name, "application/pdf", &cmd_output.stdout)
            .await
            .unwrap();
        self.pdf_link_google_drive = format!("https://drive.google.com/open?id={}", drive_file.id);

        // Delete our temporary file.
        if path.exists() && !path.is_dir() {
            fs::remove_file(path).unwrap();
        }
    }

    /// Expand the fields in the RFD.
    /// This will get the content, html, sha, commit_date as well as fill in all generated fields.
    pub async fn expand(&mut self, github: &octorust::Client, company: &Company) {
        let owner = &company.github_org;
        let repo = "rfd";
        let r = github.repos().get(owner, repo).await.unwrap();

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
            get_rfd_contents_from_repo(github, owner, repo, &branch, &rfd_dir, company).await;
        self.content = rfd_content;
        self.sha = sha;

        // Get the commit date.
        if let Ok(commits) = github
            .repos()
            .list_commits(owner, repo, &branch, &rfd_dir, "", None, None, 0, 0)
            .await
        {
            let commit = commits.get(0).unwrap();
            self.commit_date = commit.commit.author.as_ref().unwrap().date.parse().unwrap();
        }

        // Parse the HTML.
        self.html = self.get_html(github, owner, repo, &branch, is_markdown).await;

        self.authors = NewRFD::get_authors(&self.content, is_markdown);

        // Set the pdf link
        let file_name = self.get_pdf_filename();
        let rfd_path = format!("/pdfs/{}", file_name);
        self.pdf_link_github = format!("https://github.com/{}/rfd/blob/master{}", company.github_org, rfd_path);

        self.cio_company_id = company.id;
    }
}

/// Implement updating the Airtable record for an RFD.
#[async_trait]
impl UpdateAirtableRecord<RFD> for RFD {
    async fn update_airtable_record(&mut self, record: RFD) {
        // Set the Link to People from the original so it stays intact.
        self.milestones = record.milestones.clone();
        self.relevant_components = record.relevant_components;
        // Airtable can only hold 100,000 chars. IDK which one is that long but LOL
        // https://community.airtable.com/t/what-is-the-long-text-character-limit/1780
        self.content = truncate(&self.content, 100000);
        self.html = truncate(&self.html, 100000);
    }
}

/// Get the RFDs from the rfd GitHub repo.
pub async fn get_rfds_from_repo(github: &octorust::Client, company: &Company) -> BTreeMap<i32, NewRFD> {
    let owner = &company.github_org;
    let repo = "rfd";
    let r = github.repos().get(owner, repo).await.unwrap();

    // Get the contents of the .helpers/rfd.csv file.
    let (rfd_csv_content, _) =
        get_file_content_from_repo(github, owner, repo, &r.default_branch, "/.helpers/rfd.csv").await;
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

        // TODO: this whole thing is a mess jessfraz needs to cleanup
        rfd.number_string = NewRFD::generate_number_string(rfd.number);
        rfd.name = NewRFD::generate_name(rfd.number, &rfd.title);
        rfd.cio_company_id = company.id;

        // Add this to our BTreeMap.
        rfds.insert(rfd.number, rfd);
    }

    rfds
}

/// Try to get the markdown or asciidoc contents from the repo.
pub async fn get_rfd_contents_from_repo(
    github: &octorust::Client,
    _owner: &str,
    _repo: &str,
    branch: &str,
    dir: &str,
    company: &Company,
) -> (String, bool, String) {
    let owner = &company.github_org;
    let repo = "rfd";
    let r = github.repos().get(owner, repo).await.unwrap();
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
            println!(
                "[rfd] getting file contents for {} failed: {}, trying markdown instead...",
                path, e
            );

            // Try to get the markdown instead.
            is_markdown = true;
            let f = github
                .repos()
                .get_content_file(owner, repo, &format!("{}/README.md", dir), branch)
                .await
                .unwrap();

            decoded = decode_base64_to_string(&f.content);
            sha = f.sha;
        }
    }

    // Get all the images in the branch and make sure they are in the images directory on master.
    let images = get_images_in_branch(github, owner, repo, dir, branch).await;
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
        .await;
    }

    (deunicode::deunicode(&decoded), is_markdown, sha)
}

// Get all the images in a specific directory of a GitHub branch.
#[async_recursion]
pub async fn get_images_in_branch(
    github: &octorust::Client,
    owner: &str,
    repo: &str,
    dir: &str,
    branch: &str,
) -> Vec<octorust::types::ContentFile> {
    let mut files: Vec<octorust::types::ContentFile> = Default::default();

    // Get all the images in the branch and make sure they are in the images directory on master.
    let resp = github
        .repos()
        .get_content_vec_entries(owner, repo, dir, branch)
        .await
        .unwrap();
    for file in resp {
        if file.type_ == "dir" {
            let path = file.path.trim_end_matches('/');
            // We have a directory. We need to get the file contents recursively.
            let mut fs = get_images_in_branch(github, owner, repo, path, branch).await;
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
                        let blob = github.git().get_blob(owner, repo, &file.sha).await.unwrap();

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

                    println!("[rfd] getting file contents for {} failed: {}", file.path, e);
                }
            }
        }
    }

    files
}

pub fn parse_markdown(content: &str) -> String {
    markdown_to_html(content, &ComrakOptions::default())
}

/// Return if the file is an image.
pub fn is_image(file: &str) -> bool {
    file.ends_with(".svg") || file.ends_with(".png") || file.ends_with(".jpg") || file.ends_with(".jpeg")
}

pub fn clean_rfd_html_links(content: &str, num: &str) -> String {
    let mut cleaned = content
        .replace(r#"href="\#"#, &format!(r#"href="/rfd/{}#"#, num))
        .replace("href=\"#", &format!("href=\"/rfd/{}#", num))
        .replace(r#"img src=""#, &format!(r#"img src="/static/images/{}/"#, num))
        .replace(r#"object data=""#, &format!(r#"object data="/static/images/{}/"#, num))
        .replace(
            r#"object type="image/svg+xml" data=""#,
            &format!(r#"object type="image/svg+xml" data="/static/images/{}/"#, num),
        );

    let mut re = Regex::new(r"https://(?P<num>[0-9]).rfd.oxide.computer").unwrap();
    cleaned = re
        .replace_all(&cleaned, "https://rfd.shared.oxide.computer/rfd/000$num")
        .to_string();
    re = Regex::new(r"https://(?P<num>[0-9][0-9]).rfd.oxide.computer").unwrap();
    cleaned = re
        .replace_all(&cleaned, "https://rfd.shared.oxide.computer/rfd/00$num")
        .to_string();
    re = Regex::new(r"https://(?P<num>[0-9][0-9][0-9]).rfd.oxide.computer").unwrap();
    cleaned = re
        .replace_all(&cleaned, "https://rfd.shared.oxide.computer/rfd/0$num")
        .to_string();
    re = Regex::new(r"https://(?P<num>[0-9][0-9][0-9][0-9]).rfd.oxide.computer").unwrap();
    cleaned = re
        .replace_all(&cleaned, "https://rfd.shared.oxide.computer/rfd/$num")
        .to_string();

    cleaned
}

pub fn update_discussion_link(content: &str, link: &str, is_markdown: bool) -> String {
    // TODO: there is probably a better way to do these regexes.
    let mut re = Regex::new(r"(?m)(:discussion:.*$)").unwrap();
    // Asciidoc starts with a colon.
    let mut pre = ":";
    if is_markdown {
        // Markdown does not start with a colon.
        pre = "";
        re = Regex::new(r"(?m)(discussion:.*$)").unwrap();
    }

    let replacement = if let Some(v) = re.find(content) {
        v.as_str().to_string()
    } else {
        String::new()
    };

    content.replacen(&replacement, &format!("{}discussion: {}", pre, link.trim()), 1)
}

pub fn update_state(content: &str, state: &str, is_markdown: bool) -> String {
    // TODO: there is probably a better way to do these regexes.
    let mut re = Regex::new(r"(?m)(:state:.*$)").unwrap();
    // Asciidoc starts with a colon.
    let mut pre = ":";
    if is_markdown {
        // Markdown does not start with a colon.
        pre = "";
        re = Regex::new(r"(?m)(state:.*$)").unwrap();
    }

    let replacement = if let Some(v) = re.find(content) {
        v.as_str().to_string()
    } else {
        String::new()
    };

    content.replacen(&replacement, &format!("{}state: {}", pre, state.trim()), 1)
}

// Sync the rfds with our database.
pub async fn refresh_db_rfds(db: &Database, company: &Company) {
    // Authenticate GitHub.
    let github = company.authenticate_github();

    // Get gsuite token.
    let token = company.authenticate_google(db).await;

    // Initialize the Google Drive client.
    let drive_client = GoogleDrive::new(token);

    let rfds = get_rfds_from_repo(&github, company).await;

    // Sync rfds.
    for (_, rfd) in rfds {
        let mut new_rfd = rfd.upsert(db).await;

        // Expand the fields in the RFD.
        new_rfd.expand(&github, company).await;

        // Make and update the PDF versions.
        new_rfd.convert_and_upload_pdf(&github, &drive_client, company).await;

        // Update the RFD again.
        // We do this so the expand functions are only one place.
        new_rfd.update(db).await;
    }
}

/// Create a changelog email for the RFDs.
pub async fn send_rfd_changelog(company: &Company) {
    // Initialize our database.
    let db = Database::new();

    let github = company.authenticate_github();
    let seven_days_ago = Utc::now() - Duration::days(7);
    let week_format = format!(
        "from {} to {}",
        seven_days_ago.format("%m-%d-%Y"),
        Utc::now().format("%m-%d-%Y")
    );

    let mut changelog = format!("Changes to RFDs for the week {}:\n", week_format);

    // Iterate over the RFDs.
    let rfds = RFDs::get_from_db(&db, company.id);
    for rfd in rfds {
        let changes = rfd.get_weekly_changelog(&github, seven_days_ago, company).await;
        if !changes.is_empty() {
            changelog += &format!("\n{} {}\n{}", rfd.name, rfd.short_link, changes);
        }
    }

    // Initialize the SendGrid clVient.
    let sendgrid_client = SendGrid::new_from_env();

    // Send the message.
    sendgrid_client
        .send_mail(
            format!("RFD changelog for the week from {}", week_format),
            changelog,
            vec![format!("all@{}", company.gsuite_domain)],
            vec![],
            vec![],
            format!("rfds@{}", company.gsuite_domain),
        )
        .await;
}

#[cfg(test)]
mod tests {
    use crate::{
        companies::Company,
        db::Database,
        rfds::{
            clean_rfd_html_links, refresh_db_rfds, send_rfd_changelog, update_discussion_link, update_state, NewRFD,
            RFDs,
        },
    };

    #[ignore]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_rfds() {
        // Initialize our database.
        let db = Database::new();

        // Get the company id for Oxide.
        // TODO: split this out per company.
        let oxide = Company::get_from_db(&db, "Oxide".to_string()).unwrap();

        refresh_db_rfds(&db, &oxide).await;

        // Update rfds in airtable.
        RFDs::get_from_db(&db, oxide.id).update_airtable(&db).await;
    }

    #[ignore]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_monday_cron_rfds_changelog() {
        // Initialize our database.
        let db = Database::new();

        // Get the company id for Oxide.
        // TODO: split this out per company.
        let oxide = Company::get_from_db(&db, "Oxide".to_string()).unwrap();

        send_rfd_changelog(&oxide).await;
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

        let cleaned = clean_rfd_html_links(content, "0032");

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
        let mut authors = NewRFD::get_authors(content, true);
        let mut expected = "things, joe".to_string();
        assert_eq!(expected, authors);

        content = r#"sdfsdf
= sdfgsdfgsdfg
things, joe
dsfsdf
sdf
:authors: nope"#;
        authors = NewRFD::get_authors(content, true);
        assert_eq!(expected, authors);

        content = r#"sdfsdf
= sdfgsdfgsdfg
things <things@email.com>, joe <joe@email.com>
dsfsdf
sdf
authors: nope"#;
        authors = NewRFD::get_authors(content, false);
        expected = r#"things <things@email.com>, joe <joe@email.com>"#.to_string();
        assert_eq!(expected, authors);

        content = r#":authors: Jess <jess@thing.com>

= sdfgsdfgsdfg
{authors}
dsfsdf
sdf"#;
        authors = NewRFD::get_authors(content, false);
        expected = r#"Jess <jess@thing.com>"#.to_string();
        assert_eq!(expected, authors);
    }

    #[test]
    fn test_get_state() {
        let mut content = r#"sdfsdf
sdfsdf
state: discussion
dsfsdf
sdf
authors: nope"#;
        let mut state = NewRFD::get_state(content);
        let mut expected = "discussion".to_string();
        assert_eq!(expected, state);

        content = r#"sdfsdf
= sdfgsdfgsdfg
:state: prediscussion
dsfsdf
sdf
:state: nope"#;
        state = NewRFD::get_state(content);
        expected = "prediscussion".to_string();
        assert_eq!(expected, state);
    }

    #[test]
    fn test_get_discussion() {
        let mut content = r#"sdfsdf
sdfsdf
discussion: https://github.com/oxidecomputer/rfd/pulls/1
dsfsdf
sdf
authors: nope"#;
        let mut discussion = NewRFD::get_discussion(content);
        let expected = "https://github.com/oxidecomputer/rfd/pulls/1".to_string();
        assert_eq!(expected, discussion);

        content = r#"sdfsdf
= sdfgsdfgsdfg
:discussion: https://github.com/oxidecomputer/rfd/pulls/1
dsfsdf
sdf
:discussion: nope"#;
        discussion = NewRFD::get_discussion(content);
        assert_eq!(expected, discussion);
    }

    #[test]
    fn test_update_discussion_link() {
        let link = "https://github.com/oxidecomputer/rfd/pulls/2019";
        let mut content = r#"sdfsdf
sdfsdf
discussion:   https://github.com/oxidecomputer/rfd/pulls/1
dsfsdf
sdf
authors: nope"#;
        let mut result = update_discussion_link(content, link, true);
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
        result = update_discussion_link(content, link, false);
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
        result = update_discussion_link(content, link, false);
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
        let state = "discussion";
        let mut content = r#"sdfsdf
sdfsdf
state:   sdfsdfsdf
dsfsdf
sdf
authors: nope"#;
        let mut result = update_state(content, state, true);
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
        result = update_state(content, state, false);
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
        result = update_state(content, state, false);
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
        let mut content = r#"things
# RFD 43 Identity and Access Management (IAM)
sdfsdf
title: https://github.com/oxidecomputer/rfd/pulls/1
dsfsdf
sdf
authors: nope"#;
        let mut title = NewRFD::get_title(content);
        let expected = "Identity and Access Management (IAM)".to_string();
        assert_eq!(expected, title);

        content = r#"sdfsdf
= RFD 43 Identity and Access Management (IAM)
:title: https://github.com/oxidecomputer/rfd/pulls/1
dsfsdf
= RFD 53 Bye
sdf
:title: nope"#;
        title = NewRFD::get_title(content);
        assert_eq!(expected, title);

        // Add a test to show what happens for rfd 31 where there is no "RFD" in
        // the title.
        content = r#"sdfsdf
= Identity and Access Management (IAM)
:title: https://github.com/oxidecomputer/rfd/pulls/1
dsfsdf
sdf
:title: nope"#;
        title = NewRFD::get_title(content);
        assert_eq!(expected, title);
    }
}
