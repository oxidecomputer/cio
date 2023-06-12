use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use csv::ReaderBuilder;
use log::info;
use octorust::Client as Octorust;
use serde::Deserialize;
use std::{borrow::Cow, fmt, future::Future, pin::Pin, str::from_utf8, sync::Arc};

use crate::{
    companies::Company,
    core::GitHubPullRequest,
    utils::is_image,
    utils::{create_or_update_file_in_github_repo, decode_base64_to_string, get_file_content_from_repo},
};

use super::{PDFStorage, RFDContent, RFDNumber, RFDPdf};

#[derive(Clone)]
pub struct GitHubRFDRepo {
    client: Arc<Octorust>,
    pub owner: String,
    pub repo: String,
    pub default_branch: String,
}

impl fmt::Debug for GitHubRFDRepo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GitHubRFDBranch")
            .field("owner", &self.owner)
            .field("repo", &self.repo)
            .field("default_branch", &self.default_branch)
            .finish()
    }
}

impl GitHubRFDRepo {
    /// Create a new RFD repo for the provided company. Assumes that the RFD repo is named "rfd"
    pub async fn new(company: &Company) -> Result<Self> {
        let github = company.authenticate_github()?;
        Self::new_with_client(company, Arc::new(github)).await
    }

    pub async fn new_with_client(company: &Company, client: Arc<octorust::Client>) -> Result<Self> {
        let full_repo = client.repos().get(&company.github_org, "rfd").await?.body;

        Ok(Self {
            client,
            owner: company.github_org.to_string(),
            repo: "rfd".to_string(),
            default_branch: full_repo.default_branch,
        })
    }

    /// Get an accessor for a RFD on a specific branch
    pub fn branch(&self, branch: String) -> GitHubRFDBranch {
        GitHubRFDBranch {
            client: self.client.clone(),
            owner: self.owner.clone(),
            repo: self.repo.clone(),
            default_branch: self.default_branch.clone(),
            branch,
        }
    }

    /// Read the remote rfd.csv file stored in GitHub and return a map from RFD number to RFD. The
    /// RFDs returned may or may have already been persisted
    pub async fn get_rfd_sync_updates(&self) -> Result<Vec<GitHubRFDUpdate>> {
        // Get the contents of the .helpers/rfd.csv file.
        let (rfd_csv_content, _) = get_file_content_from_repo(
            &self.client,
            &self.owner,
            &self.repo,
            &self.default_branch,
            "/.helpers/rfd.csv",
        )
        .await?;

        let rfd_csv_string = from_utf8(&rfd_csv_content)?;

        // Create the csv reader.
        let mut csv_reader = ReaderBuilder::new()
            .delimiter(b',')
            .has_headers(true)
            .from_reader(rfd_csv_string.as_bytes());

        Ok(csv_reader
            .deserialize::<RFDCsvRow>()
            .filter_map(|row| {
                row.ok().map(|row| {
                    let number: RFDNumber = row.num.into();

                    let branch_name = if row.link.contains(&format!("/{}/", self.default_branch)) {
                        self.default_branch.clone()
                    } else {
                        number.as_number_string()
                    };

                    GitHubRFDUpdate {
                        number,
                        branch: self.branch(branch_name),
                    }
                })
            })
            .collect())
    }
}

#[derive(Clone)]
pub struct GitHubRFDBranch {
    client: Arc<Octorust>,
    pub owner: String,
    pub repo: String,
    pub default_branch: String,
    pub branch: String,
}

impl fmt::Debug for GitHubRFDBranch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GitHubRFDBranch")
            .field("owner", &self.owner)
            .field("repo", &self.repo)
            .field("default_branch", &self.default_branch)
            .field("branch", &self.branch)
            .finish()
    }
}

impl GitHubRFDBranch {
    pub fn client(&self) -> &Octorust {
        &self.client
    }

    /// Checks if this branch actually exists in the remote system (GitHub)
    pub async fn exists_in_remote(&self) -> bool {
        self.client
            .repos()
            .get_branch(&self.owner, &self.repo, &self.branch)
            .await
            .is_ok()
    }

    /// Try to get the markdown or asciidoc contents from the repo.
    pub async fn get_readme_contents<'a>(&self, rfd_number: &RFDNumber) -> Result<GitHubRFDReadme<'a>> {
        info!("[rfd.contents] Enter {} / {}", self.repo, self.branch);

        #[cfg(debug_assertions)]
        {
            info!(
                "[rfd.contents] Remaining stack size: {:?} {} / {}",
                stacker::remaining_stack(),
                self.repo,
                self.branch
            );
        }

        info!("[rfd.contents] Fetched full repo {} / {}", self.repo, self.branch);

        // Use the supplied RFD number to determine the location in the RFD repo to read from
        let dir = rfd_number.repo_directory();

        // Get the contents of the file.
        let path = format!("{}/README.adoc", dir);

        let content_file = self
            .client
            .repos()
            .get_content_file(&self.owner, &self.repo, &path, &self.branch)
            .await
            .map(|response| response.body);

        info!(
            "[rfd.contents] Retrieved asciidoc README from GitHub {} / {}",
            &self.repo, &self.branch
        );

        let (file, decoded, is_markdown, sha, link) = match content_file {
            Ok(f) => {
                let decoded = decode_base64_to_string(&f.content);
                info!("[rfd.contents] Decoded asciidoc README {} / {}", self.repo, self.branch);
                (path, decoded, false, f.sha, f.html_url)
            }
            Err(e) => {
                info!(
                    "getting file contents for {} failed: {}, trying markdown instead...",
                    path, e
                );

                let md_path = format!("{}/README.md", dir);

                let f = self
                    .client
                    .repos()
                    .get_content_file(&self.owner, &self.repo, &md_path, &self.branch)
                    .await?
                    .body;

                let decoded = decode_base64_to_string(&f.content);
                (md_path, decoded, true, f.sha, f.html_url)
            }
        };

        let content = if is_markdown {
            RFDContent::new_markdown(Cow::Owned(decoded))
        } else {
            RFDContent::new_asciidoc(Cow::Owned(decoded))
        };

        // The html_url for the README.* file will look something like:
        //   https://github.com/<owner>/<repo>/blob/<number>/rfd/<number>/README.*
        // and we want to transform it to
        //   https://github.com/<owner>/<repo>/tree/<number>/rfd/<number>
        let tree_link = link.rsplit_once('/').map(|(dir, _)| dir.replace("blob", "tree"));

        Ok(GitHubRFDReadme {
            content,
            sha,
            location: GitHubRFDReadmeLocation {
                file,
                blob_link: link,
                tree_link,
                branch: self.clone(),
            },
        })
    }

