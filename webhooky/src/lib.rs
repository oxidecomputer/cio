#![recursion_limit = "256"]
#![feature(async_closure)]
pub mod auth;
pub mod bearer;
#[macro_use]
pub mod core;
mod event_types;
mod github_types;
mod handlers;
pub mod handlers_auth;
pub mod handlers_checkr;
pub mod handlers_cron;
pub mod handlers_docusign;
pub mod handlers_github;
// mod handlers_sendgrid;
mod http;
mod repos;
mod sagas;
pub mod server;
pub mod sig;
mod slack_commands;
pub mod token;
mod tracking_numbers;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate serde_json;
#[macro_use]
extern crate cio_api;
