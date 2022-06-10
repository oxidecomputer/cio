#![recursion_limit = "256"]
#![feature(async_closure)]
#[macro_use]
pub mod core;
mod event_types;
mod github_types;
mod handlers;
mod handlers_auth;
mod handlers_checkr;
mod handlers_cron;
mod handlers_github;
mod handlers_sendgrid;
mod http;
mod repos;
mod sagas;
mod sig;
pub mod server;
mod slack_commands;
mod tracking_numbers;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate serde_json;
#[macro_use]
extern crate cio_api;
