#![allow(clippy::type_complexity)]
use std::{collections::HashMap, env, pin::Pin};

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use cio_api::{
    analytics::NewPageView,
    functions::Function,
    rfd::{RFDEntry, RFDIndexEntry},
    swag_store::Order,
};
use clokwerk::{AsyncScheduler, Job, TimeUnits};
use docusign::DocuSign;
use dropshot::{
    endpoint, ApiDescription, ConfigDropshot, ConfigLogging, ConfigLoggingLevel, HttpError, HttpResponseAccepted,
    HttpResponseHeaders, HttpResponseOk, HttpServerStarter, OpenApiDefinition, PaginationOrder, PaginationParams, Path,
    Query, RequestContext, ResultsPage, TypedBody, WhichPage,
};
use dropshot_verify_request::{
    bearer::{Bearer, BearerToken},
    query::{QueryToken, QueryTokenAudit},
    sig::{HmacVerifiedBody, HmacVerifiedBodyAudit},
};
use google_drive::Client as GoogleDrive;
use gusto_api::Client as Gusto;
use http::header::HeaderValue;
use log::{error, info, warn};
use quickbooks::QuickBooks;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use signal_hook::{
    consts::{SIGINT, SIGTERM},
    iterator::Signals,
};
use slack_chat_api::{BotCommand, Slack};
use std::any::Any;
use zoom_api::Client as Zoom;

use crate::{
    auth::{AirtableToken, HiringToken, InternalToken, RFDToken, ShippoToken},
    context::ServerContext,
    github_types::GitHubWebhook,
    handlers_hiring::{ApplicantInfo, ApplicantUploadToken},
    handlers_slack::InteractiveEvent,
};

pub struct APIConfig {
    pub api: ApiDescription<ServerContext>,
    pub schema: serde_json::Value,
}

impl APIConfig {
    pub fn new() -> Result<Self> {
        let api = create_api();
        let open_api = create_open_api(&api);
        let schema = open_api.json()?;

        Ok(APIConfig { api, schema })
    }

    pub fn open_api(&self) -> OpenApiDefinition<ServerContext> {
        create_open_api(&self.api)
    }
}

fn create_api() -> ApiDescription<ServerContext> {
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
    api.register(listen_airtable_applicants_recreate_piia_webhooks).unwrap();
    api.register(listen_airtable_assets_items_print_barcode_label_webhooks)
        .unwrap();
    api.register(listen_airtable_employees_print_home_address_label_webhooks)
        .unwrap();
    api.register(listen_airtable_certificates_renew_webhooks).unwrap();
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
    api.register(listen_test_application_submit_requests).unwrap();
    api.register(listen_applicant_review_requests).unwrap();
    api.register(listen_test_application_files_upload_requests_cors)
        .unwrap();
    api.register(listen_test_application_files_upload_requests).unwrap();
    api.register(listen_application_files_upload_requests_cors).unwrap();
    api.register(listen_application_files_upload_requests).unwrap();
    api.register(listen_applicant_info).unwrap();
    api.register(listen_applicant_upload_token).unwrap();

    api.register(listen_auth_docusign_callback).unwrap();
    api.register(listen_auth_docusign_consent).unwrap();
    api.register(listen_auth_github_callback).unwrap();
    api.register(listen_auth_github_consent).unwrap();
    api.register(listen_auth_google_callback).unwrap();
    api.register(listen_auth_google_consent).unwrap();
    api.register(listen_auth_gusto_callback).unwrap();
    api.register(listen_auth_gusto_consent).unwrap();
    api.register(listen_auth_plaid_callback).unwrap();
    api.register(listen_auth_zoom_callback).unwrap();
    api.register(listen_auth_zoom_consent).unwrap();
    api.register(listen_auth_zoom_deauthorization).unwrap();
    api.register(listen_auth_slack_callback).unwrap();
    api.register(listen_auth_slack_consent).unwrap();
    api.register(listen_auth_quickbooks_callback).unwrap();
    api.register(listen_auth_quickbooks_consent).unwrap();
    api.register(listen_checkr_background_update_webhooks).unwrap();
    api.register(listen_docusign_envelope_update_webhooks).unwrap();
    api.register(listen_github_webhooks).unwrap();
    api.register(listen_products_sold_count_requests).unwrap();
    api.register(listen_shippo_tracking_update_webhooks).unwrap();
    api.register(listen_easypost_tracking_update_webhooks).unwrap();
    api.register(listen_slack_commands_webhooks).unwrap();
    api.register(listen_slack_interactive_webhooks).unwrap();
    api.register(listen_shipbob_webhooks).unwrap();
    api.register(listen_store_order_create).unwrap();
    api.register(listen_rfd_index).unwrap();
    api.register(listen_rfd_view).unwrap();
    api.register(trigger_rfd_update_by_number).unwrap();
    api.register(trigger_cleanup_create).unwrap();

    api.register(trigger_sync_analytics_create).unwrap();
    api.register(trigger_sync_api_tokens_create).unwrap();
    api.register(trigger_sync_applications_create).unwrap();
    api.register(trigger_sync_asset_inventory_create).unwrap();
    api.register(trigger_sync_companies_create).unwrap();
    api.register(trigger_sync_configs_create).unwrap();
    api.register(trigger_sync_finance_create).unwrap();
    api.register(trigger_sync_functions_create).unwrap();
    api.register(trigger_sync_huddles_create).unwrap();
    api.register(trigger_sync_interviews_create).unwrap();
    api.register(trigger_sync_journal_clubs_create).unwrap();
    api.register(trigger_sync_mailing_lists_create).unwrap();
    api.register(trigger_sync_other_create).unwrap();
    api.register(trigger_sync_recorded_meetings_create).unwrap();
    api.register(trigger_sync_repos_create).unwrap();
    // api.register(trigger_sync_rfds_create).unwrap();
    api.register(trigger_sync_shipments_create).unwrap();
    api.register(trigger_sync_salesforce_create).unwrap();
    api.register(trigger_sync_shorturls_create).unwrap();
    api.register(trigger_sync_swag_inventory_create).unwrap();
    api.register(trigger_sync_travel_create).unwrap();
    api.register(trigger_sync_zoho_create).unwrap();

    api
}

fn create_open_api(api: &ApiDescription<ServerContext>) -> OpenApiDefinition<ServerContext> {
    // Create the API schema.
    let mut api_definition = api.openapi("Webhooks API", clap::crate_version!());
    api_definition
        .description("Internal webhooks server for listening to several third party webhooks")
        .contact_url("https://oxide.computer")
        .contact_email("webhooks@oxide.computer");

    api_definition
}

pub async fn create_server(
    s: &crate::core::Server,
    api: ApiDescription<ServerContext>,
    api_context: ServerContext,
    debug: bool,
) -> Result<dropshot::HttpServer<ServerContext>> {
    /*
     * We must specify a configuration with a bind address.  We'll use 127.0.0.1
     * since it's available and won't expose this server outside the host.  We
     * request port 8080.
     */
    let config_dropshot = ConfigDropshot {
        bind_address: s.address.parse()?,
        request_body_max_bytes: 107374182400, // 100 Gigiabytes.
        ..Default::default()
    };

    /*
     * For simplicity, we'll configure an "info"-level logger that writes to
     * stderr assuming that it's a terminal.
     */
    let mut log_level = ConfigLoggingLevel::Info;
    if debug {
        log_level = ConfigLoggingLevel::Debug;
    }
    let config_logging = ConfigLogging::StderrTerminal { level: log_level };
    let log = config_logging.to_logger("webhooky-server")?;
    /*
     * Set up the server.
     */
    let server = HttpServerStarter::new(&config_dropshot, api, api_context, &log)
        .map_err(|error| anyhow!("failed to create server: {}", error))?
        .start();

    Ok(server)
}

