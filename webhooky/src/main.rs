#![recursion_limit = "256"]
#![feature(async_closure)]
mod auth;
mod context;
#[macro_use]
mod core;
mod cors;
mod event_types;
mod github_types;
mod handlers;
mod handlers_auth;
mod handlers_checkr;
mod handlers_cron;
mod handlers_docusign;
mod handlers_github;
mod handlers_hiring;
mod handlers_rfd;
mod handlers_slack;
// mod handlers_sendgrid;
mod http;
mod repos;
mod sagas;
mod server;
mod slack_commands;
// mod tracking_numbers;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate serde_json;
#[macro_use]
extern crate cio_api;

use std::env;

use anyhow::{bail, Result};
use clap::Parser;
use log::info;
use sentry::{
    protocol::{Context as SentryContext, Event},
    IntoDsn,
};
use slog::Drain;
use std::fs::File;

use crate::context::Context;
use crate::server::APIConfig;

fn main() -> Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(4 * 1024 * 1024)
        .build()?
        .block_on(async { tokio_main().await })
}

async fn tokio_main() -> Result<()> {
    let opts: crate::core::Opts = crate::core::Opts::parse();

    // Initialize sentry.
    let sentry_dsn = env::var("WEBHOOKY_SENTRY_DSN").unwrap_or_default();
    let _guard = sentry::init(sentry::ClientOptions {
        debug: opts.debug,
        dsn: sentry_dsn.clone().into_dsn()?,

        // Send 10% of all transactions to Sentry.
        // This can be increased as we figure out what volume looks like at
        traces_sample_rate: 0.1,

        // Define custom rate limiting for database query events. Without aggressive rate limiting
        // these will far exceed any transactions limits we are allowed.
        before_send: Some(std::sync::Arc::new(|event: Event<'static>| {
            if let Some(SentryContext::Trace(trace_ctx)) = event.contexts.get("trace") {
                if let Some(ref op) = trace_ctx.op {
                    if op == "db.sql.query" && rand::random::<f32>() > 0.001 {
                        return None;
                    }
                }
            }

            Some(event)
        })),

        release: Some(env::var("GIT_HASH").unwrap_or_default().into()),
        environment: Some(
            env::var("SENTRY_ENV")
                .unwrap_or_else(|_| "development".to_string())
                .into(),
        ),

        // We want to send 100% of errors to Sentry.
        sample_rate: 1.0,

        default_integrations: true,

        session_mode: sentry::SessionMode::Request,
        ..sentry::ClientOptions::default()
    });

    let logger = if opts.json {
        // TODO: the problem is the global logger, LOGGER, is not being changed to use json so
        // the output from the reexec functions will not be json formatted. This should be
        // fixed.
        // Build a JSON slog logger.
        // This way cloud run can read the logs as JSON.
        let drain = slog_json::Json::new(std::io::stdout())
            .add_default_keys()
            .build()
            .fuse();
        let drain = slog_async::Async::new(drain).build().fuse();
        let drain = sentry::integrations::slog::SentryDrain::new(drain);
        slog::Logger::root(drain, slog::slog_o!())
    } else {
        let decorator = slog_term::TermDecorator::new().build();
        let drain = slog_term::FullFormat::new(decorator).build().fuse();
        let drain = slog_async::Async::new(drain).build().fuse();
        let drain = sentry::integrations::slog::SentryDrain::new(drain);
        slog::Logger::root(drain, slog::slog_o!())
    };

    // Initialize our logger.
    let _scope_guard = slog_scope::set_global_logger(logger.clone());

    // Set the logging level.
    let mut log_level = log::Level::Info;
    if opts.debug {
        log_level = log::Level::Debug;
    }
    let _log_guard = slog_stdlog::init_with_level(log_level)?;

    let api = APIConfig::new()?;

    let context = Context::new(1, api.schema.clone(), logger).await?;

    if let Err(err) = run_cmd(opts.clone(), api, context).await {
        sentry::integrations::anyhow::capture_anyhow(&anyhow::anyhow!("{:?}", err));
        bail!("running cmd `{:?}` failed: {:?}", &opts.subcmd, err);
    }

    Ok(())
}

