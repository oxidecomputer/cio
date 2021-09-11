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

    SyncFinance(SyncFinance),
    SyncRepos(SyncRepos),
    #[clap(name = "sync-rfds")]
    SyncRFDs(SyncRFDs),
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
}

/// A subcommand for running the background job of syncing finance data.
#[derive(Clap)]
pub struct SyncFinance {}

/// A subcommand for running the background job of syncing repos.
#[derive(Clap)]
pub struct SyncRepos {}

/// A subcommand for running the background job of syncing RFDs.
#[derive(Clap)]
pub struct SyncRFDs {}

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
            crate::server::server(&s, logger).await?;
        }
        SubCommand::SyncFinance(_) => {
            let db = Database::new();
            let companies = Companys::get_from_db(&db, 1)?;

            // Iterate over the companies and update.
            for company in companies {
                cio_api::finance::refresh_all_finance(&db, &company).await?;
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
