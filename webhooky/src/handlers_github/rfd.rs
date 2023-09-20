use anyhow::{anyhow, Result};
use async_trait::async_trait;
use cio_api::{
    core::GitHubPullRequest,
    features::Features,
    rfd::{GitHubRFDReadmeLocation, GitHubRFDUpdate, NewRFD, RFDOutputError, RFDSearchIndex, RemoteRFD, RFD},
    shorturls::generate_shorturls_for_rfds,
    utils::{create_or_update_file_in_github_repo, decode_base64, get_file_content_from_repo},
};
use google_drive::traits::{DriveOps, FileOps};
use google_storage1::{
    api::{Object, Storage},
    hyper, hyper_rustls,
};
use log::{info, warn};
use std::cmp::Ordering;

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
            Box::new(CopyImagesToGCP),
            Box::new(UpdateSearch),
            Box::new(UpdatePDFs),
            Box::new(GenerateShortUrls),
            Box::new(CreatePullRequest),
            Box::new(UpdatePullRequest),
            Box::new(UpdateDiscussionUrl),                    // Stops on error
            Box::new(EnsureRFDWithPullRequestIsInValidState), // Stops on error
            Box::new(EnsureRFDOnDefaultIsInValidState),       // Stops on error
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

        let mut ctx = RFDUpdateActionContext {
            api_context,
            github,
            pull_requests,
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
    pub pull_requests: Vec<GitHubPullRequest>,
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

        let images = update.branch.get_images(&update.number).await.map_err(into_continue)?;

        let gcp_auth = api_context.company.authenticate_gcp().await.map_err(into_continue)?;

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
            let sub_path = image
                .path
                .replace(&format!("rfd/{}/", update.number.as_number_string()), "");
            let object_name = format!("rfd/{}/latest/{}", update.number, sub_path);
            let mime_type = mime_guess::from_path(&object_name).first_or_octet_stream();
            let data = decode_base64(&image.content);

            log::info!(
                "Writing {} ({}) with size {} to GCP",
                object_name,
                mime_type,
                data.len()
            );

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
        let client = RFDSearchIndex::default_client().map_err(into_continue)?;
        RFDSearchIndex::index_rfd(&client, "rfd".to_string(), &rfd.number.into(), &rfd.content)
            .await
            .map_err(into_continue)?;
        info!("Updated search index with RFD {}", update.number);

        Ok(RFDUpdateActionResponse::default())
    }
}

pub struct UpdatePDFs;

impl UpdatePDFs {
    async fn upload(api_context: &Context, update: &GitHubRFDUpdate, rfd: &mut RFD) -> Result<()> {
        // Generate the PDFs for the RFD
        let pdf = match rfd.content()?.to_pdf(&rfd.title, &update.number, &update.branch).await {
            Ok(pdf) => pdf,
            Err(err) => {
                match &err {
                    RFDOutputError::FormatNotSupported(_) => {
                        log::info!("RFD {} is not in a format that supports PDF output", rfd.number);

                        // If an RFD does not support PDF output than we do not want to report an
                        // error. We return early instead
                        return Ok(());
                    }
                    RFDOutputError::Generic(inner) => {
                        log::error!("Failed trying to generate PDF for RFD {}: {:?}", rfd.number, inner);
                        return Err(err.into());
                    }
                }
            }
        };

        // Upload the generate PDF
        let upload = pdf.upload(&api_context.db, &api_context.company).await?;

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
                    let shared_drive = drive.drives().get_by_name("Automated Documents").await?.body;

                    // Get the directory by the name.
                    let parent_id = drive.files().create_folder(&shared_drive.id, "", "rfds").await?.body.id;

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

        Self::upload(api_context, update, rfd).await.map_err(into_continue)?;
        Self::delete_old(api_context, github, update, old_rfd, rfd)
            .await
            .map_err(into_continue)?;

        Ok(RFDUpdateActionResponse::default())
    }
}

pub struct GenerateShortUrls;

