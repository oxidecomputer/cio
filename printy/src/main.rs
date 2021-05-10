use std::env;
use std::fs::File;
use std::io::Write;
use std::process::Command;
use std::str::from_utf8;
use std::sync::Arc;

use dropshot::{endpoint, ApiDescription, ConfigDropshot, ConfigLogging, ConfigLoggingLevel, HttpError, HttpResponseAccepted, HttpResponseOk, HttpServerStarter, RequestContext, TypedBody};
use sentry::IntoDsn;
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<(), String> {
    // Try to get the current git hash.
    let git_hash = if let Ok(gh) = env::var("GIT_HASH") {
        gh
    } else {
        // Try to shell out.
        let output = Command::new("git").arg("rev-parse").arg("HEAD").output().expect("failed to execute process");
        let o = std::str::from_utf8(&output.stdout).unwrap();
        o[0..8].to_string()
    };
    println!("git hash: {}", git_hash);

    // Initialize sentry.
    let sentry_dsn = env::var("PRINTY_SENTRY_DSN").unwrap_or_default();
    let _guard = sentry::init(sentry::ClientOptions {
        dsn: sentry_dsn.into_dsn().unwrap(),

        release: Some(git_hash.into()),
        environment: Some(env::var("SENTRY_ENV").unwrap_or_else(|_| "development".to_string()).into()),
        ..Default::default()
    });

    let service_address = "0.0.0.0:8080";

    /*
     * We must specify a configuration with a bind address.  We'll use 127.0.0.1
     * since it's available and won't expose this server outside the host.  We
     * request port 8080.
     */
    let config_dropshot = ConfigDropshot {
        bind_address: service_address.parse().unwrap(),
        request_body_max_bytes: 100000000,
    };

    /*
     * For simplicity, we'll configure an "info"-level logger that writes to
     * stderr assuming that it's a terminal.
     */
    let config_logging = ConfigLogging::StderrTerminal { level: ConfigLoggingLevel::Info };
    let log = config_logging.to_logger("printy-server").map_err(|error| format!("failed to create logger: {}", error)).unwrap();

    // Describe the API.
    let mut api = ApiDescription::new();
    /*
     * Register our endpoint and its handler function.  The "endpoint" macro
     * specifies the HTTP method and URI path that identify the endpoint,
     * allowing this metadata to live right alongside the handler function.
     */
    api.register(ping).unwrap();
    api.register(listen_print_requests).unwrap();

    let mut api_definition = &mut api.openapi(&"Print API", &"0.0.1");
    api_definition = api_definition
        .description("Internal API server for printing shipping labels on a Rollo printer")
        .contact_url("https://oxide.computer")
        .contact_email("printy@oxide.computer");
    let api_file = "openapi-printy.json";
    println!("Writing OpenAPI spec to {}...", api_file);
    let mut buffer = File::create(api_file).unwrap();
    let schema = api_definition.json().unwrap().to_string();
    api_definition.write(&mut buffer).unwrap();

    /*
     * The functions that implement our API endpoints will share this context.
     */
    let api_context = Context::new(schema).await;

    /*
     * Set up the server.
     */
    let server = HttpServerStarter::new(&config_dropshot, api, api_context, &log)
        .map_err(|error| format!("failed to start server: {}", error))
        .unwrap()
        .start();
    server.await
}

/**
 * Application-specific context (state shared by handler functions)
 */
struct Context {
    schema: String,
}

impl Context {
    /**
     * Return a new Context.
     */
    pub async fn new(schema: String) -> Context {
        Context { schema }
    }
}

/*
 * HTTP API interface
 */

/**
 * Return the OpenAPI schema in JSON format.
 */
#[endpoint {
    method = GET,
    path = "/",
}]
async fn api_get_schema(rqctx: Arc<RequestContext<Context>>) -> Result<HttpResponseOk<String>, HttpError> {
    let api_context = rqctx.context();

    Ok(HttpResponseOk(api_context.schema.to_string()))
}

/** Return pong. */
#[endpoint {
    method = GET,
    path = "/ping",
}]
async fn ping(_rqctx: Arc<RequestContext<Context>>) -> Result<HttpResponseOk<String>, HttpError> {
    Ok(HttpResponseOk("pong".to_string()))
}

/** Listen for GitHub webhooks. */
#[endpoint {
    method = POST,
    path = "/print",
}]
async fn listen_print_requests(_rqctx: Arc<RequestContext<Context>>, body_param: TypedBody<String>) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();
    let url = body_param.into_inner();
    let printer = get_rollo_printer();
    println!("{:?}", printer);

    // Save the contents of our URL to a file.
    let file = save_url_to_file(url).await;

    // Print the file.
    print_file(&printer, &file);

    // Print the body to the rollo printer.
    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

// Return our rollo printer.
fn get_rollo_printer() -> String {
    let output = Command::new("lpstat").args(&["-a"]).output().expect("failed to execute process");
    if !output.status.success() {
        let e = format!("[lpstat] stderr: {}\nstdout: {}", from_utf8(&output.stderr).unwrap(), from_utf8(&output.stdout).unwrap());
        println!("{}", e);
        sentry::capture_message(&e, sentry::Level::Fatal);
        return "".to_string();
    }

    let os = from_utf8(&output.stdout).unwrap();
    let printers = os.trim().split('\n');
    for printer in printers {
        if printer.to_lowercase().contains("rollo") {
            let (p, _r) = printer.split_once(' ').unwrap();
            return p.to_string();
        }
    }

    "".to_string()
}

// Save URL contents to a temporary file.
// Returns the filepath.
async fn save_url_to_file(url: String) -> String {
    println!("Getting contents of URL `{}` to print", url);
    let body = reqwest::get(&url).await.unwrap().bytes().await.unwrap();

    let mut dir = env::temp_dir();
    let file_name = format!("{}.pdf", Uuid::new_v4());
    dir.push(file_name);

    let mut file = File::create(&dir).unwrap();
    file.write_all(&body).unwrap();

    let path = dir.to_str().unwrap().to_string();
    println!("Saved contents of URL to `{}`", path);

    path
}

// Save URL contents to a temporary file.
// Returns the filepath.
fn print_file(printer: &str, file: &str) {
    println!("Sending file `{}` to printer `{}`", file, printer);
    let output = Command::new("lp").args(&["-d", printer, "-o", "media=4.00x6.00\"", file]).output().expect("failed to execute process");
    if !output.status.success() {
        let e = format!("[lpstat] stderr: {}\nstdout: {}", from_utf8(&output.stderr).unwrap(), from_utf8(&output.stdout).unwrap());
        println!("{}", e);
        sentry::capture_message(&e, sentry::Level::Fatal);
        return;
    }

    println!("Printing: {}", from_utf8(&output.stdout).unwrap());
}
