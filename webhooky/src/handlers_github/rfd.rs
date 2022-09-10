use anyhow::{anyhow, Result};
use async_trait::async_trait;
use cio_api::{
    core::{GitHubCommit, GitHubPullRequest},
    features::Features,
    rfd::{GitHubRFDBranch, GitHubRFDRepo, GitHubRFDUpdate, RFDSearchIndex},
    rfds::{NewRFD, RFD},
    shorturls::generate_shorturls_for_rfds,
    utils::{create_or_update_file_in_github_repo, get_file_content_from_repo, is_image},
};
use google_drive::traits::{DriveOps, FileOps};
use log::{error, info, warn};

use crate::{context::Context, github_types::GitHubWebhook};

fn get_rfd_updates(branch: &GitHubRFDBranch, commit: &GitHubCommit) -> Vec<GitHubRFDUpdate> {
    let mut updates = vec![];

    // Iterate through all of the updated files and for anything that looks like and RFD document
    // we generate a update. (These are documents that exist in /rfd/ and are named either
    // README.adoc or README.md
    let mut changed_files: Vec<&String> = vec![];
    changed_files.extend(commit.added.iter());
    changed_files.extend(commit.modified.iter());

    for file in changed_files {
        // We only care about files in the rfd/ directory
        if !file.starts_with("rfd/") {
            continue;
        }

        // If the file is a README.md or README.adoc, this
        if file.ends_with("README.md") || file.ends_with("README.adoc") {
            // Parse the RFD directory as an int.
            let (dir, _) = file.trim_start_matches("rfd/").split_once('/').unwrap();

            // If we can not easily parse an RFD number from the path than we ignore it
            if let Ok(number) = dir.trim_start_matches('0').parse::<i32>() {
                if let Some(commit_date) = commit.timestamp {
                    updates.push(GitHubRFDUpdate {
                        number: number.into(),
                        branch: branch.clone(),
                        file: file.clone(),
                        commit_date,
                    });
                } else {
                    log::error!("RFD document commit is missing a timestamp. {}", commit.id);
                }
            } else {
                log::warn!(
                    "Found README document that looks like an RFD, but could not determine an RFD number. {} {}",
                    commit.id,
                    file
                );
            }
        }
    }

    updates
}

trait Validate {
    fn is_valid(&self) -> bool;
}

impl Validate for GitHubRFDUpdate {
    fn is_valid(&self) -> bool {
        // An RFD update is only valid in one of two cases:
        //  `1. The update is occurring on the default branch. In this case it does not matter what
        //      RFD is being updated, the update is always considered valid
        //   2. The update is occurring on an RFD branch with a name of the pattern \d\d\d\d . In
        //      this case, the update is only valid if the number of the RFD being updated matches
        //      the branch the update is occurring on.
        self.branch.branch == self.branch.default_branch || self.branch.branch == self.number.as_number_string()
    }
}

pub struct RFDPushHandler {
    post_update_hooks: Vec<Box<dyn PostRFDUpdateHook + Send + Sync>>,
}

impl RFDPushHandler {
    pub fn new() -> Self {
        Self {
            post_update_hooks: vec![
                Box::new(UpdateSearch),
                Box::new(UpdatePDFs),
                Box::new(GenerateShortUrls),
                Box::new(CreatePullRequest),
                Box::new(UpdatePullRequest),
                Box::new(UpdateDiscussionUrl),
                Box::new(EnsureRFDOnDefaultIsInPublishedState),
                Box::new(DeleteOldPDFs),
            ],
        }
    }