    /// Get a list of images that are store in this branch
    pub async fn get_images(&self, rfd_number: &RFDNumber) -> Result<Vec<octorust::types::ContentFile>> {
        let dir = rfd_number.repo_directory();
        Self::get_images_internal(self.clone(), dir).await
    }

    fn get_images_internal(
        branch: Self,
        dir: String,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<octorust::types::ContentFile>>> + Send + 'static>> {
        Box::pin(async move {
            let mut files: Vec<octorust::types::ContentFile> = Default::default();

            let resp = branch
                .client
                .repos()
                .get_content_vec_entries(&branch.owner, &branch.repo, &dir, &branch.branch)
                .await?;

            for file in resp.body {
                info!(
                    "[rfd.get_images] Processing file {} ({}) {} / {}",
                    file.path, file.type_, branch.repo, branch.branch
                );

                if file.type_ == "dir" {
                    let images = Self::get_images_internal(branch.clone(), file.path).await?;

                    for image in images {
                        files.push(image)
                    }
                } else if is_image(&file.name) {
                    let f = crate::utils::get_github_entry_contents(
                        &branch.client,
                        &branch.owner,
                        &branch.repo,
                        &branch.branch,
                        &file,
                    )
                    .await?;
                    files.push(f);
                }
            }

            Ok(files)
        })
    }

    /// Find any existing pull request coming from the branch for this RFD
    pub async fn find_pull_requests(&self) -> Result<Vec<GitHubPullRequest>> {
        // If this is an update is occurring on the master branch than we can skip the look up as
        // we only want pull requests that are coming from an RFD branch
        let prs = if self.branch != self.default_branch {
            let pulls = self
                .client
                .pulls()
                .list_all(
                    &self.owner,
                    &self.repo,
                    octorust::types::IssuesListState::All,
                    // head
                    "",
                    // base
                    "",
                    // sort
                    Default::default(),
                    // direction
                    Default::default(),
                )
                .await?
                .body;

            pulls
                .into_iter()
                .filter_map(|pull| {
                    let pull_branch = pull.head.ref_.trim_start_matches("refs/heads/");

                    if pull_branch == self.branch {
                        Some(pull.into())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        } else {
            vec![]
        };

        Ok(prs)
    }

    pub async fn get_latest_commit_date(&self, rfd_number: &RFDNumber) -> Result<DateTime<Utc>> {
        let commits = self
            .client
            .repos()
            .list_commits(
                &self.owner,
                &self.repo,
                &self.branch,
                &rfd_number.repo_directory(),
                "",
                None,
                None,
                0,
                0,
            )
            .await?
            .body;
        let latest_commit = commits
            .get(0)
            .ok_or_else(|| anyhow!("No commits found for branch {}", self.branch))?;

        Ok(latest_commit
            .commit
            .committer
            .as_ref()
            .ok_or_else(|| anyhow!("Failed to find committer on latest commit to branch {}", self.branch))?
            .date
            .parse()?)
    }
}

#[async_trait]
impl PDFStorage for GitHubRFDBranch {
    async fn store_rfd_pdf(&self, pdf: &RFDPdf) -> Result<String> {
        let rfd_path = format!("/pdfs/{}", pdf.filename);

        create_or_update_file_in_github_repo(
            &self.client,
            &self.owner,
            &self.repo,
            &self.branch,
            &rfd_path,
            pdf.contents.to_vec(),
        )
        .await
        .map(|_| "".to_string())
    }
}

pub struct GitHubRFDReadme<'a> {
    pub content: RFDContent<'a>,
    pub sha: String,
    pub location: GitHubRFDReadmeLocation,
}

pub struct GitHubRFDReadmeLocation {
    pub file: String,
    pub blob_link: String,
    pub tree_link: Option<String>,
    pub branch: GitHubRFDBranch,
}

#[derive(Debug)]
pub struct GitHubRFDUpdate {
    pub number: RFDNumber,
    pub branch: GitHubRFDBranch,
}

impl GitHubRFDUpdate {
    pub fn client(&self) -> &Octorust {
        self.branch.client()
    }
}

#[derive(Deserialize)]
struct RFDCsvRow {
    num: i32,
    link: String,
}
