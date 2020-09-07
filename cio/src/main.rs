use std::any::Any;
use std::collections::BTreeMap;
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
    get_configs_from_repo, BuildingConfig, Config, GroupConfig, LabelConfig,
    LinkConfig, ResourceConfig, UserConfig,
};
use cio_api::journal_clubs::get_meetings_from_repo;
use cio_api::models::{
    JournalClubMeeting, NewApplicant as Applicant,
    NewMailingListSubscriber as MailingListSubscriber, NewRFD as RFD, Repo,
};
use cio_api::utils::{authenticate_github, list_all_github_repos};

#[tokio::main]
async fn main() -> Result<(), String> {
    /*
     * We must specify a configuration with a bind address.  We'll use 127.0.0.1
     * since it's available and won't expose this server outside the host.  We
     * request port 0, which allows the operating system to pick any available
     * port.
     */
    let config_dropshot = ConfigDropshot {
        bind_address: "127.0.0.1:8888".parse().unwrap(),
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

    // A cache of our applicants that we will continuously update.
    applicants: Vec<Applicant>,
    // A cache of our configs that we will continuously update.
    configs: Config,
    // A cache of journal club meetings that we will continuously update.
    journal_club_meetings: Vec<JournalClubMeeting>,
    // A cache of mailing list subscribers that we will continuously update.
    mailing_list_subscribers: Vec<MailingListSubscriber>,
    // A cache of our repos that we will continuously update.
    repos: Vec<Repo>,
    // A cache of our RFDs that we will continuously update.
    rfds: BTreeMap<i32, RFD>,
}

impl Context {
    /**
     * Return a new Context.
     */
    pub async fn new(github: Github) -> Arc<Context> {
        let mut api_context = Context {
            github,
            configs: Default::default(),
            applicants: Default::default(),
            journal_club_meetings: Default::default(),
            mailing_list_subscribers: Default::default(),
            repos: Default::default(),
            rfds: Default::default(),
        };

        // Refresh our context.
        api_context.refresh().await;

        Arc::new(api_context)
    }

    pub async fn refresh(&mut self) {
        println!("Refreshing cache of applicants...");
        // TODO: make this real
        let applicants = Default::default();
        self.applicants = applicants;

        println!("Refreshing cache of configs...");
        let configs = get_configs_from_repo(&self.github).await;
        self.configs = configs;

        println!("Refreshing cache of journal club meetings...");
        let journal_club_meetings = get_meetings_from_repo(&self.github).await;
        self.journal_club_meetings = journal_club_meetings;

        println!("Refreshing cache of mailing list subscribers...");
        // TODO: make this real
        let mailing_list_subscribers = Default::default();
        self.mailing_list_subscribers = mailing_list_subscribers;

        println!("Refreshing cache of GitHub repos...");
        let repos = list_all_github_repos(&self.github).await;
        self.repos = repos;

        println!("Refreshing cache of RFDs...");
        // TODO: make this real
        let rfds = Default::default();
        self.rfds = rfds;
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
 * Fetch all applicants.
 */
#[endpoint {
    method = GET,
    path = "/applicants",
}]
async fn api_get_applicants(
    rqctx: Arc<RequestContext>,
) -> Result<HttpResponseOk<Vec<Applicant>>, HttpError> {
    let api_context = Context::from_rqctx(&rqctx);

    Ok(HttpResponseOk(api_context.applicants.clone()))
}

/**
 * Fetch a list of office buildings.
 */
#[endpoint {
    method = GET,
    path = "/buildings",
}]
async fn api_get_buildings(
    rqctx: Arc<RequestContext>,
) -> Result<HttpResponseOk<BTreeMap<String, BuildingConfig>>, HttpError> {
    let api_context = Context::from_rqctx(&rqctx);

    Ok(HttpResponseOk(api_context.configs.buildings.clone()))
}

/**
 * Fetch a list of conference rooms.
 */
#[endpoint {
    method = GET,
    path = "/conferenceRooms",
}]
async fn api_get_conference_rooms(
    rqctx: Arc<RequestContext>,
) -> Result<HttpResponseOk<BTreeMap<String, ResourceConfig>>, HttpError> {
    let api_context = Context::from_rqctx(&rqctx);

    Ok(HttpResponseOk(api_context.configs.resources.clone()))
}

/**
 * Fetch a list of our GitHub labels that get added to all repositories.
 */
#[endpoint {
    method = GET,
    path = "/github/labels",
}]
async fn api_get_github_labels(
    rqctx: Arc<RequestContext>,
) -> Result<HttpResponseOk<Vec<LabelConfig>>, HttpError> {
    let api_context = Context::from_rqctx(&rqctx);

    Ok(HttpResponseOk(api_context.configs.labels.clone()))
}

/**
 * Fetch a list of our GitHub repositories.
 */
#[endpoint {
    method = GET,
    path = "/github/repos",
}]
async fn api_get_github_repos(
    rqctx: Arc<RequestContext>,
) -> Result<HttpResponseOk<Vec<Repo>>, HttpError> {
    let api_context = Context::from_rqctx(&rqctx);

    Ok(HttpResponseOk(api_context.repos.clone()))
}

/**
 * Fetch a list of Google groups.
 */
#[endpoint {
    method = GET,
    path = "/groups",
}]
async fn api_get_groups(
    rqctx: Arc<RequestContext>,
) -> Result<HttpResponseOk<BTreeMap<String, GroupConfig>>, HttpError> {
    let api_context = Context::from_rqctx(&rqctx);

    Ok(HttpResponseOk(api_context.configs.groups.clone()))
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
    rqctx: Arc<RequestContext>,
) -> Result<HttpResponseOk<BTreeMap<String, LinkConfig>>, HttpError> {
    let api_context = Context::from_rqctx(&rqctx);

    Ok(HttpResponseOk(api_context.configs.links.clone()))
}

/**
 * Fetch a list of mailing list subscribers.
 */
#[endpoint {
    method = GET,
    path = "/mailingListSubscribers",
}]
async fn api_get_mailing_list_subscribers(
    rqctx: Arc<RequestContext>,
) -> Result<HttpResponseOk<Vec<MailingListSubscriber>>, HttpError> {
    let api_context = Context::from_rqctx(&rqctx);

    Ok(HttpResponseOk(api_context.mailing_list_subscribers.clone()))
}

/**
 * Fetch all RFDs.
 */
#[endpoint {
    method = GET,
    path = "/rfds",
}]
async fn api_get_rfds(
    rqctx: Arc<RequestContext>,
) -> Result<HttpResponseOk<BTreeMap<i32, RFD>>, HttpError> {
    let api_context = Context::from_rqctx(&rqctx);

    Ok(HttpResponseOk(api_context.rfds.clone()))
}

/**
 * Fetch a list of employees.
 */
#[endpoint {
    method = GET,
    path = "/users",
}]
async fn api_get_users(
    rqctx: Arc<RequestContext>,
) -> Result<HttpResponseOk<BTreeMap<String, UserConfig>>, HttpError> {
    let api_context = Context::from_rqctx(&rqctx);

    Ok(HttpResponseOk(api_context.configs.users.clone()))
}
