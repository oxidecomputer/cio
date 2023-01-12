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

use crate::context::ServerContext;
use crate::health::SelfMemory;
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

    let context = ServerContext::new(1, logger).await?;

    if let Err(err) = run_main_cmd(opts.clone(), api, context).await {
        sentry::integrations::anyhow::capture_anyhow(&anyhow::anyhow!("{:?}", err));
        bail!("running cmd `{:?}` failed: {:?}", &opts.subcmd, err);
    }

    Ok(())
}

pub async fn run_main_cmd(opts: crate::core::Opts, api: APIConfig, context: ServerContext) -> Result<()> {
    sentry::configure_scope(|scope| {
        scope.set_tag("command", &std::env::args().collect::<Vec<String>>().join(" "));
    });

    if let Ok(mem) = SelfMemory::new() {
        log::info!("Memory at start of command run {:?}: {:?}", opts.subcmd, mem);
    }

    match opts.subcmd.clone() {
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
        job => crate::job::run_job_cmd(job, context.app).await?,
    }

    if let Ok(mem) = SelfMemory::new() {
        log::info!("Memory at end of command run {:?}: {:?}", opts.subcmd, mem);
    }

    Ok(())
}
