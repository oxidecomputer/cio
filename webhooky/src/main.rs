use std::any::Any;
use std::sync::Arc;

use chrono::offset::Utc;
use chrono::DateTime;
use dropshot::{
    endpoint, ApiDescription, ConfigDropshot, ConfigLogging,
    ConfigLoggingLevel, HttpError, HttpResponseAccepted, HttpResponseOk,
    HttpServer, RequestContext, TypedBody,
};
use hubcaps::Github;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cio_api::models::{GitHubUser, GithubRepo, NewRFD};
use cio_api::utils::authenticate_github;

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
    github: Github,
}

impl Context {
    /**
     * Return a new Context.
     */
    pub async fn new() -> Arc<Context> {
        let api_context = Context {
            github: authenticate_github(),
        };

        Arc::new(api_context)
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

    if event_type != "push".to_string() {
        // If we did not get a push event we can log it and return early.
        let msg =
            format!("Aborted, not a `push` event, got `{}`", event.action);
        println!("[github]: {}", msg);
        return Ok(HttpResponseAccepted(msg));
    }

    // Handle the push event.
    // Check if it came from the rfd repo.
    let repo = event.clone().repository.unwrap();
    let repo_name = repo.name;
    if repo_name != "rfd" {
        // We only care about the rfd repo push events for now.
        // We can throw this out, log it and return early.
        let msg =
            format!("Aborted, `push` event was to the {} repo, no automations are set up for this repo yet", repo_name);
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
            &api_context.github,
            branch,
            &file,
            commit.timestamp,
        )
        .await;
        println!("[github] RFD: {:?}", new_rfd);
    }

    // TODO: should we do something if the file gets deleted (?)

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
    pub id: u64,
    // account: Account
    pub access_tokens_url: String,
    pub repositories_url: String,
    pub html_url: String,
    pub app_id: i32,
    pub target_id: i32,
    pub target_type: String,
    // permissions: Permissions
    pub events: Vec<String>,
    // created_at, updated_at
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub single_file_name: String,
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
}

/// A GitHub commit.
/// FROM: https://docs.github.com/en/free-pro-team@latest/developers/webhooks-and-events/webhook-events-and-payloads#push
#[derive(Debug, Clone, PartialEq, JsonSchema, Deserialize, Serialize)]
pub struct GitHubCommit {
    /// The SHA of the commit.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    /// The ISO 8601 timestamp of the commit.
    pub timestamp: DateTime<Utc>,
    /// The commit message.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub message: String,
    /// The git author of the commit.
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