pub async fn server(
    s: crate::core::Server,
    api: ApiDescription<ServerContext>,
    server_context: ServerContext,
    debug: bool,
) -> Result<()> {
    let server = create_server(&s, api, server_context.clone(), debug).await?;

    // This really only applied for when we are running with `do-cron` but we need the variable
    // for the scheduler to be in the top level so we can run as async later based on the options.
    let mut scheduler = AsyncScheduler::with_tz(chrono_tz::US::Pacific);

    // Copy the Server struct so we can move it into our loop.
    if s.do_cron {
        /*
         * Setup our cron jobs, with our timezone.
         */
        // scheduler
        //     .every(1.day())
        //     .run(enclose! { (server_context) move || create_do_job_fn(server_context.clone(), "sync-analytics")});
        scheduler
            .every(23.hours())
            .run(enclose! { (server_context) move || create_do_job_fn(server_context.clone(), "sync-api-tokens")});
        scheduler
            .every(7.hours())
            .run(enclose! { (server_context) move || create_do_job_fn(server_context.clone(), "sync-applications")});
        // scheduler
        //     .every(2.hours())
        //     .run(enclose! { (server_context) move || create_do_job_fn(server_context.clone(), "sync-asset-inventory")});
        scheduler
            .every(12.hours())
            .run(enclose! { (server_context) move || create_do_job_fn(server_context.clone(), "sync-companies")});
        scheduler
            .every(1.hours())
            .run(enclose! { (server_context) move || create_do_job_fn(server_context.clone(), "sync-configs")});
        // scheduler
        //     .every(6.hours())
        //     .run(enclose! { (server_context) move || create_do_job_fn(server_context.clone(), "sync-finance")});
        // scheduler
        //     .every(12.hours())
        //     .run(enclose! { (server_context) move || create_do_job_fn(server_context.clone(), "sync-functions")});
        scheduler
            .every(1.hours())
            .run(enclose! { (server_context) move || create_do_job_fn(server_context.clone(), "sync-huddles")});
        scheduler
            .every(4.hours())
            .run(enclose! { (server_context) move || create_do_job_fn(server_context.clone(), "sync-interviews")});
        // scheduler
        //     .every(12.hours())
        //     .run(enclose! { (server_context) move || create_do_job_fn(server_context.clone(), "sync-journal-clubs")});
        scheduler
            .every(3.hours())
            .run(enclose! { (server_context) move || create_do_job_fn(server_context.clone(), "sync-mailing-lists")});
        // scheduler
        //     .every(18.hours())
        //     .run(enclose! { (server_context) move || create_do_job_fn(server_context.clone(), "sync-other")});
        // scheduler.every(3.hours()).run(
        //     enclose! { (server_context) move || create_do_job_fn(server_context.clone(), "sync-recorded-meetings")},
        // );
        scheduler
            .every(16.hours())
            .run(enclose! { (server_context) move || create_do_job_fn(server_context.clone(), "sync-repos")});
        // scheduler
        //     .every(14.hours())
        //     .run(enclose! { (server_context) move || create_do_job_fn(server_context.clone(), "sync-rfds")});
        scheduler
            .every(30.minutes())
            .run(enclose! { (server_context) move || create_do_job_fn(server_context.clone(), "sync-salesforce")});
        // scheduler
        //     .every(2.hours())
        //     .run(enclose! { (server_context) move || create_do_job_fn(server_context.clone(), "sync-shipments")});
        scheduler
            .every(3.hours())
            .run(enclose! { (server_context) move || create_do_job_fn(server_context.clone(), "sync-shorturls")});
        // scheduler
        //     .every(9.hours())
        //     .run(enclose! { (server_context) move || create_do_job_fn(server_context.clone(), "sync-swag-inventory")});
        // scheduler
        //     .every(5.hours())
        //     .run(enclose! { (server_context) move || create_do_job_fn(server_context.clone(), "sync-travel")});
        scheduler
            .every(1.minutes())
            .run(|| async { crate::health::scheduler_health_check() });

        // Run the RFD changelog.
        scheduler
            .every(clokwerk::Interval::Monday)
            .at("8:00 am")
            .run(enclose! { (server_context) move || create_do_job_fn(server_context.clone(), "send-rfd-changelog")});
    }

    // For Cloud run & ctrl+c, shutdown gracefully.
    // "The main process inside the container will receive SIGTERM, and after a grace period,
    // SIGKILL."
    // Regsitering SIGKILL here will panic at runtime, so let's avoid that.
    let mut signals = Signals::new([SIGINT, SIGTERM])?;

    tokio::spawn(enclose! { (server_context) async move {
        for sig in signals.forever() {
            let pid = std::process::id();
            info!("received signal: {:?} pid: {}", sig, pid);
            info!("triggering cleanup... {}", pid);

            // Run the cleanup job.
            if let Err(e) = do_cleanup(&server_context).await {
                error!("Failed to cleanly shutdown the server {:?}", e);
            }
            // Exit the process.
            info!("all clean, exiting! pid: {}", pid);
            std::process::exit(0);
        }
    }});

    if s.do_cron {
        // Trigger the server in the background.
        tokio::spawn(async move {
            server.await.unwrap();
        });

        info!("starting cron job scheduler...");

        // Loop the scheduler.
        loop {
            scheduler.run_pending().await;
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    } else {
        server.await.unwrap();
    }

    Ok(())
}

pub fn create_do_job_fn(ctx: ServerContext, job: &str) -> Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
    Box::pin(do_job(ctx, job.to_string()))
}

pub async fn do_job(ctx: ServerContext, job: String) {
    info!("triggering cron job `{}`", job);

    if let Err(err) = crate::handlers_cron::run_subcmd_job(&ctx, &job).await {
        error!("Failed to spawn job: {:?}", err)
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
async fn ping(_rqctx: RequestContext<ServerContext>) -> Result<HttpResponseOk<String>, HttpError> {
    Ok(HttpResponseOk("pong".to_string()))
}

#[derive(Deserialize, Serialize, Default, Clone, Debug, JsonSchema)]
pub struct CounterResponse {
    #[serde(default)]
    pub count: i32,
}

/** Return the count of products sold. */
#[endpoint {
    method = GET,
    path = "/products/sold/count",
}]
async fn listen_products_sold_count_requests(
    rqctx: RequestContext<ServerContext>,
) -> Result<HttpResponseOk<CounterResponse>, HttpError> {
    crate::handlers::handle_products_sold_count(&rqctx)
        .await
        .map(HttpResponseOk)
        .map_err(handle_anyhow_err_as_http_err)
}

/** Listen for GitHub webhooks. */
#[endpoint {
    method = POST,
    path = "/github",
}]
async fn listen_github_webhooks(
    rqctx: RequestContext<ServerContext>,
    body: HmacVerifiedBody<crate::handlers_github::GitHubWebhookVerification, GitHubWebhook>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    crate::handlers_github::handle_github(&rqctx, body.into_inner()?)
        .await
        .map(accepted)
        .map_err(handle_anyhow_err_as_http_err)
}

#[derive(Deserialize, Debug, JsonSchema)]
pub struct RFDPathParams {
    pub num: i32,
}

