#![recursion_limit = "256"]
#![feature(async_closure)]
#[macro_use]
mod event_types;
mod github_types;
mod handlers;
mod handlers_auth;
mod handlers_cron;
mod handlers_github;
mod repos;
mod sagas;
mod server;
mod slack_commands;
mod tracking_numbers;
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
use sentry::IntoDsn;
use slog::Drain;

lazy_static! {
    // We need a slog::Logger for steno and when we export out the logs from re-exec-ed processes.
    static ref LOGGER: slog::Logger = {
        let decorator = slog_term::TermDecorator::new().build();
        let drain = slog_term::FullFormat::new(decorator).build().fuse();
        let drain = slog_async::Async::new(drain).build().fuse();
        slog::Logger::root(drain, slog::slog_o!())
    };
}

/// This doc string acts as a help message when the user runs '--help'
/// as do all doc strings on fields.
#[derive(Parser, Debug, Clone)]
#[clap(version = clap::crate_version!(), author = clap::crate_authors!("\n"))]
struct Opts {
    /// Print debug info
    #[clap(short, long)]
    debug: bool,

    /// Print logs as json
    #[clap(short, long)]
    json: bool,

    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[derive(Parser, Debug, Clone)]
enum SubCommand {
    Server(Server),

    SendRFDChangelog(SendRFDChangelog),
    SyncAnalytics(SyncAnalytics),
    #[clap(name = "sync-api-tokens")]
    SyncAPITokens(SyncAPITokens),
    SyncApplications(SyncApplications),
    SyncAssetInventory(SyncAssetInventory),
    SyncCompanies(SyncCompanies),
    SyncConfigs(SyncConfigs),
    SyncFinance(SyncFinance),
    SyncFunctions(SyncFunctions),
    SyncHuddles(SyncHuddles),
    SyncInterviews(SyncInterviews),
    SyncJournalClubs(SyncJournalClubs),
    SyncMailingLists(SyncMailingLists),
    SyncOther(SyncOther),
    SyncRecordedMeetings(SyncRecordedMeetings),
    SyncRepos(SyncRepos),
    #[clap(name = "sync-rfds")]
    SyncRFDs(SyncRFDs),
    SyncShipments(SyncShipments),
    SyncShorturls(SyncShorturls),
    SyncSwagInventory(SyncSwagInventory),
    SyncTravel(SyncTravel),
}

/// A subcommand for running the server.
#[derive(Parser, Clone, Debug)]
pub struct Server {
    /// IP address and port that the server should listen
    #[clap(short, long, default_value = "0.0.0.0:8080")]
    address: String,

    /// Sets an optional output file for the API spec
    #[clap(short, long, parse(from_os_str), value_hint = clap::ValueHint::FilePath)]
    spec_file: Option<std::path::PathBuf>,

