use anyhow::Result;
use async_trait::async_trait;
use chrono::offset::Utc;
use cio_api::{
    companies::Company,
    configs::{
        get_configs_from_repo, sync_buildings, sync_certificates, sync_github_outside_collaborators, sync_groups,
        sync_links, sync_resources, sync_users,
    },
    core::GitHubCommit,
    repos::NewRepo,
    rfd::{GitHubRFDBranch, GitHubRFDRepo, GitHubRFDUpdate},
    shorturls::{generate_shorturls_for_configs_links, generate_shorturls_for_repos},
    utils::{get_file_content_from_repo, is_image},
};
use dropshot::{Extractor, RequestContext, ServerContext};
use dropshot_verify_request::sig::HmacSignatureVerifier;
use hmac::Hmac;
use log::{error, info, warn};
use sha2::Sha256;
use std::{str::FromStr, sync::Arc};

use crate::{context::Context, event_types::EventType, github_types::GitHubWebhook, http::Headers, repos::Repo};

pub mod rfd;

pub use rfd::RFDUpdater;

#[derive(Debug)]
pub struct GitHubWebhookVerification;

#[async_trait]
impl HmacSignatureVerifier for GitHubWebhookVerification {
    type Algo = Hmac<Sha256>;

    async fn key<Context: ServerContext>(_: Arc<RequestContext<Context>>) -> Result<Vec<u8>> {
        Ok(std::env::var("GH_WH_KEY").map(|key| key.into_bytes()).map_err(|err| {
            warn!("Failed to find webhook key for verifying GitHub webhooks");
            err
        })?)
    }

    async fn signature<Context: ServerContext>(rqctx: Arc<RequestContext<Context>>) -> Result<Vec<u8>> {
        let headers = Headers::from_request(rqctx.clone()).await?;
        let signature = headers
            .0
            .get("X-Hub-Signature-256")
            .ok_or_else(|| anyhow::anyhow!("GitHub webhook is missing signature"))
            .and_then(|header_value| Ok(header_value.to_str()?))
            .and_then(|header| {
                log::debug!("Found GitHub signature header {}", header);
                Ok(hex::decode(header.trim_start_matches("sha256="))?)
            })
            .map_err(|err| {
                info!("GitHub webhook is missing a well-formed signature: {}", err);
                err
            })?;

        Ok(signature)
    }
}

