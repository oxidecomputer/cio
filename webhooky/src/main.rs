#![allow(clippy::field_reassign_with_default)]
pub mod event_types;
use crate::event_types::EventType;
pub mod slack_commands;
use crate::slack_commands::SlackCommand;
pub mod repos;
use crate::repos::Repo;
pub mod tracking_numbers;
#[macro_use]
extern crate serde_json;

use std::{
    collections::HashMap, convert::TryInto, env, ffi::OsStr, fs::File, str::FromStr, sync::Arc,
};

use chrono::{offset::Utc, DateTime, NaiveDate, TimeZone};
use chrono_humanize::HumanTime;
use cio_api::{
    analytics::NewPageView,
    api_tokens::{APIToken, NewAPIToken},
    applicant_reviews::{ApplicantReview, NewApplicantReview},
    applicants::{get_docusign_template_id, get_role_from_sheet_id, Applicant, NewApplicant},
    asset_inventory::AssetItem,
    companies::Company,
    configs::{
        get_configs_from_repo, sync_buildings, sync_certificates, sync_conference_rooms,
        sync_github_outside_collaborators, sync_groups, sync_links, sync_users, User,
    },
    db::Database,
    journal_clubs::JournalClubMeeting,
    mailing_list::MailingListSubscriber,
    rack_line::RackLineSubscriber,
    repos::{GitHubUser, NewRepo},
    rfds::{is_image, NewRFD, RFD},
    schema::{api_tokens, applicants, journal_club_meetings, rfds},
    shipments::{InboundShipment, NewInboundShipment, OutboundShipment, OutboundShipments},
    shorturls::{
        generate_shorturls_for_configs_links, generate_shorturls_for_repos,
        generate_shorturls_for_rfds,
    },
    swag_inventory::SwagInventoryItem,
    swag_store::Order,
    templates::generate_terraform_files_for_okta,
    utils::{
        create_or_update_file_in_github_repo, decode_base64, decode_base64_to_string,
        get_file_content_from_repo, merge_json,
    },
};
use diesel::prelude::*;
use docusign::DocuSign;
use dropshot::{
    endpoint, ApiDescription, ConfigDropshot, ConfigLogging, ConfigLoggingLevel, HttpError,
    HttpResponseAccepted, HttpResponseOk, HttpServerStarter, Path, Query, RequestContext,
    TypedBody, UntypedBody,
};
use google_drive::GoogleDrive;
use gusto_api::Client as Gusto;
use mailchimp_api::{MailChimp, Webhook as MailChimpWebhook};
use quickbooks::QuickBooks;
use ramp_api::Client as Ramp;
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use schemars::JsonSchema;
use sentry::IntoDsn;
use serde::{Deserialize, Serialize};
use serde_qs::Config as QSConfig;
use sheets::Sheets;
use slack_chat_api::{BotCommand, MessageResponse, MessageResponseType, Slack};
use zoom_api::Client as Zoom;

