use std::sync::Arc;

use dropshot::{
    endpoint, ApiDescription, ConfigDropshot, ConfigLogging,
    ConfigLoggingLevel, HttpError, HttpResponseOk,
    HttpResponseUpdatedNoContent, HttpServer, RequestContext, TypedBody,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

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

    // Start the server.
    let mut server = HttpServer::new(&config_dropshot, api, Arc::new(()), &log)
        .map_err(|error| format!("failed to start server: {}", error))
        .unwrap();

    let server_task = server.run();
    server.wait_for_shutdown(server_task).await
}

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
    _rqctx: Arc<RequestContext>,
    body_param: TypedBody<GitHubWebhook>,
) -> Result<HttpResponseUpdatedNoContent, HttpError> {
    println!("{:?}", body_param.into_inner());
    Ok(HttpResponseUpdatedNoContent())
}

/// A GitHub actor.
#[derive(Debug, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
pub struct GitHubActor {
    /// The unique identifier for the actor.
    pub id: String,
    /// The username of the actor.
    pub login: String,
    /// The specific display format of the username.
    pub display_login: String,
    /// The unique identifier of the Gravatar profile for the actor.
    pub gravatar_id: String,
    /// The REST API URL used to retrieve the user object, which includes
    /// additional user information.
    pub url: String,
    /// The URL of the actor's profile image.
    pub avatar_url: String,
}

/// A GitHub repo, abbreviated datatype for the webhook.
#[derive(Debug, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
pub struct GitHubRepo {
    /// The unique identifier of the repository.
    pub id: String,
    /// The name of the repository, which includes the owner and repository name.
    /// For example, octocat/hello-world is the name of the hello-world
    /// repository owned by the octocat user account.
    pub name: String,
    /// The REST API URL used to retrieve the repository object, which includes
    /// additional repository information.
    pub url: String,
}

/// A GitHub webhook event.
#[derive(Debug, Clone, JsonSchema, Deserialize, Serialize)]
pub struct GitHubWebhook {
    /// Unique identifier for the event.
    pub id: String,
    /// The type of event. Events uses PascalCase for the name.
    #[serde(default, rename = "type")]
    pub typev: GitHubEventType,
    /// The user that triggered the event.
    pub actor: GitHubActor,
    /// The repository object where the event occurred.
    pub repo: GitHubRepo,
    /// The event payload object is unique to the event type.
    /// See the event type below for the event API payload object.
    pub payload: Value,
}

#[derive(Debug, Clone, JsonSchema, Deserialize, Serialize)]
pub enum GitHubEventType {
    CommitCommentEvent,
    CreateEvent,
    DeleteEvent,
    ForkEvent,
    GollumEvent,
    IssueCommentEvent,
    IssuesEvent,
    MemberEvent,
    PublicEvent,
    PullRequestEvent,
    PullRequestReviewCommentEvent,
    PushEvent,
    ReleaseEvent,
    SponsorshipEvent,
    WatchEvent,
    /// NoopEvent is not a real GitHub event type, it is merely here as a default.
    NoopEvent,
}

impl Default for GitHubEventType {
    fn default() -> Self {
        GitHubEventType::NoopEvent
    }
}
