use std::any::Any;
use std::fs::File;
use std::sync::Arc;

use dropshot::endpoint;
use dropshot::ApiDescription;
use dropshot::ConfigDropshot;
use dropshot::ConfigLogging;
use dropshot::ConfigLoggingLevel;
use dropshot::HttpError;
use dropshot::HttpResponseOk;
use dropshot::HttpServer;
use dropshot::RequestContext;
use hubcaps::Github;

use cio_api::configs::{
    Building, ConferenceRoom, GithubLabel, Group, Link, User,
};
use cio_api::db::Database;
use cio_api::journal_clubs::get_meetings_from_repo;
use cio_api::models::{
    Applicant, AuthLogin, GithubRepo, JournalClubMeeting,
    MailingListSubscriber, RFD,
};
use cio_api::utils::authenticate_github;

#[tokio::main]
async fn main() -> Result<(), String> {
    /*
     * We must specify a configuration with a bind address.  We'll use 127.0.0.1
     * since it's available and won't expose this server outside the host.  We
     * request port 0, which allows the operating system to pick any available
     * port.
     */
    let config_dropshot = ConfigDropshot {
        bind_address: "0.0.0.0:8888".parse().unwrap(),
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
    api.register(api_get_users).unwrap();

    // Print the OpenAPI Spec to stdout.
    println!("Writing OpenAPI spec to openapi-cio.json...");
    let mut buffer = File::create("openapi-cio.json").unwrap();
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

    /*
     * The functions that implement our API endpoints will share this context.
     */
    let github = authenticate_github();
    let api_context = Context::new(github).await;

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
    // A GitHub client.
    github: Github,

    // A cache of journal club meetings that we will continuously update.
    journal_club_meetings: Vec<JournalClubMeeting>,
}

impl Context {
    /**
     * Return a new Context.
     */
    pub async fn new(github: Github) -> Arc<Context> {
        let mut api_context = Context {
            github,
            journal_club_meetings: Default::default(),
        };

        // Refresh our context.
        api_context.refresh().await;

        Arc::new(api_context)
    }

    pub async fn refresh(&mut self) {
        println!("Refreshing cache of journal club meetings...");
        let journal_club_meetings = get_meetings_from_repo(&self.github).await;
        self.journal_club_meetings = journal_club_meetings;
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
 * Fetch all auth users.
 */
#[endpoint {
    method = GET,
    path = "/auth/users",
}]
async fn api_get_auth_users(
    _rqctx: Arc<RequestContext>,
) -> Result<HttpResponseOk<Vec<AuthLogin>>, HttpError> {
    // TODO: figure out how to share this between threads.
    let db = Database::new();

    Ok(HttpResponseOk(db.get_auth_logins()))
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
    path = "/conferenceRooms",
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
    path = "/journalClubMeetings",
}]
async fn api_get_journal_club_meetings(
    rqctx: Arc<RequestContext>,
) -> Result<HttpResponseOk<Vec<JournalClubMeeting>>, HttpError> {
    let api_context = Context::from_rqctx(&rqctx);

    Ok(HttpResponseOk(api_context.journal_club_meetings.clone()))
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
    path = "/mailingListSubscribers",
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
