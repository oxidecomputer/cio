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
extern crate serde_json;

use std::env;

use anyhow::Result;
use cio_api::{companies::Companys, db::Database};
use clap::{AppSettings, Clap};
use sentry::IntoDsn;
use slog::Drain;

/// This doc string acts as a help message when the user runs '--help'
/// as do all doc strings on fields.
#[derive(Clap)]
#[clap(version = clap::crate_version!(), author = clap::crate_authors!("\n"))]
#[clap(setting = AppSettings::ColoredHelp)]
struct Opts {
    /// Print debug info
    #[clap(short, long)]
    debug: bool,

    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[derive(Clap)]
enum SubCommand {
    Server(Server),

    SyncApplications(SyncApplications),
    SyncAssetInventory(SyncAssetInventory),
    SyncConfigs(SyncConfigs),
    SyncFinance(SyncFinance),
    SyncInterviews(SyncInterviews),
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
#[derive(Clap)]
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

/// A subcommand for running the background job of syncing applications.
#[derive(Clap)]
pub struct SyncApplications {}

/// A subcommand for running the background job of syncing asset inventory.
#[derive(Clap)]
pub struct SyncAssetInventory {}

/// A subcommand for running the background job of syncing configs.
#[derive(Clap)]
pub struct SyncConfigs {}

/// A subcommand for running the background job of syncing finance data.
#[derive(Clap)]
pub struct SyncFinance {}

/// A subcommand for running the background job of syncing interviews.
#[derive(Clap)]
pub struct SyncInterviews {}

/// A subcommand for running the background job of syncing recorded_meetings.
#[derive(Clap)]
pub struct SyncRecordedMeetings {}

/// A subcommand for running the background job of syncing repos.
#[derive(Clap)]
pub struct SyncRepos {}

/// A subcommand for running the background job of syncing RFDs.
#[derive(Clap)]
pub struct SyncRFDs {}

/// A subcommand for running the background job of syncing shipments.
#[derive(Clap)]
pub struct SyncShipments {}

/// A subcommand for running the background job of syncing shorturls.
#[derive(Clap)]
pub struct SyncShorturls {}

/// A subcommand for running the background job of syncing swag inventory.
#[derive(Clap)]
pub struct SyncSwagInventory {}

/// A subcommand for running the background job of syncing travel data.
#[derive(Clap)]
pub struct SyncTravel {}

#[tokio::main]
async fn main() -> Result<()> {
    let opts: Opts = Opts::parse();

    // Initialize sentry.
    let sentry_dsn = env::var("WEBHOOKY_SENTRY_DSN").unwrap_or_default();
    if !sentry_dsn.is_empty() {
        let _guard = sentry::init(sentry::ClientOptions {
            dsn: sentry_dsn.clone().into_dsn()?,

            release: Some(env::var("GIT_HASH").unwrap_or_default().into()),
            environment: Some(
                env::var("SENTRY_ENV")
                    .unwrap_or_else(|_| "development".to_string())
                    .into(),
            ),
            ..Default::default()
        });
    }

    // Initialize our logger.
    let decorator = slog_term::TermDecorator::new().build();
    let drain = slog_term::FullFormat::new(decorator).build().fuse();
    let drain = slog_async::Async::new(drain).build().fuse();
    let logger = if !sentry_dsn.is_empty() {
        let drain = sentry_slog::SentryDrain::new(drain);
        slog::Logger::root(drain, slog::slog_o!())
    } else {
        slog::Logger::root(drain, slog::slog_o!())
    };

    let _scope_guard = slog_scope::set_global_logger(logger.clone());

    // Set the logging level.
    let mut log_level = log::Level::Info;
    if opts.debug {
        log_level = log::Level::Debug;
    }
    let _log_guard = slog_stdlog::init_with_level(log_level)?;

    match opts.subcmd {
        SubCommand::Server(s) => {
            crate::server::server(s, logger).await?;
        }
        SubCommand::SyncApplications(_) => {
            let db = Database::new();
            let companies = Companys::get_from_db(&db, 1)?;

            // Iterate over the companies and update.
            for company in companies {
                // Do the new applicants.
                cio_api::applicants::refresh_new_applicants_and_reviews(&db, &company).await?;
                cio_api::applicant_reviews::refresh_reviews(&db, &company).await?;

                // Refresh DocuSign for the applicants.
                cio_api::applicants::refresh_docusign_for_applicants(&db, &company).await?;

                cio_api::applicants::refresh_background_checks(&db, &company).await?;

                // TODO: when we remove google sheets, we no longer need these.
                //
                // Do the old applicants from Google sheets.
                cio_api::applicants::refresh_db_applicants(&db, &company).await?;

                // These come from the sheet at:
                // https://docs.google.com/spreadsheets/d/1BOeZTdSNixkJsVHwf3Z0LMVlaXsc_0J8Fsy9BkCa7XM/edit#gid=2017435653
                cio_api::applicants::update_applications_with_scoring_forms(&db, &company).await?;

                // This must be after cio_api::applicants::update_applications_with_scoring_forms, so that if someone
                // has done the application then we remove them from the scorers.
                cio_api::applicants::update_applications_with_scoring_results(&db, &company).await?;
                cio_api::applicants::update_applicant_reviewers_leaderboard(&db, &company).await?;
            }
        }
        SubCommand::SyncAssetInventory(_) => {
            let db = Database::new();
            let companies = Companys::get_from_db(&db, 1)?;

            // Iterate over the companies and update.
            for company in companies {
                cio_api::asset_inventory::refresh_asset_items(&db, &company).await?;
            }
        }
        SubCommand::SyncConfigs(_) => {
            let db = Database::new();
            let companies = Companys::get_from_db(&db, 1)?;

            // Iterate over the companies and update.
            for company in companies {
                cio_api::configs::refresh_db_configs_and_airtable(&db, &company).await?;
            }
        }
        SubCommand::SyncFinance(_) => {
            let db = Database::new();
            let companies = Companys::get_from_db(&db, 1)?;

            // Iterate over the companies and update.
            for company in companies {
                cio_api::finance::refresh_all_finance(&db, &company).await?;
            }
        }
        SubCommand::SyncInterviews(_) => {
            let db = Database::new();
            let companies = Companys::get_from_db(&db, 1)?;

            // Iterate over the companies and update.
            for company in companies {
                cio_api::interviews::refresh_interviews(&db, &company).await?;
                cio_api::interviews::compile_packets(&db, &company).await?;
            }
        }
        SubCommand::SyncRecordedMeetings(_) => {
            let db = Database::new();
            let companies = Companys::get_from_db(&db, 1)?;

            // Iterate over the companies and update.
            for company in companies {
                cio_api::recorded_meetings::refresh_zoom_recorded_meetings(&db, &company).await?;
                cio_api::recorded_meetings::refresh_google_recorded_meetings(&db, &company).await?;
            }
        }
        SubCommand::SyncRepos(_) => {
            let db = Database::new();
            let companies = Companys::get_from_db(&db, 1)?;

            // Iterate over the companies and update.
            for company in companies {
                let github = company.authenticate_github()?;
                cio_api::repos::sync_all_repo_settings(&db, &github, &company).await?;
                cio_api::repos::refresh_db_github_repos(&db, &github, &company).await?;
            }
        }
        SubCommand::SyncRFDs(_) => {
            let db = Database::new();
            let companies = Companys::get_from_db(&db, 1)?;

            // Iterate over the companies and update.
            for company in companies {
                cio_api::rfds::refresh_db_rfds(&db, &company).await?;
                cio_api::rfds::cleanup_rfd_pdfs(&db, &company).await?;
            }
        }
        SubCommand::SyncShipments(_) => {
            let db = Database::new();
            let companies = Companys::get_from_db(&db, 1)?;

            // Iterate over the companies and update.
            for company in companies {
                cio_api::shipments::refresh_inbound_shipments(&db, &company).await?;
                cio_api::shipments::refresh_outbound_shipments(&db, &company).await?;
            }
        }
        SubCommand::SyncShorturls(_) => {
            cio_api::shorturls::refresh_shorturls().await?;
        }
        SubCommand::SyncSwagInventory(_) => {
            let db = Database::new();
            let companies = Companys::get_from_db(&db, 1)?;

            // Iterate over the companies and update.
            for company in companies {
                cio_api::swag_inventory::refresh_swag_items(&db, &company).await?;
                cio_api::swag_inventory::refresh_swag_inventory_items(&db, &company).await?;
                cio_api::swag_inventory::refresh_barcode_scans(&db, &company).await?;
            }
        }
        SubCommand::SyncTravel(_) => {
            let db = Database::new();
            let companies = Companys::get_from_db(&db, 1)?;

            // Iterate over the companies and update.
            for company in companies {
                cio_api::travel::refresh_trip_actions(&db, &company).await?;
            }
        }
    }

    Ok(())
}
