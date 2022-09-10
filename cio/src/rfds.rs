#![allow(clippy::from_over_into)]

use anyhow::{anyhow, Result};
use async_bb8_diesel::AsyncRunQueryDsl;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use log::{info, warn};
use macros::db;
use partial_struct::partial;
use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    airtable::AIRTABLE_RFD_TABLE,
    companies::Company,
    core::{GitHubPullRequest, UpdateAirtableRecord},
    db::Database,
    rfd::{GitHubRFDBranch, GitHubRFDReadme, GitHubRFDRepo, GitHubRFDUpdate, RFDContent, RFDSearchIndex},
    schema::rfds as r_f_ds,
    schema::rfds,
    utils::truncate,
};

/// The data type for an RFD.
#[partial(RFDIndexEntry, with(Queryable), without(Insertable, AsChangeset))]
#[partial(RFDEntry)]
#[db {
    target_struct = "NewRFD",
    new_struct_name = "RFD",
    airtable_base = "roadmap",
    airtable_table = "AIRTABLE_RFD_TABLE",
    match_on = {
        "number" = "i32",
    }
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[diesel(table_name = rfds)]
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
    #[partial(RFDIndexEntry(skip))]
    pub html: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    #[partial(RFDIndexEntry(skip))]
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
    #[partial(RFDIndexEntry(skip))]
    #[partial(RFDEntry(skip))]
    pub pdf_link_github: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    #[partial(RFDIndexEntry(skip))]
    #[partial(RFDEntry(skip))]
    pub pdf_link_google_drive: String,
    /// The CIO company ID.
    #[serde(default)]
    #[partial(RFDIndexEntry(skip))]
    #[partial(RFDEntry(skip))]
    pub cio_company_id: i32,
}

