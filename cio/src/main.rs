#![recursion_limit = "256"]
use std::{fs::File, sync::Arc};

use cio_api::{
    applicants::{Applicant, Applicants},
    auth_logins::{AuthUser, AuthUsers},
    configs::{Building, Buildings, ConferenceRoom, ConferenceRooms, Group, Groups, Link, Links, User, Users},
    db::Database,
    journal_clubs::{JournalClubMeeting, JournalClubMeetings},
    mailing_list::{MailingListSubscriber, MailingListSubscribers},
    repos::{GithubRepo, GithubRepos},
    rfds::{RFDs, RFD},
};
use dropshot::{
    endpoint, ApiDescription, ConfigDropshot, ConfigLogging, ConfigLoggingLevel, HttpError, HttpResponseOk,
    HttpServerStarter, RequestContext,
};

#[tokio::main]
async fn main() -> Result<(), String> {
    let service_address = "0.0.0.0:8888";

    /*
     * We must specify a configuration with a bind address.  We'll use 127.0.0.1
     * since it's available and won't expose this server outside the host.  We
     * request port 8888.
     */
    let config_dropshot = ConfigDropshot {
        bind_address: service_address.parse().unwrap(),
        request_body_max_bytes: 100000000,
        tls: None,
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
    api.register(api_get_github_repos).unwrap();
    api.register(api_get_groups).unwrap();
    api.register(api_get_journal_club_meetings).unwrap();
    api.register(api_get_links).unwrap();
    api.register(api_get_mailing_list_subscribers).unwrap();
    api.register(api_get_rfds).unwrap();
    api.register(api_get_schema).unwrap();
    api.register(api_get_users).unwrap();

    // Print the OpenAPI Spec to stdout.
    let mut api_definition = &mut api.openapi(&"CIO API", &"0.0.1");
    api_definition = api_definition
        .description("Internal API server for information about the company, employess, etc")
        .contact_url("https://oxide.computer")
        .contact_email("cio@oxide.computer");
    let api_file = "openapi-cio.json";
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
        .map_err(|error| format!("failed to start server: {}", error))?
        .start();
    server.await
}

/**
 * Application-specific context (state shared by handler functions)
 */
struct Context {
    db: Database,
    schema: String,
}

impl Context {
    /**
     * Return a new Context.
     */
    pub async fn new(schema: String) -> Context {
        Context {
            schema,
            db: Database::new().await,
        }
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

/**
 * Fetch all auth users.
 */
#[endpoint {
    method = GET,
    path = "/auth/users",
}]
async fn api_get_auth_users(rqctx: Arc<RequestContext<Context>>) -> Result<HttpResponseOk<Vec<AuthUser>>, HttpError> {
    let api_context = rqctx.context();
    let db = &api_context.db;

    Ok(HttpResponseOk(AuthUsers::get_from_db(db, 1).unwrap().0))
}

/**
 * Fetch all applicants.
 */
#[endpoint {
    method = GET,
    path = "/applicants",
}]
async fn api_get_applicants(rqctx: Arc<RequestContext<Context>>) -> Result<HttpResponseOk<Vec<Applicant>>, HttpError> {
    let api_context = rqctx.context();
    let db = &api_context.db;

    Ok(HttpResponseOk(Applicants::get_from_db(db, 1).unwrap().0))
}

/**
 * Fetch a list of office buildings.
 */
#[endpoint {
    method = GET,
    path = "/buildings",
}]
async fn api_get_buildings(rqctx: Arc<RequestContext<Context>>) -> Result<HttpResponseOk<Vec<Building>>, HttpError> {
    let api_context = rqctx.context();
    let db = &api_context.db;

    Ok(HttpResponseOk(Buildings::get_from_db(db, 1).unwrap().0))
}

/**
 * Fetch a list of conference rooms.
 */
#[endpoint {
    method = GET,
    path = "/conference_rooms",
}]
#[inline]
async fn api_get_conference_rooms(
    rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseOk<Vec<ConferenceRoom>>, HttpError> {
    let api_context = rqctx.context();
    let db = &api_context.db;

    Ok(HttpResponseOk(ConferenceRooms::get_from_db(db, 1).unwrap().0))
}

/**
 * Fetch a list of our GitHub repositories.
 */
#[endpoint {
    method = GET,
    path = "/github/repos",
}]
async fn api_get_github_repos(
    rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseOk<Vec<GithubRepo>>, HttpError> {
    let api_context = rqctx.context();
    let db = &api_context.db;

    Ok(HttpResponseOk(GithubRepos::get_from_db(db, 1).unwrap().0))
}

/**
 * Fetch a list of Google groups.
 */
#[endpoint {
    method = GET,
    path = "/groups",
}]
async fn api_get_groups(rqctx: Arc<RequestContext<Context>>) -> Result<HttpResponseOk<Vec<Group>>, HttpError> {
    let api_context = rqctx.context();
    let db = &api_context.db;

    Ok(HttpResponseOk(Groups::get_from_db(db, 1).unwrap().0))
}

/**
 * Fetch a list of journal club meetings.
 */
#[endpoint {
    method = GET,
    path = "/journal_club_meetings",
}]
async fn api_get_journal_club_meetings(
    rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseOk<Vec<JournalClubMeeting>>, HttpError> {
    let api_context = rqctx.context();
    let db = &api_context.db;

    Ok(HttpResponseOk(JournalClubMeetings::get_from_db(db, 1).unwrap().0))
}

/**
 * Fetch a list of internal links.
 */
#[endpoint {
    method = GET,
    path = "/links",
}]
async fn api_get_links(rqctx: Arc<RequestContext<Context>>) -> Result<HttpResponseOk<Vec<Link>>, HttpError> {
    let api_context = rqctx.context();
    let db = &api_context.db;

    Ok(HttpResponseOk(Links::get_from_db(db, 1).unwrap().0))
}

/**
 * Fetch a list of mailing list subscribers.
 */
#[endpoint {
    method = GET,
    path = "/mailing_list_subscribers",
}]
async fn api_get_mailing_list_subscribers(
    rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseOk<Vec<MailingListSubscriber>>, HttpError> {
    let api_context = rqctx.context();
    let db = &api_context.db;

    Ok(HttpResponseOk(MailingListSubscribers::get_from_db(db, 1).unwrap().0))
}

/**
 * Fetch all RFDs.
 */
#[endpoint {
    method = GET,
    path = "/rfds",
}]
async fn api_get_rfds(rqctx: Arc<RequestContext<Context>>) -> Result<HttpResponseOk<Vec<RFD>>, HttpError> {
    let api_context = rqctx.context();
    let db = &api_context.db;

    Ok(HttpResponseOk(RFDs::get_from_db(db, 1).unwrap().0))
}

/**
 * Fetch a list of employees.
 */
#[endpoint {
    method = GET,
    path = "/users",
}]
async fn api_get_users(rqctx: Arc<RequestContext<Context>>) -> Result<HttpResponseOk<Vec<User>>, HttpError> {
    let api_context = rqctx.context();
    let db = &api_context.db;

    Ok(HttpResponseOk(Users::get_from_db(db, 1).unwrap().0))
}
