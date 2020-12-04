use std::any::Any;
use std::env;
use std::fs::File;
use std::io::Read;
use std::sync::Arc;

use dropshot::{
    endpoint, ApiDescription, ConfigDropshot, ConfigLogging,
    ConfigLoggingLevel, HttpError, HttpResponseOk, HttpServer, RequestContext,
};
use hyper::{Body, Response, StatusCode};

use cio_api::configs::{
    Building, ConferenceRoom, GithubLabel, Group, Link, User,
};
use cio_api::db::Database;
use cio_api::models::{
    Applicant, AuthUser, GithubRepo, JournalClubMeeting, MailingListSubscriber,
    RFD,
};

#[macro_use]
extern crate serde_json;

#[tokio::main]
async fn main() -> Result<(), String> {
    /*
     * We must specify a configuration with a bind address.  We'll use 127.0.0.1
     * since it's available and won't expose this server outside the host.  We
     * request port 8888.
     */
    let config_dropshot = ConfigDropshot {
        bind_address: "0.0.0.0:8888".parse().unwrap(),
        request_body_max_bytes: dropshot::RequestBodyMaxBytes(100000000),
    };

    /*
     * For simplicity, we'll configure an "info"-level logger that writes to
     * stderr assuming that it's a terminal.
     */
    let config_logging = ConfigLogging::StderrTerminal {
        level: ConfigLoggingLevel::Info,
    };
    let log = config_logging
        .to_logger("cio-server")
        .map_err(|error| format!("failed to create logger: {}", error))
        .unwrap();

    /*
     * Build a description of the API.
     */
    let mut api = ApiDescription::new();
    api.register(api_get_applicants).unwrap();
    api.register(api_get_auth_users).unwrap();
    api.register(api_get_buildings).unwrap();
    api.register(api_get_conference_rooms).unwrap();
    api.register(api_get_github_labels).unwrap();
    api.register(api_get_github_repos).unwrap();
    api.register(api_get_groups).unwrap();
    api.register(api_get_journal_club_meetings).unwrap();
    api.register(api_get_links).unwrap();
    api.register(api_get_mailing_list_subscribers).unwrap();
    api.register(api_get_rfds).unwrap();
    api.register(api_get_schema).unwrap();
    api.register(api_get_users).unwrap();

    // Print the OpenAPI Spec to stdout.
    let api_file = "openapi-cio.json";
    let mut tmp_file = env::temp_dir();
    tmp_file.push("openapi-cio.json");
    println!("Writing OpenAPI spec to {}...", api_file);
    let mut buffer = File::create(tmp_file.clone()).unwrap();
    api.print_openapi(
        &mut buffer,
        &"CIO API",
        Some(&"API for interacting with the data our CIO bot handles"),
        None,
        Some(&"Jess Frazelle"),
        Some(&"https://oxide.computer"),
        Some(&"cio@oxide.computer"),
        None,
        None,
        &"0.0.1",
    )
    .unwrap();
    let mut f = File::open(tmp_file).unwrap();
    let mut api_schema = String::new();
    f.read_to_string(&mut api_schema).unwrap();
    let mut schema: openapiv3::OpenAPI =
        serde_json::from_str(&api_schema).unwrap();
    // Modify more of the schema.
    // TODO: make this cleaner when dropshot allows for it.
    schema.servers = vec![openapiv3::Server {
        url: "http://api.internal.oxide.computer".to_string(),
        description: Some("Hosted behind our VPN".to_string()),
        variables: None,
    }];
    schema.external_docs = Some(openapiv3::ExternalDocumentation{
        description: Some("Automatically updated documentation site, public, not behind the VPN.".to_string()),
        url: "https://api.docs.corp.oxide.computer".to_string(),
    });
    // Save it back to the file.
    serde_json::to_writer_pretty(&File::create(api_file).unwrap(), &schema)
        .unwrap();

    /*
     * The functions that implement our API endpoints will share this context.
     */
    let api_context = Context::new(schema).await;

    /*
     * Set up the server.
     */
    let mut server = HttpServer::new(&config_dropshot, api, api_context, &log)
        .map_err(|error| format!("failed to create server: {}", error))?;
    let server_task = server.run();

    /*
     * Wait for the server to stop.  Note that there's not any code to shut down
     * this server, so we should never get past this point.
     */
    server.wait_for_shutdown(server_task).await
}

/**
 * Application-specific context (state shared by handler functions)
 */
struct Context {
    // TODO: share a database connection here.
    schema: openapiv3::OpenAPI,
}

impl Context {
    /**
     * Return a new Context.
     */
    pub async fn new(schema: openapiv3::OpenAPI) -> Arc<Context> {
        let api_context = Context { schema };

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

/**
 * Return the OpenAPI schema in JSON format.
 */
#[endpoint {
    method = GET,
    path = "/",
}]
async fn api_get_schema(
    rqctx: Arc<RequestContext>,
) -> Result<Response<Body>, HttpError> {
    let api_context = Context::from_rqctx(&rqctx);

    Ok(Response::builder()
        .status(StatusCode::OK)
        .body(Body::from(json!(api_context.schema).to_string()))
        .unwrap())
}

