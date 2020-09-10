use std::env;

pub static AIRTABLE_BASE_ID_RACK_ROADMAP: &str = "appvAEzcMvB2QNboC";
pub static AIRTABLE_BASE_ID_PRODUCT_HUDDLE: &str = "appbQqnE3nykcnkbx";
pub static AIRTABLE_BASE_ID_RECURITING_APPLICATIONS: &str = "appIw5FNBqWTXFTeV";
pub static AIRTABLE_BASE_ID_CUSTOMER_LEADS: &str = "appr7imQLcR3pWaNa";

pub static AIRTABLE_RFD_TABLE: &str = "RFDs";
pub static AIRTABLE_CUSTOMER_INTERACTIONS_TABLE: &str = "Interactions";
pub static AIRTABLE_APPLICATIONS_TABLE: &str = "Applicants";
pub static AIRTABLE_MAILING_LIST_SIGNUPS_TABLE: &str = "Mailing List Signups";
pub static AIRTABLE_AUTH_USERS_TABLE: &str = "Auth Users";
pub static AIRTABLE_AUTH_USER_LOGINS_TABLE: &str = "Auth User Logins";
pub static AIRTABLE_MEETING_SCHEDULE_TABLE: &str = "Meeting schedule";
pub static AIRTABLE_DISCUSSION_TOPICS_TABLE: &str = "Discussion topics";

pub static AIRTABLE_GRID_VIEW: &str = "Grid view";

pub fn airtable_api_key() -> String {
    env::var("AIRTABLE_API_KEY").unwrap()
}