    /// Handle a `push` event for the rfd repo.
    pub async fn handle(&self, github: &octorust::Client, api_context: &Context, event: GitHubWebhook) -> Result<()> {
        info!("[rfd.push] Remaining stack size: {:?}", stacker::remaining_stack());

        // Perform validation checks first to determine if we need to process this call or if we can
        // drop it early
        if event.repository.name != "repo" {
            error!(
                "Attempting to run rfd `push` handler on the {} repo. Exiting as this should not occur.",
                event.repository.name
            );
            return Ok(());
        }

        if event.commits.is_empty() {
            // Return early that there are no commits.
            // IDK how we got here, since we check this above in the main github handler.
            warn!("rfd `push` event had no commits");
            return Ok(());
        }

        // Get the commit.
        let mut commit = event.commits.get(0).unwrap().clone();

        // Ignore any changes that are not to the `rfd/` directory.
        let dir = "rfd/";
        commit.filter_files_by_path(dir);
        if !commit.has_changed_files() {
            // No files changed that we care about.
            // We can throw this out, log it and return early.
            info!(
                "`push` event commit `{}` does not include any changes to the `{}` directory",
                commit.id, dir
            );
            return Ok(());
        }

        // Look up our RFD repo
        let repo = GitHubRFDRepo::new(&api_context.company).await?;

        // Get the branch name.
        let branch_name = event.refv.trim_start_matches("refs/heads/");
        let branch = repo.branch(branch_name.to_string());

        // We are always creating updates based on the branch that is defined by the event, independent
        // of it if corresponds with RFD number of the file(s) updated. We are only responsible for
        // generating updates, not determining if they make sense to process.
        let updates = get_rfd_updates(&branch, &commit);

        let log_message = |s: &str| {
            info!("[rfd] [{}] {}", commit.sha, s);
        };

        // Loop through the updates that were found and process them individually. We also throw out any
        // updates that attempt to update a mismatched RFD
        for update in updates {
            // Skip any updates that fail validation
            if !update.is_valid() {
                continue;
            }

            // We have a README file that changed, let's parse the RFD and update it
            // in our database.
            info!(
                "`push` event -> file {} was modified on branch {}",
                update.file, update.branch.branch
            );

            // If this branch does not actually exist in GitHub, then we drop the update
            if !update.branch.exists_in_remote().await {
                info!("Dropping RFD update as the remote branch has gone missing {:?}", update);
                continue;
            }

            // Fetch the latest RFD information from GitHub
            let new_rfd = NewRFD::new_from_update(&api_context.company, &update).await?;

            info!("Generated RFD for branch {} from GitHub", update.branch.branch);

            // Get the old RFD from the database.
            // DO THIS BEFORE UPDATING THE RFD.
            // We will need this later to check if the RFD's state changed.
            let old_rfd = RFD::get_from_db(&api_context.db, new_rfd.number).await;

            info!(
                "Checked for existing RFD in the database {:?}",
                old_rfd.as_ref().map(|o| o.id)
            );

            // Update the RFD in the database.
            let mut rfd = new_rfd.upsert(&api_context.db).await?;

            info!("Updated RFD {} in the database", update.number);

            // The RFD has been stored internally, now trigger the post update actions
            self.run_hooks(api_context, &update, old_rfd.as_ref(), &mut rfd).await?;

            info!("[SUCCESS]: RFD {} `push` operations completed", rfd.number_string);
        }

        // Iterate over the removed files and remove any images that we no longer
        // need for the HTML rendered RFD website.
        for file in &commit.removed {
            // Make sure the file has a prefix of "rfd/".
            if !file.starts_with("rfd/") {
                // Continue through the loop early.
                // We only care if a file change in the rfd/ directory.
                continue;
            }

            if is_image(file) {
                // Remove the image from the `src/public/static/images` path since we no
                // longer need it.
                // We delete these on the default branch ONLY.
                let website_file = file.replace("rfd/", "src/public/static/images/");

                // We need to get the current sha for the file we want to delete.
                let (_, gh_file_sha) = if let Ok((v, s)) = get_file_content_from_repo(
                    github,
                    &branch.owner,
                    &branch.repo,
                    &branch.default_branch,
                    &website_file,
                )
                .await
                {
                    (v, s)
                } else {
                    // If there was an error, likely the file does not exist, so we can continue
                    // anyways.
                    (vec![], "".to_string())
                };

                if !gh_file_sha.is_empty() {
                    github
                        .repos()
                        .delete_file(
                            &repo.owner,
                            &repo.repo,
                            &website_file,
                            &octorust::types::ReposDeleteFileRequest {
                                message: format!(
                                    "Deleting file content {} programatically\n\nThis is done from \
                                    the cio repo webhooky::listen_github_webhooks function.",
                                    website_file
                                ),
                                sha: gh_file_sha,
                                committer: None,
                                author: None,
                                branch: branch.default_branch.to_string(),
                            },
                        )
                        .await?;
                    log_message(&format!(
                        "[SUCCESS]: deleted file `{}` since it was removed in this push",
                        website_file,
                    ));
                }
            }
        }

        // Iterate over the files and update the RFDs that have been added or
        // modified in our database.
        let mut changed_files = commit.added.clone();
        changed_files.append(&mut commit.modified.clone());
        for file in changed_files {
            // Make sure the file has a prefix of "rfd/".
            if !file.starts_with("rfd/") {
                // Continue through the loop early.
                // We only care if a file change in the rfd/ directory.
                continue;
            }

            // Update images for the static site.
            if is_image(&file) {
                // Some image for an RFD updated. Let's make sure we have that image in the right place
                // for the RFD shared site.
                // First, let's read the file contents.
                let (gh_file_content, _) =
                    get_file_content_from_repo(github, &branch.owner, &branch.repo, &branch.branch, &file).await?;

                // Let's write the file contents to the location for the static website.
                // We replace the `rfd/` path with the `src/public/static/images/` path since
                // this is where images go for the static website.
                // We update these on the default branch ONLY
                let website_file = file.replace("rfd/", "src/public/static/images/");
                create_or_update_file_in_github_repo(
                    github,
                    &branch.owner,
                    &branch.repo,
                    &branch.default_branch,
                    &website_file,
                    gh_file_content,
                )
                .await?;

                info!(
                    "[SUCCESS]: updated file `{}` since it was modified in this push",
                    website_file
                );
            }
        }

        // TODO: should we do something if the file gets deleted (?)
        Ok(())
    }

