use std::sync::Arc;

use dropshot::{
    endpoint, ApiDescription, ConfigDropshot, ConfigLogging,
    ConfigLoggingLevel, HttpError, HttpResponseUpdatedNoContent, HttpServer,
    RequestContext,
};

#[tokio::main]
async fn main() -> Result<(), String> {
    /*
     * We must specify a configuration with a bind address.  We'll use 127.0.0.1
     * since it's available and won't expose this server outside the host.  We
     * request port 8000.
     */
    let config_dropshot = ConfigDropshot {
        bind_address: "0.0.0.0:8000".parse().unwrap(),
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
    api.register(listen_github_webhooks).unwrap();

    // Start the server.
    let mut server = HttpServer::new(&config_dropshot, api, Arc::new(()), &log)
        .map_err(|error| format!("failed to start server: {}", error))
        .unwrap();

    let server_task = server.run();
    server.wait_for_shutdown(server_task).await
}

/** Listen for GitHub webhooks. */
#[endpoint {
    method = GET,
    path = "/github",
}]
async fn listen_github_webhooks(
    _rqctx: Arc<RequestContext>,
) -> Result<HttpResponseUpdatedNoContent, HttpError> {
    Ok(HttpResponseUpdatedNoContent())
}