/// Handle a request to the /github endpoint.
pub async fn handle_github(rqctx: Arc<RequestContext<Context>>, event: GitHubWebhook) -> Result<()> {
    let api_context = rqctx.context();

    // Parse the `X-GitHub-Event` header. Ensure the request lock is dropped once the
    // event_type has been extracted.
    // TODO: make this nicer when supported as a first class method in dropshot.
    let event_type_string = {
        let req = rqctx.request.lock().await;
        let req_headers = req.headers();
        req_headers
            .get("X-GitHub-Event")
            .unwrap_or(&http::header::HeaderValue::from_str("")?)
            .to_str()
            .unwrap()
            .to_string()
    };

    let event_type =
        EventType::from_str(&event_type_string).expect("Event type from GitHub does not match a known event type");

    info!(
        "Processing incoming {} webhook event on {}",
        event_type, event.repository.name
    );

    // Filter by event type any actions we can rule out for all repos.
    match event_type {
        EventType::Push => {
            // Ensure we have commits.
            if event.commits.is_empty() {
                // `push` event has no commits.
                // This happens on bot commits, tags etc.
                // Just throw it away.
                return Ok(());
            }

            let commit = event.commits.get(0).unwrap().clone();
            // We only care about distinct commits.
            if !commit.distinct {
                // The commit is not distinct.
                // This happens on merges sometimes, it's nothing to worry about.
                // We can throw this out, log it and return early.
                return Ok(());
            }

            // Get the branch name.
            let branch = event.refv.trim_start_matches("refs/heads/");
            // Make sure we have a branch.
            if branch.is_empty() {
                // The branch name is empty.
                // We can throw this out, log it and return early.
                // This should never happen, but we won't rule it out because computers.
                sentry::with_scope(
                    |scope| {
                        scope.set_context("github.webhook", sentry::protocol::Context::Other(event.clone().into()));
                        scope.set_tag("github.event.type", &event_type_string);
                    },
                    || {
                        warn!("`push` event branch name is empty");
                    },
                );
                return Ok(());
            }
        }
        EventType::Repository => {
            let company = Company::get_from_github_org(&api_context.db, &event.repository.owner.login).await?;
            let github = company.authenticate_github()?;

            sentry::configure_scope(|scope| {
                scope.set_context("github.webhook", sentry::protocol::Context::Other(event.clone().into()));
                scope.set_tag("github.event.type", &event_type_string);
            });

            let result = handle_repository_event(&github, api_context, event.clone(), &company).await;

            match result {
                Ok(message) => log::info!(
                    "Completed repo sync for new repo {}. Message: {}",
                    event.repository.name,
                    message
                ),
                Err(e) => {
                    log::warn!(
                        "Failed to handle repo sync for new repo {}. err: {:?}",
                        event.repository.name,
                        e
                    );
                    sentry::integrations::anyhow::capture_anyhow(&e);
                }
            }

            return Ok(());
        }
        _ => (),
    }

    // Run the correct handler function based on the event type and repo.
    if !event.repository.name.is_empty() {
        let repo = &event.repository;
        let repo_name = Repo::from_str(&repo.name).unwrap();

        let company = Company::get_from_github_org(&api_context.db, &repo.owner.login).await?;
        let github = Arc::new(company.authenticate_github()?);

        match repo_name {
            Repo::RFD => match event_type {
                EventType::Push => {
                    sentry::configure_scope(|scope| {
                        scope.set_context("github.webhook", sentry::protocol::Context::Other(event.clone().into()));
                        scope.set_tag("github.event.type", &event_type_string);
                    });

                    match handle_rfd_push(github.clone(), api_context, event.clone()).await {
                        Ok(_) => ( /* Silence */ ),
                        Err(e) => {
                            event
                                .create_comment(&github, &event.get_error_string("updating RFD on `push`", e))
                                .await?;
                        }
                    }
                }
                EventType::PullRequest => {
                    sentry::configure_scope(|scope| {
                        scope.set_context("github.webhook", sentry::protocol::Context::Other(event.clone().into()));
                        scope.set_tag("github.event.type", &event_type_string);
                    });
                    // Let's create the check run.
                    let check_run_id = event.create_check_run(&github).await?;

                    match handle_rfd_pull_request(api_context, event.clone(), &company).await {
                        Ok((conclusion, message)) => {
                            event
                                .update_check_run(&github, check_run_id, &message, conclusion)
                                .await?;
                        }
                        Err(e) => {
                            event
                                .update_check_run(
                                    &github,
                                    check_run_id,
                                    &event.get_error_string("updating RFD on `pull_request`", e),
                                    octorust::types::ChecksCreateRequestConclusion::Failure,
                                )
                                .await?;
                        }
                    }
                }
                EventType::CheckRun => {}
                EventType::CheckSuite => {}
                _ => (),
            },
            Repo::Configs => {
                if let EventType::Push = event_type {
                    sentry::configure_scope(|scope| {
                        scope.set_context("github.webhook", sentry::protocol::Context::Other(event.clone().into()));
                        scope.set_tag("github.event.type", &event_type_string);
                    });

                    match handle_configs_push(&github, api_context, event.clone(), &company).await {
                        Ok(message) => {
                            info!("{}", message);
                            event.create_comment(&github, &message).await?;
                        }
                        Err(e) => {
                            event
                                .create_comment(&github, &event.get_error_string("updating configs on `push`", e))
                                .await?;
                        }
                    }
                }
            }
            _ => {
                // We can throw this out, log it and return early.
                info!(
                    "`{}` event was to the {} repo, no automations are set up for this repo yet",
                    event_type, repo_name
                );
            }
        }
    }

    Ok(())
}

