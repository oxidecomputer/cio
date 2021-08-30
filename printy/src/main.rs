use std::{env, fs::File, io::Write, process::Command, str::from_utf8, sync::Arc};

use cio_api::swag_inventory::PrintRequest;
use dropshot::{
    endpoint, ApiDescription, ConfigDropshot, ConfigLogging, ConfigLoggingLevel, HttpError, HttpResponseAccepted,
    HttpResponseOk, HttpServerStarter, RequestContext, TypedBody,
};
use sentry::IntoDsn;
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<(), String> {
    // Try to get the current git hash.
    let git_hash = if let Ok(gh) = env::var("GIT_HASH") {
        gh
    } else {
        // Try to shell out.
        let output = Command::new("git")
            .arg("rev-parse")
            .arg("HEAD")
            .output()
            .expect("failed to execute process");
        let o = std::str::from_utf8(&output.stdout).unwrap();
        o[0..8].to_string()
    };
    println!("git hash: {}", git_hash);

    // Initialize sentry.
    let sentry_dsn = env::var("PRINTY_SENTRY_DSN").unwrap_or_default();
    let _guard = sentry::init(sentry::ClientOptions {
        dsn: sentry_dsn.into_dsn().unwrap(),

        release: Some(git_hash.into()),
        environment: Some(
            env::var("SENTRY_ENV")
                .unwrap_or_else(|_| "development".to_string())
                .into(),
        ),
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
    let config_logging = ConfigLogging::StderrTerminal {
        level: ConfigLoggingLevel::Info,
    };
    let log = config_logging
        .to_logger("printy-server")
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
    api.register(listen_print_receipt_requests).unwrap();
    api.register(listen_print_rollo_requests).unwrap();
    api.register(listen_print_zebra_requests).unwrap();

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

/** Listen for print requests for the Rollo label printer */
#[endpoint {
    method = POST,
    path = "/print/rollo",
}]
async fn listen_print_rollo_requests(
    _rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<String>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();
    let url = body_param.into_inner();
    let printer = get_printer("rollo");
    println!("{:?}", printer);

    if !url.trim().is_empty() {
        // Save the contents of our URL to a file.
        let file = save_url_to_file(&url, "pdf").await;

        // Print the file.
        print_file(&printer, &file, "4.00x6.00", 1);
    }

    // Print the body to the rollo printer.
    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/** Listen for print requests for the Zebra label printer */
#[endpoint {
    method = POST,
    path = "/print/zebra",
}]
async fn listen_print_zebra_requests(
    _rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<PrintRequest>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();
    let r = body_param.into_inner();
    let printer = get_printer("zebra");
    println!("{:?}", printer);

    if !r.url.trim().is_empty() && r.quantity > 0 {
        // Save the contents of our URL to a file.
        let file = save_url_to_file(&r.url, "pdf").await;

        // Print the file.
        print_file(&printer, &file, "2.00x1.33", r.quantity);
    }

    // Print the body to the rollo printer.
    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/** Listen for print requests for the receipt printer */
#[endpoint {
    method = POST,
    path = "/print/receipt",
}]
async fn listen_print_receipt_requests(
    _rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<PrintRequest>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();
    let r = body_param.into_inner();
    let printer = get_printer("receipt");
    println!("{:?}", printer);

    if !r.content.trim().is_empty() && r.quantity > 0 {
        // Save the contents of our URL to a file.
        let file = save_content_to_file(r.content.as_bytes(), "txt");

        // Print the file.
        print_file(&printer, &file, "", r.quantity);
    }

    // Print the body to the rollo printer.
    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

// Return the printer we are looking for.
fn get_printer(name: &str) -> String {
    let output = Command::new("lpstat")
        .args(&["-a"])
        .output()
        .expect("failed to execute process");
    if !output.status.success() {
        let e = format!(
            "[lpstat] stderr: {}\nstdout: {}",
            from_utf8(&output.stderr).unwrap(),
            from_utf8(&output.stdout).unwrap()
        );
        println!("{}", e);
        sentry::capture_message(&e, sentry::Level::Fatal);
        return "".to_string();
    }

    let os = from_utf8(&output.stdout).unwrap();
    let printers = os.trim().split('\n');
    for printer in printers {
        if printer.to_lowercase().contains(name) {
            let (p, _r) = printer.split_once(' ').unwrap();
            return p.to_string();
        }
    }

    "".to_string()
}

// Save URL contents to a temporary file.
// Returns the filepath.
async fn save_url_to_file(url: &str, ext: &str) -> String {
    println!("Getting contents of URL `{}` to print", url);
    let body = reqwest::get(url).await.unwrap().bytes().await.unwrap();

    save_content_to_file(&body, ext)
}

// Save content to a temporary file.
// Returns the filepath.
fn save_content_to_file(body: &[u8], ext: &str) -> String {
    let mut dir = env::temp_dir();
    let file_name = format!("{}.{}", Uuid::new_v4(), ext);
    dir.push(file_name);

    let mut file = File::create(&dir).unwrap();
    file.write_all(body).unwrap();

    let path = dir.to_str().unwrap().to_string();
    println!("Saved contents of URL to `{}`", path);

    path
}

// Save URL contents to a temporary file.
// Returns the filepath.
fn print_file(printer: &str, file: &str, media: &str, copies: i32) {
    println!("Sending file `{}` to printer `{}`", file, printer);
    let output = if !media.is_empty() {
        Command::new("lp")
            .args(&[
                "-d",
                printer,
                "-n",
                &format!("{}", copies),
                "-o",
                "fit-to-page",
                "-o",
                &format!("media={}\"", media),
                "-o",
                "page-left=0",
                "-o",
                "page-right=0",
                "-o",
                "page-top=0",
                "-o",
                "page-bottom=0",
                file,
            ])
            .output()
            .expect("failed to execute process")
    } else {
        Command::new("lp")
            .args(&["-d", printer, "-n", &format!("{}", copies), file])
            .output()
            .expect("failed to execute process")
    };
    if !output.status.success() {
        let e = format!(
            "[lpstat] stderr: {}\nstdout: {}",
            from_utf8(&output.stderr).unwrap(),
            from_utf8(&output.stdout).unwrap()
        );
        println!("{}", e);
        sentry::capture_message(&e, sentry::Level::Fatal);
        return;
    }

    println!("Printing: {}", from_utf8(&output.stdout).unwrap());
}
