use anyhow::{anyhow, Result};
use async_trait::async_trait;
use csv::ReaderBuilder;
use log::{info, warn};
use octorust::{
    Client as Octorust,
};
use std::{
    collections::BTreeMap,
    str::from_utf8
};

use crate::{
    companies::Company,
    core::GitHubPullRequest,
    rfds::NewRFD,
    utils::{
        create_or_update_file_in_github_repo,
        decode_base64,
        decode_base64_to_string,
        get_file_content_from_repo,
    },
};
use super::{
    PDFStorage,
    RFDContent,
    RFDNumber,
    RFDPdf,
};

#[derive(Clone)]
pub struct GitHubRFDRepo {
    client: Octorust,
    owner: String,
    repo: String,
    default_branch: String,
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
            default_branch: full_repo.default_branch
        })
    }

    /// Get an accessor for a RFD on a specific branch
    pub fn branch(&self, rfd_number: i32, branch: String) -> GitHubRFDBranch {
        GitHubRFDBranch {
            client: self.client.clone(),
            owner: self.owner.clone(),
            repo: self.repo.clone(),
            default_branch: self.default_branch.clone(),
            rfd_number: rfd_number.into(),
            branch
        }
    }

    /// Read the remote rfd.csv file stored in GitHub and return a map from RFD number to RFD. The
    /// RFDs returned may or may have already been persisted
    pub async fn get_rfds_from_repo(&self) -> Result<BTreeMap<i32, NewRFD>> {

        // Get the contents of the .helpers/rfd.csv file.
        let (rfd_csv_content, _) =
            get_file_content_from_repo(&self.client, &self.owner, &self.repo, &self.default_branch, "/.helpers/rfd.csv").await?;
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

pub struct GitHubRFDBranch {
    pub client: Octorust,
    pub owner: String,
    pub repo: String,
    pub default_branch: String,
    pub rfd_number: RFDNumber,
    pub branch: String,
}

impl GitHubRFDBranch {

    /// Get the path to where the source contents of this RFD exists in the RFD repo.
    pub fn repo_directory(&self) -> String {
        format!("/rfd/{}", self.rfd_number.as_number_string())
    }

    /// Try to get the markdown or asciidoc contents from the repo.
    pub async fn get_readme_contents(&self) -> Result<GitHubRFDReadme> {
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

        let dir = self.repo_directory();

        // Get the contents of the file.
        let path = format!("{}/README.adoc", dir);

        let content_file = self.client.repos().get_content_file(&self.owner, &self.repo, &path, &self.branch).await;

        info!(
            "[rfd.contents] Retrieved asciidoc README from GitHub {} / {}",
            &self.repo, &self.branch
        );

        let (decoded, is_markdown, sha) = match content_file {
            Ok(f) => {
                let decoded = decode_base64_to_string(&f.content);
                info!("[rfd.contents] Decoded asciidoc README {} / {}", self.repo, self.branch);
                (decoded, false, f.sha)
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
                (decoded, true, f.sha)
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
            RFDContent::new_markdown(transliterated)
        } else {
            RFDContent::new_asciidoc(transliterated)
        };

        Ok(GitHubRFDReadme {
            content,
            sha,
        })
    }

    pub async fn copy_images_to_default_branch(&self) -> Result<()> {
        let dir = self.repo_directory();

        info!("[rfd.contents] Getting images from branch {} / {}", self.repo, self.branch);

        // Get all the images in the branch and make sure they are in the images directory on master.
        let images = self.get_images(dir).await?;

        info!("[rfd.contents] Updating images in branch {} / {}", self.repo, self.branch);

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
        let resp = self.client.repos().get_content_vec_entries(&self.owner, &self.repo, dir, &self.branch).await?;

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
                        let f = crate::utils::get_github_file(&self.client, &self.owner, &self.repo, &self.branch, &file2).await?;
                        files.push(f);
                    }
                }
            }

            if is_image(&file.name) {
                let f = crate::utils::get_github_file(&self.client, &self.owner, &self.repo, &self.branch, &file).await?;
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
}

/// Utility function for checking if a file extension looks like an image extension
pub fn is_image(file: &str) -> bool {
    file.ends_with(".svg") || file.ends_with(".png") || file.ends_with(".jpg") || file.ends_with(".jpeg")
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

pub struct GitHubRFDReadme {
    pub content: RFDContent,
    pub sha: String,
}

pub struct GitHubRFDPullRequest {

}

impl GitHubRFDPullRequest {
}