impl GenerateShortUrls {
    pub async fn generate(api_context: &Context, github: &octorust::Client) -> Result<()> {
        let out_repos = vec![api_context.company.shorturl_repo()];

        // Create all the shorturls for the RFD if we need to, this would be on added files, only.
        generate_shorturls_for_rfds(
            &api_context.db,
            github,
            &api_context.company,
            &api_context.company.authenticate_dns_providers().await?,
            &out_repos,
        )
        .await?;

        info!("[SUCCESS]: updated shorturls for the rfds");

        Ok(())
    }
}

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

        Self::generate(api_context, github)
            .await
            .map(|_| RFDUpdateActionResponse::default())
            .map_err(into_continue)
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
            pull_requests,
            api_context,
            old_rfd,
            ..
        } = ctx;

        // We only ever create pull requests if the RFD is in the discussion state, we are not
        // handling an update on the default branch, and there are no previous pull requests for
        // for this branch. This includes Closed pull requests, therefore this action will not
        // re-open or create a new pull request for a branch that previously had an open PR
        if update.branch.branch != update.branch.default_branch && rfd.state == "discussion" && pull_requests.is_empty()
        {
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
                .map_err(into_continue)?
                .body;

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
            pull_requests,
            github,
            ..
        } = ctx;

        // We only want to operate on open pull requests
        let open_prs = pull_requests
            .iter()
            .filter(|pr| pr.state == "open")
            .collect::<Vec<&GitHubPullRequest>>();

        // Explicitly we will only update a pull request if it is the only open pull request for the
        // branch that we are working on
        match open_prs.len().cmp(&1) {
            Ordering::Equal => {
                if let Some(pull_request) = open_prs.get(0) {
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
                            .map_err(into_continue)?
                            .body;

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

                    if rfd.state == "discussion"
                        && !pull_request
                            .labels
                            .iter()
                            .any(|label| label.name.ends_with("discussion"))
                    {
                        labels.push(":thought_balloon: discussion".to_string());
                    } else if rfd.state == "ideation"
                        && !pull_request.labels.iter().any(|label| label.name.ends_with("ideation"))
                    {
                        labels.push(":hatching_chick: ideation".to_string());
                    }

                    // Only add a label if there is label missing.
                    if !labels.is_empty() {
                        github
                            .issues()
                            .add_labels(
                                &update.branch.owner,
                                &update.branch.repo,
                                pull_request.number,
                                &octorust::types::IssuesAddLabelsRequestOneOf::StringVector(labels),
                            )
                            .await
                            .map_err(into_continue)?;
                    }
                }
            }
            Ordering::Greater => {
                info!(
                    "Found multiple pull requests for RFD {}. Unable to update title",
                    rfd.number
                );
            }
            Ordering::Less => {
                // Nothing to do, there are no PRs
            }
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
        let RFDUpdateActionContext { pull_requests, .. } = ctx;

        let mut requires_source_commit = false;

        // We only want to operate on open pull requests
        let open_prs = pull_requests
            .iter()
            .filter(|pr| pr.state == "open")
            .collect::<Vec<&GitHubPullRequest>>();

        // Explicitly we will only update a pull request if it is the only open pull request for the
        // branch that we are working on
        match open_prs.len().cmp(&1) {
            Ordering::Equal => {
                if let Some(pull_request) = open_prs.get(0) {
                    // If the stored discussion link does not match the PR we found, then and
                    // update is required
                    if rfd.discussion != pull_request.html_url && !pull_request.html_url.is_empty() {
                        info!(
                            "Stored discussion link \"{}\" does not match the PR found \"{}\"",
                            rfd.discussion, pull_request.html_url
                        );

                        rfd.update_discussion(&pull_request.html_url).map_err(into_continue)?;

                        info!("[SUCCESS]: updated RFD file in GitHub with discussion link changes");

                        requires_source_commit = true;
                    }
                }
            }
            Ordering::Greater => {
                info!(
                    "Found multiple pull requests for RFD {}. Unable to update discussion url",
                    rfd.number
                );
            }
            Ordering::Less => {
                // Nothing to do, there are no PRs
            }
        }

        Ok(RFDUpdateActionResponse { requires_source_commit })
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
        let RFDUpdateActionContext { pull_requests, .. } = ctx;

        let mut requires_source_commit = false;

        // We only want to operate on open pull requests
        let open_prs = pull_requests.iter().filter(|pr| pr.state == "open");

        // Explicitly we will only update a pull request if it is the only open pull request for the
        // branch that we are working on
        match open_prs.count().cmp(&1) {
            Ordering::Equal => {
                // If there is a pull request open for this branch, then check to ensure that it is in one
                // of three valid states:
                //   * published  - A RFD may be in this state if it had previously been published and an
                //                  an update is being made, Or the RFD may be in the process of being
                //                  published
                //   * committed  - A RFD may be in this state if it had previously been committed and an
                //                  an update is being made. Or the RFD may be in the process of being
                //                  committed
                //   * discussion - The default state for a RFD that has an open pull request and has yet to
                //                  to be merged. If the document on this branch is found to be in an
                //                  invalid state, it will be set back to the discussion state
                //   * ideation   - An alternative state to discussion where the RFD is not yet merged, but
                //                  may not be ready for discussion. A pull request is being used to share
                //                  initial thoughts on an idea
                //   * abandoned  - A RFD may be in this state if it had previously been abandoned or is in
                //                  the process of being abandoned
                if rfd.state != "discussion"
                    && rfd.state != "published"
                    && rfd.state != "committed"
                    && rfd.state != "ideation"
                    && rfd.state != "abandoned"
                {
                    rfd.update_state("discussion").map_err(RFDUpdateActionErr::Stop)?;
                    requires_source_commit = true;
                }
            }
            Ordering::Greater => {
                info!(
                    "Found multiple pull requests for RFD {}. Unable to update state to discussion",
                    rfd.number
                );
            }
            Ordering::Less => {
                // Nothing to do, there are no PRs
            }
        }

        Ok(RFDUpdateActionResponse { requires_source_commit })
    }
}

pub struct EnsureRFDOnDefaultIsInValidState;

#[async_trait]
impl RFDUpdateAction for EnsureRFDOnDefaultIsInValidState {
    async fn run(
        &self,
        ctx: &mut RFDUpdateActionContext,
        rfd: &mut RFD,
    ) -> Result<RFDUpdateActionResponse, RFDUpdateActionErr> {
        let RFDUpdateActionContext { update, .. } = ctx;

        // If an RFD exists on the default branch then it should be in either the published or
        // abandoned state
        if update.branch.branch == update.branch.default_branch
            && rfd.state != "published"
            && rfd.state != "committed"
            && rfd.state != "abandoned"
        {
            log::warn!("RFD {} on the default branch is in an invalid state. It needs to be updated to either publisehd or abandoned", rfd.number);
        }

        Ok(RFDUpdateActionResponse::default())
    }
}

fn into_continue(err: impl Into<anyhow::Error>) -> RFDUpdateActionErr {
    RFDUpdateActionErr::Continue(err.into())
}

pub struct ParseRFDLabels;

#[async_trait]
impl RFDUpdateAction for ParseRFDLabels {
    async fn run(
        &self,
        _ctx: &mut RFDUpdateActionContext,
        rfd: &mut RFD,
    ) -> Result<RFDUpdateActionResponse, RFDUpdateActionErr> {
        rfd.labels = rfd.content().map_err(into_continue)?.get_labels();
        Ok(RFDUpdateActionResponse::default())
    }
}