/** Trigger an update for an RFD. */
#[endpoint {
    method = POST,
    path = "/rfd/{num}",
}]
async fn trigger_rfd_update_by_number(
    rqctx: RequestContext<ServerContext>,
    _auth: Bearer<InternalToken>,
    path_params: Path<RFDPathParams>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    crate::handlers::handle_rfd_update_by_number(&rqctx, path_params)
        .await
        .map(accepted)
        .map_err(handle_anyhow_err_as_http_err)
}

/** Get our current GitHub rate limit. */
#[endpoint {
    method = GET,
    path = "/github/ratelimit",
}]
async fn github_rate_limit(rqctx: RequestContext<ServerContext>) -> Result<HttpResponseOk<GitHubRateLimit>, HttpError> {
    crate::handlers::handle_github_rate_limit(&rqctx)
        .await
        .map(HttpResponseOk)
        .map_err(handle_anyhow_err_as_http_err)
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
 * Listen for a button pressed to print a home address label for employees.
 */
#[endpoint {
    method = POST,
    path = "/airtable/employees/print_home_address_label",
}]
async fn listen_airtable_employees_print_home_address_label_webhooks(
    rqctx: RequestContext<ServerContext>,
    _auth: Bearer<AirtableToken>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    crate::handlers::handle_airtable_employees_print_home_address_label(&rqctx, body_param.into_inner())
        .await
        .map(accepted)
        .map_err(handle_anyhow_err_as_http_err)
}

/**
 * Listen for a button pressed to renew a certificate.
 */
#[endpoint {
    method = POST,
    path = "/airtable/certificates/renew",
}]
async fn listen_airtable_certificates_renew_webhooks(
    rqctx: RequestContext<ServerContext>,
    _auth: Bearer<AirtableToken>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    crate::handlers::handle_airtable_certificates_renew(&rqctx, body_param.into_inner())
        .await
        .map(accepted)
        .map_err(handle_anyhow_err_as_http_err)
}

/**
 * Listen for a button pressed to print a barcode label for an asset item.
 */
#[endpoint {
    method = POST,
    path = "/airtable/assets/items/print_barcode_label",
}]
async fn listen_airtable_assets_items_print_barcode_label_webhooks(
    rqctx: RequestContext<ServerContext>,
    _auth: Bearer<AirtableToken>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    crate::handlers::handle_airtable_assets_items_print_barcode_label(&rqctx, body_param.into_inner())
        .await
        .map(accepted)
        .map_err(handle_anyhow_err_as_http_err)
}

/**
 * Listen for a button pressed to print barcode labels for a swag inventory item.
 */
#[endpoint {
    method = POST,
    path = "/airtable/swag/inventory/items/print_barcode_labels",
}]
async fn listen_airtable_swag_inventory_items_print_barcode_labels_webhooks(
    rqctx: RequestContext<ServerContext>,
    _auth: Bearer<AirtableToken>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    crate::handlers::handle_airtable_swag_inventory_items_print_barcode_labels(&rqctx, body_param.into_inner())
        .await
        .map(accepted)
        .map_err(handle_anyhow_err_as_http_err)
}

/**
 * Listen for a button pressed to request a background check for an applicant.
 */
#[endpoint {
    method = POST,
    path = "/airtable/applicants/request_background_check",
}]
async fn listen_airtable_applicants_request_background_check_webhooks(
    rqctx: RequestContext<ServerContext>,
    _auth: Bearer<AirtableToken>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    crate::handlers::handle_airtable_applicants_request_background_check(&rqctx, body_param.into_inner())
        .await
        .map(accepted)
        .map_err(handle_anyhow_err_as_http_err)
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
    rqctx: RequestContext<ServerContext>,
    _auth: Bearer<AirtableToken>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    crate::handlers::handle_airtable_applicants_update(&rqctx, body_param.into_inner())
        .await
        .map(accepted)
        .map_err(handle_anyhow_err_as_http_err)
}

/**
 * Listen for requests to recreate and resend PIIA documents for a given applicant
 * These are set up with an Airtable script on the workspaces themselves.
 */
#[endpoint {
    method = POST,
    path = "/airtable/applicants/recreate_piia",
}]
async fn listen_airtable_applicants_recreate_piia_webhooks(
    rqctx: RequestContext<ServerContext>,
    _auth: Bearer<AirtableToken>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    crate::handlers::listen_airtable_applicants_recreate_piia_webhooks(&rqctx, body_param.into_inner())
        .await
        .map(accepted)
        .map_err(handle_anyhow_err_as_http_err)
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
    rqctx: RequestContext<ServerContext>,
    _auth: Bearer<AirtableToken>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    crate::handlers::handle_airtable_shipments_outbound_create(&rqctx, body_param.into_inner())
        .await
        .map(accepted)
        .map_err(handle_anyhow_err_as_http_err)
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
    rqctx: RequestContext<ServerContext>,
    _auth: Bearer<AirtableToken>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    crate::handlers::handle_airtable_shipments_outbound_reprint_label(&rqctx, body_param.into_inner())
        .await
        .map(accepted)
        .map_err(handle_anyhow_err_as_http_err)
}

/**
 * Listen for a button pressed to reprint a receipt for an outbound shipment.
 */
#[endpoint {
    method = POST,
    path = "/airtable/shipments/outbound/reprint_receipt",
}]
async fn listen_airtable_shipments_outbound_reprint_receipt_webhooks(
    rqctx: RequestContext<ServerContext>,
    _auth: Bearer<AirtableToken>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    crate::handlers::handle_airtable_shipments_outbound_reprint_receipt(&rqctx, body_param.into_inner())
        .await
        .map(accepted)
        .map_err(handle_anyhow_err_as_http_err)
}

/**
 * Listen for a button pressed to resend a shipment status email to the recipient for an outbound shipment.
 */
#[endpoint {
    method = POST,
    path = "/airtable/shipments/outbound/resend_shipment_status_email_to_recipient",
}]
async fn listen_airtable_shipments_outbound_resend_shipment_status_email_to_recipient_webhooks(
    rqctx: RequestContext<ServerContext>,
    _auth: Bearer<AirtableToken>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    crate::handlers::handle_airtable_shipments_outbound_resend_shipment_status_email_to_recipient(
        &rqctx,
        body_param.into_inner(),
    )
    .await
    .map(accepted)
    .map_err(handle_anyhow_err_as_http_err)
}

/**
 * Listen for a button pressed to schedule a pickup for an outbound shipment.
 */
#[endpoint {
    method = POST,
    path = "/airtable/shipments/outbound/schedule_pickup",
}]
async fn listen_airtable_shipments_outbound_schedule_pickup_webhooks(
    rqctx: RequestContext<ServerContext>,
    _auth: Bearer<AirtableToken>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    crate::handlers::handle_airtable_shipments_outbound_schedule_pickup(&rqctx, body_param.into_inner())
        .await
        .map(accepted)
        .map_err(handle_anyhow_err_as_http_err)
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
 * Listen for applicant reviews being submitted for job applicants */
#[endpoint {
    method = POST,
    path = "/applicant/review/submit",
}]
async fn listen_applicant_review_requests(
    rqctx: RequestContext<ServerContext>,
    _auth: Bearer<InternalToken>,
    body_param: TypedBody<cio_api::applicant_reviews::NewApplicantReview>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    crate::handlers::handle_applicant_review(&rqctx, body_param.into_inner())
        .await
        .map(accepted)
        .map_err(handle_anyhow_err_as_http_err)
}

#[derive(Deserialize, JsonSchema)]
struct ApplicantInfoParams {
    email: String,
}