/**
 * Fetch all auth users.
 */
#[endpoint {
    method = GET,
    path = "/auth/users",
}]
async fn api_get_auth_users(
    _rqctx: Arc<RequestContext>,
) -> Result<HttpResponseOk<Vec<AuthUser>>, HttpError> {
    // TODO: figure out how to share this between threads.
    let db = Database::new();

    Ok(HttpResponseOk(db.get_auth_users()))
}

/**
 * Fetch all applicants.
 */
#[endpoint {
    method = GET,
    path = "/applicants",
}]
async fn api_get_applicants(
    _rqctx: Arc<RequestContext>,
) -> Result<HttpResponseOk<Vec<Applicant>>, HttpError> {
    let db = Database::new();

    Ok(HttpResponseOk(db.get_applicants()))
}

/**
 * Fetch a list of office buildings.
 */
#[endpoint {
    method = GET,
    path = "/buildings",
}]
async fn api_get_buildings(
    _rqctx: Arc<RequestContext>,
) -> Result<HttpResponseOk<Vec<Building>>, HttpError> {
    let db = Database::new();

    Ok(HttpResponseOk(db.get_buildings()))
}

/**
 * Fetch a list of conference rooms.
 */
#[endpoint {
    method = GET,
    path = "/conference_rooms",
}]
async fn api_get_conference_rooms(
    _rqctx: Arc<RequestContext>,
) -> Result<HttpResponseOk<Vec<ConferenceRoom>>, HttpError> {
    let db = Database::new();

    Ok(HttpResponseOk(db.get_conference_rooms()))
}

/**
 * Fetch a list of our GitHub labels that get added to all repositories.
 */
#[endpoint {
    method = GET,
    path = "/github/labels",
}]
async fn api_get_github_labels(
    _rqctx: Arc<RequestContext>,
) -> Result<HttpResponseOk<Vec<GithubLabel>>, HttpError> {
    let db = Database::new();

    Ok(HttpResponseOk(db.get_github_labels()))
}

/**
 * Fetch a list of our GitHub repositories.
 */
#[endpoint {
    method = GET,
    path = "/github/repos",
}]
async fn api_get_github_repos(
    _rqctx: Arc<RequestContext>,
) -> Result<HttpResponseOk<Vec<GithubRepo>>, HttpError> {
    let db = Database::new();

    Ok(HttpResponseOk(db.get_github_repos()))
}

/**
 * Fetch a list of Google groups.
 */
#[endpoint {
    method = GET,
    path = "/groups",
}]
async fn api_get_groups(
    _rqctx: Arc<RequestContext>,
) -> Result<HttpResponseOk<Vec<Group>>, HttpError> {
    let db = Database::new();

    Ok(HttpResponseOk(db.get_groups()))
}

/**
 * Fetch a list of journal club meetings.
 */
#[endpoint {
    method = GET,
    path = "/journal_club_meetings",
}]
async fn api_get_journal_club_meetings(
    _rqctx: Arc<RequestContext>,
) -> Result<HttpResponseOk<Vec<JournalClubMeeting>>, HttpError> {
    let db = Database::new();

    Ok(HttpResponseOk(db.get_journal_club_meetings()))
}

/**
 * Fetch a list of internal links.
 */
#[endpoint {
    method = GET,
    path = "/links",
}]
async fn api_get_links(
    _rqctx: Arc<RequestContext>,
) -> Result<HttpResponseOk<Vec<Link>>, HttpError> {
    let db = Database::new();

    Ok(HttpResponseOk(db.get_links()))
}

/**
 * Fetch a list of mailing list subscribers.
 */
#[endpoint {
    method = GET,
    path = "/mailing_list_subscribers",
}]
async fn api_get_mailing_list_subscribers(
    _rqctx: Arc<RequestContext>,
) -> Result<HttpResponseOk<Vec<MailingListSubscriber>>, HttpError> {
    let db = Database::new();

    Ok(HttpResponseOk(db.get_mailing_list_subscribers()))
}

/**
 * Fetch all RFDs.
 */
#[endpoint {
    method = GET,
    path = "/rfds",
}]
async fn api_get_rfds(
    _rqctx: Arc<RequestContext>,
) -> Result<HttpResponseOk<Vec<RFD>>, HttpError> {
    let db = Database::new();

    Ok(HttpResponseOk(db.get_rfds()))
}

/**
 * Fetch a list of employees.
 */
#[endpoint {
    method = GET,
    path = "/users",
}]
async fn api_get_users(
    _rqctx: Arc<RequestContext>,
) -> Result<HttpResponseOk<Vec<User>>, HttpError> {
    let db = Database::new();

    Ok(HttpResponseOk(db.get_users()))
}
