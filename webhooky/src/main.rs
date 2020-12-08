#![allow(clippy::field_reassign_with_default)]
pub mod event_types;
use crate::event_types::EventType;
pub mod influx;
#[macro_use]
extern crate serde_json;

use std::any::Any;
use std::collections::HashMap;
use std::convert::TryInto;
use std::env;
use std::error::Error;
use std::str::FromStr;
use std::sync::Arc;

use chrono::offset::Utc;
use chrono::{DateTime, TimeZone};
use chrono_humanize::HumanTime;
use dropshot::{endpoint, ApiDescription, ConfigDropshot, ConfigLogging, ConfigLoggingLevel, HttpError, HttpResponseAccepted, HttpResponseOk, HttpServer, Query, RequestContext, TypedBody};
use google_drive::GoogleDrive;
use hubcaps::Github;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sheets::Sheets;
use tracing::{instrument, span, Level};
use tracing_subscriber::prelude::*;

use cio_api::applicants::{email_send_new_applicant_notification, get_role_from_sheet_id};
use cio_api::db::Database;
use cio_api::mailing_list::MailchimpWebhook;
use cio_api::models::{GitHubUser, GithubRepo, NewApplicant, NewRFD};
use cio_api::slack::{get_hiring_channel_post_url, get_public_relations_channel_post_url, post_to_channel};
use cio_api::utils::{authenticate_github_jwt, get_gsuite_token, github_org};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
    let service_address = "0.0.0.0:8080";

    // Set up tracing.
    //let (tracer, _uninstall) = opentelemetry::exporter::trace::stdout::new_pipeline().install();
    let (tracer, _uninstall) = opentelemetry_zipkin::new_pipeline()
        .with_service_name("webhooky")
        .with_collector_endpoint("https://ingest.lightstep.com:443/api/v2/spans")
        .with_trace_config(
            opentelemetry::sdk::trace::config()
                .with_default_sampler(opentelemetry::sdk::trace::Sampler::AlwaysOn)
                .with_resource(opentelemetry::sdk::Resource::new(vec![
                    opentelemetry::KeyValue::new("lightstep.service_name", "webhooky"),
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
    let log = config_logging.to_logger("webhooky-server").map_err(|error| format!("failed to create logger: {}", error)).unwrap();

    // Describe the API.
    let mut api = ApiDescription::new();
    /*
     * Register our endpoint and its handler function.  The "endpoint" macro
     * specifies the HTTP method and URI path that identify the endpoint,
     * allowing this metadata to live right alongside the handler function.
     */
    api.register(ping).unwrap();
    api.register(github_rate_limit).unwrap();
    api.register(listen_google_sheets_edit_webhooks).unwrap();
    api.register(listen_google_sheets_row_create_webhooks).unwrap();
    api.register(listen_github_webhooks).unwrap();
    api.register(listen_mailchimp_webhooks).unwrap();
    api.register(ping_mailchimp_webhooks).unwrap();

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
struct Context {
    // TODO: share a database connection here.
    drive: GoogleDrive,
    drive_rfd_shared_id: String,
    drive_rfd_dir_id: String,
    github: Github,
    github_org: String,
    influx: influx::Client,
    sheets: Sheets,
}

impl Context {
    /**
     * Return a new Context.
     */
    pub async fn new() -> Arc<Context> {
        // Get gsuite token.
        let token = get_gsuite_token().await;

        // Initialize the GSuite sheets client.
        let sheets = Sheets::new(token.clone());

        // Initialize the Google Drive client.
        let drive = GoogleDrive::new(token);

        // Figure out where our directory is.
        // It should be in the shared drive : "Automated Documents"/"rfds"
        let shared_drive = drive.get_drive_by_name("Automated Documents").await.unwrap();
        let drive_rfd_shared_id = shared_drive.id.to_string();

        // Get the directory by the name.
        let drive_rfd_dir = drive.get_file_by_name(&drive_rfd_shared_id, "rfds").await.unwrap();

        // Create the context.
        Arc::new(Context {
            drive,
            drive_rfd_shared_id,
            drive_rfd_dir_id: drive_rfd_dir.get(0).unwrap().id.to_string(),
            github: authenticate_github_jwt(),
            github_org: github_org(),
            influx: influx::Client::new_from_env(),
            sheets,
        })
    }

    /**
     * Given `rqctx` (which is provided by Dropshot to all HTTP handler
     * functions), return our application-specific context.
     */
    pub fn from_rqctx(rqctx: &Arc<RequestContext>) -> Arc<Context> {
        let ctx: Arc<dyn Any + Send + Sync + 'static> = Arc::clone(&rqctx.server.private);
        ctx.downcast::<Context>().expect("wrong type for private data")
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
    path = "/github",
}]
#[instrument]
#[inline]
async fn listen_github_webhooks(rqctx: Arc<RequestContext>, body_param: TypedBody<GitHubWebhook>) -> Result<HttpResponseAccepted<String>, HttpError> {
    let api_context = Context::from_rqctx(&rqctx);
    let github_repo = api_context.github.repo(api_context.github_org.to_string(), "rfd");

    // TODO: share the database connection in the context.
    let db = Database::new();

    let event = body_param.into_inner();

    // Parse the `X-GitHub-Event` header.
    // TODO: make this nicer when supported as a first class method in dropshot.
    let req = rqctx.request.lock().await;
    let req_headers = req.headers();
    let event_type_string = req_headers
        .get("X-GitHub-Event")
        .unwrap_or(&http::header::HeaderValue::from_str("").unwrap())
        .to_str()
        .unwrap()
        .to_string();
    let event_type = EventType::from_str(&event_type_string).unwrap();

    // Save all events to influxdb.
    match event_type {
        EventType::Push => {
            println!("[{}] {:?}", event_type.name(), event);
            event.as_influx_push(&api_context.influx, &api_context.github).await;
        }
        EventType::PullRequest => {
            println!("[{}] {:?}", event_type.name(), event);
            let influx_event = event.as_influx_pull_request();
            api_context.influx.query(influx_event, event_type.name()).await;
        }
        EventType::PullRequestReviewComment => {
            println!("[{}] {:?}", event_type.name(), event);
            let influx_event = event.as_influx_pull_request_review_comment();
            api_context.influx.query(influx_event, event_type.name()).await;
        }
        EventType::Issues => {
            println!("[{}] {:?}", event_type.name(), event);
            let influx_event = event.as_influx_issue();
            api_context.influx.query(influx_event, event_type.name()).await;
        }
        EventType::IssueComment => {
            println!("[{}] {:?}", event_type.name(), event);
            let influx_event = event.as_influx_issue_comment();
            api_context.influx.query(influx_event, event_type.name()).await;
        }
        EventType::CheckSuite => {
            println!("[{}] {:?}", event_type.name(), event);
            let influx_event = event.as_influx_check_suite();
            api_context.influx.query(influx_event, event_type.name()).await;
        }
        EventType::CheckRun => {
            println!("[{}] {:?}", event_type.name(), event);
            let influx_event = event.as_influx_check_run();
            api_context.influx.query(influx_event, event_type.name()).await;
        }
        _ => (),
    }

    if event_type != EventType::Push && event_type != EventType::PullRequest {
        let msg = format!("Aborted, not a `push` or `pull_request` event, got `{}`", event_type);
        println!("[github]: {}", msg);
        return Ok(HttpResponseAccepted(msg));
    }

    // Check if the event came from the rfd repo.
    let repo = event.clone().repository.unwrap();
    let repo_name = repo.name;
    if repo_name != "rfd" {
        // We only care about the rfd repo push events for now.
        // We can throw this out, log it and return early.
        let msg = format!("Aborted, `{}` event was to the {} repo, no automations are set up for this repo yet", event_type, repo_name);
        println!("[github]: {}", msg);
        return Ok(HttpResponseAccepted(msg));
    }

    // Handle if we got a pull_request.
    if event_type == EventType::PullRequest {
        // We only care if the pull request was `opened`.
        if event.action != "opened" {
            // We can throw this out, log it and return early.
            let msg = format!(
                "Aborted, `{}` event was to the {} repo, no automations are set up for action `{}` yet",
                event_type, repo_name, event.action
            );
            println!("[github]: {}", msg);
            return Ok(HttpResponseAccepted(msg));
        }

        // We have a newly opened pull request.
        // TODO: Let's update the discussion link for the RFD.

        let msg = format!(
            "`{}` event was to the {} repo with action `{}`, updated discussion link for the RFD",
            event_type, repo_name, event.action
        );
        println!("[github]: {}", msg);
        return Ok(HttpResponseAccepted(msg));
    }

    // Now we can continue since we have a push event to the rfd repo.
    // Ensure we have commits.
    if event.commits.is_empty() {
        // `push` even has no commits.
        // We can throw this out, log it and return early.
        let msg = "Aborted, `push` event has no commits".to_string();
        println!("[github]: {}", msg);
        return Ok(HttpResponseAccepted(msg));
    }

    let mut commit = event.commits.get(0).unwrap().clone();
    // We only care about distinct commits.
    if !commit.distinct {
        // The commit is not distinct.
        // We can throw this out, log it and return early.
        let msg = format!("Aborted, `push` event commit `{}` is not distinct", commit.id);
        println!("[github]: {}", msg);
        return Ok(HttpResponseAccepted(msg));
    }

    // Ignore any changes that are not to the `rfd/` directory.
    let dir = "rfd/";
    commit.filter_files_by_path(dir);
    if !commit.has_changed_files() {
        // No files changed that we care about.
        // We can throw this out, log it and return early.
        let msg = format!("Aborted, `push` event commit `{}` does not include any changes to the `{}` directory", commit.id, dir);
        println!("[github]: {}", msg);
        return Ok(HttpResponseAccepted(msg));
    }

    // Get the branch name.
    let branch = event.refv.trim_start_matches("refs/heads/");
    // Make sure we have a branch.
    if branch.is_empty() {
        // The branch name is empty.
        // We can throw this out, log it and return early.
        let msg = "Aborted, `push` event branch name is empty".to_string();
        println!("[github]: {}", msg);
        return Ok(HttpResponseAccepted(msg));
    }

    // Iterate over the files and update the RFDs that have been added or
    // modified in our database.
    let mut changed_files = commit.added.clone();
    changed_files.append(&mut commit.modified.clone());
    for file in changed_files {
        // If the file is not a README.md or README.adoc, skip it.
        // TODO: handle the updating of images.
        if !file.ends_with("README.md") && !file.ends_with("README.adoc") {
            // Continue through the loop.
            continue;
        }

        // We have a README file that changed, let's parse the RFD and update it
        // in our database.
        println!("[github] `{}` event -> file {} was modified on branch {}", event_type.name(), file, branch);
        // Parse the RFD.
        let new_rfd = NewRFD::new_from_github(&github_repo, branch, &file, commit.timestamp.unwrap()).await;

        // Get the old RFD from the database. We will need this later to
        // check if the RFD's state changed.
        let old_rfd = db.get_rfd(new_rfd.number);
        let mut old_rfd_state = "".to_string();
        let mut old_rfd_pdf = "".to_string();
        if let Some(o) = old_rfd {
            old_rfd_state = o.state.to_string();
            old_rfd_pdf = o.get_pdf_filename();
        }

        // Update the RFD in the database.
        let rfd = db.upsert_rfd(&new_rfd);

        // Create all the shorturls for the RFD if we need to,
        // this would be on added files, only.
        // TODO: see if we can make this faster by doing something better than
        // dispatching the workflow.
        github_repo
            .actions()
            .workflows()
            .dispatch(
                "run-shorturls",
                &hubcaps::workflows::WorkflowDispatchOptions::builder().reference(repo.default_branch.to_string()).build(),
            )
            .await
            .unwrap();

        // Update airtable with the new RFD.
        let mut airtable_rfd = rfd.clone();
        airtable_rfd.create_or_update_in_airtable().await;

        // Update the PDFs for the RFD.
        rfd.convert_and_upload_pdf(&api_context.github, &api_context.drive, &api_context.drive_rfd_shared_id, &api_context.drive_rfd_dir_id)
            .await;

        // Check if the RFD state changed from what is currently in the
        // database.
        // If the RFD's state was changed to `discussion`, we need to open a PR
        // for that RFD.
        // Make sure we are not on the master branch, since then we would not need
        // a PR. Instead, below, the state of the RFD would be moved to `published`.
        // TODO: see if we drop events if we do we might want to remove the check with
        // the old state and just do it everytime an RFD is in discussion.
        if old_rfd_state != rfd.state && rfd.state == "discussion" && branch != repo.default_branch {
            // First, we need to make sure we don't already have a pull request open.
            let pulls = github_repo
                .pulls()
                .list(&hubcaps::pulls::PullListOptions::builder().state(hubcaps::issues::State::Open).build())
                .await
                .unwrap();
            // Check if any pull requests are from our branch.
            let mut has_pull = false;
            for pull in pulls {
                // Check if the pull request is for our branch.
                let pull_branch = pull.head.commit_ref.trim_start_matches("refs/heads/");
                println!("pull branch: {}", pull_branch);

                if pull_branch == branch {
                    println!(
                        "[github] RFD {} has moved from state {} -> {}, on branch {}, we already have a pull request: {}",
                        rfd.number_string, old_rfd_state, rfd.state, branch, pull.html_url
                    );

                    has_pull = true;
                    break;
                }
            }

            // Open a pull request, if we don't already have one.
            if !has_pull {
                println!(
                    "[github] RFD {} has moved from state {} -> {}, on branch {}, opening a PR",
                    rfd.number_string, old_rfd_state, rfd.state, branch
                );

                github_repo
                                    .pulls()
                                    .create(&hubcaps::pulls::PullOptions::new(
                rfd.name.to_string(),
                format!("{}:{}", api_context.github_org,branch),
                repo.default_branch.to_string(),
                Some("Automatically opening the pull request since the document is marked as being in discussion. If you wish to not have a pull request open, change the state of your document and close this pull request."),
                                            ))
                                    .await
                                    .unwrap();

                // We could update the discussion link here, but we will already
                // trigger a pull request created event, so we might as well let
                // that do its thing.
            }
        }

        // If the RFD was merged into the default branch, but the RFD state is not `published`,
        // update the state of the RFD in GitHub to show it as `published`.
        if branch == repo.default_branch && rfd.state != "published" {
            println!(
                "[github] RFD {} is the branch {} but its state is {}, updating it to `published`",
                rfd.number_string, repo.default_branch, old_rfd_state,
            );

            // TODO: Update the state of the RFD in GitHub to show it as `published`.
            // After we change the file, this will kick off another webhook event, so we do not
            // need to update the database again.
        }

        // If the title of the RFD changed, delete the old PDF file so it
        // doesn't linger in GitHub and Google Drive.
        if old_rfd_pdf != rfd.get_pdf_filename() {
            let pdf_path = format!("/pdfs/{}", old_rfd_pdf);

            // First get the sha of the old pdf.
            let old_pdf = github_repo.content().file(&pdf_path, &repo.default_branch).await.unwrap();

            // Delete the old filename from GitHub.
            github_repo
                .content()
                .delete(
                    &pdf_path,
                    &format!(
                        "Deleting file content {} programatically\n\nThis is done from the cio repo webhooky::listen_github_webhooks function.",
                        old_rfd_pdf
                    ),
                    &old_pdf.sha,
                )
                .await
                .unwrap();

            // Delete the old filename from drive.
            api_context.drive.delete_file_by_name(&api_context.drive_rfd_shared_id, &old_rfd_pdf).await.unwrap();

            println!(
                "[github] RFD {} PDF changed name from {} -> {}, deleted old file from GitHub and Google Drive",
                rfd.number_string,
                old_rfd_pdf,
                rfd.get_pdf_filename()
            );
        }
    }

    // TODO: should we do something if the file gets deleted (?)

    Ok(HttpResponseAccepted("Updated successfully".to_string()))
}

/** Get our current GitHub rate limit. */
#[endpoint {
    method = GET,
    path = "/github/ratelimit",
}]
#[instrument]
#[inline]
async fn github_rate_limit(rqctx: Arc<RequestContext>) -> Result<HttpResponseOk<GitHubRateLimit>, HttpError> {
    let api_context = Context::from_rqctx(&rqctx);
    let github = &api_context.github;

    let response = github.rate_limit().get().await.unwrap();
    let reset_time = Utc.timestamp(response.resources.core.reset.into(), 0);

    let dur = reset_time - Utc::now();

    Ok(HttpResponseOk(GitHubRateLimit {
        limit: response.resources.core.limit,
        remaining: response.resources.core.remaining,
        reset: HumanTime::from(dur).to_string(),
    }))
}

/// A GitHub RateLimit
#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct GitHubRateLimit {
    pub limit: u32,
    pub remaining: u32,
    pub reset: String,
}

/**
 * Listen for edits to our Google Sheets.
 * These are set up with a Google Apps script on the sheets themselves.
 */
#[endpoint {
    method = POST,
    path = "/google/sheets/edit",
}]
#[instrument]
#[inline]
async fn listen_google_sheets_edit_webhooks(_rqctx: Arc<RequestContext>, body_param: TypedBody<GoogleSpreadsheetEditEvent>) -> Result<HttpResponseAccepted<String>, HttpError> {
    let event = body_param.into_inner();
    println!("[google/sheets/edit]: {:?}", event);

    Ok(HttpResponseAccepted("ok".to_string()))
}

