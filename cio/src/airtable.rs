use std::env;

pub static AIRTABLE_BASE_ID_RECURITING_APPLICATIONS: &str = "appIw5FNBqWTXFTeV";
pub static AIRTABLE_BASE_ID_CUSTOMER_LEADS: &str = "appr7imQLcR3pWaNa";

pub static AIRTABLE_APPLICATIONS_TABLE: &str = "Applicants";
pub static AIRTABLE_MAILING_LIST_SIGNUPS_TABLE: &str = "Mailing List Signups";
pub static AIRTABLE_AUTH0_LOGINS_TABLE: &str = "Auth0 Logins to RFD Site";

pub static AIRTABLE_GRID_VIEW: &str = "Grid view";

pub fn airtable_api_key() -> String {
    env::var("AIRTABLE_API_KEY").unwrap()
}