async fn handle_rfd_push(github: Arc<octorust::Client>, api_context: &Context, event: GitHubWebhook) -> Result<()> {
    info!("[rfd.push] Remaining stack size: {:?}", stacker::remaining_stack());

    // Perform validation checks first to determine if we need to process this call or if we can
    // drop it early
    if event.repository.name != "rfd" {
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
    let repo = GitHubRFDRepo::new_with_client(&api_context.company, github.clone()).await?;

    // Get the branch name.
    let branch_name = event.refv.trim_start_matches("refs/heads/");
    let branch = repo.branch(branch_name.to_string());

    // Iterate over the removed files and remove any images that we no longer need for the HTML
    // rendered RFD website. This is a special code path that only runs on RFD pushes as opposed to
    // the rest of the RFD update logic which can be centralized
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
                &github,
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
                info!(
                    "[SUCCESS]: deleted file `{}` since it was removed in this push",
                    website_file
                );
            }
        }
    }

    // We are always creating updates based on the branch that is defined by the event, independent
    // of it if corresponds with RFD number of the file(s) updated. We are only responsible for
    // generating updates, not determining if they make sense to process.
    let updates = get_rfd_updates(&branch, &commit);

    let handler = RFDUpdater::default();

    handler.handle(api_context, &updates).await
}

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
                updates.push(GitHubRFDUpdate {
                    number: number.into(),
                    branch: branch.clone(),
                });
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

/// Handle a `pull_request` event for the rfd repo.
pub async fn handle_rfd_pull_request(
    api_context: &Context,
    event: GitHubWebhook,
    company: &Company,
) -> Result<(octorust::types::ChecksCreateRequestConclusion, String)> {
    // Perform validation checks first to determine if we need to process this call or if we can
    // drop it early
    if event.repository.name != "rfd" {
        error!(
            "Attempting to run rfd `push` handler on the {} repo. Exiting as this should not occur.",
            event.repository.name
        );
        return Ok((
            octorust::types::ChecksCreateRequestConclusion::Skipped,
            format!(
                "Attempting to run rfd `push` handler on the {} repo. Exiting as this should not occur.",
                event.repository.name
            ),
        ));
    }

    // Check if we somehow had a pull request opened from the default branch.
    // This should never happen, but let's check regardless.
    if event.pull_request.head.commit_ref == event.repository.default_branch {
        // Return early.
        return Ok((octorust::types::ChecksCreateRequestConclusion::Skipped, format!(
            "event was to the default branch `{}`, we don't care, but also this would be pretty weird to have a pull request opened from the default branch",
            event.repository.default_branch,
        )));
    }

    // We only care if the pull request was `opened`.
    if event.action != "opened" {
        return Ok((
            octorust::types::ChecksCreateRequestConclusion::Success,
            format!("Ignoring pull_request hook due to unhandled action: {}", event.action),
        ));
    }

    // Attempt to parse an RFD number from the branch name, if one can not be determined we will
    // drop handling this pull request event
    let branch_name = &event.pull_request.head.commit_ref;

    if let Ok(number) = branch_name.trim_start_matches('0').parse::<i32>() {
        let repo = GitHubRFDRepo::new(company).await?;
        let branch = repo.branch(branch_name.to_string());

        let handler = RFDUpdater::default();

        handler
            .handle(
                api_context,
                &[GitHubRFDUpdate {
                    number: number.into(),
                    branch,
                }],
            )
            .await?;
    }

    Ok((
        octorust::types::ChecksCreateRequestConclusion::Success,
        "Completed pull_request event handling".to_string(),
    ))
}

