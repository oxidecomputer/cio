use std::any::Any;
use std::sync::Arc;

use chrono::offset::Utc;
use chrono::DateTime;
use dropshot::{
    endpoint, ApiDescription, ConfigDropshot, ConfigLogging,
    ConfigLoggingLevel, HttpError, HttpResponseAccepted, HttpResponseOk,
    HttpServer, Query, RequestContext, TypedBody,
};
use google_drive::GoogleDrive;
use hubcaps::Github;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cio_api::db::Database;
use cio_api::mailing_list::MailchimpWebhook;
use cio_api::models::{GitHubUser, GithubRepo, NewRFD};
use cio_api::slack::{get_public_relations_channel_post_url, post_to_channel};
use cio_api::utils::{authenticate_github_jwt, get_gsuite_token, github_org};

#[tokio::main]
async fn main() -> Result<(), String> {
    /*
     * We must specify a configuration with a bind address.  We'll use 127.0.0.1
     * since it's available and won't expose this server outside the host.  We
     * request port 8080.
     */
    let config_dropshot = ConfigDropshot {
        bind_address: "0.0.0.0:8080".parse().unwrap(),
    };

    /*
     * For simplicity, we'll configure an "info"-level logger that writes to
     * stderr assuming that it's a terminal.
     */
    let config_logging = ConfigLogging::StderrTerminal {
        level: ConfigLoggingLevel::Info,
    };
    let log = config_logging
        .to_logger("webhooky-server")
        .map_err(|error| format!("failed to create logger: {}", error))
        .unwrap();

    // Describe the API.
    let mut api = ApiDescription::new();
    /*
     * Register our endpoint and its handler function.  The "endpoint" macro
     * specifies the HTTP method and URI path that identify the endpoint,
     * allowing this metadata to live right alongside the handler function.
     */
    api.register(ping).unwrap();
    api.register(listen_github_webhooks).unwrap();
    api.register(listen_mailchimp_webhooks).unwrap();
    api.register(ping_mailchimp_webhooks).unwrap();

    /*
     * The functions that implement our API endpoints will share this context.
     */
    let api_context = Context::new().await;

    /*
     * Set up the server.
     */
    let mut server = HttpServer::new(&config_dropshot, api, api_context, &log)
        .map_err(|error| format!("failed to start server: {}", error))
        .unwrap();

    // Start the server.
    let server_task = server.run();
    server.wait_for_shutdown(server_task).await
}

/**
 * Application-specific context (state shared by handler functions)
 */
struct Context {
    // TODO: share a database connection here.
    drive: GoogleDrive,
    drive_rfd_shared_id: String,
    drive_rfd_dir_id: String,
    github: Github,
    github_org: String,
}

impl Context {
    /**
     * Return a new Context.
     */
    pub async fn new() -> Arc<Context> {
        // Get gsuite token.
        let token = get_gsuite_token().await;

        // Initialize the Google Drive client.
        let drive = GoogleDrive::new(token);

        // Figure out where our directory is.
        // It should be in the shared drive : "Automated Documents"/"rfds"
        let shared_drive = drive
            .get_drive_by_name("Automated Documents")
            .await
            .unwrap();
        let drive_rfd_shared_id = shared_drive.id.to_string();

        // Get the directory by the name.
        let drive_rfd_dir = drive
            .get_file_by_name(&drive_rfd_shared_id, "rfds")
            .await
            .unwrap();

        // Create the context.
        Arc::new(Context {
            drive,
            drive_rfd_shared_id,
            drive_rfd_dir_id: drive_rfd_dir.get(0).unwrap().id.to_string(),
            github: authenticate_github_jwt(),
            github_org: github_org(),
        })
    }

    /**
     * Given `rqctx` (which is provided by Dropshot to all HTTP handler
     * functions), return our application-specific context.
     */
    pub fn from_rqctx(rqctx: &Arc<RequestContext>) -> Arc<Context> {
        let ctx: Arc<dyn Any + Send + Sync + 'static> =
            Arc::clone(&rqctx.server.private);
        ctx.downcast::<Context>()
            .expect("wrong type for private data")
    }
}

/*
 * HTTP API interface
 */

/** Return pong. */
#[endpoint {
    method = GET,
    path = "/ping",
}]
async fn ping(
    _rqctx: Arc<RequestContext>,
) -> Result<HttpResponseOk<String>, HttpError> {
    Ok(HttpResponseOk("pong".to_string()))
}