    async fn run_hooks(
        &self,
        api_context: &Context,
        update: &GitHubRFDUpdate,
        old_rfd: Option<&RFD>,
        new_rfd: &mut RFD,
    ) -> Result<()> {
        let github = api_context.company.authenticate_github()?;
        let pull_requests = update.branch.find_pull_requests().await?;

        // This is here to remain consistent with previous behavior. This likely needs to be
        // refactored to account for multiple pull requests existing (even though there *should*
        // never be multiple)
        let pull_request = pull_requests.get(0);

        for hook in &self.post_update_hooks {
            hook.run(api_context, &github, pull_request, update, old_rfd, new_rfd)
                .await?;
        }

        Ok(())
    }
}

#[async_trait]
pub trait PostRFDUpdateHook {
    async fn run(
        &self,
        api_context: &Context,
        github: &octorust::Client,
        pull_request: Option<&GitHubPullRequest>,
        update: &GitHubRFDUpdate,
        old_rfd: Option<&RFD>,
        new_rfd: &mut RFD,
    ) -> Result<()>;
}

pub struct UpdateSearch;

#[async_trait]
impl PostRFDUpdateHook for UpdateSearch {
    async fn run(
        &self,
        _api_context: &Context,
        _github: &octorust::Client,
        _pull_request: Option<&GitHubPullRequest>,
        update: &GitHubRFDUpdate,
        _old_rfd: Option<&RFD>,
        new_rfd: &mut RFD,
    ) -> Result<()> {
        RFDSearchIndex::index_rfd(&new_rfd.number.into()).await?;
        info!("Triggered update of the search index for RFD {}", update.number);

        Ok(())
    }
}

pub struct UpdatePDFs;

#[async_trait]
impl PostRFDUpdateHook for UpdatePDFs {
    async fn run(
        &self,
        api_context: &Context,
        _github: &octorust::Client,
        _pull_request: Option<&GitHubPullRequest>,
        update: &GitHubRFDUpdate,
        _old_rfd: Option<&RFD>,
        new_rfd: &mut RFD,
    ) -> Result<()> {
        // Generate the PDFs for the RFD and upload them
        let upload = new_rfd
            .content()?
            .to_pdf(&new_rfd.title, &update.number, &update.branch)
            .await?
            .upload(&api_context.db, &api_context.company)
            .await?;

        // Store the PDF urls as needed to the RFD record
        if let Some(github_url) = upload.github_url {
            new_rfd.pdf_link_github = github_url;
        }

        if let Some(google_drive_url) = upload.google_drive_url {
            new_rfd.pdf_link_google_drive = google_drive_url;
        }

        Ok(())
    }
}

pub struct GenerateShortUrls;

#[async_trait]
impl PostRFDUpdateHook for GenerateShortUrls {
    async fn run(
        &self,
        api_context: &Context,
        _github: &octorust::Client,
        _pull_request: Option<&GitHubPullRequest>,
        _update: &GitHubRFDUpdate,
        _old_rfd: Option<&RFD>,
        _new_rfd: &mut RFD,
    ) -> Result<()> {
        // Create all the shorturls for the RFD if we need to, this would be on added files, only.
        generate_shorturls_for_rfds(
            &api_context.db,
            &api_context.company.authenticate_github()?,
            &api_context.company,
            &api_context.company.authenticate_cloudflare()?,
            "configs",
        )
        .await?;

        info!("[SUCCESS]: updated shorturls for the rfds");

        Ok(())
    }
}

pub struct CreatePullRequest;

