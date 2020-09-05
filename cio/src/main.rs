/* TODO: make these all private once we use this API for everything */
pub mod applicants;
pub mod journal_clubs;
pub mod mailing_list;
pub mod rfds;
pub mod slack;
pub mod utils;

#[macro_use]
extern crate serde_json;

use std::any::Any;
use std::collections::BTreeMap;
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

use crate::applicants::{get_all_applicants_from_cache, ApplicantFields};
use crate::journal_clubs::{get_meetings_from_repo, Meeting};
use crate::mailing_list::{get_all_subscribers, Signup};
use crate::rfds::{get_rfds_from_repo, RFD};
use crate::utils::{authenticate_github, list_all_github_repos, Repo};

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
    api.register(api_get_journal_club_meetings).unwrap();
    api.register(api_get_mailing_list_subscribers).unwrap();
    api.register(api_get_repos).unwrap();
    // TODO: actually parse the RFD like we do in the shared website javascript.
    api.register(api_get_rfds).unwrap();

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
    applicants: Vec<ApplicantFields>,
    // A cache of journal club meetings that we will continuously update.
    journal_club_meetings: Vec<Meeting>,
    // A cache of mailing list subscribers that we will continuously update.
    mailing_list_subscribers: Vec<Signup>,
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
        let applicants = get_all_applicants_from_cache().await;
        self.applicants = applicants;

        println!("Refreshing cache of journal club meetings...");
        let journal_club_meetings = get_meetings_from_repo(&self.github).await;
        self.journal_club_meetings = journal_club_meetings;

        println!("Refreshing cache of mailing list subscribers...");
        let mailing_list_subscribers = get_all_subscribers().await;
        self.mailing_list_subscribers = mailing_list_subscribers;

        println!("Refreshing cache of GitHub repos...");
        let repos = list_all_github_repos(&self.github).await;
        self.repos = repos;

        println!("Refreshing cache of RFDs...");
        let rfds = get_rfds_from_repo(&self.github).await;
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
) -> Result<HttpResponseOk<Vec<ApplicantFields>>, HttpError> {
    let api_context = Context::from_rqctx(&rqctx);

    Ok(HttpResponseOk(api_context.applicants.clone()))
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
) -> Result<HttpResponseOk<Vec<Meeting>>, HttpError> {
    let api_context = Context::from_rqctx(&rqctx);

    Ok(HttpResponseOk(api_context.journal_club_meetings.clone()))
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
) -> Result<HttpResponseOk<Vec<Signup>>, HttpError> {
    let api_context = Context::from_rqctx(&rqctx);

    Ok(HttpResponseOk(api_context.mailing_list_subscribers.clone()))
}

/**
 * Fetch a list of our GitHub repositories.
 */
#[endpoint {
    method = GET,
    path = "/repos",
}]
async fn api_get_repos(
    rqctx: Arc<RequestContext>,
) -> Result<HttpResponseOk<Vec<Repo>>, HttpError> {
    let api_context = Context::from_rqctx(&rqctx);

    Ok(HttpResponseOk(api_context.repos.clone()))
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