/** Listen for GitHub webhooks. */
#[endpoint {
    method = POST,
    path = "/github",
}]
async fn listen_github_webhooks(
    rqctx: Arc<RequestContext>,
    body_param: TypedBody<GitHubWebhook>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    let api_context = Context::from_rqctx(&rqctx);
    let github_repo = api_context
        .github
        .repo(api_context.github_org.to_string(), "rfd");

    // TODO: share the database connection in the context.
    let db = Database::new();

    let event = body_param.into_inner();

    // Parse the `X-GitHub-Event` header.
    // TODO: make this nicer when supported as a first class method in dropshot.
    let req = rqctx.request.lock().await;
    let req_headers = req.headers();
    let event_type = req_headers
        .get("X-GitHub-Event")
        .unwrap_or(&http::header::HeaderValue::from_str("").unwrap())
        .to_str()
        .unwrap()
        .to_string();

    if event_type != "push".to_string()
        && event_type != "pull_request".to_string()
    {
        let msg = format!(
            "Aborted, not a `push` or `pull_request` event, got `{}`",
            event_type
        );
        println!("[github]: {}", msg);
        return Ok(HttpResponseAccepted(msg));
    }

    // Check if the event came from the rfd repo.
    let repo = event.clone().repository.unwrap();
    let repo_name = repo.name;
    if repo_name != "rfd" {
        // We only care about the rfd repo push events for now.
        // We can throw this out, log it and return early.
        let msg =
            format!("Aborted, `{}` event was to the {} repo, no automations are set up for this repo yet",event_type, repo_name);
        println!("[github]: {}", msg);
        return Ok(HttpResponseAccepted(msg));
    }

    // Handle if we got a pull_request.
    if event_type == "pull_request" {
        println!("[github]: pull_request {:?}", event);

        // We only care if the pull request was `opened`.
        if event.action != "opened" {
            // We can throw this out, log it and return early.
            let msg =
            format!("Aborted, `{}` event was to the {} repo, no automations are set up for action `{}` yet",event_type, repo_name, event.action);
            println!("[github]: {}", msg);
            return Ok(HttpResponseAccepted(msg));
        }

        // We have a newly opened pull request.
        // TODO: Let's update the discussion link for the RFD.

        let msg =
            format!("`{}` event was to the {} repo with action `{}`, updated discussion link for the RFD",event_type, repo_name, event.action);
        println!("[github]: {}", msg);
        return Ok(HttpResponseAccepted(msg));
    }

    // Now we can continue since we have a push event to the rfd repo.
    // Ensure we have commits.
    if event.commits.is_empty() {
        // `push` even has no commits.
        // We can throw this out, log it and return early.
        let msg = "Aborted, `push` event has no commits".to_string();
        println!("[github]: {}", msg);
        return Ok(HttpResponseAccepted(msg));
    }

    let mut commit = event.commits.get(0).unwrap().clone();
    // We only care about distinct commits.
    if !commit.distinct {
        // The commit is not distinct.
        // We can throw this out, log it and return early.
        let msg = format!(
            "Aborted, `push` event commit `{}` is not distinct",
            commit.id
        );
        println!("[github]: {}", msg);
        return Ok(HttpResponseAccepted(msg));
    }

    // Ignore any changes that are not to the `rfd/` directory.
    let dir = "rfd/";
    commit.filter_files_by_path(dir);
    if !commit.has_changed_files() {
        // No files changed that we care about.
        // We can throw this out, log it and return early.
        let msg = format!(
            "Aborted, `push` event commit `{}` does not include any changes to the `{}` directory",
            commit.id,
            dir
        );
        println!("[github]: {}", msg);
        return Ok(HttpResponseAccepted(msg));
    }

    // Get the branch name.
    let branch = event.refv.trim_start_matches("refs/heads/");
    // Make sure we have a branch.
    if branch.is_empty() {
        // The branch name is empty.
        // We can throw this out, log it and return early.
        let msg = "Aborted, `push` event branch name is empty".to_string();
        println!("[github]: {}", msg);
        return Ok(HttpResponseAccepted(msg));
    }

    // Iterate over the files and update the RFDs that have been added or
    // modified in our database.
    let mut changed_files = commit.added.clone();
    changed_files.append(&mut commit.modified.clone());
    for file in changed_files {
        // If the file is not a README.md or README.adoc, skip it.
        // TODO: handle the updating of images.
        if !file.ends_with("README.md") && !file.ends_with("README.adoc") {
            // Continue through the loop.
            continue;
        }

        // We have a README file that changed, let's parse the RFD and update it
        // in our database.
        println!(
            "[github] `push` event -> file {} was modified on branch {}",
            file, branch
        );
        // Parse the RFD.
        let new_rfd = NewRFD::new_from_github(
            &github_repo,
            branch,
            &file,
            commit.timestamp.unwrap(),
        )
        .await;

        // Get the old RFD from the database. We will need this later to
        // check if the RFD's state changed.
        let old_rfd = db.get_rfd(new_rfd.number);
        let mut old_rfd_state = "".to_string();
        let mut old_rfd_pdf = "".to_string();
        if old_rfd.is_some() {
            let o = old_rfd.unwrap();
            old_rfd_state = o.state.to_string();
            old_rfd_pdf = o.get_pdf_filename();
        }

        // Update the RFD in the database.
        let rfd = db.upsert_rfd(&new_rfd);

        // Create all the shorturls for the RFD if we need to,
        // this would be on added files, only.
        // TODO: see if we can make this faster by doing something better than
        // dispatching the workflow.
        github_repo
            .actions()
            .workflows()
            .dispatch(
                "run-shorturls",
                &hubcaps::workflows::WorkflowDispatchOptions::builder()
                    .reference(repo.default_branch.to_string())
                    .build(),
            )
            .await
            .unwrap();

        // Update airtable with the new RFD.
        let mut airtable_rfd = rfd.clone();
        airtable_rfd.create_or_update_in_airtable().await;

        // Update the PDFs for the RFD.
        rfd.convert_and_upload_pdf(
            &api_context.github,
            &api_context.drive,
            &api_context.drive_rfd_shared_id,
            &api_context.drive_rfd_dir_id,
        )
        .await;

        // Check if the RFD state changed from what is currently in the
        // database.
        // If the RFD's state was changed to `discussion`, we need to open a PR
        // for that RFD.
        // Make sure we are not on the master branch, since then we would not need
        // a PR. Instead, below, the state of the RFD would be moved to `published`.
        // TODO: see if we drop events if we do we might want to remove the check with
        // the old state and just do it everytime an RFD is in discussion.
        if old_rfd_state != rfd.state
            && rfd.state == "discussion"
            && branch != repo.default_branch.to_string()
        {
            // First, we need to make sure we don't already have a pull request open.
            let pulls = github_repo
                .pulls()
                .list(
                    &hubcaps::pulls::PullListOptions::builder()
                        .state(hubcaps::issues::State::Open)
                        .build(),
                )
                .await
                .unwrap();
            // Check if any pull requests are from our branch.
            let mut has_pull = false;
            for pull in pulls {
                // Check if the pull request is for our branch.
                let pull_branch =
                    pull.head.commit_ref.trim_start_matches("refs/heads/");
                println!("pull branch: {}", pull_branch);

                if pull_branch == branch {
                    println!("[github] RFD {} has moved from state {} -> {}, on branch {}, we already have a pull request: {}", rfd.number_string, old_rfd_state, rfd.state, branch, pull.html_url);

                    has_pull = true;
                    break;
                }
            }

            // Open a pull request, if we don't already have one.
            if !has_pull {
                // TODO: Open a pull request.
                println!("[github] RFD {} has moved from state {} -> {}, on branch {}, opening a PR",rfd.number_string, old_rfd_state, rfd.state, branch);

                /*let pull = github_repo
                                    .pulls()
                                    .create(&hubcaps::pulls::PullOptions::new(
                rfd.name,
                format!("{}:{}", api_context.github_org,branch),
                repo.default_branch.to_string(),
                Some("Automatically opening the pull request since the document is marked as being in discussion. If you wish to not have a pull request open, change the state of your document and close this pull request."),
                                            ))
                                    .await
                                    .unwrap();*/

                // We could update the discussion link here, but we will already
                // trigger a pull request created event, so we might as well let
                // that do its thing.
            }
        }

        // If the RFD was merged into the default branch, but the RFD state is not `published`,
        // update the state of the RFD in GitHub to show it as `published`.
        if branch == repo.default_branch.to_string() && rfd.state != "published"
        {
            println!(
                "[github] RFD {} is the branch {} but its state is {}, updating it to `published`",
                rfd.number_string,repo.default_branch, old_rfd_state,
            );

            // TODO: Update the state of the RFD in GitHub to show it as `published`.
            // After we change the file, this will kick off another webhook event, so we do not
            // need to update the database again.
        }

        // If the title of the RFD changed, delete the old PDF file so it
        // doesn't linger in GitHub and Google Drive.
        if old_rfd_pdf != rfd.get_pdf_filename() {
            let pdf_path = format!("/pdfs/{}", old_rfd_pdf);

            // First get the sha of the old pdf.
            let old_pdf = github_repo
                .content()
                .file(&pdf_path, &repo.default_branch)
                .await
                .unwrap();

            // Delete the old filename from GitHub.
            github_repo.content().delete(
                &pdf_path,
                &format!("Deleting file content {} programatically\n\nThis is done from the cio repo webhooky::listen_github_webhooks function.", old_rfd_pdf),
                &old_pdf.sha,
            ).await.unwrap();

            // Delete the old filename from drive.
            api_context
                .drive
                .delete_file_by_name(
                    &api_context.drive_rfd_shared_id,
                    &old_rfd_pdf,
                )
                .await
                .unwrap();

            println!("[github] RFD {} PDF changed name from {} -> {}, deleted old file from GitHub and Google Drive", rfd.number_string, old_rfd_pdf, rfd.get_pdf_filename());
        }
    }

    // TODO: should we do something if the file gets deleted (?)

    Ok(HttpResponseAccepted("Updated successfully".to_string()))
}

