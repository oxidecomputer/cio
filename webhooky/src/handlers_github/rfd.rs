use anyhow::{anyhow, Result};
use async_trait::async_trait;
use cio_api::{
    core::GitHubPullRequest,
    features::Features,
    rfd::{GitHubRFDReadmeLocation, GitHubRFDUpdate, NewRFD, RFDSearchIndex, RemoteRFD, RFD},
    shorturls::generate_shorturls_for_rfds,
    utils::{create_or_update_file_in_github_repo, decode_base64, get_file_content_from_repo},
};
use google_drive::traits::{DriveOps, FileOps};
use google_storage1::{
    api::{Object, Storage},
    hyper, hyper_rustls,
};
use log::{info, warn};

use crate::context::Context;

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

pub struct RFDUpdater {
    actions: Vec<Box<dyn RFDUpdateAction + Send + Sync>>,
}

impl Default for RFDUpdater {
    fn default() -> Self {
        Self::new(vec![
            Box::new(CopyImagesToFrontend),
            Box::new(CopyImagesToGCP),
            Box::new(UpdateSearch),
            Box::new(UpdatePDFs),
            Box::new(GenerateShortUrls),
            Box::new(CreatePullRequest),
            Box::new(UpdatePullRequest),
            Box::new(UpdateDiscussionUrl),                    // Stops on error
            Box::new(EnsureRFDWithPullRequestIsInValidState), // Stops on error
            Box::new(EnsureRFDOnDefaultIsInPublishedState),   // Stops on error
        ])
    }
}

impl RFDUpdater {
    pub fn new(actions: Vec<Box<dyn RFDUpdateAction + Send + Sync>>) -> Self {
        Self { actions }
    }

    /// Handle a `push` event for the rfd repo.
    pub async fn handle(&self, api_context: &Context, updates: &[GitHubRFDUpdate]) -> Result<()> {
        // Loop through the updates that were provided and process them individually. We also throw
        // out any updates that attempt to update a mismatched RFD
        for update in updates {
            // Skip any updates that fail validation
            if update.is_valid() {
                // If this branch does not actually exist in GitHub, then we drop the update
                if update.branch.exists_in_remote().await {
                    if let Err(err) = self.run_update(api_context, update).await {
                        warn!(
                            "Failed to run update for RFD {} on the {} branch to completion. Ended with the error: {:?}",
                            update.number, update.branch.branch, err
                        );
                    }
                } else {
                    info!(
                        "Dropping RFD {} update as the remote branch {} has gone missing",
                        update.number, update.branch.branch
                    );
                }
            } else {
                warn!("Encountered invalid RFD update (it will not be run) {:?}", update);
            }
        }

        Ok(())
    }

    async fn run_update(&self, api_context: &Context, update: &GitHubRFDUpdate) -> Result<()> {
        // We have a README file that changed, let's parse the RFD and update it
        // in our database.
        info!("Updating RFD {} on the {} branch", update.number, update.branch.branch);

        // Fetch the latest RFD information from GitHub
        let RemoteRFD { rfd: new_rfd, location } = NewRFD::new_from_update(&api_context.company, update).await?;

        info!(
            "Generated RFD {} from branch {} on GitHub",
            update.number, update.branch.branch
        );

        // Get the old RFD from the database.
        // DO THIS BEFORE UPDATING THE RFD.
        // We will need this later to check if the RFD's state changed.
        let old_rfd = RFD::get_from_db(&api_context.db, new_rfd.number).await;

        info!(
            "Checked for existing version of RFD {} in the database: {}",
            update.number,
            old_rfd.is_some()
        );

        // Update the RFD in the database.
        let mut rfd = new_rfd.upsert(&api_context.db).await?;

        info!("Upserted RFD {} in to the database", rfd.number);

        // The RFD has been stored internally, now trigger the update actions
        self.run_actions(api_context, update, &location, old_rfd.as_ref(), &mut rfd)
            .await?;

        // Perform a final update to capture and modifications made during update actions
        rfd.update(&api_context.db).await?;

        info!(
            "Update for RFD {} via the {} branch completed",
            rfd.number, update.branch.branch
        );

        Ok(())
    }