    /// Sets if the server should run cron jobs in the background
    #[clap(long)]
    do_cron: bool,
}

/// A subcommand for sending the RFD changelog.
#[derive(Parser, Clone, Debug)]
pub struct SendRFDChangelog {}

/// A subcommand for running the background job of syncing analytics.
#[derive(Parser, Debug, Clone)]
pub struct SyncAnalytics {}

/// A subcommand for running the background job of syncing API tokens.
#[derive(Parser, Debug, Clone)]
pub struct SyncAPITokens {}

/// A subcommand for running the background job of syncing applications.
#[derive(Parser, Debug, Clone)]
pub struct SyncApplications {}

/// A subcommand for running the background job of syncing asset inventory.
#[derive(Parser, Debug, Clone)]
pub struct SyncAssetInventory {}

/// A subcommand for running the background job of syncing companies.
#[derive(Parser, Debug, Clone)]
pub struct SyncCompanies {}

/// A subcommand for running the background job of syncing configs.
#[derive(Parser, Debug, Clone)]
pub struct SyncConfigs {}

/// A subcommand for running the background job of syncing finance data.
#[derive(Parser, Debug, Clone)]
pub struct SyncFinance {}

/// A subcommand for running the background job of syncing functions.
#[derive(Parser, Debug, Clone)]
pub struct SyncFunctions {}

/// A subcommand for running the background job of syncing interviews.
#[derive(Parser, Debug, Clone)]
pub struct SyncInterviews {}

/// A subcommand for running the background job of syncing huddles.
#[derive(Parser, Debug, Clone)]
pub struct SyncHuddles {}

/// A subcommand for running the background job of syncing journal clubs.
#[derive(Parser, Debug, Clone)]
pub struct SyncJournalClubs {}

/// A subcommand for running the background job of syncing mailing lists.
#[derive(Parser, Debug, Clone)]
pub struct SyncMailingLists {}

/// A subcommand for running the background job of syncing other things.
#[derive(Parser, Debug, Clone)]
pub struct SyncOther {}

/// A subcommand for running the background job of syncing recorded_meetings.
#[derive(Parser, Debug, Clone)]
pub struct SyncRecordedMeetings {}

/// A subcommand for running the background job of syncing repos.
#[derive(Parser, Debug, Clone)]
pub struct SyncRepos {}

/// A subcommand for running the background job of syncing RFDs.
#[derive(Parser, Debug, Clone)]
pub struct SyncRFDs {}

/// A subcommand for running the background job of syncing shipments.
#[derive(Parser, Debug, Clone)]
pub struct SyncShipments {}

/// A subcommand for running the background job of syncing shorturls.
#[derive(Parser, Debug, Clone)]
pub struct SyncShorturls {}

/// A subcommand for running the background job of syncing swag inventory.
#[derive(Parser, Debug, Clone)]
pub struct SyncSwagInventory {}

/// A subcommand for running the background job of syncing travel data.
#[derive(Parser, Debug, Clone)]
pub struct SyncTravel {}

#[tokio::main]
async fn main() -> Result<()> {
    let opts: Opts = Opts::parse();

    // Initialize sentry.
    let sentry_dsn = env::var("WEBHOOKY_SENTRY_DSN").unwrap_or_default();
    let _guard = sentry::init(sentry::ClientOptions {
        debug: opts.debug,
        dsn: sentry_dsn.clone().into_dsn()?,

        // Send 100% of all transactions to Sentry.
        // This is for testing purposes only, after a bit of testing set this to be like 20%.
        // Or we can keep it at 100% if it is not messing with performance.
        traces_sample_rate: 1.0,

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

async fn run_cmd(opts: Opts, logger: slog::Logger) -> Result<()> {
    sentry::configure_scope(|scope| {
        scope.set_tag("command", &std::env::args().collect::<Vec<String>>().join(" "));
    });

    match opts.subcmd {
        SubCommand::Server(s) => {
            sentry::configure_scope(|scope| {
                scope.set_tag("do-cron", s.do_cron.to_string());
            });
            crate::server::server(s, logger, opts.debug).await?;
        }
        SubCommand::SendRFDChangelog(_) => {
            let db = Database::new().await;
            let companies = Companys::get_from_db(&db, 1).await?;

            // Iterate over the companies and send.
            for company in companies {
                cio_api::rfds::send_rfd_changelog(&db, &company).await?;
            }
        }
        SubCommand::SyncAnalytics(_) => {
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
        SubCommand::SyncAPITokens(_) => {
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
        SubCommand::SyncApplications(_) => {
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
        SubCommand::SyncAssetInventory(_) => {
            let db = Database::new().await;
            let companies = Companys::get_from_db(&db, 1).await?;

            // Iterate over the companies and update.
            for company in companies {
                cio_api::asset_inventory::refresh_asset_items(&db, &company).await?;
            }
        }
        SubCommand::SyncCompanies(_) => {
            cio_api::companies::refresh_companies().await?;
        }
        SubCommand::SyncConfigs(_) => {
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
        SubCommand::SyncFinance(_) => {
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
        SubCommand::SyncFunctions(_) => {
            cio_api::functions::refresh_functions().await?;
        }
        SubCommand::SyncHuddles(_) => {
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
        SubCommand::SyncInterviews(_) => {
            let db = Database::new().await;
            let companies = Companys::get_from_db(&db, 1).await?;

            // Iterate over the companies and update.
            for company in companies {
                cio_api::interviews::refresh_interviews(&db, &company).await?;
                cio_api::interviews::compile_packets(&db, &company).await?;
            }
        }
        SubCommand::SyncJournalClubs(_) => {
            let db = Database::new().await;
            let companies = Companys::get_from_db(&db, 1).await?;

            // Iterate over the companies and update.
            for company in companies {
                cio_api::journal_clubs::refresh_db_journal_club_meetings(&db, &company).await?;
            }
        }
        SubCommand::SyncMailingLists(_) => {
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
        SubCommand::SyncRecordedMeetings(_) => {
            let db = Database::new().await;
            let companies = Companys::get_from_db(&db, 1).await?;

            // Iterate over the companies and update.
            for company in companies {
                cio_api::recorded_meetings::refresh_zoom_recorded_meetings(&db, &company).await?;
                cio_api::recorded_meetings::refresh_google_recorded_meetings(&db, &company).await?;
            }
        }
        SubCommand::SyncRepos(_) => {
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
        SubCommand::SyncRFDs(_) => {
            let db = Database::new().await;
            let companies = Companys::get_from_db(&db, 1).await?;

            // Iterate over the companies and update.
            for company in companies {
                cio_api::rfds::refresh_db_rfds(&db, &company).await?;
                cio_api::rfds::cleanup_rfd_pdfs(&db, &company).await?;
            }
        }
        SubCommand::SyncOther(_) => {
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
        SubCommand::SyncShipments(_) => {
            let db = Database::new().await;
            let companies = Companys::get_from_db(&db, 1).await?;

            // Iterate over the companies and update.
            let tasks: Vec<_> = companies
                .into_iter()
                .map(|company| {
                    tokio::spawn(enclose! { (db) async move {
                        // Ensure we have the webhooks set up for shipbob, if applicable.
                        tokio::join!(
                            company.ensure_shipbob_webhooks(&db),
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
        SubCommand::SyncShorturls(_) => {
            cio_api::shorturls::refresh_shorturls().await?;
        }
        SubCommand::SyncSwagInventory(_) => {
            let db = Database::new().await;
            let companies = Companys::get_from_db(&db, 1).await?;

            // Iterate over the companies and update.
            for company in companies {
                cio_api::swag_inventory::refresh_swag_items(&db, &company).await?;
                cio_api::swag_inventory::refresh_swag_inventory_items(&db, &company).await?;
                cio_api::swag_inventory::refresh_barcode_scans(&db, &company).await?;
            }
        }
        SubCommand::SyncTravel(_) => {
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
    }

    Ok(())
}
