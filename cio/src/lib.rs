#![feature(str_split_once)]
#![recursion_limit = "256"]
#![allow(clippy::field_reassign_with_default)]

pub mod airtable;
pub mod applicant_status;
pub mod applicants;
pub mod auth_logins;
pub mod certs;
pub mod configs;
pub mod core;
pub mod db;
pub mod journal_clubs;
pub mod mailing_list;
pub mod models;
pub mod rfds;
pub mod schema;
pub mod slack;
pub mod utils;

#[macro_use]
extern crate diesel;

#[macro_use]
extern crate serde_json;