    async fn run_actions(
        &self,
        api_context: &Context,
        update: &GitHubRFDUpdate,
        location: &GitHubRFDReadmeLocation,
        old_rfd: Option<&RFD>,
        rfd: &mut RFD,
    ) -> Result<()> {
        let github = update.client();
        let pull_requests = update.branch.find_pull_requests().await?;

        // This is here to remain consistent with previous behavior. This likely needs to be
        // refactored to account for multiple pull requests existing (even though there *should*
        // never be multiple)
        let pull_request = pull_requests.into_iter().next();
        let mut ctx = RFDUpdateActionContext {
            api_context,
            github,
            pull_request,
            update,
            location,
            old_rfd,
        };

        let mut responses = vec![];

        for action in &self.actions {
            match action.run(&mut ctx, rfd).await {
                Ok(response) => responses.push(response),
                Err(err) => match err {
                    RFDUpdateActionErr::Continue(action_err) => {
                        warn!(
                            "Updating RFD {} on {} errored with non-fatal error {:?}",
                            update.number, update.branch.branch, action_err
                        );
                    }
                    RFDUpdateActionErr::Stop(action_err) => {
                        warn!(
                            "Updating RFD {} on {} errored with fatal error {:?}",
                            update.number, update.branch.branch, action_err
                        );

                        return Err(action_err);
                    }
                },
            }
        }

        let response: RFDUpdateActionResponse = responses.into();

        if response.requires_source_commit {
            // Update the file in GitHub.
            // Keep in mind: this push will kick off another webhook.
            create_or_update_file_in_github_repo(
                ctx.github,
                &ctx.update.branch.owner,
                &ctx.update.branch.repo,
                &ctx.update.branch.branch,
                &location.file,
                rfd.content.as_bytes().to_vec(),
            )
            .await?;
        }

        Ok(())
    }
}

pub struct RFDUpdateActionContext<'a, 'b, 'd, 'e, 'f> {
    pub api_context: &'a Context,
    pub github: &'b octorust::Client,
    pub pull_request: Option<GitHubPullRequest>,
    pub update: &'d GitHubRFDUpdate,
    pub location: &'e GitHubRFDReadmeLocation,
    pub old_rfd: Option<&'f RFD>,
}

#[async_trait]
pub trait RFDUpdateAction {
    async fn run(
        &self,
        ctx: &mut RFDUpdateActionContext,
        rfd: &mut RFD,
    ) -> Result<RFDUpdateActionResponse, RFDUpdateActionErr>;
}

#[derive(Default)]
pub struct RFDUpdateActionResponse {
    pub requires_source_commit: bool,
}

impl From<Vec<RFDUpdateActionResponse>> for RFDUpdateActionResponse {
    fn from(responses: Vec<RFDUpdateActionResponse>) -> Self {
        responses
            .iter()
            .fold(RFDUpdateActionResponse::default(), |acc, response| {
                RFDUpdateActionResponse {
                    requires_source_commit: acc.requires_source_commit || response.requires_source_commit,
                }
            })
    }
}

#[derive(Debug)]
pub enum RFDUpdateActionErr {
    Continue(anyhow::Error),
    Stop(anyhow::Error),
}

pub struct CopyImagesToFrontend;

#[async_trait]
impl RFDUpdateAction for CopyImagesToFrontend {
    async fn run(
        &self,
        ctx: &mut RFDUpdateActionContext,
        _rfd: &mut RFD,
    ) -> Result<RFDUpdateActionResponse, RFDUpdateActionErr> {
        let RFDUpdateActionContext { update, .. } = ctx;
        update
            .branch
            .copy_images_to_frontend(&update.number)
            .await
            .map_err(RFDUpdateActionErr::Continue)?;

        info!(
            "Copied images for RFD {} on {} to frontend storage",
            update.number, update.branch.branch
        );

        Ok(RFDUpdateActionResponse::default())
    }
}

pub struct CopyImagesToGCP;

