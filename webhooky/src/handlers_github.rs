use std::{str::FromStr, sync::Arc};

use anyhow::Result;
use chrono::offset::Utc;
use cio_api::{
    companies::Company,
    configs::{
        get_configs_from_repo, sync_buildings, sync_certificates, sync_conference_rooms,
        sync_github_outside_collaborators, sync_groups, sync_links, sync_users,
    },
    repos::NewRepo,
    rfds::{is_image, NewRFD, RFD},
    shorturls::{generate_shorturls_for_configs_links, generate_shorturls_for_repos, generate_shorturls_for_rfds},
    utils::{create_or_update_file_in_github_repo, decode_base64_to_string, get_file_content_from_repo},
};
use dropshot::{RequestContext, TypedBody};
use google_drive::traits::{DriveOps, FileOps};
use log::{info, warn};

use crate::{event_types::EventType, github_types::GitHubWebhook, repos::Repo, server::Context};

/// Handle a request to the /github endpoint.
pub async fn handle_github(rqctx: Arc<RequestContext<Context>>, body_param: TypedBody<GitHubWebhook>) -> Result<()> {
    let api_context = rqctx.context();

    let event = body_param.into_inner();

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

            // Now let's handle the event.
            if let Err(e) = handle_repository_event(&github, api_context, event.clone(), &company).await {
                // Send the error to sentry.
                sentry::integrations::anyhow::capture_anyhow(&e);
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
                    match handle_rfd_push(&github, api_context, event.clone(), &company).await {
                        Ok(message) => {
                            event.create_comment(&github, &message).await?;
                        }
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
                "[ERROR]: update pull request title and labels: {} cc @jessfraz @augustuswm",
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
    rfd.update_discussion(&discussion_link, path.ends_with(".md"));
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
        rfd.update_state("discussion", path.ends_with(".md"))?;
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

/// Handle a `push` event for the rfd repo.
pub async fn handle_rfd_push(
    github: &octorust::Client,
    api_context: &Context,
    event: GitHubWebhook,
    company: &Company,
) -> Result<String> {
    info!("[rfd.push] Remaining stack size: {:?}", stacker::remaining_stack());

    let db = &api_context.db;

    // Initialize the Google Drive client.
    let drive = company.authenticate_google_drive(db).await?;

    // Figure out where our directory is.
    // It should be in the shared drive : "Automated Documents"/"rfds"
    let shared_drive = drive.drives().get_by_name("Automated Documents").await?;

    // Get the repo.
    let owner = &company.github_org;
    let repo = event.repository.name.to_string();

    if event.commits.is_empty() {
        // Return early that there are no commits.
        // IDK how we got here, since we check this above in the main github handler.
        warn!("rfd `push` event had no commits");
        return Ok(String::new());
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
        return Ok(String::new());
    }

    // Get the branch name.
    let branch = event.refv.trim_start_matches("refs/heads/");

    let mut message = String::new();

    let mut a = |s: &str| {
        info!("[rfd] {}", s);
        message.push_str(&format!("[{}] ", Utc::now().format("%+")));
        message.push_str(s);
        message.push('\n');
    };

    // Iterate over the removed files and remove any images that we no longer
    // need for the HTML rendered RFD website.
    for file in commit.removed {
        // Make sure the file has a prefix of "rfd/".
        if !file.starts_with("rfd/") {
            // Continue through the loop early.
            // We only care if a file change in the rfd/ directory.
            continue;
        }

        if is_image(&file) {
            // Remove the image from the `src/public/static/images` path since we no
            // longer need it.
            // We delete these on the default branch ONLY.
            let website_file = file.replace("rfd/", "src/public/static/images/");

            // We need to get the current sha for the file we want to delete.
            let (_, gh_file_sha) = if let Ok((v, s)) =
                get_file_content_from_repo(github, owner, &repo, &event.repository.default_branch, &website_file).await
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
                        owner,
                        &repo,
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
                            branch: event.repository.default_branch.to_string(),
                        },
                    )
                    .await?;
                a(&format!(
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
            let (gh_file_content, _) = get_file_content_from_repo(github, owner, &repo, branch, &file).await?;

            // Let's write the file contents to the location for the static website.
            // We replace the `rfd/` path with the `src/public/static/images/` path since
            // this is where images go for the static website.
            // We update these on the default branch ONLY
            let website_file = file.replace("rfd/", "src/public/static/images/");
            create_or_update_file_in_github_repo(
                github,
                owner,
                &repo,
                &event.repository.default_branch,
                &website_file,
                gh_file_content,
            )
            .await?;
            a(&format!(
                "[SUCCESS]: updated file `{}` since it was modified in this push",
                website_file,
            ));
            // We are done so we can continue throught the loop.
            continue;
        }

        // If the file is a README.md or README.adoc, an RFD doc changed, let's handle it.
        if file.ends_with("README.md") || file.ends_with("README.adoc") {
            // We have a README file that changed, let's parse the RFD and update it
            // in our database.
            info!("`push` event -> file {} was modified on branch {}", file, branch,);
            // Parse the RFD.
            let mut new_rfd =
                NewRFD::new_from_github(company, github, owner, &repo, branch, &file, commit.timestamp.unwrap())
                    .await?;

            info!("Generated RFD for branch {} from GitHub", branch);

            // If the branch does not equal exactly the number string,
            // exit early since we have an update to an existing RFD not an explicit
            // RFD itself. This usually happens when the branch name can parse as a
            // number like `0001-some-change`, we want to skip those changes as
            // they are not named explicitly `0001`.
            if branch != new_rfd.number_string {
                a(&format!(
                    "Skipping updates to RFD in database since branch name `{}` \
                    does not equal RFD number `{}` explicitly.",
                    branch, new_rfd.number_string
                ));
                return Ok(message);
            }

            // Ensure the branch exists.
            // Basically what might happen is the following:
            // - User changes status to published.
            // - There is a merge right after.
            // - The branch no longer exists, but we try to get the branch here.
            if let Err(e) = github.repos().get_branch(owner, &repo, branch).await {
                // If we get an error here, we need to return early.
                a(&format!(
                    "Skipping updates to RFD in database since branch name `{}` \
                    does not exist anymore. Likely this branch was already merged. Error getting branch: `{}`",
                    branch, e
                ));
                return Ok(message);
            }

            // Get the old RFD from the database.
            // DO THIS BEFORE UPDATING THE RFD.
            // We will need this later to check if the RFD's state changed.
            let old_rfd = RFD::get_from_db(db, new_rfd.number).await;

            info!(
                "Checking for existing RFD in database {:?}",
                old_rfd.as_ref().map(|o| o.id)
            );

            let mut old_rfd_state = "".to_string();
            let mut old_rfd_pdf = "".to_string();
            if let Some(o) = old_rfd {
                old_rfd_state = o.state.to_string();
                old_rfd_pdf = o.get_pdf_filename();

                // Set the html just so it's not blank momentarily.
                new_rfd.content = o.content.to_string();
                new_rfd.authors = o.authors.to_string();
                new_rfd.html = o.html.to_string();
                new_rfd.commit_date = o.commit_date;
                new_rfd.sha = o.sha.to_string();
                new_rfd.pdf_link_github = o.pdf_link_github.to_string();
                new_rfd.pdf_link_google_drive = o.pdf_link_google_drive;
            }

            // Update the RFD in the database.
            let mut rfd = new_rfd.upsert(db).await?;

            info!(
                "Upserted new rfd into database. Id: {} AirtableId: {}",
                rfd.id, rfd.airtable_record_id
            );

            // Update all the fields for the RFD.
            rfd.expand(github, company).await?;
            rfd.update(db).await?;
            a(&format!(
                "[SUCCESS]: updated RFD {} in the database",
                new_rfd.number_string
            ));
            a(&format!(
                "[SUCCESS]: updated airtable for RFD {}",
                new_rfd.number_string
            ));

            // Now that the database is updated, update the search index.
            rfd.update_search_index().await?;
            a("[SUCCESS]: triggered update of the search index");

            // Create all the shorturls for the RFD if we need to,
            // this would be on added files, only.
            generate_shorturls_for_rfds(db, github, company, &company.authenticate_cloudflare()?, "configs").await?;
            a("[SUCCESS]: updated shorturls for the rfds");

            // Update the PDFs for the RFD.
            rfd.convert_and_upload_pdf(db, github, company).await?;
            rfd.update(db).await?;
            a(&format!(
                "[SUCCESS]: updated pdf `{}` for RFD {}",
                rfd.get_pdf_filename(),
                new_rfd.number_string,
            ));

            // Check if the RFD state changed from what is currently in the
            // database.
            // If the RFD's state was changed to `discussion`, we need to open a PR
            // for that RFD.
            // Make sure we are not on the default branch, since then we would not need
            // a PR. Instead, below, the state of the RFD would be moved to `published`.
            if rfd.state == "discussion" && branch != event.repository.default_branch {
                let pull_requests = RFD::find_pull_requests(github, owner, &repo, branch).await?;

                if pull_requests.is_empty() {
                    let pull = github
                        .pulls()
                        .create(
                            owner,
                            &repo,
                            &octorust::types::PullsCreateRequest {
                                title: rfd.name.to_string(),
                                head: format!("{}:{}", company.github_org, branch),
                                base: event.repository.default_branch.to_string(),
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

                    a(&format!(
                        "[SUCCESS]: RFD {} has moved from state {} -> {}, on branch {}, opened pull request {}",
                        rfd.number_string, old_rfd_state, rfd.state, branch, pull.number,
                    ));
                } else {
                    // This is here to remain consistent with previous behavior. This block
                    // likely needs to be refactored to account for multiple pull requests
                    // existing (even though there *should* never be multiple)
                    let pull = &pull_requests[0];

                    // This block updates the title and labels for the pull request. This only
                    // runs when the state changes, which means that if there is a manual title
                    // update or a label is deleted by a user, then this process will not fix
                    // that data. This needs to largely be refactored (in conjunction with the
                    // discussion link handling below) to be more coherent.
                    if old_rfd_state != rfd.state {
                        a(&format!(
                            "[SUCCESS]: RFD {} has moved from state {} -> {}, on branch {}, we already have a pull request: {}",
                            rfd.number_string,
                            old_rfd_state,
                            rfd.state,
                            branch,
                            pull.html_url
                        ));

                        // Let's update the pull request stuff tho just in case.
                        match rfd.update_pull_request(github, company, pull).await {
                            Ok(_) => {
                                a("[SUCCESS]: update pull request title and labels");
                            }
                            Err(e) => {
                                warn!(
                                    "unable to update pull request for pr#{}: {}",
                                    event.pull_request.number, e,
                                );

                                a(&format!(
                                    "[ERROR]: update pull request title and labels: {} cc @jessfraz @augustuswm",
                                    e
                                ));
                            }
                        }
                    }

                    // If the stored discussion link does not match the PR we found, then and
                    // update is required
                    if rfd.discussion != pull.html_url {
                        rfd.update_discussion(&pull.html_url, file.ends_with("README.md"));

                        // Update the file in GitHub. This will trigger another commit webhook
                        // and therefore must only occur when there is a change that needs to
                        // be made. If this is handled unconditionally then commit hooks could
                        // loop indefinitely.
                        create_or_update_file_in_github_repo(
                            github,
                            owner,
                            &repo,
                            branch,
                            &file,
                            rfd.content.as_bytes().to_vec(),
                        )
                        .await?;
                        a("[SUCCESS]: updated RFD file in GitHub with discussion link changes");

                        if let Err(err) = rfd.update(db).await {
                            a(&format!(
                                "[ERROR]: failed to update disucussion url: {} cc @jessfraz @augustuswm",
                                err
                            ));
                        }
                    }
                }
            }

            // If the RFD was merged into the default branch, but the RFD state is not `published`,
            // update the state of the RFD in GitHub to show it as `published`.
            if branch == event.repository.default_branch && rfd.state != "published" {
                //  Update the state of the RFD in GitHub to show it as `published`.
                let mut rfd_mut = rfd.clone();
                rfd_mut.update_state("published", file.ends_with(".md"))?;

                // Update the RFD to show the new state in the database.
                rfd_mut.update(db).await?;

                // Update the file in GitHub.
                // Keep in mind: this push will kick off another webhook.
                create_or_update_file_in_github_repo(
                    github,
                    owner,
                    &repo,
                    branch,
                    &file,
                    rfd_mut.content.as_bytes().to_vec(),
                )
                .await?;
                a(&format!(
                    "[SUCCESS]: updated state to `published` for RFD {}, since it was merged into branch {}",
                    new_rfd.number_string, event.repository.default_branch
                ));
            }

            // If the title of the RFD changed, delete the old PDF file so it
            // doesn't linger in GitHub and Google Drive.
            if !old_rfd_pdf.is_empty() && old_rfd_pdf != rfd.get_pdf_filename() {
                let pdf_path = format!("/pdfs/{}", old_rfd_pdf);

                // First get the sha of the old pdf.
                let (_, old_pdf_sha) =
                    get_file_content_from_repo(github, owner, &repo, &event.repository.default_branch, &pdf_path)
                        .await?;

                if !old_pdf_sha.is_empty() {
                    // Delete the old filename from GitHub.
                    github
                        .repos()
                        .delete_file(
                            owner,
                            &repo,
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
                                branch: event.repository.default_branch.to_string(),
                            },
                        )
                        .await?;
                    a(&format!(
                        "[SUCCESS]: deleted old pdf file in GitHub {} since the new name is {}",
                        old_rfd_pdf,
                        rfd.get_pdf_filename()
                    ));
                }

                // Get the directory by the name.
                let parent_id = drive.files().create_folder(&shared_drive.id, "", "rfds").await?;

                // Delete the old filename from drive.
                drive
                    .files()
                    .delete_by_name(&shared_drive.id, &parent_id, &old_rfd_pdf)
                    .await?;
                a(&format!(
                    "[SUCCESS]: deleted old pdf file in Google Drive {} since the new name is {}",
                    old_rfd_pdf,
                    rfd.get_pdf_filename()
                ));
            }

            a(&format!(
                "[SUCCESS]: RFD {} `push` operations completed",
                new_rfd.number_string
            ));
        }
    }

    // TODO: should we do something if the file gets deleted (?)
    Ok(message)
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
        sync_users(&api_context.db, github, configs.users, company).await?;
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
        sync_conference_rooms(&api_context.db, configs.resources, company).await?;
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