/// A Google Sheet edit event.
#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct GoogleSpreadsheetEditEvent {
    #[serde(default)]
    pub event: GoogleSpreadsheetEvent,
    #[serde(default)]
    pub spreadsheet: GoogleSpreadsheet,
}

#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct GoogleSpreadsheetEvent {
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "authMode")]
    pub auth_mode: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "oldValue", deserialize_with = "deserialize_null_string::deserialize")]
    pub old_value: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub value: String,
    #[serde(default)]
    pub range: GoogleSpreadsheetRange,
    #[serde(default)]
    pub source: GoogleSpreadsheetSource,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "triggerUid")]
    pub trigger_uid: String,
    #[serde(default)]
    pub user: GoogleSpreadsheetUser,
    #[serde(default, skip_serializing_if = "HashMap::is_empty", rename = "namedValues")]
    pub named_values: HashMap<String, Vec<String>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
}

#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct GoogleSpreadsheetRange {
    #[serde(default, rename = "columnEnd")]
    pub column_end: i64,
    #[serde(default, rename = "columnStart")]
    pub column_start: i64,
    #[serde(default, rename = "rowEnd")]
    pub row_end: i64,
    #[serde(default, rename = "rowStart")]
    pub row_start: i64,
}

#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct GoogleSpreadsheetSource {}