#[tokio::main]
async fn main() -> Result<(), String> {
    // Initialize sentry.
    let sentry_dsn = env::var("WEBHOOKY_SENTRY_DSN").unwrap_or_default();
    let _guard = sentry::init(sentry::ClientOptions {
        dsn: sentry_dsn.into_dsn().unwrap(),

        release: Some(env::var("GIT_HASH").unwrap_or_default().into()),
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
    api.register(ping).unwrap();
    api.register(github_rate_limit).unwrap();
    api.register(listen_airtable_applicants_request_background_check_webhooks)
        .unwrap();
    api.register(listen_airtable_applicants_review_create_webhooks)
        .unwrap();
    api.register(listen_airtable_applicants_update_webhooks)
        .unwrap();
    api.register(listen_airtable_assets_items_print_barcode_label_webhooks)
        .unwrap();
    api.register(listen_airtable_employees_print_home_address_label_webhooks)
        .unwrap();
    api.register(listen_airtable_shipments_inbound_create_webhooks)
        .unwrap();
    api.register(listen_airtable_shipments_outbound_create_webhooks)
        .unwrap();
    api.register(listen_airtable_shipments_outbound_reprint_label_webhooks)
        .unwrap();
    api.register(
        listen_airtable_shipments_outbound_resend_shipment_status_email_to_recipient_webhooks,
    )
    .unwrap();
    api.register(listen_airtable_shipments_outbound_schedule_pickup_webhooks)
        .unwrap();
    api.register(listen_airtable_swag_inventory_items_print_barcode_labels_webhooks)
        .unwrap();
    api.register(listen_analytics_page_view_webhooks).unwrap();
    api.register(listen_application_submit_requests).unwrap();
    api.register(listen_applicant_review_requests).unwrap();
    api.register(listen_application_files_upload_requests)
        .unwrap();
    api.register(listen_auth_docusign_callback).unwrap();
    api.register(listen_auth_docusign_consent).unwrap();
    api.register(listen_auth_github_callback).unwrap();
    api.register(listen_auth_github_consent).unwrap();
    api.register(listen_auth_google_callback).unwrap();
    api.register(listen_auth_google_consent).unwrap();
    api.register(listen_auth_gusto_callback).unwrap();
    api.register(listen_auth_gusto_consent).unwrap();
    api.register(listen_auth_mailchimp_callback).unwrap();
    api.register(listen_auth_mailchimp_consent).unwrap();
    api.register(listen_auth_plaid_callback).unwrap();
    api.register(listen_auth_ramp_callback).unwrap();
    api.register(listen_auth_ramp_consent).unwrap();
    api.register(listen_auth_zoom_callback).unwrap();
    api.register(listen_auth_zoom_consent).unwrap();
    api.register(listen_auth_zoom_deauthorization).unwrap();
    api.register(listen_auth_slack_callback).unwrap();
    api.register(listen_auth_slack_consent).unwrap();
    api.register(listen_auth_quickbooks_callback).unwrap();
    api.register(listen_auth_quickbooks_consent).unwrap();
    api.register(listen_checkr_background_update_webhooks)
        .unwrap();
    api.register(listen_docusign_envelope_update_webhooks)
        .unwrap();
    api.register(listen_emails_incoming_sendgrid_parse_webhooks)
        .unwrap();
    api.register(listen_google_sheets_edit_webhooks).unwrap();
    api.register(listen_google_sheets_row_create_webhooks)
        .unwrap();
    api.register(listen_github_webhooks).unwrap();
    api.register(listen_mailchimp_mailing_list_webhooks)
        .unwrap();
    api.register(listen_mailchimp_rack_line_webhooks).unwrap();
    api.register(listen_products_sold_count_requests).unwrap();
    api.register(listen_shippo_tracking_update_webhooks)
        .unwrap();
    api.register(listen_slack_commands_webhooks).unwrap();
    api.register(listen_store_order_create).unwrap();
    api.register(ping_mailchimp_mailing_list_webhooks).unwrap();
    api.register(ping_mailchimp_rack_line_webhooks).unwrap();
    api.register(trigger_rfd_update_by_number).unwrap();
    api.register(api_get_schema).unwrap();

    // Print the OpenAPI Spec to stdout.
    let mut api_definition = &mut api.openapi(&"Webhooks API", &"0.0.1");
    api_definition = api_definition
        .description("Internal webhooks server for listening to several third party webhooks")
        .contact_url("https://oxide.computer")
        .contact_email("webhooks@oxide.computer");
    let api_file = "openapi-webhooky.json";
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
    db: Database,

    schema: String,
}

impl Context {
    /**
     * Return a new Context.
     */
    pub async fn new(schema: String) -> Context {
        let db = Database::new();

        // Create the context.
        Context { db, schema }
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
    rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseOk<String>, HttpError> {
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

#[derive(Deserialize, Serialize, Debug, JsonSchema)]
struct CounterResponse {
    count: i32,
}

/** Return the count of products sold. */
#[endpoint {
    method = GET,
    path = "/products/sold/count",
}]
async fn listen_products_sold_count_requests(
    rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseOk<CounterResponse>, HttpError> {
    let api_context = rqctx.context();

    // TODO: find a better way to do this.
    let company = Company::get_from_db(&api_context.db, "Oxide".to_string()).unwrap();

    // TODO: change this one day to be the number of racks sold.
    // For now, use it as number of applications that need to be triaged.
    // Get the applicants that need to be triaged.
    let applicants = applicants::dsl::applicants
        .filter(
            applicants::dsl::cio_company_id.eq(company.id).and(
                applicants::dsl::status
                    .eq(cio_api::applicant_status::Status::NeedsToBeTriaged.to_string()),
            ),
        )
        .load::<Applicant>(&api_context.db.conn())
        .unwrap();

    Ok(HttpResponseOk(CounterResponse {
        count: applicants.len() as i32,
    }))
}

/** Listen for GitHub webhooks. */
#[endpoint {
    method = POST,
    path = "/github",
}]
async fn listen_github_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<GitHubWebhook>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();
    let api_context = rqctx.context();

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

    // Filter by event type any actions we can rule out for all repos.
    match event_type {
        EventType::Push => {
            println!("`{}` {:?}", event_type.name(), event);

            // Ensure we have commits.
            if event.commits.is_empty() {
                // `push` event has no commits.
                // We can throw this out, log it and return early.
                println!("`push` event has no commits: {:?}", event);
                sentry::end_session();
                return Ok(HttpResponseAccepted("ok".to_string()));
            }

            let commit = event.commits.get(0).unwrap().clone();
            // We only care about distinct commits.
            if !commit.distinct {
                // The commit is not distinct.
                // We can throw this out, log it and return early.
                println!("`push` event commit `{}` is not distinct", commit.id);
                sentry::end_session();
                return Ok(HttpResponseAccepted("ok".to_string()));
            }

            // Get the branch name.
            let branch = event.refv.trim_start_matches("refs/heads/");
            // Make sure we have a branch.
            if branch.is_empty() {
                // The branch name is empty.
                // We can throw this out, log it and return early.
                // This should never happen, but we won't rule it out because computers.
                sentry::capture_message(
                    &format!("`push` event branch name is empty: {:?}", event),
                    sentry::Level::Fatal,
                );
                sentry::end_session();
                return Ok(HttpResponseAccepted("ok".to_string()));
            }
        }
        EventType::Repository => {
            println!("`{}` {:?}", event_type.name(), event);

            let company =
                Company::get_from_github_org(&api_context.db, &event.repository.owner.login);
            let github = company.authenticate_github();

            // Now let's handle the event.
            let resp = handle_repository_event(&github, api_context, event, &company).await;
            sentry::end_session();
            return resp;
        }
        _ => (),
    }

    // Run the correct handler function based on the event type and repo.
    if !event.repository.name.is_empty() {
        let repo = &event.repository;
        let repo_name = Repo::from_str(&repo.name).unwrap();

        let company = Company::get_from_github_org(&api_context.db, &repo.owner.login);
        let github = company.authenticate_github();

        match repo_name {
            Repo::RFD => match event_type {
                EventType::Push => {
                    let resp = handle_rfd_push(&github, api_context, event, &company).await;
                    sentry::end_session();
                    return resp;
                }
                EventType::PullRequest => {
                    let resp = handle_rfd_pull_request(&github, api_context, event, &company).await;
                    sentry::end_session();
                    return resp;
                }
                _ => (),
            },
            Repo::Configs => {
                if let EventType::Push = event_type {
                    let resp = handle_configs_push(&github, api_context, event, &company).await;
                    sentry::end_session();
                    return resp;
                }
            }
            _ => {
                // We can throw this out, log it and return early.
                println!(
                    "`{}` event was to the {} repo, no automations are set up for this repo yet",
                    event_type, repo_name
                );
            }
        }
    }

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

#[derive(Deserialize, Debug, JsonSchema)]
struct RFDPathParams {
    num: i32,
}

/** Trigger an update for an RFD. */
#[endpoint {
    method = POST,
    path = "/rfd/{num}",
}]
async fn trigger_rfd_update_by_number(
    rqctx: Arc<RequestContext<Context>>,
    path_params: Path<RFDPathParams>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();
    let num = path_params.into_inner().num;
    println!("Triggering an update for RFD number `{}`", num);

    let api_context = rqctx.context();
    let db = &api_context.db;

    // Get the company id for Oxide.
    // TODO: split this out per company.
    let oxide = Company::get_from_db(db, "Oxide".to_string()).unwrap();

    let github = oxide.authenticate_github();

    // Get gsuite token.
    let token = oxide.authenticate_google(db).await;

    // Initialize the Google Drive client.
    let drive_client = GoogleDrive::new(token);

    let result = RFD::get_from_db(db, num);
    if result.is_none() {
        // Return early, we couldn't find an RFD.
        sentry::capture_message(
            &format!("No RFD was found with number `{}`", num),
            sentry::Level::Fatal,
        );
        sentry::end_session();
        return Ok(HttpResponseAccepted("ok".to_string()));
    }
    let mut rfd = result.unwrap();
    // Update the RFD.
    rfd.expand(&github, &oxide).await;
    println!("updated  RFD {}", rfd.number_string);

    rfd.convert_and_upload_pdf(&github, &drive_client, &oxide)
        .await;
    println!(
        "updated pdf `{}` for RFD {}",
        rfd.get_pdf_filename(),
        rfd.number_string
    );

    // Save the rfd back to our database.
    rfd.update(db).await;

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/** Get our current GitHub rate limit. */
#[endpoint {
    method = GET,
    path = "/github/ratelimit",
}]
async fn github_rate_limit(
    rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseOk<GitHubRateLimit>, HttpError> {
    sentry::start_session();
    let api_context = rqctx.context();

    let db = &api_context.db;

    // Get the company id for Oxide.
    // TODO: split this out per company.
    let oxide = Company::get_from_db(db, "Oxide".to_string()).unwrap();

    let github = oxide.authenticate_github();

    let response = github.rate_limit().get().await.unwrap();
    let reset_time = Utc.timestamp(response.resources.core.reset, 0);

    let dur = reset_time - Utc::now();

    sentry::end_session();
    Ok(HttpResponseOk(GitHubRateLimit {
        limit: response.resources.core.limit as u32,
        remaining: response.resources.core.remaining as u32,
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
async fn listen_google_sheets_edit_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<GoogleSpreadsheetEditEvent>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();

    let api_context = rqctx.context();
    let db = &api_context.db;

    // Get the company id for Oxide.
    // TODO: split this out per company.
    let oxide = Company::get_from_db(db, "Oxide".to_string()).unwrap();

    let github = oxide.authenticate_github();

    // Get gsuite token.
    // We re-get the token here since otherwise it will expire.
    let token = oxide.authenticate_google(db).await;
    // Initialize the GSuite sheets client.
    let sheets = Sheets::new(token.clone());

    let event = body_param.into_inner();
    println!("{:?}", event);

    // Ensure this was an applicant and not some other google form!!
    let role = get_role_from_sheet_id(&event.spreadsheet.id);
    if role.is_empty() {
        println!("event is not for an application spreadsheet: {:?}", event);
        sentry::end_session();
        return Ok(HttpResponseAccepted("ok".to_string()));
    }

    // Some value was changed. We need to get two things to update the airtable
    // and the database:
    //  - The applicant's email
    //  - The name of the column that was updated.
    // Let's first get the email for this applicant. This is always in column B.
    let mut cell_name = format!("B{}", event.event.range.row_start);
    let email = sheets
        .get_value(&event.spreadsheet.id, cell_name)
        .await
        .unwrap();

    if email.is_empty() {
        // We can return early, the row does not have an email.
        sentry::capture_message(
            &format!("email cell returned empty for event: {:?}", event),
            sentry::Level::Fatal,
        );
        sentry::end_session();
        return Ok(HttpResponseAccepted("ok".to_string()));
    }

    // Now let's get the header for the column of the cell that changed.
    // This is always in row 1.
    // These should be zero indexed.
    let column_letters = "0ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    cell_name = format!(
        "{}1",
        column_letters
            .chars()
            .nth(event.event.range.column_start.try_into().unwrap())
            .unwrap()
            .to_string()
    );
    let column_header = sheets
        .get_value(&event.spreadsheet.id, cell_name)
        .await
        .unwrap()
        .to_lowercase();

    // Now let's get the applicant from the database so we can update it.
    let result = applicants::dsl::applicants
        .filter(applicants::dsl::email.eq(email.to_string()))
        .filter(applicants::dsl::sheet_id.eq(event.spreadsheet.id.to_string()))
        .first::<Applicant>(&db.conn());
    if result.is_err() {
        sentry::capture_message(
            &format!(
                "could not find applicant with email `{}`, sheet_id `{}` in the database",
                email, event.spreadsheet.id
            ),
            sentry::Level::Fatal,
        );
        sentry::end_session();
        return Ok(HttpResponseAccepted("ok".to_string()));
    }
    let mut a = result.unwrap();

    // Now let's update the correct item for them.
    if column_header.contains("have sent email that we received their application?") {
        // Parse the boolean.
        if event.event.value.to_lowercase() == "true" {
            a.sent_email_received = true;
        }
    } else if column_header.contains("have sent follow up email?") {
        // Parse the boolean.
        if event.event.value.to_lowercase() == "true" {
            a.sent_email_follow_up = true;
        }
    } else if column_header.contains("status") {
        // Parse the new status.
        let mut status = cio_api::applicant_status::Status::from_str(&event.event.value)
            .unwrap_or_default()
            .to_string();
        status = status.trim().to_string();
        if !status.is_empty() {
            a.status = status;
            a.raw_status = event.event.value.to_string();

            // If they changed their status to OnBoarding let's do the docusign updates.
            if a.status == cio_api::applicant_status::Status::Onboarding.to_string() {
                // First let's update the applicant.
                a.update(db).await;

                // Create our docusign client.
                let dsa = oxide.authenticate_docusign(db).await;
                if let Some(ds) = dsa {
                    // Get the template we need.
                    let offer_template_id =
                        get_docusign_template_id(&ds, cio_api::applicants::DOCUSIGN_OFFER_TEMPLATE)
                            .await;

                    a.do_docusign_offer(db, &ds, &offer_template_id, &oxide)
                        .await;

                    let piia_template_id =
                        get_docusign_template_id(&ds, cio_api::applicants::DOCUSIGN_PIIA_TEMPLATE)
                            .await;
                    a.do_docusign_piia(db, &ds, &piia_template_id, &oxide).await;
                }
            }
        }
    } else if column_header.contains("start date") {
        if event.event.value.trim().is_empty() {
            a.start_date = None;
        } else {
            match NaiveDate::parse_from_str(event.event.value.trim(), "%m/%d/%Y") {
                Ok(v) => a.start_date = Some(v),
                Err(e) => {
                    sentry::capture_message(
                        &format!(
                            "error parsing start date from spreadsheet {}: {}",
                            event.event.value.trim(),
                            e
                        ),
                        sentry::Level::Info,
                    );
                    a.start_date = None
                }
            }
        }
    } else if column_header.contains("value reflected") {
        // Update the value reflected.
        a.value_reflected = event.event.value.to_lowercase();
    } else if column_header.contains("value violated") {
        // Update the value violated.
        a.value_violated = event.event.value.to_lowercase();
    } else if column_header.contains("value in tension [1]") {
        // The person updated the values in tension.
        // We need to get the other value in tension in the next column to the right.
        let value_column = event.event.range.column_start + 1;
        cell_name = format!(
            "{}{}",
            column_letters
                .chars()
                .nth(value_column.try_into().unwrap())
                .unwrap()
                .to_string(),
            event.event.range.row_start
        );
        let value_in_tension_2 = sheets
            .get_value(&event.spreadsheet.id, cell_name)
            .await
            .unwrap()
            .to_lowercase();
        a.values_in_tension = vec![value_in_tension_2, event.event.value.to_lowercase()];
    } else if column_header.contains("value in tension [2]") {
        // The person updated the values in tension.
        // We need to get the other value in tension in the next column to the left.
        let value_column = event.event.range.column_start - 1;
        cell_name = format!(
            "{}{}",
            column_letters
                .chars()
                .nth(value_column.try_into().unwrap())
                .unwrap()
                .to_string(),
            event.event.range.row_start
        );
        let value_in_tension_1 = sheets
            .get_value(&event.spreadsheet.id, cell_name)
            .await
            .unwrap()
            .to_lowercase();
        a.values_in_tension = vec![value_in_tension_1, event.event.value.to_lowercase()];
    } else {
        // If this is a field wehipmentdon't care about, return early.
        println!(
            "column updated was `{}`, no automations set up for that column yet",
            column_header
        );
        sentry::end_session();
        return Ok(HttpResponseAccepted("ok".to_string()));
    }

    // Update the applicant in the database and Airtable.
    let new_applicant = a.update(db).await;
    let company = Company::get_by_id(db, new_applicant.cio_company_id);

    // Get all the hiring issues on the configs repository.
    let configs_issues = github
        .issues()
        .list_all_for_repo(
            &company.github_org,
            "configs",
            // milestone
            "",
            octorust::types::IssuesListState::All,
            // assignee
            "",
            // creator
            "",
            // mentioned
            "",
            // labels
            "hiring",
            // sort
            Default::default(),
            // direction
            Default::default(),
            // since
            None,
        )
        .await
        .unwrap();

    new_applicant
        .create_github_onboarding_issue(db, &github, &configs_issues)
        .await;

    println!("applicant {} updated successfully", new_applicant.email);
    sentry::end_session();
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
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "oldValue",
        deserialize_with = "deserialize_null_string::deserialize"
    )]
    pub old_value: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "deserialize_null_string::deserialize"
    )]
    pub value: String,
    #[serde(default)]
    pub range: GoogleSpreadsheetRange,
    #[serde(default)]
    pub source: GoogleSpreadsheetSource,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "triggerUid"
    )]
    pub trigger_uid: String,
    #[serde(default)]
    pub user: GoogleSpreadsheetUser,
    #[serde(
        default,
        skip_serializing_if = "HashMap::is_empty",
        rename = "namedValues"
    )]
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
async fn listen_google_sheets_row_create_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<GoogleSpreadsheetRowCreateEvent>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();

    let api_context = rqctx.context();
    let db = &api_context.db;

    // Get the company id for Oxide.
    // TODO: split this out per company.
    let oxide = Company::get_from_db(db, "Oxide".to_string()).unwrap();

    // Get gsuite token.
    // We re-get the token here since otherwise it will expire.
    let token = oxide.authenticate_google(db).await;

    // Initialize the GSuite sheets client.
    let sheets = Sheets::new(token.clone());
    // Initialize the Google Drive client.
    let drive = GoogleDrive::new(token);

    let event = body_param.into_inner();
    println!("{:?}", event);

    // Ensure this was an applicant and not some other google form!!
    let role = get_role_from_sheet_id(&event.spreadsheet.id);
    if role.is_empty() {
        // Return early if not
        println!("event is not for an application spreadsheet: {:?}", event);
        sentry::end_session();
        return Ok(HttpResponseAccepted("ok".to_string()));
    }

    // Parse the applicant out of the row information.
    let mut applicant =
        NewApplicant::parse_from_row(&event.spreadsheet.id, &event.event.named_values).await;

    if applicant.email.is_empty() {
        sentry::capture_message(
            &format!("applicant has an empty email: {:?}", applicant),
            sentry::Level::Fatal,
        );
        sentry::end_session();
        return Ok(HttpResponseAccepted("ok".to_string()));
    }

    // We do not need to add one to the end of the columns to get the column where the email sent verification is
    // because google sheets index's at 0, so adding one would put us over, we are just right here.
    let sent_email_received_column_index = event.event.range.column_end;
    let sent_email_follow_up_index = event.event.range.column_end + 6;
    applicant
        .expand(
            db,
            &drive,
            &sheets,
            sent_email_received_column_index.try_into().unwrap(),
            sent_email_follow_up_index.try_into().unwrap(),
            event.event.range.row_start.try_into().unwrap(),
        )
        .await;

    if !applicant.sent_email_received {
        println!(
            "applicant is new, sending internal notifications: {:?}",
            applicant
        );

        // Send a company-wide email.
        applicant.send_email_internally(db).await;

        applicant.sent_email_received = true;
    }

    // Send the applicant to the database and Airtable.
    let a = applicant.upsert(db).await;

    println!("applicant {} created successfully", a.email);
    sentry::end_session();
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

/**
 * Listen for a button pressed to print a home address label for employees.
 */
#[endpoint {
    method = POST,
    path = "/airtable/employees/print_home_address_label",
}]
async fn listen_airtable_employees_print_home_address_label_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();
    let api_context = rqctx.context();

    let event = body_param.into_inner();
    println!("{:?}", event);

    if event.record_id.is_empty() {
        sentry::capture_message("Record id is empty", sentry::Level::Fatal);
        sentry::end_session();
        return Ok(HttpResponseAccepted("ok".to_string()));
    }

    // Get the row from airtable.
    let user =
        User::get_from_airtable(&event.record_id, &api_context.db, event.cio_company_id).await;

    // Create a new shipment for the employee and print the label.
    user.create_shipment_to_home_address(&api_context.db).await;

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/**
 * Listen for a button pressed to print a barcode label for an asset item.
 */
#[endpoint {
    method = POST,
    path = "/airtable/assets/items/print_barcode_label",
}]
async fn listen_airtable_assets_items_print_barcode_label_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();
    let api_context = rqctx.context();

    let event = body_param.into_inner();
    println!("{:?}", event);

    if event.record_id.is_empty() {
        sentry::capture_message("Record id is empty", sentry::Level::Fatal);
        sentry::end_session();
        return Ok(HttpResponseAccepted("ok".to_string()));
    }

    // Get the row from airtable.
    let asset_item =
        AssetItem::get_from_airtable(&event.record_id, &api_context.db, event.cio_company_id).await;

    // Print the barcode label(s).
    asset_item.print_label(&api_context.db).await;
    println!("asset item {} printed label", asset_item.name);

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/**
 * Listen for a button pressed to print barcode labels for a swag inventory item.
 */
#[endpoint {
    method = POST,
    path = "/airtable/swag/inventory/items/print_barcode_labels",
}]
async fn listen_airtable_swag_inventory_items_print_barcode_labels_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();
    let api_context = rqctx.context();

    let event = body_param.into_inner();
    println!("{:?}", event);

    if event.record_id.is_empty() {
        sentry::capture_message("Record id is empty", sentry::Level::Fatal);
        sentry::end_session();
        return Ok(HttpResponseAccepted("ok".to_string()));
    }

    // Get the row from airtable.
    let swag_inventory_item = SwagInventoryItem::get_from_airtable(
        &event.record_id,
        &api_context.db,
        event.cio_company_id,
    )
    .await;

    // Print the barcode label(s).
    swag_inventory_item.print_label(&api_context.db).await;
    println!(
        "swag inventory item {} printed label",
        swag_inventory_item.name
    );

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/**
 * Listen for a button pressed to request a background check for an applicant.
 */