// Listen for applicant info requests. This assume that the caller has performed the necessary
// authentication to verify ownership of the email that we are being sent
#[endpoint {
    method = GET,
    path = "/applicant/info/{email}",
}]
async fn listen_applicant_info(
    rqctx: RequestContext<ServerContext>,
    _auth: Bearer<HiringToken>,
    path_params: Path<ApplicantInfoParams>,
) -> Result<HttpResponseOk<ApplicantInfo>, HttpError> {
    log::info!("Running applicant info handler");
    crate::handlers_hiring::handle_applicant_info(&rqctx.context().app, path_params.into_inner().email)
        .await
        .map(HttpResponseOk)
        .map_err(handle_anyhow_err_as_http_err)
}

// Listen for applicant upload token requests. This returns a short-lived, one time token that can
// be used to upload materials against the supplied email address. This assume that the caller has
// performed the necessary authentication to verify ownership of the email that we are being sent
#[endpoint {
    method = GET,
    path = "/applicant/info/{email}/upload-token",
}]
async fn listen_applicant_upload_token(
    rqctx: RequestContext<ServerContext>,
    _auth: Bearer<HiringToken>,
    path_params: Path<ApplicantInfoParams>,
) -> Result<HttpResponseOk<ApplicantUploadToken>, HttpError> {
    log::info!("Running applicant upload token handler");
    crate::handlers_hiring::handle_applicant_upload_token(&rqctx.context().app, path_params.into_inner().email)
        .await
        .map(HttpResponseOk)
        .map_err(handle_anyhow_err_as_http_err)
}

/**
 * Listen for applications being submitted for incoming job applications */
#[endpoint {
    method = POST,
    path = "/application-test/submit",
}]
async fn listen_test_application_submit_requests(
    rqctx: RequestContext<ServerContext>,
    _auth: Bearer<HiringToken>,
    body_param: TypedBody<cio_api::application_form::ApplicationForm>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    crate::handlers::handle_test_application_submit(&rqctx, body_param.into_inner())
        .await
        .map(accepted)
        .map_err(handle_anyhow_err_as_http_err)
}

/**
 * Listen for applications being submitted for incoming job applications */
#[endpoint {
    method = POST,
    path = "/application/submit",
}]
async fn listen_application_submit_requests(
    rqctx: RequestContext<ServerContext>,
    _auth: Bearer<HiringToken>,
    body_param: TypedBody<cio_api::application_form::ApplicationForm>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    crate::handlers::handle_application_submit(&rqctx, body_param.into_inner())
        .await
        .map(accepted)
        .map_err(handle_anyhow_err_as_http_err)
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
    pub portfolio_pdf_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub role: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub interested_in: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub user_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub resume_contents: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub materials_contents: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub portfolio_pdf_contents: String,
}

/**
 * CORS functionality for file uploads
 */
#[endpoint {
    method = OPTIONS,
    path = "/application-test/files/upload",
}]
async fn listen_test_application_files_upload_requests_cors(
    _qctx: RequestContext<ServerContext>,
) -> Result<HttpResponseHeaders<HttpResponseOk<String>>, HttpError> {
    let mut resp = HttpResponseHeaders::new_unnamed(HttpResponseOk("".to_string()));
    let headers = resp.headers_mut();

    headers.insert("Access-Control-Allow-Origin", HeaderValue::from_static("*"));
    headers.insert("Access-Control-Allow-Headers", HeaderValue::from_static("*"));
    headers.insert("Access-Control-Allow-Method", HeaderValue::from_static("*"));

    Ok(resp)
}

/**
 * Listen for files being uploaded for incoming job applications */
#[endpoint {
    method = POST,
    path = "/application-test/files/upload",
}]
async fn listen_test_application_files_upload_requests(
    rqctx: RequestContext<ServerContext>,
    bearer: BearerToken,
    body_param: TypedBody<ApplicationFileUploadData>,
) -> Result<HttpResponseHeaders<HttpResponseOk<HashMap<String, String>>>, HttpError> {
    let body = body_param.into_inner();

    // We require that the user has supplied an upload token in the bearer header
    if let Some(token) = bearer.inner() {
        // Attempt to consume the token and mark it as unusable by other requests. A token may fail
        // to be consumed due a number of reasons to:
        //  1. Token was previously used
        //  2. Token is invalid
        //  3. Token does not match the email submitted
        //  4. Token is expired
        //
        // We currently return a single error code, 409 Conflict so as not to expose which of these
        // cases occurred. In the future we may want to relax this and return individual error codes
        let token_result = rqctx
            .context()
            .app
            .upload_token_store
            .consume(&body.email, token)
            .await
            .map_err(|err| {
                log::info!("Failed to consume upload token due to {:?}", err);
                HttpError::for_status(None, http::StatusCode::CONFLICT)
            });

        match token_result {
            Ok(_) => {
                let upload_result = crate::handlers::handle_test_application_files_upload(&rqctx, body).await;

                match upload_result {
                    Ok(r) => {
                        let mut resp = HttpResponseHeaders::new_unnamed(HttpResponseOk(r));

                        let headers = resp.headers_mut();
                        headers.insert("Access-Control-Allow-Origin", http::HeaderValue::from_static("*"));

                        Ok(resp)
                    }
                    Err(e) => Err(handle_anyhow_err_as_http_err(e)),
                }
            }
            Err(err) => Err(err),
        }
    } else {
        Err(HttpError::for_status(None, http::StatusCode::UNAUTHORIZED))
    }
}

/**
 * CORS functionality for file uploads
 */
#[endpoint {
    method = OPTIONS,
    path = "/application/files/upload",
}]
async fn listen_application_files_upload_requests_cors(
    rqctx: RequestContext<ServerContext>,
) -> Result<HttpResponseHeaders<HttpResponseOk<String>>, HttpError> {
    let mut resp = HttpResponseHeaders::new_unnamed(HttpResponseOk("".to_string()));
    let headers = resp.headers_mut();

    let allowed_origins =
        crate::cors::get_cors_origin_header(&rqctx, &["https://apply.oxide.computer", "https://oxide.computer"])
            .await?;
    headers.insert("Access-Control-Allow-Origin", allowed_origins);
    headers.insert("Access-Control-Allow-Headers", HeaderValue::from_static("*"));
    headers.insert("Access-Control-Allow-Method", HeaderValue::from_static("*"));

    Ok(resp)
}

/**
 * Listen for files being uploaded for incoming job applications */