/** Ping endpoint for MailChimp webhooks. */
#[endpoint {
    method = GET,
    path = "/mailchimp",
}]
async fn ping_mailchimp_webhooks(
    _rqctx: Arc<RequestContext>,
) -> Result<HttpResponseOk<String>, HttpError> {
    Ok(HttpResponseOk("ok".to_string()))
}

/** Listen for MailChimp webhooks. */
#[endpoint {
    method = POST,
    path = "/mailchimp",
}]
async fn listen_mailchimp_webhooks(
    _rqctx: Arc<RequestContext>,
    query_args: Query<MailchimpWebhook>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    // TODO: share the database connection in the context.
    let db = Database::new();

    let event = query_args.into_inner();

    println!("[mailchimp] {:?}", event);

    if event.webhook_type != "subscribe".to_string() {
        let msg = format!(
            "Aborted, not a `subscribe` event, got `{}`",
            event.webhook_type
        );
        println!("[mailchimp]: {}", msg);
        return Ok(HttpResponseAccepted(msg));
    }

    // Parse the webhook as a new mailing list subscriber.
    /*let new_subscriber = event.as_subscriber();

    // TODO: Update the subscriber in the database.
    let subscriber = db.upsert_mailing_list_subscriber(&new_subscriber);

    // TODO: Update airtable with the new subscriber.
    let mut airtable_subscriber = subscriber.clone();
    airtable_subscriber.create_or_update_in_airtable().await;

    // Parse the signup into a slack message.
    // Send the message to the slack channel.
    post_to_channel(
        get_public_relations_channel_post_url(),
        new_subscriber.as_slack_msg(),
    )
    .await;*/

    Ok(HttpResponseAccepted("Updated successfully".to_string()))
}