/// Handle a `push` event for the configs repo.
pub async fn handle_configs_push(
    github: &octorust::Client,
    api_context: &Context,
    event: GitHubWebhook,
    company: &Company,
) -> Result<String> {
    // Get the repo.
    let repo = event.repository.name.to_string();

    if event.commits.is_empty() {
        // Return early that there are no commits.
        // IDK how we got here.
        // We should never get here since we check this above in the main loop.
        warn!("configs `push` event had no commits");
        return Ok("".to_string());
    }

    log::info!("configs `push` event");

    // Get the commit.
    let mut commit = event.commits.get(0).unwrap().clone();

    // Ignore any changes that are not to the `configs/` directory.
    let dir = "configs/";
    commit.filter_files_by_path(dir);
    if !commit.has_changed_files() {
        // No files changed that we care about.
        // We can throw this out, log it and return early.
        info!(
            "`push` event commit `{}` did not include any changes to the `{}` directory",
            commit.id, dir
        );
        return Ok("".to_string());
    }
    log::info!("configs `push` event: after changed files");

    // Get the branch name.
    let branch = event.refv.trim_start_matches("refs/heads/");
    // Make sure this is to the default branch, we don't care about anything else.
    if branch != event.repository.default_branch {
        // We can throw this out, log it and return early.
        return Ok("".to_string());
    }

    let mut message = String::new();

    let mut a = |s: &str| {
        info!("[configs] {}", s);
        message.push_str(&format!("[{}] ", Utc::now().format("%+")));
        message.push_str(s);
        message.push('\n');
    };

    log::info!("configs `push` event: after branch check");
    // Get the configs from our repo.
    let configs = get_configs_from_repo(github, company).await?;

    log::info!("configs `push` event: after get_configs_from_repo");

    // Check if the cio.toml file has changed. This contains app configuration data and should be
    // used to overwrite the existing app config
    if commit.file_changed("configs/cio.toml") {
        let mut app_config = api_context.app_config.write().unwrap();
        *app_config = configs.app_config;
    }

    // Check if the links.toml file changed.
    if commit.file_changed("configs/links.toml") || commit.file_changed("configs/huddles.toml") {
        // Update our links in the database.
        sync_links(&api_context.db, configs.links, configs.huddles, company).await?;
        a("[SUCCESS]: links");

        // We need to update the short URLs for the links.
        generate_shorturls_for_configs_links(
            &api_context.db,
            github,
            company,
            &company.authenticate_cloudflare()?,
            &repo,
        )
        .await?;
        a("[SUCCESS]: links shorturls");
    }

    // Check if the groups.toml file changed.
    // IMPORTANT: we need to sync the groups _before_ we sync the users in case we
    // added a new group to GSuite.
    if commit.file_changed("configs/groups.toml") {
        sync_groups(&api_context.db, configs.groups, company).await?;
        a("[SUCCESS]: groups");
    }

    // Check if the users.toml file changed.
    if commit.file_changed("configs/users.toml") {
        let config = api_context.app_config.read().unwrap().clone();
        sync_users(&api_context.db, github, configs.users, company, &config).await?;
        a("[SUCCESS]: users");
    }

    // Check if the buildings.toml file changed.
    // Buildings needs to be synchronized _before_ we move on to conference rooms.
    if commit.file_changed("configs/buildings.toml") {
        sync_buildings(&api_context.db, configs.buildings, company).await?;
        a("[SUCCESS]: buildings");
    }

    // Check if the resources.toml file changed.
    if commit.file_changed("configs/resources.toml") {
        sync_resources(&api_context.db, configs.resources, company).await?;
        a("[SUCCESS]: conference rooms");
    }

    // Check if the certificates.toml file changed.
    if commit.file_changed("configs/certificates.toml") {
        sync_certificates(&api_context.db, github, configs.certificates, company).await?;
        a("[SUCCESS]: certificates");
    }

    // Check if the github-outside-collaborators.toml file changed.
    if commit.file_changed("configs/github-outside-collaborators.toml") {
        // Sync github outside collaborators.
        sync_github_outside_collaborators(&api_context.db, github, configs.github_outside_collaborators, company)
            .await?;
        a("[SUCCESS]: GitHub outside collaborators");
    }

    // Check if the huddles file changed.
    if commit.file_changed("configs/huddles.toml") {
        // Sync huddles.
        cio_api::huddles::sync_huddles(&api_context.db, company).await?;
        a("[SUCCESS]: huddles");
    }

    message = message.trim().to_string();

    Ok(message)
}

/// Handle the `repository` event for all repos.
pub async fn handle_repository_event(
    github: &octorust::Client,
    api_context: &Context,
    event: GitHubWebhook,
    company: &Company,
) -> Result<String> {
    let mut message = String::new();

    let mut a = |s: &str| {
        message.push_str(&format!("[{}] ", Utc::now().format("%+")));
        message.push_str(s);
        message.push('\n');
    };

    let repo = github.repos().get(&company.github_org, &event.repository.name).await?;
    let nr = NewRepo::new_from_full(repo.clone(), company.id);
    let new_repo = nr.upsert(&api_context.db).await?;
    a(&format!(
        "[SUCCESS]: added repo `{}` to the database",
        new_repo.full_name
    ));

    // TODO: since we know only one repo changed we don't need to refresh them all,
    // make this a bit better.
    // Update the short urls for all the repos.
    generate_shorturls_for_repos(
        &api_context.db,
        github,
        company,
        &company.authenticate_cloudflare()?,
        "configs",
    )
    .await?;
    a("[SUCCESS]: generated short urls");

    // Sync the settings for this repo.
    new_repo.sync_settings(github, company).await?;
    a("[SUCCESS]: synced settings");

    Ok(message)
}