#[endpoint {
    method = POST,
    path = "/airtable/applicants/request_background_check",
}]
async fn listen_airtable_applicants_request_background_check_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();
    let api_context = rqctx.context();

    let event = body_param.into_inner();
    println!("{:?}", event);

    if event.record_id.is_empty() {
        sentry::capture_message("Record id is empty", sentry::Level::Fatal);
        sentry::end_session();
        return Ok(HttpResponseAccepted("ok".to_string()));
    }

    // Get the row from airtable.
    let mut applicant =
        Applicant::get_from_airtable(&event.record_id, &api_context.db, event.cio_company_id).await;
    if applicant.criminal_background_check_status.is_empty() {
        // Request the background check, since we previously have not requested one.
        applicant
            .send_background_check_invitation(&api_context.db)
            .await;
        println!(
            "sent background check invitation to applicant: {}",
            applicant.email
        );
    }

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/**
 * Listen for rows created in our Airtable workspace.
 * These are set up with an Airtable script on the workspaces themselves.
 */
#[endpoint {
    method = POST,
    path = "/airtable/applicants/review/create",
}]
async fn listen_airtable_applicants_review_create_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();
    let event = body_param.into_inner();
    println!("{:?}", event);

    let api_context = rqctx.context();

    if event.record_id.is_empty() {
        sentry::capture_message("Record id is empty", sentry::Level::Fatal);
        sentry::end_session();
        return Ok(HttpResponseAccepted("ok".to_string()));
    }

    // Get the row from airtable.
    let mut review =
        ApplicantReview::get_from_airtable(&event.record_id, &api_context.db, event.cio_company_id)
            .await;

    if review.applicant.is_empty() || review.reviewer.is_empty() {
        sentry::capture_message(
            &format!("Applicant and reviewer cannot be empty for {:?}", review),
            sentry::Level::Fatal,
        );
        sentry::end_session();
        return Ok(HttpResponseAccepted("ok".to_string()));
    }

    if review.id == 0 {
        // We don't have it in the databse.
        // Add it to the database.
        let r: NewApplicantReview = review.into();
        let mut new_review = r.upsert_in_db(&api_context.db);
        if new_review.airtable_record_id.is_empty() {
            new_review.airtable_record_id = event.record_id.to_string();
        }
        new_review.cio_company_id = event.cio_company_id;
        new_review.expand(&api_context.db);
        review = new_review.update(&api_context.db).await;
    }

    // Get the applicant for the review.
    let mut applicant = Applicant::get_from_airtable(
        // Get the record id for the applicant.
        review.applicant.get(0).unwrap(),
        &api_context.db,
        event.cio_company_id,
    )
    .await;

    // Update the scores for the applicant.
    // This will also update the database after.
    applicant.update_reviews_scoring(&api_context.db).await;

    println!(
        "applicant {} with review by {} updated successfully",
        applicant.email, review.reviewer
    );
    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/**
 * Listen for rows updated in our Airtable workspace.
 * These are set up with an Airtable script on the workspaces themselves.
 */
#[endpoint {
    method = POST,
    path = "/airtable/applicants/update",
}]
async fn listen_airtable_applicants_update_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();
    let event = body_param.into_inner();
    println!("{:?}", event);

    let api_context = rqctx.context();

    if event.record_id.is_empty() {
        sentry::capture_message("Record id is empty", sentry::Level::Fatal);
        sentry::end_session();
        return Ok(HttpResponseAccepted("ok".to_string()));
    }

    // Get the row from airtable.
    let applicant =
        Applicant::get_from_airtable(&event.record_id, &api_context.db, event.cio_company_id).await;

    if applicant.status.is_empty() {
        sentry::capture_message(
            &format!("Got an empty applicant status for row: {}", applicant.email),
            sentry::Level::Fatal,
        );
        sentry::end_session();
        return Ok(HttpResponseAccepted("ok".to_string()));
    }

    // Grab our old applicant from the database.
    let mut db_applicant = Applicant::get_by_id(&api_context.db, applicant.id);

    // Grab the status and the status raw.
    let status = cio_api::applicant_status::Status::from_str(&applicant.status).unwrap();
    db_applicant.status = status.to_string();
    if !applicant.raw_status.is_empty() {
        // Update the raw status if it had changed.
        db_applicant.raw_status = applicant.raw_status.to_string();
    }

    // TODO: should we also update the start date if set in airtable?
    // If we do this, we need to update the airtable webhook settings to include it as
    // well.

    // Update the row in our database.
    db_applicant.update(&api_context.db).await;

    println!("applicant {} updated successfully", applicant.email);
    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/**
 * Listen for rows created in our Airtable workspace.
 * These are set up with an Airtable script on the workspaces themselves.
 */
#[endpoint {
    method = POST,
    path = "/airtable/shipments/outbound/create",
}]
async fn listen_airtable_shipments_outbound_create_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();
    let event = body_param.into_inner();
    println!("{:?}", event);

    let api_context = rqctx.context();

    if event.record_id.is_empty() {
        sentry::capture_message("Record id is empty", sentry::Level::Fatal);
        sentry::end_session();
        return Ok(HttpResponseAccepted("ok".to_string()));
    }

    // Get the row from airtable.
    let shipment = OutboundShipment::get_from_airtable(
        &event.record_id,
        &api_context.db,
        event.cio_company_id,
    )
    .await;

    // If it is a row we created from our internal store do nothing.
    if shipment.notes.contains("Oxide store")
        || shipment.notes.contains("Google sheet")
        || shipment.notes.contains("Internal")
        || !shipment.shippo_id.is_empty()
    {
        return Ok(HttpResponseAccepted("ok".to_string()));
    }

    if shipment.email.is_empty() {
        sentry::capture_message("Got an empty email for row", sentry::Level::Fatal);
        sentry::end_session();
        return Ok(HttpResponseAccepted("ok".to_string()));
    }

    // Update the row in our database.
    let mut new_shipment = shipment.update(&api_context.db).await;
    // Create the shipment in shippo.
    new_shipment
        .create_or_get_shippo_shipment(&api_context.db)
        .await;
    // Update airtable again.
    new_shipment.update(&api_context.db).await;

    println!("shipment {} created successfully", shipment.email);
    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/// An Airtable row event.
#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct AirtableRowEvent {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub record_id: String,
    #[serde(default)]
    pub cio_company_id: i32,
}

/**
 * Listen for a button pressed to reprint a label for an outbound shipment.
 */
#[endpoint {
    method = POST,
    path = "/airtable/shipments/outbound/reprint_label",
}]
async fn listen_airtable_shipments_outbound_reprint_label_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();
    let event = body_param.into_inner();
    println!("{:?}", event);

    if event.record_id.is_empty() {
        sentry::capture_message("Record id is empty", sentry::Level::Fatal);
        sentry::end_session();
        return Ok(HttpResponseAccepted("ok".to_string()));
    }

    let api_context = rqctx.context();

    // Get the row from airtable.
    let mut shipment = OutboundShipment::get_from_airtable(
        &event.record_id,
        &api_context.db,
        event.cio_company_id,
    )
    .await;

    // Reprint the label.
    shipment.print_label(&api_context.db).await;
    println!("shipment {} reprinted label", shipment.email);

    // Update the field.
    shipment.status = "Label printed".to_string();

    // Update Airtable.
    shipment.update(&api_context.db).await;

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/**
 * Listen for a button pressed to resend a shipment status email to the recipient for an outbound shipment.
 */
#[endpoint {
    method = POST,
    path = "/airtable/shipments/outbound/resend_shipment_status_email_to_recipient",
}]
async fn listen_airtable_shipments_outbound_resend_shipment_status_email_to_recipient_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();
    let event = body_param.into_inner();
    println!("{:?}", event);

    if event.record_id.is_empty() {
        sentry::capture_message("Record id is empty", sentry::Level::Fatal);
        sentry::end_session();
        return Ok(HttpResponseAccepted("ok".to_string()));
    }

    let api_context = rqctx.context();

    // Get the row from airtable.
    let shipment = OutboundShipment::get_from_airtable(
        &event.record_id,
        &api_context.db,
        event.cio_company_id,
    )
    .await;

    // Resend the email to the recipient.
    shipment.send_email_to_recipient(&api_context.db).await;
    println!(
        "resent the shipment email to the recipient {}",
        shipment.email
    );

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/**
 * Listen for a button pressed to schedule a pickup for an outbound shipment.
 */
#[endpoint {
    method = POST,
    path = "/airtable/shipments/outbound/schedule_pickup",
}]
async fn listen_airtable_shipments_outbound_schedule_pickup_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();
    let event = body_param.into_inner();
    println!("pickup shipment {:?}", event);

    if event.record_id.is_empty() {
        sentry::capture_message("Record id is empty", sentry::Level::Fatal);
        sentry::end_session();
        return Ok(HttpResponseAccepted("ok".to_string()));
    }

    // Schedule the pickup.
    let api_context = rqctx.context();
    let company = Company::get_by_id(&api_context.db, event.cio_company_id);
    OutboundShipments::create_pickup(&api_context.db, &company).await;

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/// A SendGrid incoming email event.
#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct IncomingEmail {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub headers: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub dkim: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        alias = "content-ids"
    )]
    pub content_ids: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub to: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub cc: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub html: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub from: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sender_ip: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub spam_report: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub envelope: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub attachments: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub subject: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub spam_score: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        alias = "attachment-info"
    )]
    pub attachment_info: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub charsets: String,
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "SPF")]
    pub spf: String,
}

/**
 * Listen for emails coming inbound from SendGrid's parse API.
 * We use this for scanning for packages in emails.
 */
#[endpoint {
    method = POST,
    path = "/emails/incoming/sendgrid/parse",
}]
async fn listen_emails_incoming_sendgrid_parse_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: UntypedBody,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();

    // Parse the body as bytes.
    let mut b = body_param.as_bytes();

    // Get the headers and parse the form data.
    let headers = rqctx.request.lock().await.headers().clone();

    let content_type = headers.get("content-type").unwrap();
    let content_length = headers.get("content-length").unwrap();
    let mut h = hyper::header::Headers::new();
    h.set_raw("content-type", vec![content_type.as_bytes().to_vec()]);
    h.set_raw("content-length", vec![content_length.as_bytes().to_vec()]);

    let form_data = formdata::read_formdata(&mut b, &h).unwrap();

    // Start creating the new shipment.
    let mut i: NewInboundShipment = Default::default();
    let mut from = "".to_string();
    // Parse the form body.
    for (name, value) in &form_data.fields {
        if i.carrier.is_empty() && (name == "html" || name == "text" || name == "email") {
            let (carrier, tracking_number) =
                crate::tracking_numbers::parse_tracking_information(value);
            if !carrier.is_empty() {
                i.carrier = carrier.to_string();
                i.tracking_number = tracking_number.to_string();
                i.notes = value.to_string();
            }
        }

        if name == "subject" {
            if value.contains("from Mouser Electronics") {
                i.name = "Mouser".to_string();
                i.order_number = value
                    .replace("Fwd:", "")
                    .replace("Shipment Notification on Your Purchase Order", "")
                    .replace("from Mouser Electronics, Inc. Invoice Attached", "")
                    .trim()
                    .to_string();
            } else if value.contains("Arrow Order") {
                i.name = "Arrow".to_string();
                i.order_number = value
                    .replace("Fwd:", "")
                    .replace("Arrow Order #", "")
                    .trim()
                    .to_string();
            } else if value.contains("Microchip Order #") {
                i.name = "Microchip".to_string();
                i.order_number = value
                    .replace("Fwd:", "")
                    .replace("Your Microchip Order #", "")
                    .replace("Has Been Shipped", "")
                    .trim()
                    .to_string();
            } else if value.contains("TI.com order") {
                i.name = "Texas Instruments".to_string();
                i.order_number = value
                    .replace("Fwd:", "")
                    .replace("TI.com order", "")
                    .replace("- DO NOT REPLY - Order", "")
                    .replace("fulfilled", "")
                    .replace("status update", "")
                    .trim()
                    .to_string();
            } else {
                i.name = format!("Email: {}", value);
            }
        }

        if name == "from" {
            from = value.to_string();
        }
    }

    i.notes = format!("Parsed email from {}:\n{}", from, i.notes);
    i.cio_company_id = 1;

    if i.carrier.is_empty() {
        sentry::capture_message(
            &format!(
                "could not find shipment for email:shipment: {:?}\nfields: {:?}\nfiles: {:?}",
                i, form_data.fields, form_data.files
            ),
            sentry::Level::Info,
        );

        // Return early.
        sentry::end_session();
        return Ok(HttpResponseAccepted("ok".to_string()));
    }

    // Add the shipment to our database.
    let api_context = rqctx.context();
    i.upsert(&api_context.db).await;

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/**
 * Listen for applicant reviews being submitted for job applicants */
#[endpoint {
    method = POST,
    path = "/applicant/review/submit",
}]
async fn listen_applicant_review_requests(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<cio_api::applicant_reviews::NewApplicantReview>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();
    let api_context = rqctx.context();
    let event = body_param.into_inner();

    if event.name.is_empty()
        || event.applicant.is_empty()
        || event.reviewer.is_empty()
        || event.evaluation.is_empty()
    {
        sentry::capture_message(
            &format!("review is empty: {:?}", event),
            sentry::Level::Fatal,
        );
        sentry::end_session();
        return Ok(HttpResponseAccepted("ok".to_string()));
    }

    // Add them to the database.
    event.upsert(&api_context.db).await;

    println!("applicant review created successfully: {:?}", event);

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/**
 * Listen for applications being submitted for incoming job applications */
#[endpoint {
    method = POST,
    path = "/application/submit",
}]
async fn listen_application_submit_requests(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<cio_api::application_form::ApplicationForm>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();
    let api_context = rqctx.context();
    let event = body_param.into_inner();

    event.do_form(&api_context.db).await;

    println!(
        "application for {} {} created successfully",
        event.email, event.role
    );

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/// Application file upload data.
#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct ApplicationFileUploadData {
    #[serde(default)]
    pub cio_company_id: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub resume: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub materials: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub role: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub user_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub resume_contents: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub materials_contents: String,
}

