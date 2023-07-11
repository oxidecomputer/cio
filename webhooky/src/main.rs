#![recursion_limit = "256"]
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
mod health;
mod http;
mod job;
mod mailing_lists;
mod repos;
mod sagas;
mod server;
mod slack_commands;
// mod tracking_numbers;
#[macro_use]
extern crate serde_json;
#[macro_use]
extern crate cio_api;

use anyhow::{bail, Result};
use cio_api::health::SelfMemory;
use clap::Parser;
use log::info;
use slog::Drain;
use std::fs::File;

use crate::context::ServerContext;
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

    if let Ok(mem) = SelfMemory::new() {
        log::info!("Memory at start of command exec {:?}: {:?}", opts.subcmd, mem);
    }

    let logger = if opts.json {
        let drain = slog_json::Json::new(std::io::stdout())
            .add_default_keys()
            .build()
            .fuse();
        let drain = slog_async::Async::new(drain).build().fuse();
        slog::Logger::root(drain, slog::slog_o!())
    } else {
        let decorator = slog_term::TermDecorator::new().build();
        let drain = slog_term::FullFormat::new(decorator).build().fuse();
        let drain = slog_async::Async::new(drain).build().fuse();
        slog::Logger::root(drain, slog::slog_o!())
    };

    // Initialize our logger.
    let _scope_guard = slog_scope::set_global_logger(logger.clone());

    // Set the logging level.
    let mut log_level = log::Level::Info;
    if opts.debug {
        log_level = log::Level::Debug;
    }
    slog_stdlog::init_with_level(log_level)?;

    let api = APIConfig::new()?;

    let context = ServerContext::new(1, logger).await?;

    if let Err(err) = run_main_cmd(opts.clone(), api, context).await {
        bail!("running cmd `{:?}` failed: {:?}", &opts.subcmd, err);
    }

    Ok(())
}

pub async fn run_main_cmd(opts: crate::core::Opts, api: APIConfig, context: ServerContext) -> Result<()> {
    if let Ok(mem) = SelfMemory::new() {
        log::info!("Memory at start of command run {:?}: {:?}", opts.subcmd, mem);
    }

    match opts.subcmd.clone() {
        crate::core::SubCommand::Server(s) => {
            crate::server::server(s, api.api, context, opts.debug).await?;
        }
        crate::core::SubCommand::CreateServerSpec(spec) => {
            let spec_file = spec.spec_file;
            info!("writing OpenAPI spec to {}...", spec_file.to_str().unwrap());
            let mut buffer = File::create(spec_file)?;
            api.open_api().write(&mut buffer)?;
        }
        job => crate::job::run_job_cmd(job, context.app).await?,
    }

    if let Ok(mem) = SelfMemory::new() {
        log::info!("Memory at end of command run {:?}: {:?}", opts.subcmd, mem);
    }

    Ok(())
}
