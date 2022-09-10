use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use csv::ReaderBuilder;
use log::{info, warn};
use octorust::Client as Octorust;
use std::{borrow::Cow, collections::BTreeMap, fmt, str::from_utf8};

use super::{PDFStorage, RFDContent, RFDNumber, RFDPdf};
use crate::{
    companies::Company,
    core::GitHubPullRequest,
    rfds::NewRFD,
    utils::is_image,
    utils::{create_or_update_file_in_github_repo, decode_base64, decode_base64_to_string, get_file_content_from_repo},
};

#[derive(Clone)]
pub struct GitHubRFDRepo {
    client: Octorust,
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
        let full_repo = github.repos().get(&company.github_org, "rfd").await?;

        Ok(Self {
            client: github,
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
    pub async fn get_rfds_from_repo(&self) -> Result<BTreeMap<i32, NewRFD>> {
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

        // Create the BTreeMap of RFDs.
        let mut rfds: BTreeMap<i32, NewRFD> = Default::default();
        for r in csv_reader.deserialize() {
            let mut rfd: NewRFD = r?;

            // TODO: this whole thing is a mess jessfraz needs to cleanup
            rfd.number_string = NewRFD::generate_number_string(rfd.number);
            rfd.name = NewRFD::generate_name(rfd.number, &rfd.title);

            // Removing company record association abstraction
            rfd.cio_company_id = 1;

            // Add this to our BTreeMap.
            rfds.insert(rfd.number, rfd);
        }

        Ok(rfds)
    }
}

#[derive(Clone)]
pub struct GitHubRFDBranch {
    client: Octorust,
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
    /// Get the path to where the source contents of this RFD exists in the RFD repo.
    pub fn repo_directory(&self) -> String {
        format!("/rfd/{}", self.branch)
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
    pub async fn get_readme_contents<'a>(&self, rfd_number: Option<RFDNumber>) -> Result<GitHubRFDReadme<'a>> {
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

        // If an rfd_number has been supplied than use that to determine the location in the RFD
        // repo to read from
        let dir = if let Some(number) = rfd_number {
            number.repo_directory()
        } else {
            // Otherwise use the branch name to determine the appropriate location
            self.repo_directory()
        };

        // Get the contents of the file.
        let path = format!("{}/README.adoc", dir);

        let content_file = self
            .client
            .repos()
            .get_content_file(&self.owner, &self.repo, &path, &self.branch)
            .await;

        info!(
            "[rfd.contents] Retrieved asciidoc README from GitHub {} / {}",
            &self.repo, &self.branch
        );

        let (decoded, is_markdown, sha, link) = match content_file {
            Ok(f) => {
                let decoded = decode_base64_to_string(&f.content);
                info!("[rfd.contents] Decoded asciidoc README {} / {}", self.repo, self.branch);
                (decoded, false, f.sha, f.html_url)
            }
            Err(e) => {
                info!(
                    "getting file contents for {} failed: {}, trying markdown instead...",
                    path, e
                );

                let f = self
                    .client
                    .repos()
                    .get_content_file(&self.owner, &self.repo, &format!("{}/README.md", dir), &self.branch)
                    .await?;

                let decoded = decode_base64_to_string(&f.content);
                (decoded, true, f.sha, f.html_url)
            }
        };

        info!(
            "[rfd.contents] Transforming from unicode to ascii (length: {}) {} / {}",
            decoded.len(),
            self.repo,
            self.branch
        );

        let transliterated = deunicode::deunicode(&decoded);

        info!(
            "[rfd.contents] Ascii version length: {} {} / {}",
            transliterated.len(),
            self.repo,
            self.branch
        );

        let content = if is_markdown {
            RFDContent::new_markdown(Cow::Owned(transliterated))
        } else {
            RFDContent::new_asciidoc(Cow::Owned(transliterated))
        };

        Ok(GitHubRFDReadme { content, link, sha })
    }

    pub async fn copy_images_to_default_branch(&self) -> Result<()> {
        info!(
            "[rfd.contents] Getting images from branch {} / {}",
            self.repo, self.branch
        );

        // Get all the images in the branch and make sure they are in the images directory on master.
        let images = self.get_images().await?;

        info!(
            "[rfd.contents] Updating images in branch {} / {}",
            self.repo, self.branch
        );

        for image in images {
            let new_path = image.path.replace("rfd/", "src/public/static/images/");

            info!(
                "[rfd.contents] Copy {} to {} {} / {}",
                image.path, new_path, self.repo, self.branch
            );

            // Make sure we have this file in the static images dir on the master branch.
            create_or_update_file_in_github_repo(
                &self.client,
                &self.owner,
                &self.repo,
                &self.default_branch,
                &new_path,
                decode_base64(&image.content),
            )
            .await?;
        }

        Ok(())
    }

    /// Get a list of images that are store in this branch
    pub async fn get_images(&self) -> Result<Vec<octorust::types::ContentFile>> {
        let dir = self.repo_directory();

        let mut files: Vec<octorust::types::ContentFile> = Default::default();

        // Get all the images in the branch and make sure they are in the images directory on master.
        let resp = self
            .client
            .repos()
            .get_content_vec_entries(&self.owner, &self.repo, &dir, &self.branch)
            .await?;

        for file in resp {
            info!(
                "[rfd.get_images] Processing file {} ({}) {} / {}",
                file.path, file.type_, self.repo, self.branch
            );

            if file.type_ == "dir" {
                let path = file.path.trim_end_matches('/');
                // We have a directory. We need to get the file contents recursively.
                // TODO: find a better way to make this recursive without pissing off tokio.
                let resp2 = self
                    .client
                    .repos()
                    .get_content_vec_entries(&self.owner, &self.repo, path, &self.branch)
                    .await?;
                for file2 in resp2 {
                    info!(
                        "[rfd.get_images] Processing inner file {} ({}) {} / {}",
                        file.path, file.type_, self.repo, self.branch
                    );

                    if file2.type_ == "dir" {
                        let path = file2.path.trim_end_matches('/');
                        warn!("skipping directory second level directory for parsing images: {}", path);
                        continue;
                    }

                    if is_image(&file2.name) {
                        let f =
                            crate::utils::get_github_file(&self.client, &self.owner, &self.repo, &self.branch, &file2)
                                .await?;
                        files.push(f);
                    }
                }
            }

            if is_image(&file.name) {
                let f =
                    crate::utils::get_github_file(&self.client, &self.owner, &self.repo, &self.branch, &file).await?;
                files.push(f);
            }
        }

        Ok(files)
    }

    /// Find any existing pull request coming from the branch for this RFD
    pub async fn find_pull_requests(&self) -> Result<Vec<GitHubPullRequest>> {
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
            .await?;

        let mut matching_pulls = vec![];

        for pull in pulls.into_iter() {
            // Check if the pull request is for our branch.
            let pull_branch = pull.head.ref_.trim_start_matches("refs/heads/");

            if pull_branch == self.branch {
                matching_pulls.push(pull.into());
            }
        }

        Ok(matching_pulls)
    }

    pub async fn get_latest_commit_date(&self) -> Result<DateTime<Utc>> {
        let commits = self
            .client
            .repos()
            .list_commits(
                &self.owner,
                &self.repo,
                &self.branch,
                &self.repo_directory(),
                "",
                None,
                None,
                0,
                0,
            )
            .await?;
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
    pub link: String,
    pub sha: String,
}

#[derive(Debug)]
pub struct GitHubRFDUpdate {
    pub number: RFDNumber,
    pub branch: GitHubRFDBranch,
    pub file: String,
    pub commit_date: DateTime<Utc>,
}