#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct GoogleSpreadsheetUser {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
}

#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct GoogleSpreadsheet {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
}

/**
 * Listen for rows created in our Google Sheets.
 * These are set up with a Google Apps script on the sheets themselves.
 */
#[endpoint {
    method = POST,
    path = "/google/sheets/row/create",
}]
#[instrument]
#[inline]
async fn listen_google_sheets_row_create_webhooks(rqctx: Arc<RequestContext>, body_param: TypedBody<GoogleSpreadsheetRowCreateEvent>) -> Result<HttpResponseAccepted<String>, HttpError> {
    let api_context = Context::from_rqctx(&rqctx);
    let event = body_param.into_inner();
    println!("[google/sheets/row/create]: {:?}", event);

    // Ensure this was an applicant and not some other google form!!
    let role = get_role_from_sheet_id(&event.spreadsheet.id);
    if role.is_empty() {
        return Ok(HttpResponseAccepted("ok".to_string()));
    }

    // Parse the applicant out of the row information.
    let mut applicant = NewApplicant::parse_from_row(&event.spreadsheet.id, &event.event.named_values);
    println!("[applicant]: {:?}", applicant);

    // TODO: remove this once we know parsing the webhook works.
    if applicant.email.is_empty() {
        panic!("applicant has an empty email");
    }

    // We add one to the end of the columns to get the column where the email sent verification is.
    let sent_email_received_column_index = event.event.range.column_end + 1;
    applicant
        .expand(
            &api_context.drive,
            &api_context.sheets,
            sent_email_received_column_index.try_into().unwrap(),
            event.event.range.row_start.try_into().unwrap(),
        )
        .await;

    if !applicant.sent_email_received {
        // Post to Slack.
        post_to_channel(get_hiring_channel_post_url(), applicant.as_slack_msg()).await;

        // Initialize the SendGrid client.
        let sendgrid_client = sendgrid_api::SendGrid::new_from_env();
        // Send a company-wide email.
        email_send_new_applicant_notification(&sendgrid_client, applicant.clone(), "oxide.computer").await;
    }

    // TODO: share the database connection in the context.
    let db = Database::new();

    // Send the applicant to the database.
    let a = db.upsert_applicant(&applicant);

    // Update airtable.
    let mut airtable_applicant = a.clone();
    airtable_applicant.create_or_update_in_airtable().await;

    Ok(HttpResponseAccepted("ok".to_string()))
}