/**
 * Listen for files being uploaded for incoming job applications */
#[endpoint {
    method = POST,
    path = "/application/files/upload",
}]
async fn listen_application_files_upload_requests(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<ApplicationFileUploadData>,
) -> Result<HttpResponseOk<HashMap<String, String>>, HttpError> {
    sentry::start_session();

    let data = body_param.into_inner();

    // We will return a key value of the name of file and the link in google drive.
    let mut response: HashMap<String, String> = Default::default();

    if data.email.is_empty()
        || data.role.is_empty()
        || data.cio_company_id <= 0
        || data.materials.is_empty()
        || data.resume.is_empty()
        || data.materials_contents.is_empty()
        || data.resume_contents.is_empty()
        || data.user_name.is_empty()
    {
        sentry::capture_message(
            &format!("could not get applicant information for: {:?}", data),
            sentry::Level::Info,
        );

        // Return early.
        sentry::end_session();
        return Ok(HttpResponseOk(response));
    }

    // TODO: Add the files to google drive.
    let api_context = rqctx.context();
    let db = &api_context.db;

    let company = Company::get_by_id(db, data.cio_company_id);

    // Get gsuite token.
    let token = company.authenticate_google(db).await;

    // Initialize the Google Drive client.
    let drive = GoogleDrive::new(token);

    // Figure out where our directory is.
    // It should be in the shared drive : "Automated Documents"/"application_content"
    let shared_drive = drive
        .get_drive_by_name("Automated Documents")
        .await
        .unwrap();

    // Get the directory by the name.
    let drive_dir = drive
        .get_file_by_name(&shared_drive.id, "application_content")
        .await
        .unwrap();
    let parent_id = drive_dir.get(0).unwrap().id.to_string();

    // Create the folder for our candidate with their email.
    let email_folder_id = drive
        .create_folder(&shared_drive.id, &parent_id, &data.email)
        .await
        .unwrap();

    // Create the folder for our candidate with the role.
    let role_folder_id = drive
        .create_folder(&shared_drive.id, &email_folder_id, &data.role)
        .await
        .unwrap();

    let mut files: HashMap<String, (String, String)> = HashMap::new();
    files.insert(
        "resume".to_string(),
        (data.resume.to_string(), data.resume_contents.to_string()),
    );
    files.insert(
        "materials".to_string(),
        (
            data.materials.to_string(),
            data.materials_contents.to_string(),
        ),
    );

    // Iterate over our files and create them in google drive.
    // Create or update the file in the google_drive.
    for (name, (file_path, contents)) in files {
        // Get the extension from the content type.
        let ext = get_extension_from_filename(&file_path).unwrap();
        let ct = mime_guess::from_ext(ext).first().unwrap();
        let content_type = ct.essence_str().to_string();
        let file_name = format!("{} - {}.{}", data.user_name, name, ext);

        // Upload our file to drive.
        let drive_file = drive
            .create_or_update_file(
                &shared_drive.id,
                &role_folder_id,
                &file_name,
                &content_type,
                &decode_base64(&contents),
            )
            .await
            .unwrap();
        // Add the file to our links.
        response.insert(
            name.to_string(),
            format!("https://drive.google.com/open?id={}", drive_file.id),
        );
    }

    sentry::end_session();
    Ok(HttpResponseOk(response))
}

fn get_extension_from_filename(filename: &str) -> Option<&str> {
    std::path::Path::new(filename)
        .extension()
        .and_then(OsStr::to_str)
}

/**
 * Listen for rows created in our Airtable workspace.
 * These are set up with an Airtable script on the workspaces themselves.
 */
#[endpoint {
    method = POST,
    path = "/airtable/shipments/inbound/create",
}]
async fn listen_airtable_shipments_inbound_create_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();
    let event = body_param.into_inner();
    println!("{:?}", event);

    if event.record_id.is_empty() {
        sentry::capture_message("Record id is empty", sentry::Level::Fatal);
        sentry::end_session();
        return Ok(HttpResponseAccepted("ok".to_string()));
    }

    let api_context = rqctx.context();
    let db = &api_context.db;

    // Get the row from airtable.
    let record =
        InboundShipment::get_from_airtable(&event.record_id, db, event.cio_company_id).await;

    if record.tracking_number.is_empty() || record.carrier.is_empty() {
        // Return early, we don't care.
        sentry::capture_message(
            "tracking_number and carrier are empty, ignoring",
            sentry::Level::Fatal,
        );
        sentry::end_session();
        return Ok(HttpResponseAccepted("ok".to_string()));
    }

    let mut new_shipment: NewInboundShipment = record.into();

    new_shipment.expand().await;
    let mut shipment = new_shipment.upsert_in_db(db);
    if shipment.airtable_record_id.is_empty() {
        shipment.airtable_record_id = event.record_id;
    }
    shipment.cio_company_id = event.cio_company_id;
    shipment.update(db).await;

    println!(
        "inbound shipment {} updated successfully",
        shipment.tracking_number
    );
    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/**
 * Listen for orders being created by the Oxide store.
 */
#[endpoint {
    method = POST,
    path = "/store/order",
}]
async fn listen_store_order_create(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<Order>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();
    let api_context = rqctx.context();

    let event = body_param.into_inner();
    println!("order {:?}", event);
    event.do_order(&api_context.db).await;

    println!("order for {} created successfully", event.email);
    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/**
 * Listen for shipment tracking updated from Shippo.
 */
#[endpoint {
    method = POST,
    path = "/shippo/tracking/update",
}]
async fn listen_shippo_tracking_update_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<serde_json::Value>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();
    let api_context = rqctx.context();

    let event = body_param.into_inner();
    let body: ShippoTrackingUpdateEvent =
        serde_json::from_str(&event.to_string()).unwrap_or_else(|e| {
            sentry::capture_message(
                &format!(
                    "decoding event body for shippo `{}` failed: {}",
                    event.to_string(),
                    e
                ),
                sentry::Level::Info,
            );

            Default::default()
        });

    let ts = body.data;
    if ts.tracking_number.is_empty() || ts.carrier.is_empty() {
        // We can reaturn early.
        // It's too early to get anything good from this event.
        sentry::end_session();
        return Ok(HttpResponseAccepted("ok".to_string()));
    }

    // Update the inbound shipment, if it exists.
    if let Some(mut shipment) = InboundShipment::get_from_db(
        &api_context.db,
        ts.carrier.to_string(),
        ts.tracking_number.to_string(),
    ) {
        // Get the tracking status for the shipment and fill in the details.
        shipment.tracking_number = ts.tracking_number.to_string();
        let tracking_status = ts.tracking_status.unwrap_or_default();
        shipment.tracking_status = tracking_status.status.to_string();
        shipment.tracking_link();
        shipment.eta = ts.eta;

        shipment.oxide_tracking_link = shipment.oxide_tracking_link();

        shipment.messages = tracking_status.status_details;

        // Iterate over the tracking history and set the shipped_time.
        // Get the first date it was maked as in transit and use that as the shipped
        // time.
        for h in ts.tracking_history {
            if h.status == *"TRANSIT" {
                if let Some(shipped_time) = h.status_date {
                    let current_shipped_time = if let Some(s) = shipment.shipped_time {
                        s
                    } else {
                        Utc::now()
                    };

                    if shipped_time < current_shipped_time {
                        shipment.shipped_time = Some(shipped_time);
                    }
                }
            }
        }

        if tracking_status.status == *"DELIVERED" {
            shipment.delivered_time = tracking_status.status_date;
        }

        shipment.update(&api_context.db).await;
    }

    // Update the outbound shipment if it exists.
    if let Some(mut shipment) = OutboundShipment::get_from_db(
        &api_context.db,
        ts.carrier.to_string(),
        ts.tracking_number.to_string(),
    ) {
        // Update the shipment in shippo.
        // TODO: we likely don't need the extra request here, but it makes the code more DRY.
        // Clean this up eventually.
        shipment
            .create_or_get_shippo_shipment(&api_context.db)
            .await;
        shipment.update(&api_context.db).await;
    }

    println!(
        "shipment {} tracking status updated successfully",
        ts.tracking_number
    );
    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/// A Shippo tracking update event.
#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct ShippoTrackingUpdateEvent {
    #[serde(default)]
    pub data: shippo::TrackingStatus,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub event: String,
    #[serde(default)]
    pub test: bool,
}

/** Listen for updates to our checkr background checks. */
#[endpoint {
    method = POST,
    path = "/checkr/background/update",
}]
async fn listen_checkr_background_update_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<checkr::WebhookEvent>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();
    let api_context = rqctx.context();
    let event = body_param.into_inner();

    // Run the update of the background checks.
    // If we have a candidate ID let's get them from checkr.
    if event.data.object.candidate_id.is_empty()
        || event.data.object.package.is_empty()
        || event.data.object.status.is_empty()
    {
        // Return early we don't care.
        sentry::capture_message(
            &format!("checkr candidate id is empty for event: {:?}", event),
            sentry::Level::Info,
        );
        return Ok(HttpResponseAccepted("ok".to_string()));
    }

    // TODO: change this to the real company name.
    let oxide = Company::get_from_db(&api_context.db, "Oxide".to_string()).unwrap();

    let checkr_auth = oxide.authenticate_checkr();
    if checkr_auth.is_none() {
        // Return early.
        sentry::capture_message(
            &format!(
                "this company {:?} does not have a checkr api key: {:?}",
                oxide, event
            ),
            sentry::Level::Info,
        );
        return Ok(HttpResponseAccepted("ok".to_string()));
    }

    let checkr = checkr_auth.unwrap();
    let candidate = checkr
        .get_candidate(&event.data.object.candidate_id)
        .await
        .unwrap();
    let result = applicants::dsl::applicants
        .filter(
            applicants::dsl::email
                .eq(candidate.email.to_string())
                // TODO: matching on name might be a bad idea here.
                .or(applicants::dsl::name
                    .eq(format!("{} {}", candidate.first_name, candidate.last_name))),
        )
        .filter(
            applicants::dsl::status.eq(cio_api::applicant_status::Status::Onboarding.to_string()),
        )
        .first::<Applicant>(&api_context.db.conn());
    if result.is_ok() {
        let mut applicant = result.unwrap();
        // Keep the fields from Airtable we need just in case they changed.
        applicant.keep_fields_from_airtable(&api_context.db).await;

        // Set the status for the report.
        if event.data.object.package.contains("premium_criminal") {
            applicant.criminal_background_check_status = event.data.object.status.to_string();
        }
        if event.data.object.package.contains("motor_vehicle") {
            applicant.motor_vehicle_background_check_status = event.data.object.status.to_string();
        }

        // Update the applicant.
        applicant.update(&api_context.db).await;
    }

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct UserConsentURL {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
}

#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct AuthCallback {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub code: String,
    /// The state that we had passed in through the user consent URL.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub state: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "realmId")]
    pub realm_id: String,
}

