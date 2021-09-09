#![allow(clippy::field_reassign_with_default)]
pub mod event_types;
pub mod github_types;
mod handlers;
mod handlers_github;
pub mod repos;
pub mod slack_commands;
pub mod tracking_numbers;
#[macro_use]
extern crate serde_json;

use std::{collections::HashMap, env, fs::File, sync::Arc};

use anyhow::Result;
use chrono::offset::Utc;
use cio_api::{
    analytics::NewPageView,
    api_tokens::{APIToken, NewAPIToken},
    applicants::Applicant,
    companies::Company,
    db::Database,
    mailing_list::MailingListSubscriber,
    rack_line::RackLineSubscriber,
    schema::{api_tokens, applicants},
    swag_store::Order,
};
use diesel::prelude::*;
use docusign::DocuSign;
use dropshot::{
    endpoint, ApiDescription, ConfigDropshot, ConfigLogging, ConfigLoggingLevel, HttpError, HttpResponseAccepted,
    HttpResponseOk, HttpServerStarter, Path, Query, RequestContext, TypedBody, UntypedBody,
};
use google_drive::Client as GoogleDrive;
use gusto_api::Client as Gusto;
use log::{info, warn};
use mailchimp_api::{MailChimp, Webhook as MailChimpWebhook};
use quickbooks::QuickBooks;
use ramp_api::Client as Ramp;
use schemars::JsonSchema;
use sentry::IntoDsn;
use serde::{Deserialize, Serialize};
use serde_qs::Config as QSConfig;
use slack_chat_api::Slack;
use zoom_api::Client as Zoom;

use crate::github_types::GitHubWebhook;

#[tokio::main]
async fn main() -> Result<(), String> {
    // Initialize our logger.
    let mut log_builder = pretty_env_logger::formatted_builder();
    log_builder.parse_filters("info");

    let logger = sentry_log::SentryLogger::with_dest(log_builder.build());

    log::set_boxed_logger(Box::new(logger)).unwrap();

    log::set_max_level(log::LevelFilter::Info);

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
        request_body_max_bytes: 10737418240, // 10 Gigiabytes.
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
    api.register(listen_airtable_applicants_update_webhooks).unwrap();
    api.register(listen_airtable_assets_items_print_barcode_label_webhooks)
        .unwrap();
    api.register(listen_airtable_employees_print_home_address_label_webhooks)
        .unwrap();
    api.register(listen_airtable_shipments_inbound_create_webhooks).unwrap();
    api.register(listen_airtable_shipments_outbound_create_webhooks)
        .unwrap();
    api.register(listen_airtable_shipments_outbound_reprint_label_webhooks)
        .unwrap();
    api.register(listen_airtable_shipments_outbound_reprint_receipt_webhooks)
        .unwrap();
    api.register(listen_airtable_shipments_outbound_resend_shipment_status_email_to_recipient_webhooks)
        .unwrap();
    api.register(listen_airtable_shipments_outbound_schedule_pickup_webhooks)
        .unwrap();
    api.register(listen_airtable_swag_inventory_items_print_barcode_labels_webhooks)
        .unwrap();
    api.register(listen_analytics_page_view_webhooks).unwrap();
    api.register(listen_application_submit_requests).unwrap();
    api.register(listen_applicant_review_requests).unwrap();
    api.register(listen_application_files_upload_requests).unwrap();
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
    api.register(listen_checkr_background_update_webhooks).unwrap();
    api.register(listen_docusign_envelope_update_webhooks).unwrap();
    api.register(listen_emails_incoming_sendgrid_parse_webhooks).unwrap();
    api.register(listen_google_sheets_edit_webhooks).unwrap();
    api.register(listen_google_sheets_row_create_webhooks).unwrap();
    api.register(listen_github_webhooks).unwrap();
    api.register(listen_mailchimp_mailing_list_webhooks).unwrap();
    api.register(listen_mailchimp_rack_line_webhooks).unwrap();
    api.register(listen_products_sold_count_requests).unwrap();
    api.register(listen_shippo_tracking_update_webhooks).unwrap();
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
    info!("writing OpenAPI spec to {}...", api_file);
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
pub struct Context {
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

#[derive(Deserialize, Serialize, Default, Clone, Debug, JsonSchema)]
pub struct CounterResponse {
    #[serde(default)]
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
    sentry::start_session();

    match crate::handlers::handle_products_sold_count(rqctx).await {
        Ok(r) => {
            sentry::end_session();
            Ok(HttpResponseOk(r))
        }
        // Send the error to sentry.
        Err(e) => {
            sentry::end_session();
            Err(handle_anyhow_err_as_http_err(e))
        }
    }
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

    if let Err(e) = crate::handlers_github::handle_github(rqctx, body_param).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

#[derive(Deserialize, Debug, JsonSchema)]
pub struct RFDPathParams {
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

    if let Err(e) = crate::handlers::handle_rfd_update_by_number(rqctx, path_params).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/** Get our current GitHub rate limit. */
#[endpoint {
    method = GET,
    path = "/github/ratelimit",
}]
async fn github_rate_limit(rqctx: Arc<RequestContext<Context>>) -> Result<HttpResponseOk<GitHubRateLimit>, HttpError> {
    sentry::start_session();

    match crate::handlers::handle_github_rate_limit(rqctx).await {
        Ok(r) => {
            sentry::end_session();
            Ok(HttpResponseOk(r))
        }
        // Send the error to sentry.
        Err(e) => {
            sentry::end_session();
            Err(handle_anyhow_err_as_http_err(e))
        }
    }
}

/// A GitHub RateLimit
#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct GitHubRateLimit {
    #[serde(default)]
    pub limit: u32,
    #[serde(default)]
    pub remaining: u32,
    #[serde(default)]
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

    if let Err(e) = crate::handlers::handle_google_sheets_edit(rqctx, body_param).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

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
        deserialize_with = "octorust::utils::deserialize_null_string::deserialize"
    )]
    pub old_value: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "octorust::utils::deserialize_null_string::deserialize"
    )]
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
async fn listen_google_sheets_row_create_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<GoogleSpreadsheetRowCreateEvent>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();

    if let Err(e) = crate::handlers::handle_google_sheets_row_create(rqctx, body_param).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

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

    if let Err(e) = crate::handlers::handle_airtable_employees_print_home_address_label(rqctx, body_param).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

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

    if let Err(e) = crate::handlers::handle_airtable_assets_items_print_barcode_label(rqctx, body_param).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

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

    if let Err(e) = crate::handlers::handle_airtable_swag_inventory_items_print_barcode_labels(rqctx, body_param).await
    {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

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

    if let Err(e) = crate::handlers::handle_airtable_applicants_request_background_check(rqctx, body_param).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

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

    if let Err(e) = crate::handlers::handle_airtable_applicants_update(rqctx, body_param).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
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
    path = "/airtable/shipments/outbound/create",
}]
async fn listen_airtable_shipments_outbound_create_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();

    if let Err(e) = crate::handlers::handle_airtable_shipments_outbound_create(rqctx, body_param).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

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

    if let Err(e) = crate::handlers::handle_airtable_shipments_outbound_reprint_label(rqctx, body_param).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/**
 * Listen for a button pressed to reprint a receipt for an outbound shipment.
 */