/// A Google Sheet row create event.
#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct GoogleSpreadsheetRowCreateEvent {
    #[serde(default)]
    pub event: GoogleSpreadsheetEvent,
    #[serde(default)]
    pub spreadsheet: GoogleSpreadsheet,
}

/** Ping endpoint for MailChimp webhooks. */
#[endpoint {
    method = GET,
    path = "/mailchimp",
}]
#[instrument]
#[inline]
async fn ping_mailchimp_webhooks(_rqctx: Arc<RequestContext>) -> Result<HttpResponseOk<String>, HttpError> {
    Ok(HttpResponseOk("ok".to_string()))
}

/** Listen for MailChimp webhooks. */
#[endpoint {
    method = POST,
    path = "/mailchimp",
}]
#[instrument]
#[inline]
async fn listen_mailchimp_webhooks(_rqctx: Arc<RequestContext>, query_args: Query<MailchimpWebhook>) -> Result<HttpResponseAccepted<String>, HttpError> {
    // TODO: share the database connection in the context.
    let db = Database::new();

    let event = query_args.into_inner();

    println!("[mailchimp] {:?}", event);

    if event.webhook_type != *"subscribe" {
        let msg = format!("Aborted, not a `subscribe` event, got `{}`", event.webhook_type);
        println!("[mailchimp]: {}", msg);
        return Ok(HttpResponseAccepted(msg));
    }

    // Parse the webhook as a new mailing list subscriber.
    let new_subscriber = event.as_subscriber();

    // Update the subscriber in the database.
    let subscriber = db.upsert_mailing_list_subscriber(&new_subscriber);

    //  Update airtable with the new subscriber.
    let mut airtable_subscriber = subscriber.clone();
    airtable_subscriber.create_or_update_in_airtable().await;

    // Parse the signup into a slack message.
    // Send the message to the slack channel.
    post_to_channel(get_public_relations_channel_post_url(), new_subscriber.as_slack_msg()).await;

    Ok(HttpResponseAccepted("Updated successfully".to_string()))
}