#[async_trait]
impl PostRFDUpdateHook for CreatePullRequest {
    async fn run(
        &self,
        api_context: &Context,
        github: &octorust::Client,
        pull_request: Option<&GitHubPullRequest>,
        update: &GitHubRFDUpdate,
        old_rfd: Option<&RFD>,
        new_rfd: &mut RFD,
    ) -> Result<()> {
        // We only ever create pull requests if the RFD is in the discussion state, and we are not
        // handling an update on the default branch
        if update.branch.branch != update.branch.default_branch
            && new_rfd.state == "discussion"
            && pull_request.is_none()
        {
            let pull = github
                .pulls()
                .create(
                    &update.branch.owner,
                    &update.branch.repo,
                    &octorust::types::PullsCreateRequest {
                        title: new_rfd.name.to_string(),
                        head: format!("{}:{}", api_context.company.github_org, update.branch.branch),
                        base: update.branch.default_branch.to_string(),
                        body: "Automatically opening the pull request since the document \
                            is marked as being in discussion. If you wish to not have \
                            a pull request open, change the state of your document and \
                            close this pull request."
                            .to_string(),
                        draft: Some(false),
                        maintainer_can_modify: Some(true),
                        issue: 0,
                    },
                )
                .await?;

            info!(
                "[SUCCESS]: RFD {} has moved from state {:?} -> {}, on branch {}, opened pull request {}",
                new_rfd.number_string,
                old_rfd.map(|rfd| &rfd.state),
                new_rfd.state,
                update.branch.branch,
                pull.number,
            );
        }

        Ok(())
    }
}

pub struct UpdatePullRequest;

#[async_trait]
impl PostRFDUpdateHook for UpdatePullRequest {
    async fn run(
        &self,
        _api_context: &Context,
        github: &octorust::Client,
        pull_request: Option<&GitHubPullRequest>,
        update: &GitHubRFDUpdate,
        _old_rfd: Option<&RFD>,
        new_rfd: &mut RFD,
    ) -> Result<()> {
        if let Some(pull_request) = pull_request {
            // Let's make sure the title of the pull request is what it should be.
            // The pull request title should be equal to the name of the pull request.
            if new_rfd.name != pull_request.title {
                // TODO: Is this call necessary?
                // Get the current set of settings for the pull request.
                // We do this because we want to keep the current state for body.
                let pull_content = github
                    .pulls()
                    .get(&update.branch.owner, &update.branch.repo, pull_request.number)
                    .await?;

                github
                    .pulls()
                    .update(
                        &update.branch.owner,
                        &update.branch.repo,
                        pull_request.number,
                        &octorust::types::PullsUpdateRequest {
                            title: new_rfd.name.to_string(),
                            body: pull_content.body,
                            base: "".to_string(),
                            maintainer_can_modify: None,
                            state: None,
                        },
                    )
                    .await
                    .map_err(|err| {
                        anyhow!(
                            "unable to update title of pull request from `{}` to `{}` for pr#{}: {}",
                            pull_request.title,
                            new_rfd.name,
                            pull_request.number,
                            err,
                        )
                    })?;
            }

            // Update the labels for the pull request.
            let mut labels: Vec<String> = Default::default();

            if new_rfd.state == "discussion" {
                labels.push(":thought_balloon: discussion".to_string());
            } else if new_rfd.state == "ideation" {
                labels.push(":hatching_chick: ideation".to_string());
            }

            github
                .issues()
                .add_labels(
                    &update.branch.owner,
                    &update.branch.repo,
                    pull_request.number,
                    &octorust::types::IssuesAddLabelsRequestOneOf::StringVector(labels),
                )
                .await?;
        }

        Ok(())
    }
}

pub struct UpdateDiscussionUrl;

#[async_trait]
impl PostRFDUpdateHook for UpdateDiscussionUrl {
    async fn run(
        &self,
        _api_context: &Context,
        github: &octorust::Client,
        pull_request: Option<&GitHubPullRequest>,
        update: &GitHubRFDUpdate,
        _old_rfd: Option<&RFD>,
        new_rfd: &mut RFD,
    ) -> Result<()> {
        if let Some(pull_request) = pull_request {
            // If the stored discussion link does not match the PR we found, then and
            // update is required
            if new_rfd.discussion != pull_request.html_url && !pull_request.html_url.is_empty() {
                info!(
                    "Stored discussion link \"{}\" does not match the PR found \"{}\"",
                    new_rfd.discussion, pull_request.html_url
                );

                new_rfd.update_discussion(&pull_request.html_url)?;

                // Update the file in GitHub. This will trigger another commit webhook
                // and therefore must only occur when there is a change that needawaits to
                // be made. If this is handled unconditionally then commit hooks could
                // loop indefinitely.
                create_or_update_file_in_github_repo(
                    &github,
                    &update.branch.owner,
                    &update.branch.repo,
                    &update.branch.branch,
                    &update.file,
                    new_rfd.content.as_bytes().to_vec(),
                )
                .await?;

                info!("[SUCCESS]: updated RFD file in GitHub with discussion link changes");
            }
        }

        Ok(())
    }
}

