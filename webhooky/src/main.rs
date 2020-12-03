use std::sync::Arc;

use dropshot::{
    endpoint, ApiDescription, ConfigDropshot, ConfigLogging,
    ConfigLoggingLevel, HttpError, HttpResponseOk,
    HttpResponseUpdatedNoContent, HttpServer, RequestContext, TypedBody,
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
) -> Result<HttpResponseUpdatedNoContent, HttpError> {
    let event = body_param.into_inner();

    if event.action != "push".to_string() {
        // If we did not get a push event we can log it and return early.
        println!("github: {:?}", event);
        return Ok(HttpResponseUpdatedNoContent());
    }

    println!("github push event: {:?}", event);

    // Handle the push event.

    Ok(HttpResponseUpdatedNoContent())
}

/// A GitHub webhook event.
/// FROM: https://docs.github.com/en/free-pro-team@latest/developers/webhooks-and-events/webhook-events-and-payloads
#[derive(Debug, Clone, JsonSchema, Deserialize, Serialize)]
pub struct GitHubWebhook {
    /// The type of action.
    pub action: String,
    /// The user that triggered the event.
    pub sender: GitHubUser,
    /// The repository object where the event occurred.
    pub repository: GithubRepo,
}
