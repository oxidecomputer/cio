#![recursion_limit = "256"]
#![feature(async_closure)]
mod auth;
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
use cio_api::{companies::Companys, db::Database};
use clap::Parser;
use sentry::{
    protocol::{Context, Event},
    IntoDsn,
};
use slog::Drain;

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
            if let Some(Context::Trace(trace_ctx)) = event.contexts.get("trace") {
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

    if let Err(err) = run_cmd(opts.clone(), logger).await {
        sentry::integrations::anyhow::capture_anyhow(&anyhow::anyhow!("{:?}", err));
        bail!("running cmd `{:?}` failed: {:?}", &opts.subcmd, err);
    }

    Ok(())
}

async fn run_cmd(opts: crate::core::Opts, logger: slog::Logger) -> Result<()> {
    sentry::configure_scope(|scope| {
        scope.set_tag("command", &std::env::args().collect::<Vec<String>>().join(" "));
    });

    match opts.subcmd {
        crate::core::SubCommand::Server(s) => {
            sentry::configure_scope(|scope| {
                scope.set_tag("do-cron", s.do_cron.to_string());
            });
            crate::server::server(s, logger, opts.debug).await?;
        }
        crate::core::SubCommand::SendRFDChangelog(_) => {
            let db = Database::new().await;
            let companies = Companys::get_from_db(&db, 1).await?;

            // Iterate over the companies and send.
            for company in companies {
                cio_api::rfds::send_rfd_changelog(&db, &company).await?;
            }
        }
        crate::core::SubCommand::SyncAnalytics(_) => {
            let db = Database::new().await;
            let companies = Companys::get_from_db(&db, 1).await?;

            // Iterate over the companies and update.
            let tasks: Vec<_> = companies
                .into_iter()
                .map(|company| {
                    tokio::spawn(enclose! { (db)  async move {
                        cio_api::analytics::refresh_analytics(&db, &company).await
                    }})
                })
                .collect();

            let mut results: Vec<Result<()>> = Default::default();
            for task in tasks {
                results.push(task.await?);
            }

            for result in results {
                result?;
            }
        }
        crate::core::SubCommand::SyncAPITokens(_) => {
            let db = Database::new().await;
            let companies = Companys::get_from_db(&db, 1).await?;

            // Iterate over the companies and update.
            let tasks: Vec<_> = companies
                .into_iter()
                .map(|company| {
                    tokio::spawn(enclose! { (db) async move {
                        cio_api::api_tokens::refresh_api_tokens(&db, &company).await
                    }})
                })
                .collect();

            let mut results: Vec<Result<()>> = Default::default();
            for task in tasks {
                results.push(task.await?);
            }

            for result in results {
                result?;
            }
        }
        crate::core::SubCommand::SyncApplications(_) => {
            let db = Database::new().await;
            let companies = Companys::get_from_db(&db, 1).await?;

            // Iterate over the companies and update.
            for company in companies {
                // Do the new applicants.
                cio_api::applicants::refresh_new_applicants_and_reviews(&db, &company).await?;
                cio_api::applicant_reviews::refresh_reviews(&db, &company).await?;

                // Refresh DocuSign for the applicants.
                cio_api::applicants::refresh_docusign_for_applicants(&db, &company).await?;
            }
        }
        crate::core::SubCommand::SyncAssetInventory(_) => {
            let db = Database::new().await;
            let companies = Companys::get_from_db(&db, 1).await?;

            // Iterate over the companies and update.
            for company in companies {
                cio_api::asset_inventory::refresh_asset_items(&db, &company).await?;
            }
        }
        crate::core::SubCommand::SyncCompanies(_) => {
            cio_api::companies::refresh_companies().await?;
        }
        crate::core::SubCommand::SyncConfigs(_) => {
            let db = Database::new().await;
            let companies = Companys::get_from_db(&db, 1).await?;

            // Iterate over the companies and update.
            let tasks: Vec<_> = companies
                .into_iter()
                .map(|company| {
                    tokio::spawn(enclose! { (db) async move {
                        cio_api::configs::refresh_db_configs_and_airtable(&db, &company).await
                    }})
                })
                .collect();

            let mut results: Vec<Result<()>> = Default::default();
            for task in tasks {
                results.push(task.await?);
            }

            for result in results {
                if let Err(e) = result {
                    log::warn!("refreshing configs for company failed: {:?}", e);
                }
            }
        }
        crate::core::SubCommand::SyncFinance(_) => {
            let db = Database::new().await;
            let companies = Companys::get_from_db(&db, 1).await?;

            // Iterate over the companies and update.
            let tasks: Vec<_> = companies
                .into_iter()
                .map(|company| {
                    tokio::spawn(enclose! { (db) async move {
                        cio_api::finance::refresh_all_finance(&db, &company).await
                    }})
                })
                .collect();

            let mut results: Vec<Result<()>> = Default::default();
            for task in tasks {
                results.push(task.await?);
            }

            for result in results {
                if let Err(e) = result {
                    log::warn!("refreshing finance for company failed: {:?}", e);
                }
            }
        }
        crate::core::SubCommand::SyncFunctions(_) => {
            cio_api::functions::refresh_functions().await?;
        }
        crate::core::SubCommand::SyncHuddles(_) => {
            let db = Database::new().await;
            let companies = Companys::get_from_db(&db, 1).await?;

            // Iterate over the companies and update.
            for company in companies {
                cio_api::huddles::sync_changes_to_google_events(&db, &company).await?;

                cio_api::huddles::sync_huddles(&db, &company).await?;

                cio_api::huddles::send_huddle_reminders(&db, &company).await?;

                cio_api::huddles::sync_huddle_meeting_notes(&company).await?;
            }
        }
        crate::core::SubCommand::SyncInterviews(_) => {
            let db = Database::new().await;
            let companies = Companys::get_from_db(&db, 1).await?;

            // Iterate over the companies and update.
            for company in companies {
                cio_api::interviews::refresh_interviews(&db, &company).await?;
                cio_api::interviews::compile_packets(&db, &company).await?;
            }
        }
        crate::core::SubCommand::SyncJournalClubs(_) => {
            let db = Database::new().await;
            let companies = Companys::get_from_db(&db, 1).await?;

            // Iterate over the companies and update.
            for company in companies {
                cio_api::journal_clubs::refresh_db_journal_club_meetings(&db, &company).await?;
            }
        }
        crate::core::SubCommand::SyncMailingLists(_) => {
            let db = Database::new().await;
            let companies = Companys::get_from_db(&db, 1).await?;

            // Iterate over the companies and update.
            for company in companies {
                cio_api::mailing_list::refresh_db_mailing_list_subscribers(&db, &company).await?;
                if company.name == "Oxide" {
                    cio_api::rack_line::refresh_db_rack_line_subscribers(&db, &company).await?;
                }
            }
        }
        crate::core::SubCommand::SyncRecordedMeetings(_) => {
            let db = Database::new().await;
            let companies = Companys::get_from_db(&db, 1).await?;

            // Iterate over the companies and update.
            for company in companies {
                cio_api::recorded_meetings::refresh_zoom_recorded_meetings(&db, &company).await?;
                cio_api::recorded_meetings::refresh_google_recorded_meetings(&db, &company).await?;
            }
        }
        crate::core::SubCommand::SyncRepos(_) => {
            let db = Database::new().await;
            let companies = Companys::get_from_db(&db, 1).await?;

            // Iterate over the companies and update.
            let tasks: Vec<_> = companies
                .into_iter()
                .map(|company| {
                    tokio::spawn(enclose! { (db) async move {
                        tokio::join!(
                            cio_api::repos::sync_all_repo_settings(&db, &company),
                            cio_api::repos::refresh_db_github_repos(&db, &company),
                        )
                    }})
                })
                .collect();

            let mut results: Vec<(Result<()>, Result<()>)> = Default::default();
            for task in tasks {
                results.push(task.await?);
            }

            for (refresh_result, cleanup_result) in results {
                refresh_result?;
                cleanup_result?;
            }
        }
        crate::core::SubCommand::SyncRFDs(_) => {
            let db = Database::new().await;
            let companies = Companys::get_from_db(&db, 1).await?;

            // Iterate over the companies and update.
            for company in companies {
                cio_api::rfds::refresh_db_rfds(&db, &company).await?;
                cio_api::rfds::cleanup_rfd_pdfs(&db, &company).await?;
            }
        }
        crate::core::SubCommand::SyncOther(_) => {
            let db = Database::new().await;
            let companies = Companys::get_from_db(&db, 1).await?;

            // Iterate over the companies and update.
            for company in companies {
                cio_api::tailscale::cleanup_old_tailscale_devices(&company).await?;
                cio_api::tailscale::cleanup_old_tailscale_cloudflare_dns(&company).await?;
                if company.name == "Oxide" {
                    cio_api::customers::sync_customer_meeting_notes(&company).await?;
                }
            }
        }
        crate::core::SubCommand::SyncShipments(_) => {
            let db = Database::new().await;
            let companies = Companys::get_from_db(&db, 1).await?;

            // Iterate over the companies and update.
            let tasks: Vec<_> = companies
                .into_iter()
                .map(|company| {
                    tokio::spawn(enclose! { (db) async move {
                        // Ensure we have the webhooks set up for shipbob, if applicable.
                        tokio::join!(
                            company.ensure_shipbob_webhooks(),
                            cio_api::shipments::refresh_inbound_shipments(&db, &company),
                            cio_api::shipments::refresh_outbound_shipments(&db, &company)
                        )
                    }})
                })
                .collect();

            let mut results: Vec<(Result<()>, Result<()>, Result<()>)> = Default::default();
            for task in tasks {
                results.push(task.await?);
            }

            for (webhooks_result, inbound_result, outbound_result) in results {
                webhooks_result?;
                inbound_result?;
                outbound_result?;
            }
        }
        crate::core::SubCommand::SyncShorturls(_) => {
            cio_api::shorturls::refresh_shorturls().await?;
        }
        crate::core::SubCommand::SyncSwagInventory(_) => {
            let db = Database::new().await;
            let companies = Companys::get_from_db(&db, 1).await?;

            // Iterate over the companies and update.
            for company in companies {
                cio_api::swag_inventory::refresh_swag_items(&db, &company).await?;
                cio_api::swag_inventory::refresh_swag_inventory_items(&db, &company).await?;
                cio_api::swag_inventory::refresh_barcode_scans(&db, &company).await?;
            }
        }
        crate::core::SubCommand::SyncTravel(_) => {
            let db = Database::new().await;
            let companies = Companys::get_from_db(&db, 1).await?;

            // Iterate over the companies and update.
            let tasks: Vec<_> = companies
                .into_iter()
                .map(|company| {
                    tokio::spawn(enclose! { (db) async move {
                        cio_api::travel::refresh_trip_actions(&db, &company).await
                    }})
                })
                .collect();

            let mut results: Vec<Result<()>> = Default::default();
            for task in tasks {
                results.push(task.await?);
            }

            for result in results {
                result?;
            }
        }
        crate::core::SubCommand::SyncZoho(_) => {
            let db = Database::new().await;
            let companies = Companys::get_from_db(&db, 1).await?;

            // Iterate over the companies and update.
            let tasks: Vec<_> = companies
                .into_iter()
                .map(|company| {
                    tokio::spawn(enclose! { (db) async move {
                        cio_api::zoho::refresh_leads(&db, &company).await
                    }})
                })
                .collect();

            let mut results: Vec<Result<()>> = Default::default();
            for task in tasks {
                results.push(task.await?);
            }

            for result in results {
                result?;
            }
        }
    }

    Ok(())
}