#[async_trait]
impl RFDUpdateAction for CopyImagesToGCP {
    async fn run(
        &self,
        ctx: &mut RFDUpdateActionContext,
        _rfd: &mut RFD,
    ) -> Result<RFDUpdateActionResponse, RFDUpdateActionErr> {
        let RFDUpdateActionContext {
            api_context, update, ..
        } = ctx;

        let images = update
            .branch
            .get_images(&update.number)
            .await
            .map_err(RFDUpdateActionErr::Continue)?;

        let gcp_auth = api_context
            .company
            .authenticate_gcp()
            .await
            .map_err(RFDUpdateActionErr::Continue)?;

        let hub = Storage::new(
            hyper::Client::builder().build(
                hyper_rustls::HttpsConnectorBuilder::new()
                    .with_native_roots()
                    .https_or_http()
                    .enable_http1()
                    .enable_http2()
                    .build(),
            ),
            gcp_auth,
        );

        for image in images {
            let object_name = format!("rfd/{}/latest/{}", update.number, image.name);
            let mime_type = mime_guess::guess_mime_type(&object_name);
            let data = decode_base64(&image.content);
            let cursor = std::io::Cursor::new(data);

            let request = Object::default();
            hub.objects()
                .insert(request, &api_context.company.rfd_static_storage())
                .name(&object_name)
                .upload(cursor, mime_type)
                .await
                .map_err(|err| RFDUpdateActionErr::Continue(err.into()))?;
        }

        Ok(RFDUpdateActionResponse::default())
    }
}

pub struct UpdateSearch;

#[async_trait]
impl RFDUpdateAction for UpdateSearch {
    async fn run(
        &self,
        ctx: &mut RFDUpdateActionContext,
        rfd: &mut RFD,
    ) -> Result<RFDUpdateActionResponse, RFDUpdateActionErr> {
        let RFDUpdateActionContext { update, .. } = ctx;
        RFDSearchIndex::index_rfd(&rfd.number.into())
            .await
            .map_err(RFDUpdateActionErr::Continue)?;
        info!("Triggered update of the search index for RFD {}", update.number);

        Ok(RFDUpdateActionResponse::default())
    }
}

pub struct UpdatePDFs;

impl UpdatePDFs {
    async fn upload(api_context: &Context, update: &GitHubRFDUpdate, rfd: &mut RFD) -> Result<()> {
        // Generate the PDFs for the RFD and upload them
        let upload = rfd
            .content()?
            .to_pdf(&rfd.title, &update.number, &update.branch)
            .await?
            .upload(&api_context.db, &api_context.company)
            .await?;

        // Store the PDF urls as needed to the RFD record
        if let Some(github_url) = upload.github_url {
            rfd.pdf_link_github.replace_range(.., &github_url);
        }

        if let Some(google_drive_url) = upload.google_drive_url {
            rfd.pdf_link_google_drive.replace_range(.., &google_drive_url);
        }

        Ok(())
    }