/// A GitHub organization.
#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct GitHubOrganization {
    pub login: String,
    pub id: u64,
    pub url: String,
    pub repos_url: String,
    pub events_url: String,
    pub hooks_url: String,
    pub issues_url: String,
    pub members_url: String,
    pub public_members_url: String,
    pub avatar_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

/// A GitHub app installation.
#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct GitHubInstallation {
    #[serde(default)]
    pub id: i64,
    // account: Account
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub access_tokens_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub repositories_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub html_url: String,
    #[serde(default)]
    pub app_id: i32,
    #[serde(default)]
    pub target_id: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub target_type: String,
    // permissions: Permissions
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<String>,
    // created_at, updated_at
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub single_file_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub repository_selection: String,
}

/// A GitHub webhook event.
/// FROM: https://docs.github.com/en/free-pro-team@latest/developers/webhooks-and-events/webhook-events-and-payloads
#[derive(Debug, Clone, JsonSchema, Deserialize, Serialize)]
pub struct GitHubWebhook {
    /// Most webhook payloads contain an action property that contains the
    /// specific activity that triggered the event.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub action: String,
    /// The user that triggered the event. This property is included in
    /// every webhook payload.
    #[serde(default)]
    pub sender: GitHubUser,
    /// The `repository` where the event occurred. Webhook payloads contain the
    /// `repository` property when the event occurs from activity in a repository.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<GithubRepo>,
    /// Webhook payloads contain the `organization` object when the webhook is
    /// configured for an organization or the event occurs from activity in a
    /// repository owned by an organization.
    #[serde(default)]
    pub organization: GitHubOrganization,
    /// The GitHub App installation. Webhook payloads contain the `installation`
    /// property when the event is configured for and sent to a GitHub App.
    #[serde(default)]
    pub installation: GitHubInstallation,