/** Get the consent URL for Google auth. */
#[endpoint {
    method = GET,
    path = "/auth/google/consent",
}]
async fn listen_auth_google_consent(
    _rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseOk<UserConsentURL>, HttpError> {
    sentry::start_session();

    sentry::end_session();
    Ok(HttpResponseOk(UserConsentURL {
        url: cio_api::companies::get_google_consent_url().await,
    }))
}

/** Listen for callbacks to Google auth. */
#[endpoint {
    method = GET,
    path = "/auth/google/callback",
}]
async fn listen_auth_google_callback(
    rqctx: Arc<RequestContext<Context>>,
    query_args: Query<AuthCallback>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();
    let event = query_args.into_inner();

    let api_context = rqctx.context();

    cio_api::companies::get_google_access_token(&api_context.db, &event.code).await;

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/** Get the consent URL for GitHub auth. */
#[endpoint {
    method = GET,
    path = "/auth/github/consent",
}]
async fn listen_auth_github_consent(
    _rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseOk<UserConsentURL>, HttpError> {
    sentry::start_session();

    sentry::end_session();
    Ok(HttpResponseOk(UserConsentURL {
        url: "https://github.com/apps/oxidecomputerbot/installations/new".to_string(),
    }))
}

/** Listen for callbacks to GitHub auth. */
#[endpoint {
    method = GET,
    path = "/auth/github/callback",
}]
async fn listen_auth_github_callback(
    _rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<serde_json::Value>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();
    let event = body_param.into_inner();

    sentry::capture_message(
        &format!("github callback: {:?}", event),
        sentry::Level::Info,
    );

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/** Get the consent URL for MailChimp auth. */
#[endpoint {
    method = GET,
    path = "/auth/mailchimp/consent",
}]
async fn listen_auth_mailchimp_consent(
    _rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseOk<UserConsentURL>, HttpError> {
    sentry::start_session();

    // Initialize the MailChimp client.
    let g = MailChimp::new_from_env("", "", "");

    sentry::end_session();
    Ok(HttpResponseOk(UserConsentURL {
        url: g.user_consent_url(),
    }))
}

/** Listen for callbacks to MailChimp auth. */
#[endpoint {
    method = GET,
    path = "/auth/mailchimp/callback",
}]
async fn listen_auth_mailchimp_callback(
    rqctx: Arc<RequestContext<Context>>,
    query_args: Query<AuthCallback>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();
    let api_context = rqctx.context();
    let event = query_args.into_inner();

    // Initialize the MailChimp client.
    let mut g = MailChimp::new_from_env("", "", "");

    // Let's get the token from the code.
    let t = g.get_access_token(&event.code).await.unwrap();

    // Let's get the metadata.
    let metadata = g.metadata().await.unwrap();

    // Let's get the domain from the email.
    let split = metadata.login.email.split('@');
    let vec: Vec<&str> = split.collect();
    let mut domain = "".to_string();
    if vec.len() > 1 {
        domain = vec.get(1).unwrap().to_string();
    }

    let company = Company::get_from_domain(&api_context.db, &domain);

    // Save the token to the database.
    let mut token = NewAPIToken {
        product: "mailchimp".to_string(),
        token_type: t.token_type.to_string(),
        access_token: t.access_token.to_string(),
        expires_in: t.expires_in as i32,
        refresh_token: t.refresh_token.to_string(),
        refresh_token_expires_in: t.x_refresh_token_expires_in as i32,
        company_id: metadata.accountname.to_string(),
        item_id: "".to_string(),
        user_email: metadata.login.email.to_string(),
        last_updated_at: Utc::now(),
        expires_date: None,
        refresh_token_expires_date: None,
        // Format the endpoint with the dc.
        // https://${server}.api.mailchimp.com
        endpoint: metadata.api_endpoint.to_string(),
        auth_company_id: company.id,
        company: Default::default(),
        // THIS SHOULD ALWAYS BE OXIDE SO THAT IT SAVES TO OUR AIRTABLE.
        cio_company_id: 1,
    };
    token.expand();
    // Update it in the database.
    token.upsert(&api_context.db).await;

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/** Get the consent URL for Gusto auth. */
#[endpoint {
    method = GET,
    path = "/auth/gusto/consent",
}]
async fn listen_auth_gusto_consent(
    _rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseOk<UserConsentURL>, HttpError> {
    sentry::start_session();

    // Initialize the Gusto client.
    let g = Gusto::new_from_env("", "");

    sentry::end_session();
    Ok(HttpResponseOk(UserConsentURL {
        // We don't need to define scopes for Gusto.
        url: g.user_consent_url(&[]),
    }))
}

/** Listen for callbacks to Gusto auth. */
#[endpoint {
    method = GET,
    path = "/auth/gusto/callback",
}]
async fn listen_auth_gusto_callback(
    rqctx: Arc<RequestContext<Context>>,
    query_args: Query<AuthCallback>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();
    let api_context = rqctx.context();
    let event = query_args.into_inner();

    // Initialize the Gusto client.
    let mut g = Gusto::new_from_env("", "");

    // Let's get the token from the code.
    let t = g.get_access_token(&event.code, &event.state).await.unwrap();

    // Let's get the company ID.
    let current_user = g.current_user().get_me().await.unwrap();
    let mut company_id = String::new();
    if let Some(roles) = current_user.roles {
        if let Some(payroll_admin) = roles.payroll_admin {
            company_id = payroll_admin.companies.get(0).unwrap().id.to_string();
        }
    }

    // Let's get the domain from the email.
    let split = current_user.email.split('@');
    let vec: Vec<&str> = split.collect();
    let mut domain = "".to_string();
    if vec.len() > 1 {
        domain = vec.get(1).unwrap().to_string();
    }

    let company = Company::get_from_domain(&api_context.db, &domain);

    // Save the token to the database.
    let mut token = NewAPIToken {
        product: "gusto".to_string(),
        token_type: t.token_type.to_string(),
        access_token: t.access_token.to_string(),
        expires_in: t.expires_in as i32,
        refresh_token: t.refresh_token.to_string(),
        refresh_token_expires_in: t.refresh_token_expires_in as i32,
        company_id: company_id.to_string(),
        item_id: "".to_string(),
        user_email: current_user.email.to_string(),
        last_updated_at: Utc::now(),
        expires_date: None,
        refresh_token_expires_date: None,
        endpoint: "".to_string(),
        auth_company_id: company.id,
        company: Default::default(),
        // THIS SHOULD ALWAYS BE OXIDE SO THAT IT SAVES TO OUR AIRTABLE.
        cio_company_id: 1,
    };
    token.expand();
    // Update it in the database.
    token.upsert(&api_context.db).await;

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/** Listen to deauthorization requests for our Zoom app. */
#[endpoint {
    method = GET,
    path = "/auth/zoom/deauthorization",
}]
async fn listen_auth_zoom_deauthorization(
    _rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<serde_json::Value>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();

    let event = body_param.into_inner();

    sentry::capture_message(
        &format!("zoom deauthorization: {:?}", event),
        sentry::Level::Info,
    );

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/** Get the consent URL for Zoom auth. */
#[endpoint {
    method = GET,
    path = "/auth/zoom/consent",
}]
async fn listen_auth_zoom_consent(
    _rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseOk<UserConsentURL>, HttpError> {
    sentry::start_session();

    // Initialize the Zoom client.
    let g = Zoom::new_from_env("", "");

    sentry::end_session();
    Ok(HttpResponseOk(UserConsentURL {
        url: g.user_consent_url(&[]),
    }))
}

/** Listen for callbacks to Zoom auth. */
#[endpoint {
    method = GET,
    path = "/auth/zoom/callback",
}]
async fn listen_auth_zoom_callback(
    rqctx: Arc<RequestContext<Context>>,
    query_args: Query<AuthCallback>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();
    let api_context = rqctx.context();
    let event = query_args.into_inner();

    sentry::capture_message(&format!("zoom callback: {:?}", event), sentry::Level::Info);

    // Initialize the Zoom client.
    let mut g = Zoom::new_from_env("", "");

    // Let's get the token from the code.
    let t = g.get_access_token(&event.code, &event.state).await.unwrap();

    // TODO: this login type means google but that might not always be true...
    let cu = g
        .users()
        .user("me", zoom_api::types::LoginType::One, false)
        .await
        .unwrap();

    sentry::capture_message(&format!("zoom current user: {:?}", cu), sentry::Level::Info);

    // Let's get the domain from the email.
    let mut domain = "".to_string();
    if !cu.user.email.is_empty() {
        let split = cu.user.email.split('@');
        let vec: Vec<&str> = split.collect();
        if vec.len() > 1 {
            domain = vec.get(1).unwrap().to_string();
        }
    }

    let company = Company::get_from_domain(&api_context.db, &domain);

    // Save the token to the database.
    let mut token = NewAPIToken {
        product: "zoom".to_string(),
        token_type: t.token_type.to_string(),
        access_token: t.access_token.to_string(),
        expires_in: t.expires_in as i32,
        refresh_token: t.refresh_token.to_string(),
        refresh_token_expires_in: t.refresh_token_expires_in as i32,
        company_id: cu.user_response.company.to_string(),
        item_id: "".to_string(),
        user_email: cu.user.email.to_string(),
        last_updated_at: Utc::now(),
        expires_date: None,
        refresh_token_expires_date: None,
        endpoint: "".to_string(),
        auth_company_id: company.id,
        company: Default::default(),
        // THIS SHOULD ALWAYS BE OXIDE SO THAT IT SAVES TO OUR AIRTABLE.
        cio_company_id: 1,
    };
    token.expand();
    // Update it in the database.
    token.upsert(&api_context.db).await;

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/** Get the consent URL for Ramp auth. */
#[endpoint {
    method = GET,
    path = "/auth/ramp/consent",
}]
async fn listen_auth_ramp_consent(
    _rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseOk<UserConsentURL>, HttpError> {
    sentry::start_session();

    // Initialize the Ramp client.
    let g = Ramp::new_from_env("", "");

    sentry::end_session();
    Ok(HttpResponseOk(UserConsentURL {
        url: g.user_consent_url(&[
            "transactions:read".to_string(),
            "users:read".to_string(),
            "users:write".to_string(),
            "receipts:read".to_string(),
            "cards:read".to_string(),
            "departments:read".to_string(),
            "reimbursements:read".to_string(),
        ]),
    }))
}

/** Listen for callbacks to Ramp auth. */
#[endpoint {
    method = GET,
    path = "/auth/ramp/callback",
}]
async fn listen_auth_ramp_callback(
    rqctx: Arc<RequestContext<Context>>,
    query_args: Query<AuthCallback>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();
    let api_context = rqctx.context();
    let event = query_args.into_inner();

    // Initialize the Ramp client.
    let mut g = Ramp::new_from_env("", "");

    // Let's get the token from the code.
    let t = g.get_access_token(&event.code, &event.state).await.unwrap();

    let ru = g
        .users()
        .get_all_users(
            "", // department id
            "", // location id
        )
        .await
        .unwrap();

    sentry::capture_message(&format!("ramp list users: {:?}", ru), sentry::Level::Info);

    // Let's get the domain from the email.
    let mut domain = "".to_string();
    if !ru.is_empty() {
        let split = ru.get(0).unwrap().email.split('@');
        let vec: Vec<&str> = split.collect();
        if vec.len() > 1 {
            domain = vec.get(1).unwrap().to_string();
        }
    }

    let company = Company::get_from_domain(&api_context.db, &domain);

    // Save the token to the database.
    let mut token = NewAPIToken {
        product: "ramp".to_string(),
        token_type: t.token_type.to_string(),
        access_token: t.access_token.to_string(),
        expires_in: t.expires_in as i32,
        refresh_token: t.refresh_token.to_string(),
        refresh_token_expires_in: t.refresh_token_expires_in as i32,
        company_id: "".to_string(),
        item_id: "".to_string(),
        user_email: "".to_string(),
        last_updated_at: Utc::now(),
        expires_date: None,
        refresh_token_expires_date: None,
        endpoint: "".to_string(),
        auth_company_id: company.id,
        company: Default::default(),
        // THIS SHOULD ALWAYS BE OXIDE SO THAT IT SAVES TO OUR AIRTABLE.
        cio_company_id: 1,
    };
    token.expand();
    // Update it in the database.
    token.upsert(&api_context.db).await;

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/** Get the consent URL for Slack auth. */
#[endpoint {
    method = GET,
    path = "/auth/slack/consent",
}]
async fn listen_auth_slack_consent(
    _rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseOk<UserConsentURL>, HttpError> {
    sentry::start_session();

    // Initialize the Slack client.
    let s = Slack::new_from_env("", "", "");

    sentry::end_session();
    Ok(HttpResponseOk(UserConsentURL {
        url: s.user_consent_url(),
    }))
}

/** Listen for callbacks to Slack auth. */
#[endpoint {
    method = GET,
    path = "/auth/slack/callback",
}]
async fn listen_auth_slack_callback(
    rqctx: Arc<RequestContext<Context>>,
    query_args: Query<AuthCallback>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();
    let api_context = rqctx.context();
    let event = query_args.into_inner();

    // Initialize the Slack client.
    let mut s = Slack::new_from_env("", "", "");

    // Let's get the token from the code.
    let t = s.get_access_token(&event.code).await.unwrap();

    // Get the current user.
    let current_user = s.current_user().await.unwrap();

    // Let's get the domain from the email.
    let split = current_user.email.split('@');
    let vec: Vec<&str> = split.collect();
    let mut domain = "".to_string();
    if vec.len() > 1 {
        domain = vec.get(1).unwrap().to_string();
    }

    let company = Company::get_from_domain(&api_context.db, &domain);

    let mut webhook = "".to_string();
    if let Some(wh) = t.incoming_webhook {
        webhook = wh.url;
    }

    // Save the bot token to the database.
    let mut token = NewAPIToken {
        product: "slack".to_string(),
        token_type: t.token_type.to_string(),
        access_token: t.access_token.to_string(),
        expires_in: 0,
        refresh_token: "".to_string(),
        refresh_token_expires_in: 0,
        company_id: t.team.id.to_string(),
        item_id: t.team.name.to_string(),
        user_email: current_user.email.to_string(),
        last_updated_at: Utc::now(),
        expires_date: None,
        refresh_token_expires_date: None,
        endpoint: webhook.to_string(),
        auth_company_id: company.id,
        company: Default::default(),
        // THIS SHOULD ALWAYS BE OXIDE SO THAT IT SAVES TO OUR AIRTABLE.
        cio_company_id: 1,
    };
    token.expand();

    // Update it in the database.
    let mut new_token = if let Ok(existing) = api_tokens::dsl::api_tokens
        .filter(
            api_tokens::dsl::cio_company_id
                .eq(1)
                .and(api_tokens::dsl::product.eq("slack".to_string()))
                .and(api_tokens::dsl::auth_company_id.eq(company.id))
                .and(api_tokens::dsl::token_type.eq(token.token_type.to_string())),
        )
        .first::<APIToken>(&api_context.db.conn())
    {
        diesel::update(&existing)
            .set(token)
            .get_result::<APIToken>(&api_context.db.conn())
            .unwrap_or_else(|e| panic!("unable to update record {}: {}", existing.id, e))
    } else {
        token.create_in_db(&api_context.db)
    };
    new_token.upsert_in_airtable(&api_context.db).await;

    // Save the user token to the database.
    if let Some(authed_user) = t.authed_user {
        let mut user_token = NewAPIToken {
            product: "slack".to_string(),
            token_type: authed_user.token_type.to_string(),
            access_token: authed_user.access_token.to_string(),
            expires_in: 0,
            refresh_token: "".to_string(),
            refresh_token_expires_in: 0,
            company_id: t.team.id.to_string(),
            item_id: t.team.name.to_string(),
            user_email: current_user.email.to_string(),
            last_updated_at: Utc::now(),
            expires_date: None,
            refresh_token_expires_date: None,
            endpoint: webhook.to_string(),
            auth_company_id: company.id,
            company: Default::default(),
            // THIS SHOULD ALWAYS BE OXIDE SO THAT IT SAVES TO OUR AIRTABLE.
            cio_company_id: 1,
        };
        user_token.expand();

        // Update it in the database.
        let mut new_user_token = if let Ok(existing) = api_tokens::dsl::api_tokens
            .filter(
                api_tokens::dsl::cio_company_id
                    .eq(1)
                    .and(api_tokens::dsl::product.eq("slack".to_string()))
                    .and(api_tokens::dsl::auth_company_id.eq(company.id))
                    .and(api_tokens::dsl::token_type.eq(user_token.token_type.to_string())),
            )
            .first::<APIToken>(&api_context.db.conn())
        {
            diesel::update(&existing)
                .set(user_token)
                .get_result::<APIToken>(&api_context.db.conn())
                .unwrap_or_else(|e| panic!("unable to update record {}: {}", existing.id, e))
        } else {
            user_token.create_in_db(&api_context.db)
        };
        new_user_token.upsert_in_airtable(&api_context.db).await;
    }

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/** Get the consent URL for QuickBooks auth. */
#[endpoint {
    method = GET,
    path = "/auth/quickbooks/consent",
}]
async fn listen_auth_quickbooks_consent(
    _rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseOk<UserConsentURL>, HttpError> {
    sentry::start_session();

    // Initialize the QuickBooks client.
    let g = QuickBooks::new_from_env("", "", "");

    sentry::end_session();
    Ok(HttpResponseOk(UserConsentURL {
        url: g.user_consent_url(),
    }))
}

/** Listen for callbacks to QuickBooks auth. */
#[endpoint {
    method = GET,
    path = "/auth/quickbooks/callback",
}]
async fn listen_auth_quickbooks_callback(
    rqctx: Arc<RequestContext<Context>>,
    query_args: Query<AuthCallback>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();
    let api_context = rqctx.context();
    let event = query_args.into_inner();

    // Initialize the QuickBooks client.
    let mut qb = QuickBooks::new_from_env("", "", "");

    // Let's get the token from the code.
    let t = qb.get_access_token(&event.code).await.unwrap();

    // Get the company info.
    let company_info = qb.company_info(&event.realm_id).await.unwrap();

    // Let's get the domain from the email.
    let split = company_info.email.address.split('@');
    let vec: Vec<&str> = split.collect();
    let mut domain = "".to_string();
    if vec.len() > 1 {
        domain = vec.get(1).unwrap().to_string();
    }

    let company = Company::get_from_domain(&api_context.db, &domain);

    // Save the token to the database.
    let mut token = NewAPIToken {
        product: "quickbooks".to_string(),
        token_type: t.token_type.to_string(),
        access_token: t.access_token.to_string(),
        expires_in: t.expires_in as i32,
        refresh_token: t.refresh_token.to_string(),
        refresh_token_expires_in: t.x_refresh_token_expires_in as i32,
        company_id: event.realm_id.to_string(),
        item_id: "".to_string(),
        user_email: company_info.email.address.to_string(),
        last_updated_at: Utc::now(),
        expires_date: None,
        refresh_token_expires_date: None,
        endpoint: "".to_string(),
        auth_company_id: company.id,
        company: Default::default(),
        // THIS SHOULD ALWAYS BE OXIDE SO THAT IT SAVES TO OUR AIRTABLE.
        cio_company_id: 1,
    };
    token.expand();

    // Update it in the database.
    token.upsert(&api_context.db).await;

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/** Listen for webhooks from Plaid. */
#[endpoint {
    method = POST,
    path = "/plaid",
}]
async fn listen_auth_plaid_callback(
    _rqctx: Arc<RequestContext<Context>>,
    body_args: TypedBody<serde_json::Value>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();
    let event = body_args.into_inner();

    sentry::capture_message(&format!("plaid callback: {:?}", event), sentry::Level::Info);

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/** Get the consent URL for DocuSign auth. */
#[endpoint {
    method = GET,
    path = "/auth/docusign/consent",
}]
async fn listen_auth_docusign_consent(
    _rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseOk<UserConsentURL>, HttpError> {
    sentry::start_session();

    // Initialize the DocuSign client.
    let g = DocuSign::new_from_env("", "", "", "");

    sentry::end_session();
    Ok(HttpResponseOk(UserConsentURL {
        url: g.user_consent_url(),
    }))
}

/** Listen for callbacks to DocuSign auth. */
#[endpoint {
    method = GET,
    path = "/auth/docusign/callback",
}]
async fn listen_auth_docusign_callback(
    rqctx: Arc<RequestContext<Context>>,
    query_args: Query<AuthCallback>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();
    let api_context = rqctx.context();
    let event = query_args.into_inner();

    // Initialize the DocuSign client.
    let mut d = DocuSign::new_from_env("", "", "", "");
    // Let's get the token from the code.
    let t = d.get_access_token(&event.code).await.unwrap();

    // Let's get the user's info as well.
    let user_info = d.get_user_info().await.unwrap();

    // Let's get the domain from the email.
    let split = user_info.email.split('@');
    let vec: Vec<&str> = split.collect();
    let mut domain = "".to_string();
    if vec.len() > 1 {
        domain = vec.get(1).unwrap().to_string();
    }

    let company = Company::get_from_domain(&api_context.db, &domain);

    // Save the token to the database.
    let mut token = NewAPIToken {
        product: "docusign".to_string(),
        token_type: t.token_type.to_string(),
        access_token: t.access_token.to_string(),
        expires_in: t.expires_in as i32,
        refresh_token: t.refresh_token.to_string(),
        refresh_token_expires_in: t.x_refresh_token_expires_in as i32,
        company_id: user_info.accounts[0].account_id.to_string(),
        endpoint: user_info.accounts[0].base_uri.to_string(),
        item_id: "".to_string(),
        user_email: user_info.email.to_string(),
        last_updated_at: Utc::now(),
        expires_date: None,
        refresh_token_expires_date: None,
        auth_company_id: company.id,
        company: Default::default(),
        // THIS SHOULD ALWAYS BE OXIDE SO THAT IT SAVES TO OUR AIRTABLE.
        cio_company_id: 1,
    };
    token.expand();

    // Update it in the database.
    token.upsert(&api_context.db).await;

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/** Listen for updates to our docusign envelopes. */
#[endpoint {
    method = POST,
    path = "/docusign/envelope/update",
}]
async fn listen_docusign_envelope_update_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<docusign::Envelope>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();
    let api_context = rqctx.context();
    let db = &api_context.db;

    let event = body_param.into_inner();

    // We need to get the applicant for the envelope.
    // Check their offer first.
    let result = applicants::dsl::applicants
        .filter(applicants::dsl::docusign_envelope_id.eq(event.envelope_id.to_string()))
        .first::<Applicant>(&db.conn());
    match result {
        Ok(mut applicant) => {
            let company = applicant.company(db);

            // Create our docusign client.
            let dsa = company.authenticate_docusign(db).await;
            if let Some(ds) = dsa {
                applicant
                    .update_applicant_from_docusign_offer_envelope(db, &ds, event.clone())
                    .await;
            }
        }
        Err(e) => {
            println!(
                "database could not find applicant with docusign offer envelope id {}: {}",
                event.envelope_id, e
            );
        }
    }

    // We need to get the applicant for the envelope.
    // Now do PIIA.
    let result = applicants::dsl::applicants
        .filter(applicants::dsl::docusign_piia_envelope_id.eq(event.envelope_id.to_string()))
        .first::<Applicant>(&db.conn());
    match result {
        Ok(mut applicant) => {
            let company = applicant.company(db);

            // Create our docusign client.
            let dsa = company.authenticate_docusign(db).await;
            if let Some(ds) = dsa {
                applicant
                    .update_applicant_from_docusign_piia_envelope(db, &ds, event)
                    .await;
            }
        }
        Err(e) => {
            println!(
                "database could not find applicant with docusign piia envelope id {}: {}",
                event.envelope_id, e
            );
        }
    }

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/** Listen for analytics page view events. */
#[endpoint {
    method = POST,
    path = "/analytics/page_view",
}]
async fn listen_analytics_page_view_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<NewPageView>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();
    let api_context = rqctx.context();
    let db = &api_context.db;

    let mut event = body_param.into_inner();
    println!("{:?}", event);

    // Expand the page_view.
    event.set_page_link();
    event.set_company_id(db);

    // Add the page_view to the database and Airttable.
    let pv = event.create(db).await;

    println!(
        "page_view `{} | {}` created successfully",
        pv.page_link, pv.user_email
    );
    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/** Ping endpoint for MailChimp mailing list webhooks. */