#[endpoint {
    method = POST,
    path = "/application/files/upload",
}]
async fn listen_application_files_upload_requests(
    rqctx: RequestContext<ServerContext>,
    bearer: BearerToken,
    body_param: TypedBody<ApplicationFileUploadData>,
) -> Result<HttpResponseHeaders<HttpResponseOk<HashMap<String, String>>>, HttpError> {
    let body = body_param.into_inner();

    // We require that the user has supplied an upload token in the bearer header
    if let Some(token) = bearer.inner() {
        // Attempt to consume the token marked it as unusable by other requests. A token may fail
        // to be consumed due to:
        //  1. Token was previously used
        //  2. Token is invalid
        //  3. Token does not match the email submitted
        //  4. Token is expired
        //
        // We currently return a single error code, 409 Conflict so as not to expose which of these
        // cases occurred. In the future we may want to relax this and return individual error codes
        let token_result = rqctx
            .context()
            .app
            .upload_token_store
            .consume(&body.email, token)
            .await
            .map_err(|err| {
                log::info!("Failed to consume upload token due to {:?}", err);
                HttpError::for_status(None, http::StatusCode::CONFLICT)
            });

        log::info!("Application materials upload token consume result {:?}", token_result);

        match token_result {
            Ok(_) => {
                // Check the origin header. In the future this may be upgraded to a hard failure
                let origin_access = crate::cors::get_cors_origin_header(
                    &rqctx,
                    &["https://apply.oxide.computer", "https://oxide.computer"],
                )
                .await;

                let upload_result = crate::handlers::handle_application_files_upload(&rqctx, body).await;

                match upload_result {
                    Ok(r) => {
                        let mut resp = HttpResponseHeaders::new_unnamed(HttpResponseOk(r));

                        match origin_access {
                            Ok(origin) => {
                                let headers = resp.headers_mut();
                                headers.insert("Access-Control-Allow-Origin", origin);
                            }
                            Err(err) => {
                                warn!(
                                    "Submission to /application/files/upload failed CORS check. Err {:?}",
                                    err
                                );
                            }
                        }

                        Ok(resp)
                    }
                    Err(e) => Err(handle_anyhow_err_as_http_err(e)),
                }
            }
            Err(err) => Err(err),
        }
    } else {
        log::info!("Applicant upload request is missing a bearer token");
        Err(HttpError::for_status(None, http::StatusCode::UNAUTHORIZED))
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
    rqctx: RequestContext<ServerContext>,
    _auth: Bearer<AirtableToken>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    crate::handlers::handle_airtable_shipments_inbound_create(&rqctx, body_param.into_inner())
        .await
        .map(accepted)
        .map_err(handle_anyhow_err_as_http_err)
}

/**
 * Listen for orders being created by the Oxide store.
 */
#[endpoint {
    method = POST,
    path = "/store/order",
}]
async fn listen_store_order_create(
    rqctx: RequestContext<ServerContext>,
    _auth: Bearer<InternalToken>,
    body_param: TypedBody<Order>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    crate::handlers::handle_store_order_create(&rqctx, body_param.into_inner())
        .await
        .map(accepted)
        .map_err(handle_anyhow_err_as_http_err)
}

/**
 * Listen for shipment tracking updated from EasyPost.
 */
#[endpoint {
    method = POST,
    path = "/easypost/tracking/update",
}]
async fn listen_easypost_tracking_update_webhooks(
    rqctx: RequestContext<ServerContext>,
    body_param: TypedBody<EasyPostTrackingUpdateEvent>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    crate::handlers::handle_easypost_tracking_update(&rqctx, body_param.into_inner())
        .await
        .map(accepted)
        .map_err(handle_anyhow_err_as_http_err)
}

/// An EasyPost tracking update event.
/// FROM: https://www.easypost.com/docs/api#events
#[derive(Debug, Clone, JsonSchema, Deserialize, Serialize)]
pub struct EasyPostTrackingUpdateEvent {
    /// "Event".
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub object: String,
    /// Unique identifier, begins with "evt_".
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    /// "test" or "production"
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub mode: String,
    /// Result type and event name, see the "Possible Event Types" section for more information.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /* /// Previous values of relevant result attributes.
    #[serde(default)]
    pub previous_attributes: serde_json::Value,
    /// The object associated with the Event. See the "object" attribute on the result to determine
    /// its specific type. This field will not be returned when retrieving events directly from the
    /// API.
    #[serde(default)]
    pub result: serde_json::Value,*/
    /// The current status of the event. Possible values are "completed", "failed", "in_queue",
    /// "retrying", or "pending" (deprecated).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub status: String,
    /// Webhook URLs that have not yet been successfully notified as of the time this webhook event
    /// was sent. The URL receiving the Event will still be listed in pending_urls, as will any
    /// other URLs that receive the Event at the same time.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_urls: Vec<String>,
    /// Webhook URLs that have already been successfully notified as of the time this webhook was
    /// sent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub completed_urls: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
}

/**
 * Listen for shipment tracking updated from Shippo.
 */
#[endpoint {
    method = POST,
    path = "/shippo/tracking/update",
}]
async fn listen_shippo_tracking_update_webhooks(
    rqctx: RequestContext<ServerContext>,
    _auth: QueryToken<ShippoToken>,
    body_param: TypedBody<serde_json::Value>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    crate::handlers::handle_shippo_tracking_update(&rqctx, body_param.into_inner())
        .await
        .map(accepted)
        .map_err(handle_anyhow_err_as_http_err)
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
    rqctx: RequestContext<ServerContext>,
    body: HmacVerifiedBodyAudit<crate::handlers_checkr::CheckrWebhookVerification, checkr::WebhookEvent>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    crate::handlers::handle_checkr_background_update(&rqctx, body.into_inner()?)
        .await
        .map(accepted)
        .map_err(handle_anyhow_err_as_http_err)
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
    _rqctx: RequestContext<ServerContext>,
) -> Result<HttpResponseOk<UserConsentURL>, HttpError> {
    // Initialize the Google client.
    // You can use any of the libs here, they all use the same endpoint
    // for tokens and we will send all the scopes.
    let g = GoogleDrive::new_from_env("", "").await;

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
    rqctx: RequestContext<ServerContext>,
    query_args: Query<AuthCallback>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    crate::handlers_auth::handle_auth_google_callback(&rqctx, query_args)
        .await
        .map(accepted)
        .map_err(handle_anyhow_err_as_http_err)
}

/** Get the consent URL for GitHub auth. */
#[endpoint {
    method = GET,
    path = "/auth/github/consent",
}]
async fn listen_auth_github_consent(
    _rqctx: RequestContext<ServerContext>,
) -> Result<HttpResponseOk<UserConsentURL>, HttpError> {
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
    _rqctx: RequestContext<ServerContext>,
    body_param: TypedBody<serde_json::Value>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    let body = body_param.into_inner();

    warn!("github callback: {:?}", body);

    Ok(HttpResponseAccepted("ok".to_string()))
}

/** Get the consent URL for Gusto auth. */
#[endpoint {
    method = GET,
    path = "/auth/gusto/consent",
}]
async fn listen_auth_gusto_consent(
    _rqctx: RequestContext<ServerContext>,
) -> Result<HttpResponseOk<UserConsentURL>, HttpError> {
    // Initialize the Gusto client.
    let g = Gusto::new_from_env("", "", gusto_api::RootProductionServer {});

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
    rqctx: RequestContext<ServerContext>,
    query_args: Query<AuthCallback>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    crate::handlers_auth::handle_auth_gusto_callback(&rqctx, query_args)
        .await
        .map(accepted)
        .map_err(handle_anyhow_err_as_http_err)
}

/** Listen to deauthorization requests for our Zoom app. */
#[endpoint {
    method = GET,
    path = "/auth/zoom/deauthorization",
}]
async fn listen_auth_zoom_deauthorization(
    _rqctx: RequestContext<ServerContext>,
    body_param: TypedBody<serde_json::Value>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    let body = body_param.into_inner();

    warn!("zoom deauthorization: {:?}", body);

    Ok(HttpResponseAccepted("ok".to_string()))
}

/** Get the consent URL for Zoom auth. */
#[endpoint {
    method = GET,
    path = "/auth/zoom/consent",
}]
async fn listen_auth_zoom_consent(
    _rqctx: RequestContext<ServerContext>,
) -> Result<HttpResponseOk<UserConsentURL>, HttpError> {
    // Initialize the Zoom client.
    let g = Zoom::new_from_env("", "");

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
    rqctx: RequestContext<ServerContext>,
    query_args: Query<AuthCallback>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    crate::handlers_auth::handle_auth_zoom_callback(&rqctx, query_args)
        .await
        .map(accepted)
        .map_err(handle_anyhow_err_as_http_err)
}