    /// `push` event fields.
    /// FROM: https://docs.github.com/en/free-pro-team@latest/developers/webhooks-and-events/webhook-events-and-payloads#push
    ///
    /// The full `git ref` that was pushed. Example: `refs/heads/main`.
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "ref")]
    pub refv: String,
    /// The SHA of the most recent commit on `ref` before the push.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub before: String,
    /// The SHA of the most recent commit on `ref` after the push.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub after: String,
    /// An array of commit objects describing the pushed commits.
    /// The array includes a maximum of 20 commits. If necessary, you can use
    /// the Commits API to fetch additional commits. This limit is applied to
    /// timeline events only and isn't applied to webhook deliveries.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commits: Vec<GitHubCommit>,

    /// `pull_request` event fields.
    /// FROM: https://docs.github.com/en/free-pro-team@latest/developers/webhooks-and-events/webhook-events-and-payloads#pull_request
    ///
    /// The pull request number.
    #[serde(default)]
    pub number: i64,
    /// The pull request itself.
    #[serde(default)]
    pub pull_request: GitHubPullRequest,

    /// `issues` event fields.
    /// FROM: https://docs.github.com/en/free-pro-team@latest/developers/webhooks-and-events/webhook-events-and-payloads#issues
    ///
    /// The issue itself.
    #[serde(default)]
    pub issue: GitHubIssue,

    /// `issue_comment` event fields.
    /// FROM: https://docs.github.com/en/free-pro-team@latest/developers/webhooks-and-events/webhook-events-and-payloads#issue_comment
    ///
    /// The comment itself.
    #[serde(default)]
    pub comment: GitHubComment,

    /// `check_suite` event fields.
    /// FROM: https://docs.github.com/en/free-pro-team@latest/developers/webhooks-and-events/webhook-events-and-payloads#check_suite
    ///
    /// The check suite itself.
    #[serde(default)]
    pub check_suite: GitHubCheckSuite,

    /// `check_run` event fields.
    /// FROM: https://docs.github.com/en/free-pro-team@latest/developers/webhooks-and-events/webhook-events-and-payloads#check_run
    ///
    /// The check run itself.
    #[serde(default)]
    pub check_run: GitHubCheckRun,
}

impl GitHubWebhook {
    // Push an event for every commit.
    pub async fn as_influx_push(&self, influx: &influx::Client, github: &Github) {
        let repo = self.repository.as_ref().unwrap();

        for commit in &self.commits {
            if commit.distinct {
                let c = github.repo(repo.owner.login.to_string(), repo.name.to_string()).commits().get(&commit.id).await.unwrap();

                if c.sha != commit.id {
                    // We have a problem.
                    panic!("commit sha mismatch: {} {}", c.sha.to_string(), commit.id.to_string());
                }

                let push_event = influx::Push {
                    time: c.commit.author.date,
                    repo_name: repo.name.to_string(),
                    sender: self.sender.login.to_string(),
                    reference: self.refv.to_string(),
                    added: commit.added.join(",").to_string(),
                    modified: commit.removed.join(",").to_string(),
                    removed: commit.removed.join(",").to_string(),
                    sha: c.sha.to_string(),
                    additions: c.stats.additions,
                    deletions: c.stats.deletions,
                    total: c.stats.total,
                    message: c.commit.message.to_string(),
                };

                influx.query(push_event, EventType::Push.name()).await;
            }
        }
    }

