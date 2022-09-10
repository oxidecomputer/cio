use anyhow::Result;
use async_trait::async_trait;
use chrono::offset::Utc;
use cio_api::{
    companies::Company,
    configs::{
        get_configs_from_repo, sync_buildings, sync_certificates, sync_github_outside_collaborators, sync_groups,
        sync_links, sync_resources, sync_users,
    },
    repos::NewRepo,
    rfds::RFD,
    shorturls::{generate_shorturls_for_configs_links, generate_shorturls_for_repos},
    utils::{create_or_update_file_in_github_repo, decode_base64_to_string},
};
use dropshot::{Extractor, RequestContext, ServerContext};
use dropshot_verify_request::sig::HmacSignatureVerifier;
use hmac::Hmac;
use log::{info, warn};
use sha2::Sha256;
use std::{str::FromStr, sync::Arc};

use crate::{context::Context, event_types::EventType, github_types::GitHubWebhook, http::Headers, repos::Repo};

mod rfd;

use rfd::RFDPushHandler;

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
        let github = company.authenticate_github()?;

        match repo_name {
            Repo::RFD => match event_type {
                EventType::Push => {
                    sentry::configure_scope(|scope| {
                        scope.set_context("github.webhook", sentry::protocol::Context::Other(event.clone().into()));
                        scope.set_tag("github.event.type", &event_type_string);
                    });

                    let handler = RFDPushHandler::new();

                    match handler.handle(&github, api_context, event.clone()).await {
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

                    match handle_rfd_pull_request(&github, api_context, event.clone(), &company).await {
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

/// Handle a `pull_request` event for the rfd repo.
pub async fn handle_rfd_pull_request(
    github: &octorust::Client,
    api_context: &Context,
    event: GitHubWebhook,
    company: &Company,
) -> Result<(octorust::types::ChecksCreateRequestConclusion, String)> {
    let db = &api_context.db;

    let owner = &company.github_org;
    let repo = "rfd";

    // Let's get the RFD.
    let branch = event.pull_request.head.commit_ref.to_string();

    // Check if we somehow had a pull request opened from the default branch.
    // This should never happen, but let's check regardless.
    if branch == event.repository.default_branch {
        // Return early.
        return Ok((octorust::types::ChecksCreateRequestConclusion::Skipped, format!(
            "event was to the default branch `{}`, we don't care, but also this would be pretty weird to have a pull request opened from the default branch",
            event.repository.default_branch,
        )));
    }

    // The branch should be equivalent to the number in the database.
    // Let's try to get the RFD from that.
    let number = branch.trim_start_matches('0').parse::<i32>().unwrap_or_default();
    // Make sure we actually have a number.
    if number == 0 {
        // Return early.
        return Ok((
            octorust::types::ChecksCreateRequestConclusion::Skipped,
            format!(
                "event was to the branch `{}`, which is not a number so it cannot be an RFD",
                branch,
            ),
        ));
    }

    // Try to get the RFD from the database.
    let result = RFD::get_from_db(db, number).await;
    if result.is_none() {
        return Ok((
            octorust::types::ChecksCreateRequestConclusion::Skipped,
            format!("could not find RFD with number `{}` in the database", number),
        ));
    }
    let mut rfd = result.unwrap();

    let mut message = String::new();

    let mut a = |s: &str| {
        message.push_str(&format!("[{}] ", Utc::now().format("%+")));
        message.push_str(s);
        message.push('\n');
    };

    let mut has_errors = false;
    match rfd.update_pull_request(github, company, &event.pull_request).await {
        Ok(_) => {
            a("[SUCCESS]: update pull request title and labels");
        }
        Err(e) => {
            warn!(
                "unable to update pull request tile and labels for pr#{}: {}",
                event.pull_request.number, e,
            );

            a(&format!(
                "[ERROR]: update pull request title and labels: {} cc @augustuswm",
                e
            ));

            has_errors = true;
        }
    }

    // We only care if the pull request was `opened`.
    if event.action != "opened" {
        // We can throw this out, log it and return early.
        a(&format!(
            "[SUCCESS]: completed automations for `{}` action",
            event.action
        ));
        return Ok((octorust::types::ChecksCreateRequestConclusion::Success, message));
    }

    // Okay, now we finally have the RFD.
    // We need to do two things.
    //  1. Update the discussion link.
    //  2. Update the state of the RFD to be in discussion if it is not
    //      in an acceptable current state. More on this below.
    // To do both these tasks we need to first get the path of the file on GitHub,
    // so we can update it later, and also find out if it is markdown or not for parsing.

    // Get the file path from GitHub.
    // We need to figure out whether this file is a README.adoc or README.md
    // before we update it.
    // Let's get the contents of the directory from GitHub.
    let dir = format!("/rfd/{}", branch);
    // Get the contents of the file.
    let mut path = format!("{}/README.adoc", dir);
    match github.repos().get_content_file(owner, repo, &path, &branch).await {
        Ok(contents) => {
            rfd.content = decode_base64_to_string(&contents.content);
            rfd.sha = contents.sha;
        }
        Err(e) => {
            info!(
                "[rfd] getting file contents for {} on branch {} failed: {}, trying markdown instead...",
                path, branch, e
            );

            // Try to get the markdown instead.
            path = format!("{}/README.md", dir);
            let contents = github.repos().get_content_file(owner, repo, &path, &branch).await?;

            rfd.content = decode_base64_to_string(&contents.content);
            rfd.sha = contents.sha;
        }
    }

    // Update the discussion link.
    let discussion_link = event.pull_request.html_url;
    rfd.update_discussion(&discussion_link)?;

    a(&format!(
        "[SUCCESS]: ensured RFD discussion link is `{}`",
        discussion_link
    ));

    // A pull request can be open for an RFD if it is in the following states:
    //  - published: a already published RFD is being updated in a pull request.
    //  - discussion: it is in discussion
    //  - ideation: it is in ideation
    // We can update the state if it is not currently in an acceptable state.
    if rfd.state != "discussion" && rfd.state != "published" && rfd.state != "ideation" {
        //  Update the state of the RFD in GitHub to show it as `discussion`.
        rfd.update_state("discussion")?;
        a("[SUCCESS]: updated RFD state to `discussion`");
    }

    // Update the RFD to show the new state and link in the database.
    rfd.update(db).await?;
    a("[SUCCESS]: updated RFD in the database");
    a("[SUCCESS]: updated RFD in Airtable");

    // Update the file in GitHub.
    // Keep in mind: this push will kick off another webhook.
    create_or_update_file_in_github_repo(github, owner, repo, &branch, &path, rfd.content.as_bytes().to_vec()).await?;
    a("[SUCCESS]: updated RFD file in GitHub with any changes");

    a(&format!(
        "[SUCCESS]: completed automations for `{}` action",
        event.action
    ));

    if has_errors {
        return Ok((octorust::types::ChecksCreateRequestConclusion::Failure, message));
    }

    Ok((octorust::types::ChecksCreateRequestConclusion::Success, message))
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