pub struct EnsureRFDOnDefaultIsInPublishedState;

#[async_trait]
impl PostRFDUpdateHook for EnsureRFDOnDefaultIsInPublishedState {
    async fn run(
        &self,
        _api_context: &Context,
        github: &octorust::Client,
        _pull_request: Option<&GitHubPullRequest>,
        update: &GitHubRFDUpdate,
        _old_rfd: Option<&RFD>,
        new_rfd: &mut RFD,
    ) -> Result<()> {
        // If the RFD was merged into the default branch, but the RFD state is not `published`,
        // update the state of the RFD in GitHub to show it as `published`.
        if update.branch.branch == update.branch.default_branch && new_rfd.state != "published" {
            //  Update the state of the RFD in GitHub to show it as `published`.
            new_rfd.update_state("published")?;

            // Update the file in GitHub.
            // Keep in mind: this push will kick off another webhook.
            create_or_update_file_in_github_repo(
                &github,
                &update.branch.owner,
                &update.branch.repo,
                &update.branch.branch,
                &update.file,
                new_rfd.content.as_bytes().to_vec(),
            )
            .await?;

            info!(
                "[SUCCESS]: updated state to `published` for RFD {}, since it was merged into branch {}",
                new_rfd.number_string, update.branch.default_branch
            );
        }

        Ok(())
    }
}

pub struct DeleteOldPDFs;

#[async_trait]
impl PostRFDUpdateHook for DeleteOldPDFs {
    async fn run(
        &self,
        api_context: &Context,
        github: &octorust::Client,
        _pull_request: Option<&GitHubPullRequest>,
        update: &GitHubRFDUpdate,
        old_rfd: Option<&RFD>,
        new_rfd: &mut RFD,
    ) -> Result<()> {
        let old_pdf_filename = old_rfd.map(|rfd| rfd.get_pdf_filename());

        // If the PDF filename has changed (likely due to a title change for an RFD), then ensure
        // that the old PDF files are deleted
        if let Some(old_pdf_filename) = old_pdf_filename {
            let new_pdf_filename = new_rfd.get_pdf_filename();

            if old_pdf_filename != new_pdf_filename {
                if Features::is_enabled("RFD_PDFS_IN_GITHUB") {
                    let pdf_path = format!("/pdfs/{}", old_pdf_filename);

                    // First get the sha of the old pdf.
                    let (_, old_pdf_sha) = get_file_content_from_repo(
                        &github,
                        &update.branch.owner,
                        &update.branch.repo,
                        &update.branch.default_branch,
                        &pdf_path,
                    )
                    .await?;

                    if !old_pdf_sha.is_empty() {
                        // Delete the old filename from GitHub.
                        github
                            .repos()
                            .delete_file(
                                &update.branch.owner,
                                &update.branch.repo,
                                pdf_path.trim_start_matches('/'),
                                &octorust::types::ReposDeleteFileRequest {
                                    message: format!(
                                        "Deleting file content {} programatically\n\nThis is done \
                                        from the cio repo webhooky::listen_github_webhooks function.",
                                        pdf_path
                                    ),
                                    sha: old_pdf_sha,
                                    committer: None,
                                    author: None,
                                    branch: update.branch.default_branch.to_string(),
                                },
                            )
                            .await?;

                        info!(
                            "[SUCCESS]: deleted old pdf file in GitHub {} since the new name is {}",
                            old_pdf_filename, new_pdf_filename
                        );
                    }
                }

                if Features::is_enabled("RFD_PDFS_IN_GOOGLE_DRIVE") {
                    // Initialize the Google Drive client.
                    let drive = api_context.company.authenticate_google_drive(&api_context.db).await?;

                    // Figure out where our directory is.
                    // It should be in the shared drive : "Automated Documents"/"rfds"
                    let shared_drive = drive.drives().get_by_name("Automated Documents").await?;

                    // Get the directory by the name.
                    let parent_id = drive.files().create_folder(&shared_drive.id, "", "rfds").await?;

                    // Delete the old filename from drive.
                    drive
                        .files()
                        .delete_by_name(&shared_drive.id, &parent_id, &old_pdf_filename)
                        .await?;

                    info!(
                        "[SUCCESS]: deleted old pdf file in Google Drive {} since the new name is {}",
                        old_pdf_filename, new_pdf_filename
                    );
                }
            }
        }

        Ok(())
    }
}