#[endpoint {
    method = POST,
    path = "/airtable/shipments/outbound/reprint_receipt",
}]
async fn listen_airtable_shipments_outbound_reprint_receipt_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();

    if let Err(e) = crate::handlers::handle_airtable_shipments_outbound_reprint_receipt(rqctx, body_param).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

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

    if let Err(e) =
        crate::handlers::handle_airtable_shipments_outbound_resend_shipment_status_email_to_recipient(rqctx, body_param)
            .await
    {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

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

    if let Err(e) = crate::handlers::handle_airtable_shipments_outbound_schedule_pickup(rqctx, body_param).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

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
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "content-ids")]
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
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "attachment-info")]
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

    if let Err(e) = crate::handlers::handle_emails_incoming_sendgrid_parse(rqctx, body_param).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

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

    if let Err(e) = crate::handlers::handle_applicant_review(rqctx, body_param).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

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

    if let Err(e) = crate::handlers::handle_application_submit(rqctx, body_param).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

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

    match crate::handlers::handle_application_files_upload(rqctx, body_param).await {
        Ok(r) => {
            sentry::end_session();
            Ok(HttpResponseOk(r))
        }
        // Send the error to sentry.
        Err(e) => {
            sentry::end_session();
            Err(handle_anyhow_err_as_http_err(e))
        }
    }
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

    if let Err(e) = crate::handlers::handle_airtable_shipments_inbound_create(rqctx, body_param).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

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

    if let Err(e) = crate::handlers::handle_store_order_create(rqctx, body_param).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

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

    if let Err(e) = crate::handlers::handle_shippo_tracking_update(rqctx, body_param).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

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
        warn!("checkr candidate id is empty for event: {:?}", event);
        return Ok(HttpResponseAccepted("ok".to_string()));
    }

    // TODO: change this to the real company name.
    let oxide = Company::get_from_db(&api_context.db, "Oxide".to_string()).unwrap();

    let checkr_auth = oxide.authenticate_checkr();
    if checkr_auth.is_none() {
        // Return early.
        warn!("this company {:?} does not have a checkr api key: {:?}", oxide, event);
        return Ok(HttpResponseAccepted("ok".to_string()));
    }

    let checkr = checkr_auth.unwrap();
    let candidate = checkr.get_candidate(&event.data.object.candidate_id).await.unwrap();
    let result = applicants::dsl::applicants
        .filter(
            applicants::dsl::email
                .eq(candidate.email.to_string())
                // TODO: matching on name might be a bad idea here.
                .or(applicants::dsl::name.eq(format!("{} {}", candidate.first_name, candidate.last_name))),
        )
        .filter(applicants::dsl::status.eq(cio_api::applicant_status::Status::Onboarding.to_string()))
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
        applicant.update(&api_context.db).await.unwrap();
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

    // Initialize the Google client.
    // You can use any of the libs here, they all use the same endpoint
    // for tokens and we will send all the scopes.
    let g = GoogleDrive::new_from_env("", "").await;

    sentry::end_session();
    Ok(HttpResponseOk(UserConsentURL {
        url: g.user_consent_url(&cio_api::companies::get_google_scopes()),
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

    // Initialize the Google client.
    // You can use any of the libs here, they all use the same endpoint
    // for tokens and we will send all the scopes.
    let mut g = GoogleDrive::new_from_env("", "").await;

    // Let's get the token from the code.
    let t = g.get_access_token(&event.code, &event.state).await.unwrap();

    let client = reqwest::Client::new();

    // Let's get the company from information about the user.
    let mut headers = reqwest::header::HeaderMap::new();
    headers.append(
        reqwest::header::ACCEPT,
        reqwest::header::HeaderValue::from_static("application/json"),
    );
    headers.append(
        reqwest::header::AUTHORIZATION,
        reqwest::header::HeaderValue::from_str(&format!("Bearer {}", t.access_token)).unwrap(),
    );

    let params = [("alt", "json")];
    let resp = client
        .get("https://www.googleapis.com/oauth2/v1/userinfo")
        .headers(headers)
        .query(&params)
        .send()
        .await
        .unwrap();

    // Unwrap the response.
    let metadata: cio_api::companies::UserInfo = resp.json().await.unwrap();

    let company = Company::get_from_domain(&api_context.db, &metadata.hd).unwrap();

    // Save the token to the database.
    let mut token = NewAPIToken {
        product: "google".to_string(),
        token_type: t.token_type.to_string(),
        access_token: t.access_token.to_string(),
        expires_in: t.expires_in as i32,
        refresh_token: t.refresh_token.to_string(),
        refresh_token_expires_in: t.refresh_token_expires_in as i32,
        company_id: metadata.hd.to_string(),
        item_id: "".to_string(),
        user_email: metadata.email.to_string(),
        last_updated_at: Utc::now(),
        expires_date: None,
        refresh_token_expires_date: None,
        endpoint: "".to_string(),
        auth_company_id: company.id,
        company: Default::default(),
        // THIS SHOULD ALWAYS BE OXIDE, NO 1.
        cio_company_id: 1,
    };
    token.expand();

    // Update it in the database.
    token.upsert(&api_context.db).await.unwrap();

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

    warn!("github callback: {:?}", event);

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

    let company = Company::get_from_domain(&api_context.db, &domain).unwrap();

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
    token.upsert(&api_context.db).await.unwrap();

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

    let company = Company::get_from_domain(&api_context.db, &domain).unwrap();

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
    token.upsert(&api_context.db).await.unwrap();

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

    warn!("zoom deauthorization: {:?}", event);

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

    // Initialize the Zoom client.
    let mut g = Zoom::new_from_env("", "");

    // Let's get the token from the code.
    let t = g.get_access_token(&event.code, &event.state).await.unwrap();

    // TODO: this login type means google but that might not always be true...
    let cu = g
        .users()
        .user(
            "me",
            zoom_api::types::LoginType::Noop, // We don't know the login type, so let's leave it empty.
            false,
        )
        .await
        .unwrap();

    // Let's get the domain from the email.
    let mut domain = "".to_string();
    if !cu.user.email.is_empty() {
        let split = cu.user.email.split('@');
        let vec: Vec<&str> = split.collect();
        if vec.len() > 1 {
            domain = vec.get(1).unwrap().to_string();
        }
    }

    let company = Company::get_from_domain(&api_context.db, &domain).unwrap();

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
    token.upsert(&api_context.db).await.unwrap();

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
        .get_all(
            "", // department id
            "", // location id
        )
        .await
        .unwrap();

    // Let's get the domain from the email.
    let mut domain = "".to_string();
    if !ru.is_empty() {
        let split = ru.get(0).unwrap().email.split('@');
        let vec: Vec<&str> = split.collect();
        if vec.len() > 1 {
            domain = vec.get(1).unwrap().to_string();
        }
    }

    let company = Company::get_from_domain(&api_context.db, &domain).unwrap();

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
    token.upsert(&api_context.db).await.unwrap();

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

    let company = Company::get_from_domain(&api_context.db, &domain).unwrap();

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
        token.create_in_db(&api_context.db).unwrap()
    };
    new_token.upsert_in_airtable(&api_context.db).await.unwrap();

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
            user_token.create_in_db(&api_context.db).unwrap()
        };
        new_user_token.upsert_in_airtable(&api_context.db).await.unwrap();
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

    let company = Company::get_from_domain(&api_context.db, &domain).unwrap();

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
    token.upsert(&api_context.db).await.unwrap();

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

    warn!("plaid callback: {:?}", event);

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

    let company = Company::get_from_domain(&api_context.db, &domain).unwrap();

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
    token.upsert(&api_context.db).await.unwrap();

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
            let company = applicant.company(db).unwrap();

            // Create our docusign client.
            let dsa = company.authenticate_docusign(db).await;
            if let Ok(ds) = dsa {
                applicant
                    .update_applicant_from_docusign_offer_envelope(db, &ds, event.clone())
                    .await
                    .unwrap();
            }
        }
        Err(e) => {
            warn!(
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
            let company = applicant.company(db).unwrap();

            // Create our docusign client.
            let dsa = company.authenticate_docusign(db).await;
            if let Ok(ds) = dsa {
                applicant
                    .update_applicant_from_docusign_piia_envelope(db, &ds, event)
                    .await
                    .unwrap();
            }
        }
        Err(e) => {
            warn!(
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

    // Expand the page_view.
    event.set_page_link();
    event.set_company_id(db).unwrap();

    // Add the page_view to the database and Airttable.
    let pv = event.create(db).await.unwrap();

    info!("page_view `{} | {}` created successfully", pv.page_link, pv.user_email);
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
    let qs_non_strict = QSConfig::new(10, false);

    let event: MailChimpWebhook = qs_non_strict.deserialize_str(&event_string).unwrap();

    if event.webhook_type != *"subscribe" {
        info!("not a `subscribe` event, got `{}`", event.webhook_type);
        return Ok(HttpResponseAccepted("ok".to_string()));
    }

    // Parse the webhook as a new mailing list subscriber.
    let new_subscriber = cio_api::mailing_list::as_mailing_list_subscriber(event, db).unwrap();

    let existing = MailingListSubscriber::get_from_db(db, new_subscriber.email.to_string());
    if existing.is_none() {
        // Update the subscriber in the database.
        let subscriber = new_subscriber.upsert(db).await.unwrap();

        info!("subscriber {} created successfully", subscriber.email);
    } else {
        info!("subscriber {} already exists", new_subscriber.email);
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
    let qs_non_strict = QSConfig::new(10, false);

    let event: MailChimpWebhook = qs_non_strict.deserialize_str(&event_string).unwrap();

    if event.webhook_type != *"subscribe" {
        info!("not a `subscribe` event, got `{}`", event.webhook_type);
        return Ok(HttpResponseAccepted("ok".to_string()));
    }

    // Parse the webhook as a new rack line subscriber.
    let new_subscriber = cio_api::rack_line::as_rack_line_subscriber(event, db);

    // let company = Company::get_by_id(db, new_subscriber.cio_company_id);

    let existing = RackLineSubscriber::get_from_db(db, new_subscriber.email.to_string());
    if existing.is_none() {
        // Update the subscriber in the database.
        let subscriber = new_subscriber.upsert(db).await.unwrap();

        // Parse the signup into a slack message.
        // Send the message to the slack channel.
        //company.post_to_slack_channel(db, new_subscriber.as_slack_msg()).await;
        info!("subscriber {} posted to Slack", subscriber.email);

        info!("subscriber {} created successfully", subscriber.email);
    } else {
        info!("subscriber {} already exists", new_subscriber.email);
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

    match crate::handlers::handle_slack_commands(rqctx, body_param).await {
        Ok(r) => {
            sentry::end_session();
            Ok(HttpResponseOk(r))
        }
        // Send the error to sentry.
        Err(e) => {
            sentry::end_session();
            Err(handle_anyhow_err_as_http_err(e))
        }
    }
}

fn handle_anyhow_err_as_http_err(err: anyhow::Error) -> HttpError {
    // Send to sentry.
    sentry_anyhow::capture_anyhow(&err);

    // We use the debug formatting here so we get the stack trace.
    return HttpError::for_internal_error(format!("{:?}", err));
}