    async fn delete_old(
        api_context: &Context,
        github: &octorust::Client,
        update: &GitHubRFDUpdate,
        old_rfd: &Option<&RFD>,
        rfd: &mut RFD,
    ) -> Result<()> {
        let old_pdf_filename = old_rfd.map(|rfd| rfd.get_pdf_filename());

        // If the PDF filename has changed (likely due to a title change for an RFD), then ensure
        // that the old PDF files are deleted
        if let Some(old_pdf_filename) = old_pdf_filename {
            let new_pdf_filename = rfd.get_pdf_filename();

            if old_pdf_filename != new_pdf_filename {
                if Features::is_enabled("RFD_PDFS_IN_GITHUB") {
                    let pdf_path = format!("/pdfs/{}", old_pdf_filename);

                    // First get the sha of the old pdf.
                    let (_, old_pdf_sha) = get_file_content_from_repo(
                        github,
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

#[async_trait]
impl RFDUpdateAction for UpdatePDFs {
    async fn run(
        &self,
        ctx: &mut RFDUpdateActionContext,
        rfd: &mut RFD,
    ) -> Result<RFDUpdateActionResponse, RFDUpdateActionErr> {
        let RFDUpdateActionContext {
            api_context,
            github,
            old_rfd,
            update,
            ..
        } = ctx;

        Self::upload(api_context, update, rfd)
            .await
            .map_err(RFDUpdateActionErr::Continue)?;
        Self::delete_old(api_context, github, update, old_rfd, rfd)
            .await
            .map_err(RFDUpdateActionErr::Continue)?;

        Ok(RFDUpdateActionResponse::default())
    }
}

pub struct GenerateShortUrls;

#[async_trait]
impl RFDUpdateAction for GenerateShortUrls {
    async fn run(
        &self,
        ctx: &mut RFDUpdateActionContext,
        _rfd: &mut RFD,
    ) -> Result<RFDUpdateActionResponse, RFDUpdateActionErr> {
        let RFDUpdateActionContext {
            api_context, github, ..
        } = ctx;

        // Create all the shorturls for the RFD if we need to, this would be on added files, only.
        generate_shorturls_for_rfds(
            &api_context.db,
            github,
            &api_context.company,
            &api_context
                .company
                .authenticate_cloudflare()
                .map_err(RFDUpdateActionErr::Continue)?,
            "configs",
        )
        .await
        .map_err(RFDUpdateActionErr::Continue)?;

        info!("[SUCCESS]: updated shorturls for the rfds");

        Ok(RFDUpdateActionResponse::default())
    }
}

pub struct CreatePullRequest;

#[async_trait]
impl RFDUpdateAction for CreatePullRequest {
    async fn run(
        &self,
        ctx: &mut RFDUpdateActionContext,
        rfd: &mut RFD,
    ) -> Result<RFDUpdateActionResponse, RFDUpdateActionErr> {
        let RFDUpdateActionContext {
            update,
            github,
            pull_request,
            api_context,
            old_rfd,
            ..
        } = ctx;

        // We only ever create pull requests if the RFD is in the discussion state, and we are not
        // handling an update on the default branch
        if update.branch.branch != update.branch.default_branch && rfd.state == "discussion" && pull_request.is_none() {
            let pull = github
                .pulls()
                .create(
                    &update.branch.owner,
                    &update.branch.repo,
                    &octorust::types::PullsCreateRequest {
                        title: rfd.name.to_string(),
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
                .await
                .map_err(RFDUpdateActionErr::Continue)?;

            info!(
                "[SUCCESS]: RFD {} has moved from state {:?} -> {}, on branch {}, opened pull request {}",
                rfd.number_string,
                old_rfd.map(|rfd| &rfd.state),
                rfd.state,
                update.branch.branch,
                pull.number,
            );
        }

        Ok(RFDUpdateActionResponse::default())
    }
}

pub struct UpdatePullRequest;

#[async_trait]
impl RFDUpdateAction for UpdatePullRequest {
    async fn run(
        &self,
        ctx: &mut RFDUpdateActionContext,
        rfd: &mut RFD,
    ) -> Result<RFDUpdateActionResponse, RFDUpdateActionErr> {
        let RFDUpdateActionContext {
            update,
            pull_request,
            github,
            ..
        } = ctx;

        if let Some(pull_request) = pull_request {
            // Let's make sure the title of the pull request is what it should be.
            // The pull request title should be equal to the name of the pull request.
            if rfd.name != pull_request.title {
                // TODO: Is this call necessary?
                // Get the current set of settings for the pull request.
                // We do this because we want to keep the current state for body.
                let pull_content = github
                    .pulls()
                    .get(&update.branch.owner, &update.branch.repo, pull_request.number)
                    .await
                    .map_err(RFDUpdateActionErr::Continue)?;

                github
                    .pulls()
                    .update(
                        &update.branch.owner,
                        &update.branch.repo,
                        pull_request.number,
                        &octorust::types::PullsUpdateRequest {
                            title: rfd.name.to_string(),
                            body: pull_content.body,
                            base: "".to_string(),
                            maintainer_can_modify: None,
                            state: None,
                        },
                    )
                    .await
                    .map_err(|err| {
                        RFDUpdateActionErr::Continue(anyhow!(
                            "unable to update title of pull request from `{}` to `{}` for pr#{}: {}",
                            pull_request.title,
                            rfd.name,
                            pull_request.number,
                            err,
                        ))
                    })?;
            }

            // Update the labels for the pull request.
            let mut labels: Vec<String> = Default::default();

            if rfd.state == "discussion" {
                labels.push(":thought_balloon: discussion".to_string());
            } else if rfd.state == "ideation" {
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
                .await
                .map_err(RFDUpdateActionErr::Continue)?;
        }

        Ok(RFDUpdateActionResponse::default())
    }
}

pub struct UpdateDiscussionUrl;

#[async_trait]
impl RFDUpdateAction for UpdateDiscussionUrl {
    async fn run(
        &self,
        ctx: &mut RFDUpdateActionContext,
        rfd: &mut RFD,
    ) -> Result<RFDUpdateActionResponse, RFDUpdateActionErr> {
        let RFDUpdateActionContext { pull_request, .. } = ctx;

        if let Some(pull_request) = pull_request {
            // If the stored discussion link does not match the PR we found, then and
            // update is required
            if rfd.discussion != pull_request.html_url && !pull_request.html_url.is_empty() {
                info!(
                    "Stored discussion link \"{}\" does not match the PR found \"{}\"",
                    rfd.discussion, pull_request.html_url
                );

                rfd.update_discussion(&pull_request.html_url)
                    .map_err(RFDUpdateActionErr::Continue)?;

                info!("[SUCCESS]: updated RFD file in GitHub with discussion link changes");
            }
        }

        Ok(RFDUpdateActionResponse {
            requires_source_commit: true,
        })
    }
}

pub struct EnsureRFDWithPullRequestIsInValidState;

#[async_trait]
impl RFDUpdateAction for EnsureRFDWithPullRequestIsInValidState {
    async fn run(
        &self,
        ctx: &mut RFDUpdateActionContext,
        rfd: &mut RFD,
    ) -> Result<RFDUpdateActionResponse, RFDUpdateActionErr> {
        let RFDUpdateActionContext { pull_request, .. } = ctx;

        // If there is a pull request open for this branch, then check to ensure that it is in one
        // of three valid states:
        //   * published  - A RFD may be in this state if it had previously been published and an
        //                  an update is being made
        //   * discussion - The default state for a RFD that has an open pull request and has yet to
        //                  to be merged. If the document on this branch is found to be in an
        //                  invalid state, it will be set back to the discussion state
        //   * ideation   - An alternative state to discussion where the RFD is not yet merged, but
        //                  may not be ready for discussion. A pull request is being used to share
        //                  initial thoughts on an idea
        if pull_request.is_some() && rfd.state != "discussion" && rfd.state != "published" && rfd.state != "ideation" {
            rfd.update_state("discussion").map_err(RFDUpdateActionErr::Stop)?;
        }

        Ok(RFDUpdateActionResponse {
            requires_source_commit: true,
        })
    }
}

pub struct EnsureRFDOnDefaultIsInPublishedState;

#[async_trait]
impl RFDUpdateAction for EnsureRFDOnDefaultIsInPublishedState {
    async fn run(
        &self,
        ctx: &mut RFDUpdateActionContext,
        rfd: &mut RFD,
    ) -> Result<RFDUpdateActionResponse, RFDUpdateActionErr> {
        let RFDUpdateActionContext { update, .. } = ctx;

        // If the RFD was merged into the default branch, but the RFD state is not `published`,
        // update the state of the RFD in GitHub to show it as `published`.
        if update.branch.branch == update.branch.default_branch && rfd.state != "published" {
            //  Update the state of the RFD in GitHub to show it as `published`.
            rfd.update_state("published").map_err(RFDUpdateActionErr::Stop)?;

            info!(
                "[SUCCESS]: updated state to `published` for RFD {}, since it was merged into branch {}",
                rfd.number_string, update.branch.default_branch
            );
        }

        Ok(RFDUpdateActionResponse {
            requires_source_commit: true,
        })
    }
}
