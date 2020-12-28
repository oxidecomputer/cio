#![allow(clippy::field_reassign_with_default)]
#![feature(str_split_once)]

use std::env;
use std::error::Error;
use std::fs::File;
use std::io::Write;
use std::process::Command;
use std::str::from_utf8;
use std::sync::Arc;

use dropshot::{endpoint, ApiDescription, ConfigDropshot, ConfigLogging, ConfigLoggingLevel, HttpError, HttpResponseAccepted, HttpResponseOk, HttpServer, RequestContext, TypedBody};
use tracing::{instrument, span, Level};
use tracing_subscriber::prelude::*;
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
    let service_address = "0.0.0.0:8080";

    // Set up tracing.
    let (tracer, _uninstall) = opentelemetry_zipkin::new_pipeline()
        .with_service_name("printy")
        .with_collector_endpoint("https://ingest.lightstep.com:443/api/v2/spans")
        .with_trace_config(
            opentelemetry::sdk::trace::config()
                .with_default_sampler(opentelemetry::sdk::trace::Sampler::AlwaysOn)
                .with_resource(opentelemetry::sdk::Resource::new(vec![
                    opentelemetry::KeyValue::new("lightstep.service_name", "printy"),
                    opentelemetry::KeyValue::new("lightstep.access_token", env::var("LIGHTSTEP_ACCESS_TOKEN").unwrap_or_default()),
                ])),
        )
        .install()
        .unwrap();
    let opentelemetry = tracing_opentelemetry::layer().with_tracer(tracer);
    let subscriber = tracing_subscriber::Registry::default().with(opentelemetry);
    tracing::subscriber::set_global_default(subscriber).expect("setting tracing default failed");

    let root = span!(Level::TRACE, "app_start", work_units = 2);
    let _enter = root.enter();

    /*
     * We must specify a configuration with a bind address.  We'll use 127.0.0.1
     * since it's available and won't expose this server outside the host.  We
     * request port 8080.
     */
    let config_dropshot = ConfigDropshot {
        bind_address: service_address.parse().unwrap(),
        request_body_max_bytes: dropshot::RequestBodyMaxBytes(100000000),
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
    server.wait_for_shutdown(server_task).await.unwrap();
    Ok(())
}

/**
 * Application-specific context (state shared by handler functions)
 */
struct Context {}

impl Context {
    /**
     * Return a new Context.
     */
    pub async fn new() -> Arc<Context> {
        // Create the context.
        Arc::new(Context {})
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
#[instrument]
#[inline]
async fn ping(_rqctx: Arc<RequestContext>) -> Result<HttpResponseOk<String>, HttpError> {
    Ok(HttpResponseOk("pong".to_string()))
}

/** Listen for GitHub webhooks. */
#[endpoint {
    method = POST,
    path = "/print",
}]
#[instrument]
#[inline]
async fn listen_print_requests(_rqctx: Arc<RequestContext>, body_param: TypedBody<String>) -> Result<HttpResponseAccepted<String>, HttpError> {
    let url = body_param.into_inner();
    let printer = get_rollo_printer();
    println!("{:?}", printer);

    // Save the contents of our URL to a file.
    let file = save_url_to_file(url).await;

    // Print the file.
    print_file(&printer, &file);

    // Print the body to the rollo printer.
    Ok(HttpResponseAccepted("ok".to_string()))
}

// Return our rollo printer.
#[instrument]
#[inline]
fn get_rollo_printer() -> String {
    let output = Command::new("lpstat").args(&["-a"]).output().expect("failed to execute process");
    if !output.status.success() {
        println!("[lpstat] stderr: {}\nstdout: {}", from_utf8(&output.stderr).unwrap(), from_utf8(&output.stdout).unwrap());
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
#[instrument]
#[inline]
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

// Print the file.
fn print_file(printer: &str, file: &str) {
    println!("Sending file `{}` to printer `{}`", file, printer);
    let output = Command::new("lp").args(&["-d", printer, "-o", "media=4.00x6.00\"", file]).output().expect("failed to execute process");
    if !output.status.success() {
        println!("[lpstat] stderr: {}\nstdout: {}", from_utf8(&output.stderr).unwrap(), from_utf8(&output.stdout).unwrap());
        return;
    }

    println!("Printing: {}", from_utf8(&output.stdout).unwrap());
}