async fn run_cmd(opts: crate::core::Opts, api: APIConfig, context: Context) -> Result<()> {
    sentry::configure_scope(|scope| {
        scope.set_tag("command", &std::env::args().collect::<Vec<String>>().join(" "));
    });

    match opts.subcmd {
        crate::core::SubCommand::Server(s) => {
            sentry::configure_scope(|scope| {
                scope.set_tag("do-cron", s.do_cron.to_string());
            });

            crate::server::server(s, api.api, context, opts.debug).await?;
        }
        crate::core::SubCommand::CreateServerSpec(spec) => {
            let spec_file = spec.spec_file;
            info!("writing OpenAPI spec to {}...", spec_file.to_str().unwrap());
            let mut buffer = File::create(spec_file)?;
            api.open_api().write(&mut buffer)?;
        }
        crate::core::SubCommand::SendRFDChangelog(_) => {
            let Context { db, company, .. } = context;
            cio_api::rfds::send_rfd_changelog(&db, &company).await?;
        }
        crate::core::SubCommand::SyncAnalytics(_) => {
            let Context { db, company, .. } = context;
            cio_api::analytics::refresh_analytics(&db, &company).await?;
        }
        crate::core::SubCommand::SyncAPITokens(_) => {
            let Context { db, company, .. } = context;
            cio_api::api_tokens::refresh_api_tokens(&db, &company).await?;
        }
        crate::core::SubCommand::SyncApplications(_) => {
            let Context {
                app_config,
                db,
                company,
                ..
            } = context;

            // Do the new applicants.
            let app_config = app_config.read().unwrap().clone();
            cio_api::applicants::refresh_new_applicants_and_reviews(&db, &company, &app_config).await?;
            cio_api::applicant_reviews::refresh_reviews(&db, &company).await?;

            // Refresh DocuSign for the applicants.
            cio_api::applicants::refresh_docusign_for_applicants(&db, &company, &app_config).await?;
        }
        crate::core::SubCommand::SyncAssetInventory(_) => {
            let Context { db, company, .. } = context;
            cio_api::asset_inventory::refresh_asset_items(&db, &company).await?;
        }
        crate::core::SubCommand::SyncCompanies(_) => {
            let Context { db, .. } = context;
            cio_api::companies::refresh_companies(&db).await?;
        }
        crate::core::SubCommand::SyncConfigs(_) => {
            let Context {
                app_config,
                db,
                company,
                ..
            } = context;
            let config = app_config.read().unwrap().clone();
            cio_api::configs::refresh_db_configs_and_airtable(&db, &company, &config).await?;
        }
        crate::core::SubCommand::SyncFinance(_) => {
            let Context {
                app_config,
                db,
                company,
                ..
            } = context;
            let app_config = app_config.read().unwrap().clone();
            cio_api::finance::refresh_all_finance(&db, &company, &app_config.finance).await?;
        }
        crate::core::SubCommand::SyncFunctions(_) => {
            let Context { db, company, .. } = context;
            cio_api::functions::refresh_functions(&db, &company).await?;
        }
        crate::core::SubCommand::SyncHuddles(_) => {
            let Context { db, company, .. } = context;
            cio_api::huddles::sync_changes_to_google_events(&db, &company).await?;
            cio_api::huddles::sync_huddles(&db, &company).await?;
            cio_api::huddles::send_huddle_reminders(&db, &company).await?;
            cio_api::huddles::sync_huddle_meeting_notes(&company).await?;
        }
        crate::core::SubCommand::SyncInterviews(_) => {
            let Context { db, company, .. } = context;
            cio_api::interviews::refresh_interviews(&db, &company).await?;
            cio_api::interviews::compile_packets(&db, &company).await?;
        }
        crate::core::SubCommand::SyncJournalClubs(_) => {
            let Context { db, company, .. } = context;
            cio_api::journal_clubs::refresh_db_journal_club_meetings(&db, &company).await?;
        }
        crate::core::SubCommand::SyncMailingLists(_) => {
            let Context { db, company, .. } = context;
            cio_api::mailing_list::refresh_db_mailing_list_subscribers(&db, &company).await?;
            cio_api::rack_line::refresh_db_rack_line_subscribers(&db, &company).await?;
        }
        crate::core::SubCommand::SyncRecordedMeetings(_) => {
            let Context { db, company, .. } = context;
            cio_api::recorded_meetings::refresh_zoom_recorded_meetings(&db, &company).await?;
            cio_api::recorded_meetings::refresh_google_recorded_meetings(&db, &company).await?;
        }
        crate::core::SubCommand::SyncRepos(_) => {
            let Context { db, company, .. } = context;
            let sync_result = cio_api::repos::sync_all_repo_settings(&db, &company).await;
            let refresh_result = cio_api::repos::refresh_db_github_repos(&db, &company).await;

            if let Err(ref e) = sync_result {
                log::error!("Failed syncing repo settings {:?}", e);
            }

            if let Err(ref e) = refresh_result {
                log::error!("Failed refreshing GitHub db repos {:?}", e);
            }

            sync_result?;
            refresh_result?;
        }
        crate::core::SubCommand::SyncRFDs(_) => {
            let Context { db, company, .. } = context;
            cio_api::rfds::refresh_db_rfds(&db, &company).await?;
            cio_api::rfds::cleanup_rfd_pdfs(&db, &company).await?;
        }
        crate::core::SubCommand::SyncOther(_) => {
            let Context { company, .. } = context;
            cio_api::tailscale::cleanup_old_tailscale_devices(&company).await?;
            cio_api::tailscale::cleanup_old_tailscale_cloudflare_dns(&company).await?;
            cio_api::customers::sync_customer_meeting_notes(&company).await?;
        }
        crate::core::SubCommand::SyncShipments(_) => {
            let Context { db, company, .. } = context;
            let inbound_result = cio_api::shipments::refresh_inbound_shipments(&db, &company).await;
            let outbound_result = cio_api::shipments::refresh_outbound_shipments(&db, &company).await;

            if let Err(ref e) = inbound_result {
                log::error!("Failed to refresh inbound shipments {:?}", e);
            }

            if let Err(ref e) = outbound_result {
                log::error!("Failed to refresh outbound shipments {:?}", e);
            }

            inbound_result?;
            outbound_result?;
        }
        crate::core::SubCommand::SyncShorturls(_) => {
            let Context { db, company, .. } = context;
            cio_api::shorturls::refresh_shorturls(&db, &company).await?;
        }
        crate::core::SubCommand::SyncSwagInventory(_) => {
            let Context { db, company, .. } = context;
            cio_api::swag_inventory::refresh_swag_items(&db, &company).await?;
            cio_api::swag_inventory::refresh_swag_inventory_items(&db, &company).await?;
            cio_api::swag_inventory::refresh_barcode_scans(&db, &company).await?;
        }
        crate::core::SubCommand::SyncTravel(_) => {
            let Context { db, company, .. } = context;
            cio_api::travel::refresh_trip_actions(&db, &company).await?;
        }
        crate::core::SubCommand::SyncZoho(_) => {
            let Context { db, company, .. } = context;
            cio_api::zoho::refresh_leads(&db, &company).await?;
        }
    }

    Ok(())
}