/** Get the consent URL for Slack auth. */
#[endpoint {
    method = GET,
    path = "/auth/slack/consent",
}]
async fn listen_auth_slack_consent(
    _rqctx: RequestContext<ServerContext>,
) -> Result<HttpResponseOk<UserConsentURL>, HttpError> {
    // Initialize the Slack client.
    let s = Slack::new_from_env("", "", "");

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
    rqctx: RequestContext<ServerContext>,
    query_args: Query<AuthCallback>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    crate::handlers_auth::handle_auth_slack_callback(&rqctx, query_args)
        .await
        .map(accepted)
        .map_err(handle_anyhow_err_as_http_err)
}

/** Get the consent URL for QuickBooks auth. */
#[endpoint {
    method = GET,
    path = "/auth/quickbooks/consent",
}]
async fn listen_auth_quickbooks_consent(
    _rqctx: RequestContext<ServerContext>,
) -> Result<HttpResponseOk<UserConsentURL>, HttpError> {
    // Initialize the QuickBooks client.
    let g = QuickBooks::new_from_env("", "", "");

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
    rqctx: RequestContext<ServerContext>,
    query_args: Query<AuthCallback>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    crate::handlers_auth::handle_auth_quickbooks_callback(&rqctx, query_args)
        .await
        .map(accepted)
        .map_err(handle_anyhow_err_as_http_err)
}

/** Listen for webhooks from Plaid. */
#[endpoint {
    method = POST,
    path = "/plaid",
}]
async fn listen_auth_plaid_callback(
    _rqctx: RequestContext<ServerContext>,
    body_param: TypedBody<serde_json::Value>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    let body = body_param.into_inner();

    warn!("plaid callback: {:?}", body);

    Ok(HttpResponseAccepted("ok".to_string()))
}

/** Get the consent URL for DocuSign auth. */
#[endpoint {
    method = GET,
    path = "/auth/docusign/consent",
}]
async fn listen_auth_docusign_consent(
    _rqctx: RequestContext<ServerContext>,
) -> Result<HttpResponseOk<UserConsentURL>, HttpError> {
    // Initialize the DocuSign client.
    let g = DocuSign::new_from_env("", "", "", "");

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
    rqctx: RequestContext<ServerContext>,
    query_args: Query<AuthCallback>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    crate::handlers_auth::handle_auth_docusign_callback(&rqctx, query_args)
        .await
        .map(accepted)
        .map_err(handle_anyhow_err_as_http_err)
}

/** Listen for updates to our docusign envelopes. */
#[endpoint {
    method = POST,
    path = "/docusign/envelope/update",
}]
async fn listen_docusign_envelope_update_webhooks(
    rqctx: RequestContext<ServerContext>,
    body: HmacVerifiedBody<crate::handlers_docusign::DocusignWebhookVerification, docusign::Envelope>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    crate::handlers::handle_docusign_envelope_update(&rqctx, body.into_inner()?)
        .await
        .map(accepted)
        .map_err(handle_anyhow_err_as_http_err)
}

/** Listen for analytics page view events. */
#[endpoint {
    method = POST,
    path = "/analytics/page_view",
}]
async fn listen_analytics_page_view_webhooks(
    rqctx: RequestContext<ServerContext>,
    body_param: TypedBody<NewPageView>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    crate::handlers::handle_analytics_page_view(&rqctx, body_param.into_inner())
        .await
        .map(accepted)
        .map_err(handle_anyhow_err_as_http_err)
}

/** Listen for Slack commands webhooks. */
#[endpoint {
    method = POST,
    path = "/slack/commands",
    content_type = "application/x-www-form-urlencoded"
}]
async fn listen_slack_commands_webhooks(
    rqctx: RequestContext<ServerContext>,
    body: HmacVerifiedBodyAudit<crate::handlers_slack::SlackWebhookVerification, BotCommand>,
) -> Result<HttpResponseOk<serde_json::Value>, HttpError> {
    crate::handlers::handle_slack_commands(&rqctx, body.into_inner()?)
        .await
        .map(HttpResponseOk)
        .map_err(handle_anyhow_err_as_http_err)
}

/** Listen for Slack interactive webhooks. */
#[endpoint {
    method = POST,
    path = "/slack/interactive",
    content_type = "application/x-www-form-urlencoded"
}]
async fn listen_slack_interactive_webhooks(
    rqctx: RequestContext<ServerContext>,
    body: HmacVerifiedBodyAudit<crate::handlers_slack::SlackWebhookVerification, InteractiveEvent>,
) -> Result<HttpResponseOk<String>, HttpError> {
    crate::handlers::handle_slack_interactive(&rqctx, body.into_inner()?.payload)
        .await
        .map(ok)
        .map_err(handle_anyhow_err_as_http_err)
}

/** Listen for shipbob webhooks. */
#[endpoint {
    method = POST,
    path = "/shipbob",
}]
async fn listen_shipbob_webhooks(
    rqctx: RequestContext<ServerContext>,
    _auth: QueryTokenAudit<InternalToken>,
    body_param: TypedBody<serde_json::Value>,
) -> Result<HttpResponseOk<String>, HttpError> {
    crate::handlers::handle_shipbob(&rqctx, body_param.into_inner())
        .await
        .map(ok)
        .map_err(handle_anyhow_err_as_http_err)
}

// The RFD index does not support any sorting mechanisms
#[derive(Debug, Deserialize, JsonSchema)]
struct RFDIndexScanParam {}

// The RFD index is always sorted by number in ascending order
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
enum RFDIndexPageSelector {
    Number(PaginationOrder, i32),
}

fn rfd_index_scan_params(_params: &WhichPage<RFDIndexScanParam, RFDIndexPageSelector>) -> RFDIndexScanParam {
    RFDIndexScanParam {}
}

fn rfd_index_page_selector(item: &RFDIndexEntry, _scan_params: &RFDIndexScanParam) -> RFDIndexPageSelector {
    RFDIndexPageSelector::Number(PaginationOrder::Ascending, item.number)
}

/// List metadata of all RFDs
#[endpoint {
    method = GET,
    path = "/rfds",
}]
async fn listen_rfd_index(
    rqctx: RequestContext<ServerContext>,
    _auth: Bearer<RFDToken>,
    query: Query<PaginationParams<RFDIndexScanParam, RFDIndexPageSelector>>,
) -> Result<HttpResponseOk<ResultsPage<RFDIndexEntry>>, HttpError> {
    let params = query.into_inner();
    let offset = match params.page {
        WhichPage::First(_) => 0,
        WhichPage::Next(RFDIndexPageSelector::Number(_dir, offset)) => offset,
    };
    let limit = rqctx.page_limit(&params)?.get();
    let scan_params = rfd_index_scan_params(&params.page);

    match crate::handlers_rfd::handle_rfd_index(&rqctx.context().app, offset, limit).await {
        Ok(entries) => Ok(HttpResponseOk(ResultsPage::new(
            entries,
            &scan_params,
            rfd_index_page_selector,
        )?)),
        Err(err) => Err(handle_anyhow_err_as_http_err(err)),
    }
}

/// Get an rfd
#[endpoint {
    method = GET,
    path = "/rfd/{num}",
}]
async fn listen_rfd_view(
    rqctx: RequestContext<ServerContext>,
    _auth: Bearer<RFDToken>,
    path_params: Path<RFDPathParams>,
) -> Result<HttpResponseOk<RFDEntry>, HttpError> {
    match crate::handlers_rfd::handle_rfd_view(&rqctx.context().app, path_params.into_inner().num).await {
        Ok(Some(rfd)) => Ok(HttpResponseOk(rfd)),
        Ok(None) => Err(HttpError::for_not_found(None, "".to_string())),
        Err(err) => Err(handle_anyhow_err_as_http_err(err)),
    }
}