#[endpoint {
    method = GET,
    path = "/mailchimp/mailing_list",
}]
async fn ping_mailchimp_mailing_list_webhooks(
    _rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseOk<String>, HttpError> {
    Ok(HttpResponseOk("ok".to_string()))
}

/** Listen for MailChimp mailing list webhooks. */
#[endpoint {
    method = POST,
    path = "/mailchimp/mailing_list",
}]
async fn listen_mailchimp_mailing_list_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: UntypedBody,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();
    let api_context = rqctx.context();
    let db = &api_context.db;

    // We should have a string, which we will then parse into our args.
    let event_string = body_param.as_str().unwrap().to_string();
    println!("{}", event_string);
    let qs_non_strict = QSConfig::new(10, false);

    let event: MailChimpWebhook = qs_non_strict.deserialize_str(&event_string).unwrap();
    println!("mailchimp {:?}", event);

    if event.webhook_type != *"subscribe" {
        println!("not a `subscribe` event, got `{}`", event.webhook_type);
        return Ok(HttpResponseAccepted("ok".to_string()));
    }

    // Parse the webhook as a new mailing list subscriber.
    let new_subscriber = cio_api::mailing_list::as_mailing_list_subscriber(event, db);

    let existing = MailingListSubscriber::get_from_db(db, new_subscriber.email.to_string());
    if existing.is_none() {
        // Update the subscriber in the database.
        let subscriber = new_subscriber.upsert(db).await;

        println!("subscriber {} created successfully", subscriber.email);
    } else {
        println!("subscriber {} already exists", new_subscriber.email);
    }

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/** Ping endpoint for MailChimp rack line webhooks. */
#[endpoint {
    method = GET,
    path = "/mailchimp/rack_line",
}]
async fn ping_mailchimp_rack_line_webhooks(
    _rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseOk<String>, HttpError> {
    Ok(HttpResponseOk("ok".to_string()))
}

/** Listen for MailChimp rack line webhooks. */
#[endpoint {
    method = POST,
    path = "/mailchimp/rack_line",
}]
async fn listen_mailchimp_rack_line_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: UntypedBody,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();
    let api_context = rqctx.context();
    let db = &api_context.db;

    // We should have a string, which we will then parse into our args.
    let event_string = body_param.as_str().unwrap().to_string();
    println!("{}", event_string);
    let qs_non_strict = QSConfig::new(10, false);

    let event: MailChimpWebhook = qs_non_strict.deserialize_str(&event_string).unwrap();
    println!("mailchimp {:?}", event);

    if event.webhook_type != *"subscribe" {
        println!("not a `subscribe` event, got `{}`", event.webhook_type);
        return Ok(HttpResponseAccepted("ok".to_string()));
    }

    // Parse the webhook as a new rack line subscriber.
    let new_subscriber = cio_api::rack_line::as_rack_line_subscriber(event, db);

    // let company = Company::get_by_id(db, new_subscriber.cio_company_id);

    let existing = RackLineSubscriber::get_from_db(db, new_subscriber.email.to_string());
    if existing.is_none() {
        // Update the subscriber in the database.
        let subscriber = new_subscriber.upsert(db).await;

        // Parse the signup into a slack message.
        // Send the message to the slack channel.
        //company.post_to_slack_channel(db, new_subscriber.as_slack_msg()).await;
        println!("subscriber {} posted to Slack", subscriber.email);

        println!("subscriber {} created successfully", subscriber.email);
    } else {
        println!("subscriber {} already exists", new_subscriber.email);
    }

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/** Listen for Slack commands webhooks. */
#[endpoint {
    method = POST,
    path = "/slack/commands",
}]
async fn listen_slack_commands_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: UntypedBody,
) -> Result<HttpResponseOk<serde_json::Value>, HttpError> {
    sentry::start_session();
    let api_context = rqctx.context();
    let db = &api_context.db;

    // We should have a string, which we will then parse into our args.
    // Parse the request body as a Slack BotCommand.
    let bot_command: BotCommand = serde_urlencoded::from_bytes(body_param.as_bytes()).unwrap();

    // Get the company from the Slack team id.
    let company = Company::get_from_slack_team_id(db, &bot_command.team_id);

    // Get the command type.
    let command = SlackCommand::from_str(&bot_command.command).unwrap();
    let text = bot_command.text.trim();

    // Filter by command type and do the command.
    let response = match command {
        SlackCommand::RFD => {
            let num = text.parse::<i32>().unwrap_or(0);
            if num > 0 {
                if let Ok(rfd) = rfds::dsl::rfds
                    .filter(
                        rfds::dsl::cio_company_id
                            .eq(company.id)
                            .and(rfds::dsl::number.eq(num)),
                    )
                    .first::<RFD>(&db.conn())
                {
                    json!(MessageResponse {
                        response_type: MessageResponseType::InChannel,
                        text: rfd.as_slack_msg(),
                    })
                } else if let Ok(rfd) = rfds::dsl::rfds
                    .filter(
                        rfds::dsl::cio_company_id
                            .eq(company.id)
                            .and(rfds::dsl::name.ilike(format!("%{}%", text))),
                    )
                    .first::<RFD>(&db.conn())
                {
                    json!(MessageResponse {
                        response_type: MessageResponseType::InChannel,
                        text: rfd.as_slack_msg(),
                    })
                } else {
                    json!(MessageResponse {
                        response_type: MessageResponseType::InChannel,
                        text: format!(
                            "Sorry <@{}> :scream: I could not find an RFD matching `{}`",
                            bot_command.user_id, text
                        ),
                    })
                }
            } else if let Ok(rfd) = rfds::dsl::rfds
                .filter(
                    rfds::dsl::cio_company_id
                        .eq(company.id)
                        .and(rfds::dsl::name.ilike(format!("%{}%", text))),
                )
                .first::<RFD>(&db.conn())
            {
                json!(MessageResponse {
                    response_type: MessageResponseType::InChannel,
                    text: rfd.as_slack_msg(),
                })
            } else {
                json!(MessageResponse {
                    response_type: MessageResponseType::InChannel,
                    text: format!(
                        "Sorry <@{}> :scream: I could not find an RFD matching `{}`",
                        bot_command.user_id, text
                    ),
                })
            }
        }
        SlackCommand::Meet => {
            let mut name = text.replace(" ", "-");
            if name.is_empty() {
                // Generate a new random string.
                name = thread_rng()
                    .sample_iter(&Alphanumeric)
                    .take(6)
                    .map(char::from)
                    .collect();
            }

            json!(MessageResponse {
                response_type: MessageResponseType::InChannel,
                text: format!("https://g.co/meet/oxide-{}", name.to_lowercase()),
            })
        }
        SlackCommand::Applicants => {
            // Get the applicants that need to be triaged.
            let applicants = applicants::dsl::applicants
                .filter(
                    applicants::dsl::cio_company_id.eq(company.id).and(
                        applicants::dsl::status
                            .eq(cio_api::applicant_status::Status::NeedsToBeTriaged.to_string()),
                    ),
                )
                .load::<Applicant>(&db.conn())
                .unwrap();

            let mut msg: serde_json::Value = Default::default();
            for (i, a) in applicants.into_iter().enumerate() {
                if i > 0 {
                    // Merge a divider onto the stack.
                    let object = json!({
                        "blocks": [{
                            "type": "divider"
                        }]
                    });

                    merge_json(&mut msg, object);
                }

                let obj = a.as_slack_msg();
                merge_json(&mut msg, obj);
            }

            sentry::capture_message(
                &format!("applicants -> {}", msg.to_string()),
                sentry::Level::Info,
            );

            msg
        }
        SlackCommand::Applicant => {
            if let Ok(applicant) = applicants::dsl::applicants
                .filter(
                    applicants::dsl::cio_company_id
                        .eq(company.id)
                        .and(applicants::dsl::name.ilike(format!("%{}%", text))),
                )
                .first::<Applicant>(&db.conn())
            {
                applicant.as_slack_msg()
            } else {
                json!(MessageResponse {
                    response_type: MessageResponseType::InChannel,
                    text: format!(
                        "Sorry <@{}> :scream: I could not find an applicant matching `{}`",
                        bot_command.user_id, text
                    ),
                })
            }
        }
        SlackCommand::Papers => {
            // If we asked for the closed meetings then only show those, otherwise
            // default to the open meetings.
            let mut state = "open";
            if text == "closed" {
                state = "closed";
            }
            let meetings = journal_club_meetings::dsl::journal_club_meetings
                .filter(
                    journal_club_meetings::dsl::cio_company_id
                        .eq(company.id)
                        .and(journal_club_meetings::dsl::state.eq(state.to_string())),
                )
                .load::<JournalClubMeeting>(&db.conn())
                .unwrap();

            let mut msg: serde_json::Value = Default::default();
            for (i, m) in meetings.into_iter().enumerate() {
                if i > 0 {
                    // Merge a divider onto the stack.
                    let object = json!({
                        "blocks": [{
                            "type": "divider"
                        }]
                    });

                    merge_json(&mut msg, object);
                }

                let obj = m.as_slack_msg();
                merge_json(&mut msg, obj);
            }

            sentry::capture_message(
                &format!("papers -> {}", msg.to_string()),
                sentry::Level::Info,
            );
            msg
        }
        SlackCommand::Paper => {
            if let Ok(meeting) = journal_club_meetings::dsl::journal_club_meetings
                .filter(
                    journal_club_meetings::dsl::cio_company_id
                        .eq(company.id)
                        .and(journal_club_meetings::dsl::title.ilike(format!("%{}%", text))),
                )
                .first::<JournalClubMeeting>(&db.conn())
            {
                meeting.as_slack_msg()
            } else {
                json!(MessageResponse {
                    response_type: MessageResponseType::InChannel,
                    text: format!(
                        "Sorry <@{}> :scream: I could not find a journal club meeting matching \
                         `{}`",
                        bot_command.user_id, text
                    ),
                })
            }
        }
    };

    sentry::end_session();
    Ok(HttpResponseOk(response))
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
    #[serde(default)]
    pub repository: GitHubRepo,
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

/// A GitHub repository.
/// FROM: https://docs.github.com/en/free-pro-team@latest/developers/webhooks-and-events/webhook-events-and-payloads#push
#[derive(Debug, Clone, Default, PartialEq, JsonSchema, Deserialize, Serialize)]
pub struct GitHubRepo {
    #[serde(default)]
    pub owner: GitHubUser,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "deserialize_null_string::deserialize"
    )]
    pub name: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "deserialize_null_string::deserialize"
    )]
    pub full_name: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "deserialize_null_string::deserialize"
    )]
    pub default_branch: String,
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

    /// Return if a specific file was added, modified, or removed in a commit.
    pub fn file_changed(&self, file: &str) -> bool {
        self.added.contains(&file.to_string())
            || self.modified.contains(&file.to_string())
            || self.removed.contains(&file.to_string())
    }
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

