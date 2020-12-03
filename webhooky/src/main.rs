use std::sync::Arc;

use dropshot::{
    endpoint, ApiDescription, ConfigDropshot, ConfigLogging,
    ConfigLoggingLevel, HttpError, HttpResponseAccepted, HttpResponseOk,
    HttpServer, RequestContext, TypedBody,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cio_api::models::{GitHubUser, GithubRepo};

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
) -> Result<HttpResponseAccepted<String>, HttpError> {
    let event = body_param.into_inner();

    if event.action != "push".to_string() {
        // If we did not get a push event we can log it and return early.
        println!("github: {:?}", event);
        return Ok(HttpResponseAccepted(
            "Aborted, event was not a push event".to_string(),
        ));
    }

    println!("github push event: {:?}", event);

    // Handle the push event.

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
}
