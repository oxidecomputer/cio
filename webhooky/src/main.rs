#![allow(clippy::field_reassign_with_default)]
mod event_types;
mod github_types;
mod handlers;
mod handlers_auth;
mod handlers_cron;
mod handlers_github;
pub mod repos;
mod server;
mod slack_commands;
mod tracking_numbers;
#[macro_use]
extern crate serde_json;

use std::{collections::HashMap, env, fs::File, sync::Arc};

use anyhow::Result;
use cio_api::{analytics::NewPageView, db::Database, functions::Function, swag_store::Order};
use clap::{AppSettings, Clap};
use docusign::DocuSign;
use dropshot::{
    endpoint, ApiDescription, ConfigDropshot, ConfigLogging, ConfigLoggingLevel, HttpError, HttpResponseAccepted,
    HttpResponseOk, HttpServerStarter, Path, Query, RequestContext, TypedBody, UntypedBody,
};
use google_drive::Client as GoogleDrive;
use gusto_api::Client as Gusto;
use log::{info, warn};
use mailchimp_api::MailChimp;
use quickbooks::QuickBooks;
use ramp_api::Client as Ramp;
use schemars::JsonSchema;
use sentry::IntoDsn;
use serde::{Deserialize, Serialize};
use slack_chat_api::Slack;
use slog::Drain;
use zoom_api::Client as Zoom;

use crate::github_types::GitHubWebhook;

/// This doc string acts as a help message when the user runs '--help'
/// as do all doc strings on fields.
#[derive(Clap)]
#[clap(version = clap::crate_version!(), author = clap::crate_authors!("\n"))]
#[clap(setting = AppSettings::ColoredHelp)]
struct Opts {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[derive(Clap)]
enum SubCommand {
    Server(Server),
}

/// A subcommand for running the server.
#[derive(Clap)]
struct Server {
    /// Print debug info
    #[clap(short, long)]
    debug: bool,
    /// IP address and port that the server should listen
    #[clap(short, long, default_value = "0.0.0.0:8080")]
    address: String,

    /// Sets an optional output file for the API spec
    #[clap(short, long, parse(from_os_str), value_hint = clap::ValueHint::FilePath)]
    spec_file: Option<std::path::PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let opts: Opts = Opts::parse();

    match opts.subcmd {
        SubCommand::Server(s) => {
            crate::server::server(&s).await?;
        }
    }

    Ok(())
}