/// A octorust::Client issue.
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

/// A octorust::Client comment.
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
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "deserialize_null_string::deserialize"
    )]
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
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "deserialize_null_string::deserialize"
    )]
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

fn filter(files: &[String], dir: &str) -> Vec<String> {
    let mut in_dir: Vec<String> = Default::default();
    for file in files {
        if file.starts_with(dir) {
            in_dir.push(file.to_string());
        }
    }

    in_dir
}

/// Handle a `pull_request` event for the rfd repo.
async fn handle_rfd_pull_request(
    github: &octorust::Client,
    api_context: &Context,
    event: GitHubWebhook,
    company: &Company,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    let db = &api_context.db;

    let owner = &company.github_org;
    let repo = "rfd";

    // Let's get the RFD.
    let branch = event.pull_request.head.commit_ref.to_string();

    // Check if we somehow had a pull request opened from the default branch.
    // This should never happen, but let's check regardless.
    if branch == event.repository.default_branch {
        // Return early.
        println!(
            "event was to the default branch `{}`, we don't care: {:?}",
            event.repository.default_branch, event
        );
        return Ok(HttpResponseAccepted("ok".to_string()));
    }

    // The branch should be equivalent to the number in the database.
    // Let's try to get the RFD from that.
    let number = branch
        .trim_start_matches('0')
        .parse::<i32>()
        .unwrap_or_default();
    // Make sure we actually have a number.
    if number == 0 {
        // Return early.
        println!(
            "event was to the branch `{}`, which is not a number so it cannot be an RFD: {:?}",
            branch, event
        );
        return Ok(HttpResponseAccepted("ok".to_string()));
    }

    // Try to get the RFD from the database.
    let result = RFD::get_from_db(db, number);
    if result.is_none() {
        println!(
            "could not find RFD with number `{}` in the database: {:?}",
            number, event
        );
        return Ok(HttpResponseAccepted("ok".to_string()));
    }
    let mut rfd = result.unwrap();

    // Let's make sure the title of the pull request is what it should be.
    // The pull request title should be equal to the name of the pull request.
    if rfd.name != event.pull_request.title {
        // Get the current set of settings for the pull request.
        // We do this because we want to keep the current state for body.
        let pull = github
            .pulls()
            .get(owner, repo, event.pull_request.number)
            .await
            .unwrap();

        // Update the title of the pull request.
        match github
            .pulls()
            .update(
                owner,
                repo,
                event.pull_request.number,
                &octorust::types::PullsUpdateRequest {
                    title: rfd.name.to_string(),
                    body: pull.body.to_string(),
                    base: "".to_string(),
                    maintainer_can_modify: None,
                    state: None,
                },
            )
            .await
        {
            Ok(_) => (),
            Err(e) => {
                sentry::capture_message(
                    &format!(
                        "unable to update title of pull request from `{}` to `{}` for pr#{}: {}, \
                         {:?} {}",
                        event.pull_request.title,
                        rfd.name,
                        event.pull_request.number,
                        e,
                        rfd,
                        number
                    ),
                    sentry::Level::Fatal,
                );
            }
        }
    }

    // Update the labels for the pull request.
    let mut labels: Vec<String> = Default::default();
    if rfd.state == "discussion" {
        labels.push(":thought_balloon: discussion".to_string());
    } else if rfd.state == "ideation" {
        labels.push(":hatching_chick: ideation".to_string());
    }
    github
        .issues()
        .add_labels(
            owner,
            repo,
            event.pull_request.number,
            &octorust::types::IssuesAddLabelsRequestOneOf::StringVector(labels),
        )
        .await
        .unwrap();

    // We only care if the pull request was `opened`.
    if event.action != "opened" {
        // We can throw this out, log it and return early.
        println!(
            "no automations are set up for action `{}` yet",
            event.action
        );
        return Ok(HttpResponseAccepted("ok".to_string()));
    }

    // Okay, now we finally have the RFD.
    // We need to do two things.
    //  1. Update the discussion link.
    //  2. Update the state of the RFD to be in discussion if it is not
    //      in an acceptable current state. More on this below.
    // To do both these tasks we need to first get the path of the file on GitHub,
    // so we can update it later, and also find out if it is markdown or not for parsing.

    // Get the file path from GitHub.
    // We need to figure out whether this file is a README.adoc or README.md
    // before we update it.
    // Let's get the contents of the directory from GitHub.
    let dir = format!("/rfd/{}", branch);
    // Get the contents of the file.
    let mut path = format!("{}/README.adoc", dir);
    match github
        .repos()
        .get_content_file(owner, repo, &path, &branch)
        .await
    {
        Ok(contents) => {
            rfd.content = decode_base64_to_string(&contents.content);
            rfd.sha = contents.sha;
        }
        Err(e) => {
            println!(
                "[rfd] getting file contents for {} on branch {} failed: {}, trying markdown \
                 instead...",
                path, branch, e
            );

            // Try to get the markdown instead.
            path = format!("{}/README.md", dir);
            let contents = github
                .repos()
                .get_content_file(owner, repo, &path, &branch)
                .await
                .unwrap_or_else(|e| {
                    panic!(
                        "getting file contents for {} on branch {} failed: {}",
                        path, branch, e
                    )
                });

            rfd.content = decode_base64_to_string(&contents.content);
            rfd.sha = contents.sha;
        }
    }

    // Update the discussion link.
    let discussion_link = event.pull_request.html_url;
    rfd.update_discussion(&discussion_link, path.ends_with(".md"));

    // A pull request can be open for an RFD if it is in the following states:
    //  - published: a already published RFD is being updated in a pull request.
    //  - discussion: it is in discussion
    //  - ideation: it is in ideation
    // We can update the state if it is not currently in an acceptable state.
    if rfd.state != "discussion" && rfd.state != "published" && rfd.state != "ideation" {
        //  Update the state of the RFD in GitHub to show it as `discussion`.
        rfd.update_state("discussion", path.ends_with(".md"));
    }

    // Update the RFD to show the new state and link in the database.
    rfd.update(db).await;

    // Update the file in GitHub.
    // Keep in mind: this push will kick off another webhook.
    create_or_update_file_in_github_repo(
        github,
        owner,
        repo,
        &branch,
        &path,
        rfd.content.as_bytes().to_vec(),
    )
    .await;

    println!("updated discussion link for RFD {}", rfd.number_string,);
    Ok(HttpResponseAccepted("ok".to_string()))
}