/** Listen for triggering a function run of sync repos. */
#[endpoint {
    method = POST,
    path = "/run/sync-repos",
}]
async fn trigger_sync_repos_create(
    rqctx: RequestContext<ServerContext>,
    _auth: Bearer<InternalToken>,
) -> Result<HttpResponseAccepted<uuid::Uuid>, HttpError> {
    crate::handlers_cron::run_subcmd_job(rqctx.context(), "sync-repos")
        .await
        .map(HttpResponseAccepted)
        .map_err(handle_anyhow_err_as_http_err)
}

/** Listen for triggering a function run of sync RFDs. */
#[endpoint {
    method = POST,
    path = "/run/sync-rfds",
}]
async fn trigger_sync_rfds_create(
    rqctx: RequestContext<ServerContext>,
    _auth: Bearer<InternalToken>,
) -> Result<HttpResponseAccepted<uuid::Uuid>, HttpError> {
    crate::handlers_cron::run_subcmd_job(rqctx.context(), "sync-rfds")
        .await
        .map(HttpResponseAccepted)
        .map_err(handle_anyhow_err_as_http_err)
}

/** Listen for triggering a function run of sync travel. */
#[endpoint {
    method = POST,
    path = "/run/sync-travel",
}]
async fn trigger_sync_travel_create(
    rqctx: RequestContext<ServerContext>,
    _auth: Bearer<InternalToken>,
) -> Result<HttpResponseAccepted<uuid::Uuid>, HttpError> {
    crate::handlers_cron::run_subcmd_job(rqctx.context(), "sync-travel")
        .await
        .map(HttpResponseAccepted)
        .map_err(handle_anyhow_err_as_http_err)
}

/** Listen for triggering a function run of sync zoho. */
#[endpoint {
    method = POST,
    path = "/run/sync-zoho",
}]
async fn trigger_sync_zoho_create(
    rqctx: RequestContext<ServerContext>,
    _auth: Bearer<InternalToken>,
) -> Result<HttpResponseAccepted<uuid::Uuid>, HttpError> {
    crate::handlers_cron::run_subcmd_job(rqctx.context(), "sync-zoho")
        .await
        .map(HttpResponseAccepted)
        .map_err(handle_anyhow_err_as_http_err)
}

/** Listen for triggering a function run of sync functions. */
#[endpoint {
    method = POST,
    path = "/run/sync-functions",
}]
async fn trigger_sync_functions_create(
    rqctx: RequestContext<ServerContext>,
    _auth: Bearer<InternalToken>,
) -> Result<HttpResponseAccepted<uuid::Uuid>, HttpError> {
    crate::handlers_cron::run_subcmd_job(rqctx.context(), "sync-functions")
        .await
        .map(HttpResponseAccepted)
        .map_err(handle_anyhow_err_as_http_err)
}

/** Listen for triggering a function run of sync finance. */
#[endpoint {
    method = POST,
    path = "/run/sync-finance",
}]
async fn trigger_sync_finance_create(
    rqctx: RequestContext<ServerContext>,
    _auth: Bearer<InternalToken>,
) -> Result<HttpResponseAccepted<uuid::Uuid>, HttpError> {
    crate::handlers_cron::run_subcmd_job(rqctx.context(), "sync-finance")
        .await
        .map(HttpResponseAccepted)
        .map_err(handle_anyhow_err_as_http_err)
}

/** Listen for triggering a function run of sync salesforce. */
#[endpoint {
    method = POST,
    path = "/run/sync-salesforce",
}]
async fn trigger_sync_salesforce_create(
    rqctx: RequestContext<ServerContext>,
    _auth: Bearer<InternalToken>,
) -> Result<HttpResponseAccepted<uuid::Uuid>, HttpError> {
    crate::handlers_cron::run_subcmd_job(rqctx.context(), "sync-salesforce")
        .await
        .map(HttpResponseAccepted)
        .map_err(handle_anyhow_err_as_http_err)
}

/** Listen for triggering a function run of sync shipments. */
#[endpoint {
    method = POST,
    path = "/run/sync-shipments",
}]
async fn trigger_sync_shipments_create(
    rqctx: RequestContext<ServerContext>,
    _auth: Bearer<InternalToken>,
) -> Result<HttpResponseAccepted<uuid::Uuid>, HttpError> {
    crate::handlers_cron::run_subcmd_job(rqctx.context(), "sync-shipments")
        .await
        .map(HttpResponseAccepted)
        .map_err(handle_anyhow_err_as_http_err)
}

/** Listen for triggering a function run of sync shorturls. */
#[endpoint {
    method = POST,
    path = "/run/sync-shorturls",
}]
async fn trigger_sync_shorturls_create(
    rqctx: RequestContext<ServerContext>,
    _auth: Bearer<InternalToken>,
) -> Result<HttpResponseAccepted<uuid::Uuid>, HttpError> {
    crate::handlers_cron::run_subcmd_job(rqctx.context(), "sync-shorturls")
        .await
        .map(HttpResponseAccepted)
        .map_err(handle_anyhow_err_as_http_err)
}

/** Listen for triggering a function run of sync configs. */
#[endpoint {
    method = POST,
    path = "/run/sync-configs",
}]
async fn trigger_sync_configs_create(
    rqctx: RequestContext<ServerContext>,
    _auth: Bearer<InternalToken>,
) -> Result<HttpResponseAccepted<uuid::Uuid>, HttpError> {
    crate::handlers_cron::run_subcmd_job(rqctx.context(), "sync-configs")
        .await
        .map(HttpResponseAccepted)
        .map_err(handle_anyhow_err_as_http_err)
}

/** Listen for triggering a function run of sync recorded meetings. */
#[endpoint {
    method = POST,
    path = "/run/sync-recorded-meetings",
}]
async fn trigger_sync_recorded_meetings_create(
    rqctx: RequestContext<ServerContext>,
    _auth: Bearer<InternalToken>,
) -> Result<HttpResponseAccepted<uuid::Uuid>, HttpError> {
    crate::handlers_cron::run_subcmd_job(rqctx.context(), "sync-recorded-meetings")
        .await
        .map(HttpResponseAccepted)
        .map_err(handle_anyhow_err_as_http_err)
}

/** Listen for triggering a function run of sync asset inventory. */
#[endpoint {
    method = POST,
    path = "/run/sync-asset-inventory",
}]
async fn trigger_sync_asset_inventory_create(
    rqctx: RequestContext<ServerContext>,
    _auth: Bearer<InternalToken>,
) -> Result<HttpResponseAccepted<uuid::Uuid>, HttpError> {
    crate::handlers_cron::run_subcmd_job(rqctx.context(), "sync-asset-inventory")
        .await
        .map(HttpResponseAccepted)
        .map_err(handle_anyhow_err_as_http_err)
}

/** Listen for triggering a function run of sync swag inventory. */
#[endpoint {
    method = POST,
    path = "/run/sync-swag-inventory",
}]
async fn trigger_sync_swag_inventory_create(
    rqctx: RequestContext<ServerContext>,
    _auth: Bearer<InternalToken>,
) -> Result<HttpResponseAccepted<uuid::Uuid>, HttpError> {
    crate::handlers_cron::run_subcmd_job(rqctx.context(), "sync-swag-inventory")
        .await
        .map(HttpResponseAccepted)
        .map_err(handle_anyhow_err_as_http_err)
}