    pub fn as_influx_pull_request(&self) -> influx::PullRequest {
        influx::PullRequest {
            time: Utc::now(),
            repo_name: self.repository.as_ref().unwrap().name.to_string(),
            sender: self.sender.login.to_string(),
            action: self.action.to_string(),
            head_reference: self.pull_request.head.commit_ref.to_string(),
            base_reference: self.pull_request.base.commit_ref.to_string(),
            number: self.number,
            github_id: self.pull_request.id,
            merged: self.pull_request.merged,
        }
    }

    pub fn as_influx_pull_request_review_comment(&self) -> influx::PullRequestReviewComment {
        influx::PullRequestReviewComment {
            time: Utc::now(),
            repo_name: self.repository.as_ref().unwrap().name.to_string(),
            sender: self.sender.login.to_string(),
            action: self.action.to_string(),
            pull_request_number: self.pull_request.number,
            github_id: self.comment.id,
            comment: self.comment.body.to_string(),
        }
    }

    pub fn as_influx_issue(&self) -> influx::Issue {
        influx::Issue {
            time: Utc::now(),
            repo_name: self.repository.as_ref().unwrap().name.to_string(),
            sender: self.sender.login.to_string(),
            action: self.action.to_string(),
            number: self.number,
            github_id: self.pull_request.id,
        }
    }

    pub fn as_influx_issue_comment(&self) -> influx::IssueComment {
        influx::IssueComment {
            time: Utc::now(),
            repo_name: self.repository.as_ref().unwrap().name.to_string(),
            sender: self.sender.login.to_string(),
            action: self.action.to_string(),
            issue_number: self.issue.number,
            github_id: self.comment.id,
            comment: self.comment.body.to_string(),
        }
    }

    pub fn as_influx_check_suite(&self) -> influx::CheckSuite {
        influx::CheckSuite {
            time: Utc::now(),
            repo_name: self.repository.as_ref().unwrap().name.to_string(),
            sender: self.sender.login.to_string(),
            action: self.action.to_string(),

            head_branch: self.check_suite.head_branch.to_string(),
            head_sha: self.check_suite.head_sha.to_string(),
            status: self.check_suite.status.to_string(),
            conclusion: self.check_suite.conclusion.to_string(),

            slug: self.check_suite.app.slug.to_string(),
            name: self.check_suite.app.name.to_string(),

            reference: self.check_suite.head_branch.to_string(),
            sha: self.check_suite.head_sha.to_string(),
            github_id: self.check_suite.id,
        }
    }

    pub fn as_influx_check_run(&self) -> influx::CheckRun {
        influx::CheckRun {
            time: Utc::now(),
            repo_name: self.repository.as_ref().unwrap().name.to_string(),
            sender: self.sender.login.to_string(),
            action: self.action.to_string(),

            head_branch: self.check_suite.head_branch.to_string(),
            head_sha: self.check_run.head_sha.to_string(),
            status: self.check_run.status.to_string(),
            conclusion: self.check_run.conclusion.to_string(),

            name: self.check_run.name.to_string(),
            app_slug: self.check_run.app.slug.to_string(),
            app_name: self.check_run.app.name.to_string(),

            reference: self.check_suite.head_branch.to_string(),
            sha: self.check_run.head_sha.to_string(),
            check_suite_id: self.check_suite.id,
            github_id: self.check_run.id,
        }
    }
}

/// A GitHub commit.
/// FROM: https://docs.github.com/en/free-pro-team@latest/developers/webhooks-and-events/webhook-events-and-payloads#push
#[derive(Debug, Clone, Default, PartialEq, JsonSchema, Deserialize, Serialize)]
pub struct GitHubCommit {
    /// The SHA of the commit.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    /// The ISO 8601 timestamp of the commit.
    pub timestamp: Option<DateTime<Utc>>,
    /// The commit message.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub message: String,
    /// The git author of the commit.
    #[serde(default, alias = "user")]
    pub author: GitHubUser,
    /// URL that points to the commit API resource.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    /// Whether this commit is distinct from any that have been pushed before.
    #[serde(default)]
    pub distinct: bool,
    /// An array of files added in the commit.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub added: Vec<String>,
    /// An array of files modified by the commit.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modified: Vec<String>,
    /// An array of files removed in the commit.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "ref")]
    pub commit_ref: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sha: String,
}

impl GitHubCommit {
    /// Filter the files that were added, modified, or removed by their prefix
    /// including a specified directory or path.
    pub fn filter_files_by_path(&mut self, dir: &str) {
        self.added = filter(&self.added, dir);
        self.modified = filter(&self.modified, dir);
        self.removed = filter(&self.removed, dir);
    }

