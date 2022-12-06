#![recursion_limit = "256"]
#![feature(async_closure)]
pub mod auth;
pub mod context;
#[macro_use]
pub mod core;
mod cors;
mod event_types;
pub mod github_types;
mod handlers;
pub mod handlers_auth;
pub mod handlers_checkr;
pub mod handlers_cron;
pub mod handlers_docusign;
pub mod handlers_github;
pub mod handlers_hiring;
pub mod handlers_rfd;
pub mod handlers_slack;
// mod handlers_sendgrid;
mod http;
mod repos;
mod sagas;
pub mod server;
mod slack_commands;
// mod tracking_numbers;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate serde_json;
#[macro_use]
extern crate cio_api;