/** Listen for triggering a function run of sync interviews. */
#[endpoint {
    method = POST,
    path = "/run/sync-interviews",
}]
async fn trigger_sync_interviews_create(
    rqctx: RequestContext<ServerContext>,
    _auth: Bearer<InternalToken>,
) -> Result<HttpResponseAccepted<uuid::Uuid>, HttpError> {
    crate::handlers_cron::run_subcmd_job(rqctx.context(), "sync-interviews")
        .await
        .map(HttpResponseAccepted)
        .map_err(handle_anyhow_err_as_http_err)
}

/** Listen for triggering a function run of sync applications. */
#[endpoint {
    method = POST,
    path = "/run/sync-applications",
}]
async fn trigger_sync_applications_create(
    rqctx: RequestContext<ServerContext>,
    _auth: Bearer<InternalToken>,
) -> Result<HttpResponseAccepted<uuid::Uuid>, HttpError> {
    crate::handlers_cron::run_subcmd_job(rqctx.context(), "sync-applications")
        .await
        .map(HttpResponseAccepted)
        .map_err(handle_anyhow_err_as_http_err)
}

/** Listen for triggering a function run of sync analytics. */
#[endpoint {
    method = POST,
    path = "/run/sync-analytics",
}]
async fn trigger_sync_analytics_create(
    rqctx: RequestContext<ServerContext>,
    _auth: Bearer<InternalToken>,
) -> Result<HttpResponseAccepted<uuid::Uuid>, HttpError> {
    crate::handlers_cron::run_subcmd_job(rqctx.context(), "sync-analytics")
        .await
        .map(HttpResponseAccepted)
        .map_err(handle_anyhow_err_as_http_err)
}

/** Listen for triggering a function run of sync companies. */
#[endpoint {
    method = POST,
    path = "/run/sync-companies",
}]
async fn trigger_sync_companies_create(
    rqctx: RequestContext<ServerContext>,
    _auth: Bearer<InternalToken>,
) -> Result<HttpResponseAccepted<uuid::Uuid>, HttpError> {
    crate::handlers_cron::run_subcmd_job(rqctx.context(), "sync-companies")
        .await
        .map(HttpResponseAccepted)
        .map_err(handle_anyhow_err_as_http_err)
}

/** Listen for triggering a function run of sync other. */
#[endpoint {
    method = POST,
    path = "/run/sync-other",
}]
async fn trigger_sync_other_create(
    rqctx: RequestContext<ServerContext>,
    _auth: Bearer<InternalToken>,
) -> Result<HttpResponseAccepted<uuid::Uuid>, HttpError> {
    crate::handlers_cron::run_subcmd_job(rqctx.context(), "sync-other")
        .await
        .map(HttpResponseAccepted)
        .map_err(handle_anyhow_err_as_http_err)
}

/** Listen for triggering a function run of sync huddles. */
#[endpoint {
    method = POST,
    path = "/run/sync-huddles",
}]
async fn trigger_sync_huddles_create(
    rqctx: RequestContext<ServerContext>,
    _auth: Bearer<InternalToken>,
) -> Result<HttpResponseAccepted<uuid::Uuid>, HttpError> {
    crate::handlers_cron::run_subcmd_job(rqctx.context(), "sync-huddles")
        .await
        .map(HttpResponseAccepted)
        .map_err(handle_anyhow_err_as_http_err)
}

/** Listen for triggering a function run of sync mailing lists. */
#[endpoint {
    method = POST,
    path = "/run/sync-mailing-lists",
}]
async fn trigger_sync_mailing_lists_create(
    rqctx: RequestContext<ServerContext>,
    _auth: Bearer<InternalToken>,
) -> Result<HttpResponseAccepted<uuid::Uuid>, HttpError> {
    crate::handlers_cron::run_subcmd_job(rqctx.context(), "sync-mailing-lists")
        .await
        .map(HttpResponseAccepted)
        .map_err(handle_anyhow_err_as_http_err)
}

/** Listen for triggering a function run of sync journal clubs. */
#[endpoint {
    method = POST,
    path = "/run/sync-journal-clubs",
}]
async fn trigger_sync_journal_clubs_create(
    rqctx: RequestContext<ServerContext>,
    _auth: Bearer<InternalToken>,
) -> Result<HttpResponseAccepted<uuid::Uuid>, HttpError> {
    crate::handlers_cron::run_subcmd_job(rqctx.context(), "sync-journal-clubs")
        .await
        .map(HttpResponseAccepted)
        .map_err(handle_anyhow_err_as_http_err)
}

/** Listen for triggering a function run of sync api tokens. */
#[endpoint {
    method = POST,
    path = "/run/sync-api-tokens",
}]
async fn trigger_sync_api_tokens_create(
    rqctx: RequestContext<ServerContext>,
    _auth: Bearer<InternalToken>,
) -> Result<HttpResponseAccepted<uuid::Uuid>, HttpError> {
    crate::handlers_cron::run_subcmd_job(rqctx.context(), "sync-api-tokens")
        .await
        .map(HttpResponseAccepted)
        .map_err(handle_anyhow_err_as_http_err)
}

/** Listen for triggering a cleanup of all in-progress sagas, we typically run this when the server
 * is shutting down. */
#[endpoint {
    method = POST,
    path = "/run/cleanup",
}]
async fn trigger_cleanup_create(
    rqctx: RequestContext<ServerContext>,
    _auth: Bearer<InternalToken>,
) -> Result<HttpResponseAccepted<()>, HttpError> {
    do_cleanup(rqctx.context())
        .await
        .map(HttpResponseAccepted)
        .map_err(handle_anyhow_err_as_http_err)
}

#[derive(Deserialize, Debug, JsonSchema)]
pub struct FunctionPathParams {
    pub uuid: String,
}

async fn do_cleanup(ctx: &ServerContext) -> Result<()> {
    let sec = &ctx.sec;
    // Get all our sagas.
    let sagas = sec.saga_list(None, std::num::NonZeroU32::new(1000).unwrap()).await;

    // TODO: Shutdown the executer.
    // This causes a compile time error, figure it out.
    //sec.shutdown().await;

    // Set all the in-progress sagas to 'cancelled'.
    for saga in sagas {
        // Get the saga in the database.
        if let Some(mut f) = Function::get_from_db(&ctx.app.db, saga.id.to_string()).await {
            // We only care about jobs that aren't completed.
            if f.status != octorust::types::JobStatus::Completed.to_string() {
                // Let's set the job to "Completed".
                f.status = octorust::types::JobStatus::Completed.to_string();
                // Let's set the conclusion to "Cancelled".
                f.conclusion = octorust::types::Conclusion::Cancelled.to_string();
                f.completed_at = Some(Utc::now());

                f.update(&ctx.app.db).await.map_err(handle_anyhow_err_as_http_err)?;
                info!("set saga `{}` `{}` as `{}`", f.name, f.saga_id, f.status);
            }
        }
    }

    Ok(())
}

fn ok(_: impl Any) -> HttpResponseOk<String> {
    HttpResponseOk("ok".to_string())
}

fn accepted(_: impl Any) -> HttpResponseAccepted<String> {
    HttpResponseAccepted("ok".to_string())
}

fn handle_anyhow_err_as_http_err(err: anyhow::Error) -> HttpError {
    error!("Http error {:?}", err);

    // We use the debug formatting here so we get the stack trace.
    HttpError::for_internal_error(format!("{:?}", err))
}