/// Handle a `push` event for the rfd repo.
async fn handle_rfd_push(
    github: &octorust::Client,
    api_context: &Context,
    event: GitHubWebhook,
    company: &Company,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    let db = &api_context.db;

    // Get gsuite token.
    let token = company.authenticate_google(db).await;

    // Initialize the Google Drive client.
    let drive = GoogleDrive::new(token);

    // Figure out where our directory is.
    // It should be in the shared drive : "Automated Documents"/"rfds"
    let shared_drive = drive
        .get_drive_by_name("Automated Documents")
        .await
        .unwrap();

    // Get the repo.
    let owner = &company.github_org;
    let repo = event.repository.name.to_string();

    // Get the commit.
    let mut commit = event.commits.get(0).unwrap().clone();

    // Ignore any changes that are not to the `rfd/` directory.
    let dir = "rfd/";
    commit.filter_files_by_path(dir);
    if !commit.has_changed_files() {
        // No files changed that we care about.
        // We can throw this out, log it and return early.
        println!(
            "`push` event commit `{}` does not include any changes to the `{}` directory",
            commit.id, dir
        );
        return Ok(HttpResponseAccepted("ok".to_string()));
    }

    // Get the branch name.
    let branch = event.refv.trim_start_matches("refs/heads/");

    // Iterate over the removed files and remove any images that we no longer
    // need for the HTML rendered RFD website.
    for file in commit.removed {
        // Make sure the file has a prefix of "rfd/".
        if !file.starts_with("rfd/") {
            // Continue through the loop early.
            // We only care if a file change in the rfd/ directory.
            continue;
        }

        if is_image(&file) {
            // Remove the image from the `src/public/static/images` path since we no
            // longer need it.
            // We delete these on the default branch ONLY.
            let website_file = file.replace("rfd/", "src/public/static/images/");

            // We need to get the current sha for the file we want to delete.
            let (_, gh_file_sha) = get_file_content_from_repo(
                github,
                owner,
                &repo,
                &website_file,
                &event.repository.default_branch,
            )
            .await;

            if !gh_file_sha.is_empty() {
                github
                    .repos()
                    .delete_file(
                        owner,
                        &repo,
                        &website_file,
                        &octorust::types::ReposDeleteFileRequest {
                            message: format!(
                                "Deleting file content {} programatically\n\nThis is done from \
                                 the cio repo webhooky::listen_github_webhooks function.",
                                website_file
                            ),
                            sha: gh_file_sha,
                            committer: None,
                            author: None,
                            branch: event.repository.default_branch.to_string(),
                        },
                    )
                    .await
                    .unwrap();
                println!(
                    "deleted file `{}` since it was removed in mose recent push for RFD {:?}",
                    website_file, event
                );
            }
        }
    }

    // Iterate over the files and update the RFDs that have been added or
    // modified in our database.
    let mut changed_files = commit.added.clone();
    changed_files.append(&mut commit.modified.clone());
    for file in changed_files {
        // Make sure the file has a prefix of "rfd/".
        if !file.starts_with("rfd/") {
            // Continue through the loop early.
            // We only care if a file change in the rfd/ directory.
            continue;
        }

        // Update images for the static site.
        if is_image(&file) {
            // Some image for an RFD updated. Let's make sure we have that image in the right place
            // for the RFD shared site.
            // First, let's read the file contents.
            let (gh_file_content, _) =
                get_file_content_from_repo(github, owner, &repo, &file, branch).await;

            // Let's write the file contents to the location for the static website.
            // We replace the `rfd/` path with the `src/public/static/images/` path since
            // this is where images go for the static website.
            // We update these on the default branch ONLY
            let website_file = file.replace("rfd/", "src/public/static/images/");
            create_or_update_file_in_github_repo(
                github,
                owner,
                &repo,
                &event.repository.default_branch,
                &website_file,
                gh_file_content,
            )
            .await;
            println!(
                "updated file `{}` since it was modified in mose recent push for RFD {:?}",
                website_file, event
            );
            // We are done so we can continue throught the loop.
            continue;
        }

        // If the file is a README.md or README.adoc, an RFD doc changed, let's handle it.
        if file.ends_with("README.md") || file.ends_with("README.adoc") {
            // We have a README file that changed, let's parse the RFD and update it
            // in our database.
            println!(
                "`push` event -> file {} was modified on branch {}",
                file, branch,
            );
            // Parse the RFD.
            let new_rfd = NewRFD::new_from_github(
                company,
                github,
                owner,
                &repo,
                branch,
                &file,
                commit.timestamp.unwrap(),
            )
            .await;

            // Get the old RFD from the database.
            // DO THIS BEFORE UPDATING THE RFD.
            // We will need this later to check if the RFD's state changed.
            let old_rfd = RFD::get_from_db(db, new_rfd.number);
            let mut old_rfd_state = "".to_string();
            let mut old_rfd_pdf = "".to_string();
            if let Some(o) = old_rfd {
                old_rfd_state = o.state.to_string();
                old_rfd_pdf = o.get_pdf_filename();
            }

            // Update the RFD in the database.
            let mut rfd = new_rfd.upsert(db).await;
            // Update all the fields for the RFD.
            rfd.expand(github, company).await;
            rfd.update(db).await;
            println!("updated RFD {} in the database", new_rfd.number_string);
            println!("updated airtable for RFD {}", new_rfd.number_string);

            // Create all the shorturls for the RFD if we need to,
            // this would be on added files, only.
            generate_shorturls_for_rfds(db, github, &company.github_org, "configs", company.id)
                .await;
            println!("generated shorturls for the rfds");

            // Update the PDFs for the RFD.
            rfd.convert_and_upload_pdf(github, &drive, company).await;
            rfd.update(db).await;
            println!(
                "updated pdf `{}` for RFD {}",
                new_rfd.number_string,
                rfd.get_pdf_filename()
            );

            // Check if the RFD state changed from what is currently in the
            // database.
            // If the RFD's state was changed to `discussion`, we need to open a PR
            // for that RFD.
            // Make sure we are not on the default branch, since then we would not need
            // a PR. Instead, below, the state of the RFD would be moved to `published`.
            // TODO: see if we drop events, if we do, we might want to remove the check with
            // the old state and just do it everytime an RFD is in discussion.
            if old_rfd_state != rfd.state
                && rfd.state == "discussion"
                && branch != event.repository.default_branch
            {
                // First, we need to make sure we don't already have a pull request open.
                let pulls = github
                    .pulls()
                    .list_all(
                        owner,
                        &repo,
                        octorust::types::IssuesListState::All,
                        // head
                        "",
                        // base
                        "",
                        // sort
                        Default::default(),
                        // direction
                        Default::default(),
                    )
                    .await
                    .unwrap();
                // Check if any pull requests are from our branch.
                let mut has_pull = false;
                for pull in pulls {
                    // Check if the pull request is for our branch.
                    let pull_branch = pull.head.ref_.trim_start_matches("refs/heads/");

                    if pull_branch == branch {
                        println!(
                            "RFD {} has moved from state {} -> {}, on branch {}, we already have \
                             a pull request: {}",
                            rfd.number_string, old_rfd_state, rfd.state, branch, pull.html_url
                        );

                        has_pull = true;
                        break;
                    }
                }

                // Open a pull request, if we don't already have one.
                if !has_pull {
                    github
                        .pulls()
                        .create(
                            owner,
                            &repo,
                            &octorust::types::PullsCreateRequest {
                                title: rfd.name.to_string(),
                                head: format!("{}:{}", company.github_org, branch),
                                base: event.repository.default_branch.to_string(),
                                body: "Automatically opening the pull request since the document \
                                       is marked as being in discussion. If you wish to not have \
                                       a pull request open, change the state of your document and \
                                       close this pull request."
                                    .to_string(),
                                draft: Some(false),
                                maintainer_can_modify: Some(true),
                                issue: 0,
                            },
                        )
                        .await
                        .unwrap();
                    println!("opened pull request for RFD {}", new_rfd.number_string);

                    // We could update the discussion link here, but we will already
                    // trigger a `pull_request` `opened` event, so we might as well let
                    // that do its thing.
                }
            }

            // If the RFD was merged into the default branch, but the RFD state is not `published`,
            // update the state of the RFD in GitHub to show it as `published`.
            if branch == event.repository.default_branch && rfd.state != "published" {
                //  Update the state of the RFD in GitHub to show it as `published`.
                let mut rfd_mut = rfd.clone();
                rfd_mut.update_state("published", file.ends_with(".md"));

                // Update the RFD to show the new state in the database.
                rfd_mut.update(db).await;

                // Update the file in GitHub.
                // Keep in mind: this push will kick off another webhook.
                create_or_update_file_in_github_repo(
                    github,
                    owner,
                    &repo,
                    branch,
                    &file,
                    rfd_mut.content.as_bytes().to_vec(),
                )
                .await;
                println!(
                    "updated state to `published` for  RFD {}",
                    new_rfd.number_string
                );
            }

            // If the title of the RFD changed, delete the old PDF file so it
            // doesn't linger in GitHub and Google Drive.
            if old_rfd_pdf != rfd.get_pdf_filename() {
                let pdf_path = format!("/pdfs/{}", old_rfd_pdf);

                // First get the sha of the old pdf.
                let (_, old_pdf_sha) = get_file_content_from_repo(
                    github,
                    owner,
                    &repo,
                    &pdf_path,
                    &event.repository.default_branch,
                )
                .await;

                if !old_pdf_sha.is_empty() {
                    // Delete the old filename from GitHub.
                    github
                        .repos()
                        .delete_file(
                            owner,
                            &repo,
                            &pdf_path,
                            &octorust::types::ReposDeleteFileRequest {
                                message: format!(
                                    "Deleting file content {} programatically\n\nThis is done \
                                     from the cio repo webhooky::listen_github_webhooks function.",
                                    pdf_path
                                ),
                                sha: old_pdf_sha,
                                committer: None,
                                author: None,
                                branch: event.repository.default_branch.to_string(),
                            },
                        )
                        .await
                        .unwrap();
                }

                // Delete the old filename from drive.
                drive
                    .delete_file_by_name(&shared_drive.id, &old_rfd_pdf)
                    .await
                    .unwrap();
            }

            println!("RFD {} `push` operations completed", new_rfd.number_string);
        }
    }

    // TODO: should we do something if the file gets deleted (?)

    Ok(HttpResponseAccepted("ok".to_string()))
}

/// Handle a `push` event for the configs repo.
async fn handle_configs_push(
    github: &octorust::Client,
    api_context: &Context,
    event: GitHubWebhook,
    company: &Company,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    // Get the repo.
    let owner = &company.github_org;
    let repo = event.repository.name;

    // Get the commit.
    let mut commit = event.commits.get(0).unwrap().clone();

    // Ignore any changes that are not to the `configs/` directory.
    let dir = "configs/";
    commit.filter_files_by_path(dir);
    if !commit.has_changed_files() {
        // No files changed that we care about.
        // We can throw this out, log it and return early.
        println!(
            "`push` event commit `{}` does not include any changes to the `{}` directory",
            commit.id, dir
        );
        return Ok(HttpResponseAccepted("ok".to_string()));
    }

    // Get the branch name.
    let branch = event.refv.trim_start_matches("refs/heads/");
    // Make sure this is to the default branch, we don't care about anything else.
    if branch != event.repository.default_branch {
        // We can throw this out, log it and return early.
        return Ok(HttpResponseAccepted("ok".to_string()));
    }

    // Get the configs from our repo.
    let configs = get_configs_from_repo(github, company).await;

    // Check if the links.toml file changed.
    if commit.file_changed("configs/links.toml") || commit.file_changed("configs/huddles.toml") {
        // Update our links in the database.
        sync_links(&api_context.db, configs.links, configs.huddles, company).await;

        // We need to update the short URLs for the links.
        generate_shorturls_for_configs_links(&api_context.db, github, owner, &repo, company.id)
            .await;
        println!("generated shorturls for the configs links");
    }

    // Check if the groups.toml file changed.
    // IMPORTANT: we need to sync the groups _before_ we sync the users in case we
    // added a new group to GSuite.
    if commit.file_changed("configs/groups.toml") {
        sync_groups(&api_context.db, configs.groups, company).await;
    }

    // Check if the users.toml file changed.
    if commit.file_changed("configs/users.toml") {
        sync_users(&api_context.db, github, configs.users, company).await;
    }

    if commit.file_changed("configs/users.toml") || commit.file_changed("configs/groups.toml") {
        // Sync okta users and group from the database.
        // Do this after we update the users and groups in the database.
        generate_terraform_files_for_okta(github, &api_context.db, company).await;
    }

    // Check if the buildings.toml file changed.
    // Buildings needs to be synchronized _before_ we move on to conference rooms.
    if commit.file_changed("configs/buildings.toml") {
        sync_buildings(&api_context.db, configs.buildings, company).await;
    }

    // Check if the resources.toml file changed.
    if commit.file_changed("configs/resources.toml") {
        sync_conference_rooms(&api_context.db, configs.resources, company).await;
    }

    // Check if the certificates.toml file changed.
    if commit.file_changed("configs/certificates.toml") {
        sync_certificates(&api_context.db, github, configs.certificates, company).await;
    }

    // Check if the github-outside-collaborators.toml file changed.
    if commit.file_changed("configs/github-outside-collaborators.toml") {
        // Sync github outside collaborators.
        sync_github_outside_collaborators(
            &api_context.db,
            github,
            configs.github_outside_collaborators,
            company,
        )
        .await;
    }

    // Check if the huddles file changed.
    if commit.file_changed("configs/huddles.toml") {
        // Sync github outside collaborators.
        cio_api::huddles::sync_huddles(&api_context.db, company).await;
    }

    Ok(HttpResponseAccepted("ok".to_string()))
}

/// Handle the `repository` event for all repos.
async fn handle_repository_event(
    github: &octorust::Client,
    api_context: &Context,
    event: GitHubWebhook,
    company: &Company,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    let repo = github
        .repos()
        .get(&company.github_org, &event.repository.name)
        .await
        .unwrap();
    let nr = NewRepo::new_from_full(repo.clone(), company.id);
    nr.upsert(&api_context.db).await;

    // TODO: since we know only one repo changed we don't need to refresh them all,
    // make this a bit better.
    // Update the short urls for all the repos.
    generate_shorturls_for_repos(
        &api_context.db,
        github,
        &company.github_org,
        "configs",
        company.id,
    )
    .await;

    // TODO: since we know only one repo changed we don't need to refresh them all,
    // make this a bit better.
    cio_api::repos::sync_repo_settings(&api_context.db, github, company).await;

    println!("generated shorturls for all the GitHub repos");

    Ok(HttpResponseAccepted("ok".to_string()))
}