impl NewRFD {
    /// We want to fetch the most up to date representation of this RFD as we can at this point in
    /// time. This RFD may or may not already have a version in our internal database, and may or
    /// may not exist in GitHub. If the RFD does not exist in the internal database then we set a
    /// number of default fields and use the data from GitHub only. If the RFD does not exist in
    /// GitHub, then this will fail as we are effectively not getting any new data.
    ///
    /// This function will return both the old RFD (representing our internal state) as well as the
    /// new merged/updated version.
    pub async fn new_from_update(company: &Company, update: &GitHubRFDUpdate) -> Result<Self> {
        // If we can not find a remote file from GitHub then we abandon here.
        let readme = update.branch.get_readme_contents(Some(update.number)).await?;

        // Parse the RFD title from the contents.
        let title = readme.content.get_title();
        let name = NewRFD::generate_name(update.number.into(), &title);

        // Parse the discussion from the contents.
        let discussion = readme.content.get_discussion();

        Ok(NewRFD {
            number: update.number.into(),
            number_string: update.number.as_number_string(),
            title,
            name,
            state: readme.content.get_state(),
            link: readme.link,
            short_link: NewRFD::generate_short_link(update.number.into()),
            rendered_link: NewRFD::generate_rendered_link(&update.number.as_number_string()),
            discussion,
            authors: readme.content.get_authors(),

            html: readme.content.to_html(&update.number, &update.branch).await?.0,
            content: readme.content.raw().to_string(),

            sha: readme.sha,
            commit_date: update.commit_date,

            // Only exists in Airtable,
            milestones: Default::default(),
            // Only exists in Airtable,
            relevant_components: Default::default(),

            // PDF links are purposefully blanked out so that they do not point at an invalid file
            // while new PDFs are generated
            pdf_link_github: Default::default(),
            pdf_link_google_drive: Default::default(),
            cio_company_id: company.id,
        })
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

impl RFD {
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
    pub fn update_state(&mut self, state: &str) -> Result<()> {
        let mut content = RFDContent::new(&self.content)?;
        content.update_state(state);

        self.content = content.into_inner();
        self.state = state.to_string();

        Ok(())
    }

    /// Update an RFDs discussion link.
    pub fn update_discussion(&mut self, link: &str) -> Result<()> {
        let mut content = RFDContent::new(&self.content)?;
        content.update_discussion_link(link);

        self.content = content.into_inner();
        self.discussion = link.to_string();

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

    pub fn content(&self) -> Result<RFDContent> {
        RFDContent::new(&self.content)
    }

    pub async fn branch(&self, company: &Company) -> Result<GitHubRFDBranch> {
        let repo = GitHubRFDRepo::new(company).await?;

        let branch = if self.link.contains(&format!("/{}/", repo.default_branch)) {
            repo.default_branch.clone()
        } else {
            self.number_string.clone()
        };

        Ok(repo.branch(branch))
    }

    /// Expand the fields in the RFD.
    /// This will get the content, html, sha, commit_date as well as fill in all generated fields.
    pub async fn expand<'a>(&mut self, company: &Company) -> Result<GitHubRFDReadme<'a>> {
        info!("[rfd.expand] Running RFD expansion {} / {}", self.id, self.number);

        // let owner = &company.github_org;
        // let repo = "rfd";
        // let r = github.repos().get(owner, repo).await?;

        info!("[rfd.expand] Fetched full RFD repo {} / {}", self.id, self.number);

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

        info!(
            "[rfd.expand] Configured autogenerated fields {} / {}",
            self.id, self.number
        );

        // let mut branch = self.number_string.to_string();
        // if self.link.contains(&format!("/{}/", r.default_branch)) {
        //     branch = r.default_branch.to_string();
        // }

        // Create the branch representation of this RFD. This may point to either the default branch
        // or a specific RFD branch. In either case this does not ensure that a branch exists
        let branch = self.branch(company).await?;

        // let rfd_dir = branch.repo_directory();

        // Fetch the readme contents for this RFD
        let readme = branch.get_readme_contents(Some(self.number.into())).await?;

        // Extract the raw contents and sha from the fetched readme
        self.content = readme.content.raw().to_string();
        self.sha = readme.sha.clone();

        info!("[rfd.expand] Fetched contents of RFD {} / {}", self.id, self.number);

        if let Ok(commit_date) = branch.get_latest_commit_date().await {
            self.commit_date = commit_date;
        }

        info!(
            "[rfd.expand] Collected commit data for RFD {} / {}",
            self.id, self.number
        );

        // Parse the HTML.
        self.html = readme.content.to_html(&self.number.into(), &branch).await?.0;

        info!(
            "[rfd.expand] Parsed RFD contents into html {} / {}",
            self.id, self.number
        );

        if self.html.trim().is_empty() {
            return Err(anyhow!("got empty html for rfd#{}", self.number));
        }

        // TODO: Fix me
        // self.authors = NewRFD::get_authors(&self.content, is_markdown)?;

        info!("[rfd.expand] Extracted authors from RFD {} / {}", self.id, self.number);

        // Set the pdf link
        let file_name = self.get_pdf_filename();
        let rfd_path = format!("/pdfs/{}", file_name);
        self.pdf_link_github = format!("https://github.com/{}/rfd/blob/master{}", company.github_org, rfd_path);

        self.cio_company_id = company.id;

        info!("[rfd.expand] Finished expansion for RFD {} / {}", self.id, self.number);

        Ok(readme)
    }

    pub async fn sync(&mut self, db: &Database, company: &Company) -> Result<()> {
        let branch = self.branch(company).await?;

        // Expand the fields in the RFD
        let readme = self.expand(company).await?;

        // Update the RFD here just in case the PDF conversion fails.
        self.update(db).await?;

        // Now that the database is updated, update the search index.
        if let Err(err) = RFDSearchIndex::index_rfd(&self.number.into()).await {
            log::error!("Failed to index RFD in to search index {:?}", err);
        }

        // Make and update the PDF versions.
        match readme.content.to_pdf(&self.title, &self.number.into(), &branch).await {
            Ok(pdf) => match pdf.upload(&db, &company).await {
                Ok(pdf_links) => {
                    if let Some(github_url) = pdf_links.github_url {
                        self.pdf_link_github = github_url;
                    }

                    if let Some(google_drive_url) = pdf_links.google_drive_url {
                        self.pdf_link_google_drive = google_drive_url;
                    }
                }
                Err(err) => {
                    warn!("Failed to upload RFD {} PDF to storage {:?}", self.number, err);
                }
            },
            Err(err) => {
                warn!("Failed to convert RFD {} to PDF {:?}", self.number, err);
            }
        }

        // Update the RFD again, for the PDF.
        self.update(db).await?;

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

        Ok(())
    }
}

// Sync the rfds with our database.
pub async fn refresh_db_rfds(db: &Database, company: &Company) -> Result<()> {
    let repo = GitHubRFDRepo::new(company).await?;
    let rfds = repo.get_rfds_from_repo().await?;

    // Iterate over the rfds and update.
    // We should do these concurrently, but limit it to maybe 3 at a time.
    let mut i = 0;
    let take = 3;
    let mut skip = 0;
    while i < rfds.clone().len() {
        let tasks: Vec<_> = rfds
            .clone()
            .into_iter()
            .skip(skip)
            .take(take)
            .map(|(_, mut rfd)| {
                tokio::spawn(enclose! { (db, company) async move {
                    rfd.sync(&db, &company).await
                }})
            })
            .collect();

        let mut results: Vec<Result<()>> = Default::default();
        for task in tasks {
            results.push(task.await?);
        }

        for result in results {
            if let Err(e) = result {
                warn!("[rfd] {}", e);
            }
        }

        i += take;
        skip += take;
    }

    // Update rfds in airtable.
    RFDs::get_from_db(db, company.id).await?.update_airtable(db).await?;

    Ok(())
}

impl NewRFD {
    async fn sync(&mut self, db: &Database, company: &Company) -> Result<()> {
        // Check if we already have an existing RFD.
        if let Some(existing) = RFD::get_from_db(db, self.number).await {
            // Make sure there is not a break in the UI where this would be blank.
            self.content = existing.content.to_string();
            self.authors = existing.authors.to_string();
            self.html = existing.html.to_string();
            self.commit_date = existing.commit_date;
            self.sha = existing.sha.to_string();
            self.pdf_link_github = existing.pdf_link_github.to_string();
            self.pdf_link_google_drive = existing.pdf_link_google_drive;
        }

        // Persist this record back to the database
        let mut rfd = self.upsert(db).await?;

        // Now with a persisted record, perform the remaining sync tasks
        rfd.sync(&db, &company).await?;

        // // Expand the fields in the RFD.
        // rfd.expand(github, company).await?;

        // // Update the RFD here just in case the PDF conversion fails.
        // let mut rfd = rfd.update(db).await?;

        // // Now that the database is updated, update the search index.
        // RFDSearchIndex::index_rfd(&rfd.number.into()).await?;

        // let branch = rfd.branch().await?;
        // let readme = rfd.readme().await?;

        // // Make and update the PDF versions.
        // if let Err(err) = rfd.convert_and_upload_pdf(db, github, company).await {
        //     warn!(
        //         "failed to convert and upload PDF for RFD {}: {}",
        //         rfd.number_string, err
        //     );
        // }

        // // Update the RFD again, for the PDF.
        // rfd.update(db).await?;

        Ok(())
    }
}

// #[cfg(test)]
// mod tests {
//     use crate::{
//         companies::Company,
//         db::Database,
//         rfds::{clean_rfd_html_links, send_rfd_changelog, update_discussion_link, update_state, NewRFD},
//     };

//     #[ignore]
//     #[tokio::test(flavor = "multi_thread")]
//     async fn test_monday_cron_rfds_changelog() {
//         crate::utils::setup_logger();

//         // Initialize our database.
//         let db = Database::new().await;

//         // Get the company id for Oxide.
//         // TODO: split this out per company.
//         let oxide = Company::get_from_db(&db, "Oxide".to_string()).await.unwrap();

//         send_rfd_changelog(&db, &oxide).await.unwrap();
//     }

//     #[test]
//     fn test_clean_rfd_html_links() {
//         crate::utils::setup_logger();

//         let content = r#"https://3.rfd.oxide.computer
//         https://41.rfd.oxide.computer
//         https://543.rfd.oxide.computer#-some-link
//         https://3245.rfd.oxide.computer/things
//         https://3265.rfd.oxide.computer/things
//         <img src="things.png" \>
//         <a href="\#_principles">
//         <object data="thing.svg">
//         <object type="image/svg+xml" data="thing.svg">
//         <a href="\#things" \>
//         link:thing.html[Our thing]
//         link:http://example.com[our example]"#;

//         let cleaned = clean_rfd_html_links(content, "0032").unwrap();

//         let expected = r#"https://rfd.shared.oxide.computer/rfd/0003
//         https://rfd.shared.oxide.computer/rfd/0041
//         https://rfd.shared.oxide.computer/rfd/0543#-some-link
//         https://rfd.shared.oxide.computer/rfd/3245/things
//         https://rfd.shared.oxide.computer/rfd/3265/things
//         <img src="/static/images/0032/things.png" \>
//         <a href="/rfd/0032#_principles">
//         <object data="/static/images/0032/thing.svg">
//         <object type="image/svg+xml" data="/static/images/0032/thing.svg">
//         <a href="/rfd/0032#things" \>
//         link:https://0032.rfd.oxide.computer/thing.html[Our thing]
//         link:http://example.com[our example]"#;

//         assert_eq!(expected, cleaned);
//     }

//     #[test]
//     fn test_get_authors() {
//         crate::utils::setup_logger();

//         let mut content = r#"sdfsdf
// sdfsdf
// authors: things, joe
// dsfsdf
// sdf
// authors: nope"#;
//         let mut authors = NewRFD::get_authors(content, true).unwrap();
//         let mut expected = "things, joe".to_string();
//         assert_eq!(expected, authors);

//         content = r#"sdfsdf
// = sdfgsdfgsdfg
// things, joe
// dsfsdf
// sdf
// :authors: nope"#;
//         authors = NewRFD::get_authors(content, true).unwrap();
//         expected = "".to_string();
//         assert_eq!(expected, authors);

//         content = r#"sdfsdf
// = sdfgsdfgsdfg
// things <things@email.com>, joe <joe@email.com>
// dsfsdf
// sdf
// authors: nope"#;
//         authors = NewRFD::get_authors(content, false).unwrap();
//         expected = r#"things <things@email.com>, joe <joe@email.com>"#.to_string();
//         assert_eq!(expected, authors);

//         content = r#":authors: Jess <jess@thing.com>

// = sdfgsdfgsdfg
// {authors}
// dsfsdf
// sdf"#;
//         authors = NewRFD::get_authors(content, false).unwrap();
//         expected = r#"Jess <jess@thing.com>"#.to_string();
//         assert_eq!(expected, authors);
//     }

//     #[test]
//     fn test_get_state() {
//         crate::utils::setup_logger();

//         let mut content = r#"sdfsdf
// sdfsdf
// state: discussion
// dsfsdf
// sdf
// authors: nope"#;
//         let mut state = NewRFD::get_state(content).unwrap();
//         let mut expected = "discussion".to_string();
//         assert_eq!(expected, state);

//         content = r#"sdfsdf
// = sdfgsdfgsdfg
// :state: prediscussion
// dsfsdf
// sdf
// :state: nope"#;
//         state = NewRFD::get_state(content).unwrap();
//         expected = "prediscussion".to_string();
//         assert_eq!(expected, state);
//     }

//     #[test]
//     fn test_get_discussion() {
//         crate::utils::setup_logger();

//         let mut content = r#"sdfsdf
// sdfsdf
// discussion: https://github.com/oxidecomputer/rfd/pulls/1
// dsfsdf
// sdf
// authors: nope"#;
//         let mut discussion = NewRFD::get_discussion(content).unwrap();
//         let expected = "https://github.com/oxidecomputer/rfd/pulls/1".to_string();
//         assert_eq!(expected, discussion);

//         content = r#"sdfsdf
// = sdfgsdfgsdfg
// :discussion: https://github.com/oxidecomputer/rfd/pulls/1
// dsfsdf
// sdf
// :discussion: nope"#;
//         discussion = NewRFD::get_discussion(content).unwrap();
//         assert_eq!(expected, discussion);
//     }

//     #[test]
//     fn test_update_discussion_link() {
//         crate::utils::setup_logger();

//         let link = "https://github.com/oxidecomputer/rfd/pulls/2019";
//         let mut content = r#"sdfsdf
// sdfsdf
// discussion:   https://github.com/oxidecomputer/rfd/pulls/1
// dsfsdf
// sdf
// authors: nope"#;
//         let mut result = update_discussion_link(content, link, true);
//         let mut expected = r#"sdfsdf
// sdfsdf
// discussion: https://github.com/oxidecomputer/rfd/pulls/2019
// dsfsdf
// sdf
// authors: nope"#;
//         assert_eq!(expected, result);

//         content = r#"sdfsdf
// = sdfgsd
// discussion: fgsdfg
// :discussion: https://github.com/oxidecomputer/rfd/pulls/1
// dsfsdf
// sdf
// :discussion: nope"#;
//         result = update_discussion_link(content, link, false);
//         expected = r#"sdfsdf
// = sdfgsd
// discussion: fgsdfg
// :discussion: https://github.com/oxidecomputer/rfd/pulls/2019
// dsfsdf
// sdf
// :discussion: nope"#;
//         assert_eq!(expected, result);

//         content = r#"sdfsdf
// = sdfgsd
// discussion: fgsdfg
// :discussion:
// dsfsdf
// sdf
// :discussion: nope"#;
//         result = update_discussion_link(content, link, false);
//         expected = r#"sdfsdf
// = sdfgsd
// discussion: fgsdfg
// :discussion: https://github.com/oxidecomputer/rfd/pulls/2019
// dsfsdf
// sdf
// :discussion: nope"#;
//         assert_eq!(expected, result);
//     }

//     #[test]
//     fn test_update_state() {
//         crate::utils::setup_logger();

//         let state = "discussion";
//         let mut content = r#"sdfsdf
// sdfsdf
// state:   sdfsdfsdf
// dsfsdf
// sdf
// authors: nope"#;
//         let mut result = update_state(content, state, true).unwrap();
//         let mut expected = r#"sdfsdf
// sdfsdf
// state: discussion
// dsfsdf
// sdf
// authors: nope"#;
//         assert_eq!(expected, result);

//         content = r#"sdfsdf
// = sdfgsd
// state: fgsdfg
// :state: prediscussion
// dsfsdf
// sdf
// :state: nope"#;
//         result = update_state(content, state, false).unwrap();
//         expected = r#"sdfsdf
// = sdfgsd
// state: fgsdfg
// :state: discussion
// dsfsdf
// sdf
// :state: nope"#;
//         assert_eq!(expected, result);

//         content = r#"sdfsdf
// = sdfgsd
// state: fgsdfg
// :state:
// dsfsdf
// sdf
// :state: nope"#;
//         result = update_state(content, state, false).unwrap();
//         expected = r#"sdfsdf
// = sdfgsd
// state: fgsdfg
// :state: discussion
// dsfsdf
// sdf
// :state: nope"#;
//         assert_eq!(expected, result);
//     }

//     #[test]
//     fn test_get_title() {
//         crate::utils::setup_logger();

//         let mut content = r#"things
// # RFD 43 Identity and Access Management (IAM)
// sdfsdf
// title: https://github.com/oxidecomputer/rfd/pulls/1
// dsfsdf
// sdf
// authors: nope"#;
//         let mut title = NewRFD::get_title(content).unwrap();
//         let expected = "Identity and Access Management (IAM)".to_string();
//         assert_eq!(expected, title);

//         content = r#"sdfsdf
// = RFD 43 Identity and Access Management (IAM)
// :title: https://github.com/oxidecomputer/rfd/pulls/1
// dsfsdf
// = RFD 53 Bye
// sdf
// :title: nope"#;
//         title = NewRFD::get_title(content).unwrap();
//         assert_eq!(expected, title);

//         // Add a test to show what happens for rfd 31 where there is no "RFD" in
//         // the title.
//         content = r#"sdfsdf
// = Identity and Access Management (IAM)
// :title: https://github.com/oxidecomputer/rfd/pulls/1
// dsfsdf
// sdf
// :title: nope"#;
//         title = NewRFD::get_title(content).unwrap();
//         assert_eq!(expected, title);
//     }
// }