    /// Return if the commit has any files that were added, modified, or removed.
    pub fn has_changed_files(&self) -> bool {
        !self.added.is_empty() || !self.modified.is_empty() || !self.removed.is_empty()
    }
}

fn filter(files: &[String], dir: &str) -> Vec<String> {
    let mut in_dir: Vec<String> = Default::default();
    for file in files {
        if file.starts_with(dir) {
            in_dir.push(file.to_string());
        }
    }

    in_dir
}

/// A GitHub pull request.
/// FROM: https://docs.github.com/en/free-pro-team@latest/rest/reference/pulls#get-a-pull-request
#[derive(Debug, Default, Clone, PartialEq, JsonSchema, Deserialize, Serialize)]
pub struct GitHubPullRequest {
    #[serde(default)]
    pub id: i64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    /// The HTML location of this pull request.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub html_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub diff_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub patch_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub issue_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub commits_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub review_comments_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub review_comment_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub comments_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub statuses_url: String,
    #[serde(default)]
    pub number: i64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub state: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub body: String,
    /*pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,*/
    #[serde(default)]
    pub closed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub merged_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub head: GitHubCommit,
    #[serde(default)]
    pub base: GitHubCommit,
    // links
    #[serde(default)]
    pub user: GitHubUser,
    #[serde(default)]
    pub merged: bool,
}

/// A Github issue.
/// FROM: https://docs.github.com/en/free-pro-team@latest/rest/reference/issues
#[derive(Debug, Default, Clone, PartialEq, JsonSchema, Deserialize, Serialize)]
pub struct GitHubIssue {
    #[serde(default)]
    pub id: i64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    pub labels_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub comments_url: String,
    pub events_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub html_url: String,
    #[serde(default)]
    pub number: i64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub state: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub body: String,
    #[serde(default)]
    pub user: GitHubUser,
    //#[serde(default, skip_serializing_if = "Vec::is_empty")]
    //pub labels: Vec<GitHubLabel>,
    #[serde(default)]
    pub assignee: GitHubUser,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub comments: i64,
    #[serde(default)]
    pub pull_request: GitHubPullRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<DateTime<Utc>>,
    /* pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,*/
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assignees: Vec<GitHubUser>,
}

/// A reference to a pull request.
#[derive(Debug, Default, Clone, PartialEq, JsonSchema, Deserialize, Serialize)]
pub struct GitHubPullRef {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub html_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub diff_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub patch_url: String,
}

/// A Github comment.
/// FROM: https://docs.github.com/en/free-pro-team@latest/rest/reference/issues#comments
#[derive(Debug, Default, Clone, PartialEq, JsonSchema, Deserialize, Serialize)]
pub struct GitHubComment {
    #[serde(default)]
    pub id: i64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub html_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub body: String,
    #[serde(default)]
    pub user: GitHubUser,
    /* pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,*/
}

/// A GitHub check suite.
/// FROM: https://docs.github.com/en/free-pro-team@latest/rest/reference/checks#suites
#[derive(Debug, Default, Clone, PartialEq, JsonSchema, Deserialize, Serialize)]
pub struct GitHubCheckSuite {
    #[serde(default)]
    pub id: i64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub head_branch: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub head_sha: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub status: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub conclusion: String,
    #[serde(default)]
    pub app: GitHubApp,
}

/// A GitHub app.
#[derive(Debug, Default, Clone, PartialEq, JsonSchema, Deserialize, Serialize)]
pub struct GitHubApp {
    pub id: i64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub slug: String,
}

/// A GitHub check run.
/// FROM: https://docs.github.com/en/free-pro-team@latest/rest/reference/checks#get-a-check-run
#[derive(Debug, Default, Clone, PartialEq, JsonSchema, Deserialize, Serialize)]
pub struct GitHubCheckRun {
    #[serde(default)]
    pub id: i64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub head_sha: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub status: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub conclusion: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default)]
    pub check_suite: GitHubCheckSuite,
    #[serde(default)]
    pub app: GitHubApp,
}

pub mod deserialize_null_string {
    use serde::{self, Deserialize, Deserializer};

    // The signature of a deserialize_with function must follow the pattern:
    //
    //    fn deserialize<'de, D>(D) -> Result<T, D::Error>
    //    where
    //        D: Deserializer<'de>
    //
    // although it may also be generic over the output types T.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<String, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Sometimes this value is passed by the API as "null" which breaks the
        // std User parsing. We fix that here.
        let s = String::deserialize(deserializer).unwrap_or_default();

        Ok(s)
    }
}
