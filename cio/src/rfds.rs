#![allow(clippy::from_over_into)]

use anyhow::{anyhow, bail, Result};
use async_bb8_diesel::AsyncRunQueryDsl;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use macros::db;
use partial_struct::partial;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    airtable::AIRTABLE_RFD_TABLE,
    companies::Company,
    core::{GitHubPullRequest, UpdateAirtableRecord},
    rfd::{GitHubRFDBranch, GitHubRFDReadmeLocation, GitHubRFDRepo, GitHubRFDUpdate, RFDContent},
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

pub struct RemoteRFD {
    pub rfd: NewRFD,
    pub location: GitHubRFDReadmeLocation,
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
    pub async fn new_from_update(company: &Company, update: &GitHubRFDUpdate) -> Result<RemoteRFD> {
        let github = update.client();

        // If we can not find a remote file from GitHub then we abandon here.
        let readme = update.branch.get_readme_contents(&update.number).await?;

        // Parse the RFD title from the contents.
        let title = readme.content.get_title().trim().to_string();
        let name = Self::generate_name(update.number.into(), &title);

        // Parse the discussion from the contents.
        let discussion = readme.content.get_discussion();

        let html = readme.content.to_html(&update.number, &update.branch).await?.0;

        // TODO: Unsure if this should actually be an error, but this mirrors the previous logic
        if html.trim().is_empty() {
            bail!("Generated RFD has empty content")
        }

        // Get the commit date.
        let commits = github
            .repos()
            .list_commits(
                &update.branch.owner,
                &update.branch.repo,
                &update.branch.branch,
                &update.number.repo_directory(),
                "",
                None,
                None,
                0,
                0,
            )
            .await?;

        let latest_commit = commits.get(0).ok_or_else(|| {
            anyhow!(
                "RFD {} on {} does not have any commits",
                update.number,
                update.branch.branch
            )
        })?;
        let commit_date = latest_commit
            .commit
            .committer
            .as_ref()
            .ok_or_else(|| {
                anyhow!(
                    "RFD {} on {} ({}) does not have a commit date",
                    update.number,
                    update.branch.branch,
                    latest_commit.sha
                )
            })?
            .date
            .parse()?;

        Ok(RemoteRFD {
            rfd: NewRFD {
                number: update.number.into(),
                number_string: update.number.as_number_string(),
                title,
                name,
                state: readme.content.get_state(),
                link: readme.link,
                short_link: Self::generate_short_link(update.number.into()),
                rendered_link: Self::generate_rendered_link(&update.number.as_number_string()),
                discussion,
                authors: readme.content.get_authors(),

                html,
                content: readme.content.raw().to_string(),

                sha: readme.sha,
                commit_date,

                // Only exists in Airtable,
                milestones: Default::default(),
                // Only exists in Airtable,
                relevant_components: Default::default(),

                // PDF links are purposefully blanked out so that they do not point at an invalid file
                // while new PDFs are generated
                pdf_link_github: Default::default(),
                pdf_link_google_drive: Default::default(),
                cio_company_id: company.id,
            },
            location: readme.location,
        })
    }

    fn generate_name(number: i32, title: &str) -> String {
        format!("RFD {} {}", number, title)
    }

    fn generate_short_link(number: i32) -> String {
        format!("https://{}.rfd.oxide.computer", number)
    }

    fn generate_rendered_link(number_string: &str) -> String {
        format!("https://rfd.shared.oxide.computer/rfd/{}", number_string)
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

    async fn branch(&self, company: &Company) -> Result<GitHubRFDBranch> {
        let repo = GitHubRFDRepo::new(company).await?;

        let branch = if self.link.contains(&format!("/{}/", repo.default_branch)) {
            repo.default_branch.clone()
        } else {
            self.number_string.clone()
        };

        Ok(repo.branch(branch))
    }

    pub async fn create_sync(&self, company: &Company) -> Result<GitHubRFDUpdate> {
        let update = GitHubRFDUpdate {
            number: self.number.into(),
            branch: self.branch(company).await?,
        };

        Ok(update)
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