/// A GitHub organization.
#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct GitHubOrganization {
    pub login: String,
    pub id: u64,
    pub url: String,
    pub repos_url: String,
    pub events_url: String,
    pub hooks_url: String,
    pub issues_url: String,
    pub members_url: String,
    pub public_members_url: String,
    pub avatar_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

/// A GitHub app installation.
#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct GitHubInstallation {
    #[serde(default)]
    pub id: i64,
    // account: Account
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub access_tokens_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub repositories_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub html_url: String,
    #[serde(default)]
    pub app_id: i32,
    #[serde(default)]
    pub target_id: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub target_type: String,
    // permissions: Permissions
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<String>,
    // created_at, updated_at
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub single_file_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub repository_selection: String,
}

/// A GitHub webhook event.
/// FROM: https://docs.github.com/en/free-pro-team@latest/developers/webhooks-and-events/webhook-events-and-payloads
#[derive(Debug, Clone, JsonSchema, Deserialize, Serialize)]
pub struct GitHubWebhook {
    /// Most webhook payloads contain an action property that contains the
    /// specific activity that triggered the event.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub action: String,
    /// The user that triggered the event. This property is included in
    /// every webhook payload.
    #[serde(default)]
    pub sender: GitHubUser,
    /// The `repository` where the event occurred. Webhook payloads contain the
    /// `repository` property when the event occurs from activity in a repository.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<GithubRepo>,
    /// Webhook payloads contain the `organization` object when the webhook is
    /// configured for an organization or the event occurs from activity in a
    /// repository owned by an organization.
    #[serde(default)]
    pub organization: GitHubOrganization,
    /// The GitHub App installation. Webhook payloads contain the `installation`
    /// property when the event is configured for and sent to a GitHub App.
    #[serde(default)]
    pub installation: GitHubInstallation,

