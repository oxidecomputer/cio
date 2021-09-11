#![recursion_limit = "256"]
#![allow(clippy::field_reassign_with_default)]
#![allow(clippy::nonstandard_macro_braces)]

pub mod airtable;
pub mod analytics;
pub mod api_tokens;
pub mod applicant_reviews;
pub mod applicant_status;
pub mod applicants;
pub mod application_form;
pub mod asset_inventory;
pub mod auth_logins;
pub mod certs;
pub mod companies;
pub mod configs;
pub mod core;
pub mod customers;
pub mod db;
pub mod finance;
pub mod functions;
pub mod gsuite;
pub mod huddles;
pub mod interviews;
pub mod journal_clubs;
pub mod mailing_list;
pub mod rack_line;
pub mod recorded_meetings;
pub mod repos;
pub mod rfds;
pub mod schema;
pub mod shipments;
pub mod shorturls;
pub mod states;
pub mod swag_inventory;
pub mod swag_store;
pub mod tailscale;
pub mod templates;
pub mod travel;
pub mod utils;

#[macro_use]
extern crate diesel;

#[macro_use]
extern crate serde_json;