    /// `push` event fields.
    /// FROM: https://docs.github.com/en/free-pro-team@latest/developers/webhooks-and-events/webhook-events-and-payloads#push
    ///
    /// The full `git ref` that was pushed. Example: `refs/heads/main`.
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "ref")]
    pub refv: String,
    /// The SHA of the most recent commit on `ref` before the push.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub before: String,
    /// The SHA of the most recent commit on `ref` after the push.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub after: String,
    /// An array of commit objects describing the pushed commits.
    /// The array includes a maximum of 20 commits. If necessary, you can use
    /// the Commits API to fetch additional commits. This limit is applied to
    /// timeline events only and isn't applied to webhook deliveries.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commits: Vec<GitHubCommit>,

    /// `pull_request` event fields.
    /// FROM: https://docs.github.com/en/free-pro-team@latest/developers/webhooks-and-events/webhook-events-and-payloads#pull_request
    ///
    /// The pull request number.
    #[serde(default)]
    pub number: i64,
    /// The pull request itself.
    #[serde(default)]
    pub pull_request: GitHubPullRequest,
}

/// A GitHub commit.
/// FROM: https://docs.github.com/en/free-pro-team@latest/developers/webhooks-and-events/webhook-events-and-payloads#push
#[derive(
    Debug, Clone, Default, PartialEq, JsonSchema, Deserialize, Serialize,
)]
pub struct GitHubCommit {
    /// The SHA of the commit.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    /// The ISO 8601 timestamp of the commit.
    pub timestamp: Option<DateTime<Utc>>,
    /// The commit message.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub message: String,
    /// The git author of the commit.
    #[serde(default, alias = "user")]
    pub author: GitHubUser,
    /// URL that points to the commit API resource.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    /// Whether this commit is distinct from any that have been pushed before.
    #[serde(default)]
    pub distinct: bool,
    /// An array of files added in the commit.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub added: Vec<String>,
    /// An array of files modified by the commit.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modified: Vec<String>,
    /// An array of files removed in the commit.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "ref")]
    pub commit_ref: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sha: String,
}

/// A GitHub pull request.
/// FROM: https://docs.github.com/en/free-pro-team@latest/rest/reference/pulls#get-a-pull-request
#[derive(
    Debug, Default, Clone, PartialEq, JsonSchema, Deserialize, Serialize,
)]
pub struct GitHubPullRequest {
    #[serde(default)]
    pub id: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    /// The HTML location of this pull request.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub html_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub diff_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub patch_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub issue_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub commits_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub review_comments_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub review_comment_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub comments_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub statuses_url: String,
    #[serde(default)]
    pub number: i64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub state: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub body: String,
    /*pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,*/
    #[serde(default)]
    pub closed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub merged_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub head: GitHubCommit,
    #[serde(default)]
    pub base: GitHubCommit,
    // links
    #[serde(default)]
    pub user: GitHubUser,
    #[serde(default)]
    pub merged: bool,
}

impl GitHubCommit {
    /// Filter the files that were added, modified, or removed by their prefix
    /// including a specified directory or path.
    pub fn filter_files_by_path(&mut self, dir: &str) {
        self.added = filter(&self.added, dir);
        self.modified = filter(&self.modified, dir);
        self.removed = filter(&self.removed, dir);
    }

    /// Return if the commit has any files that were added, modified, or removed.
    pub fn has_changed_files(&self) -> bool {
        !self.added.is_empty()
            || !self.modified.is_empty()
            || !self.removed.is_empty()
    }
}

fn filter(files: &Vec<String>, dir: &str) -> Vec<String> {
    let mut in_dir: Vec<String> = Default::default();
    for file in files {
        if file.starts_with(dir) {
            in_dir.push(file.to_string());
        }
    }

    in_dir
}
