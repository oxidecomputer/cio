#![allow(clippy::from_over_into)]
use std::collections::HashMap;
use std::env;
use std::{thread, time};

use async_trait::async_trait;
use chrono::naive::NaiveDateTime;
use chrono::offset::Utc;
use chrono::DateTime;
use chrono_humanize::HumanTime;
use macros::db;
use reqwest::{Client, StatusCode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::airtable::{AIRTABLE_AUTH_USERS_TABLE, AIRTABLE_AUTH_USER_LOGINS_TABLE};
use crate::companies::Company;
use crate::core::UpdateAirtableRecord;
use crate::db::Database;
use crate::schema::{auth_user_logins, auth_users};

/// The data type for an NewAuthUser.
#[db {
    new_struct_name = "AuthUser",
    airtable_base = "customer_leads",
    airtable_table = "AIRTABLE_AUTH_USERS_TABLE",
    custom_partial_eq = true,
    match_on = {
        "user_id" = "String",
    },
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[table_name = "auth_users"]
pub struct NewAuthUser {
    pub user_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub nickname: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub username: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email: String,
    #[serde(default)]
    pub email_verified: bool,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "airtable_api::attachment_format_as_string::deserialize")]
    pub picture: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub company: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub blog: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub phone: String,
    #[serde(default)]
    pub phone_verified: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub locale: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub login_provider: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_login: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub last_application_accessed: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub last_ip: String,
    pub logins_count: i32,
    /// link to another table in Airtable
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub link_to_people: Vec<String>,
    /// link to another table in Airtable
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub link_to_auth_user_logins: Vec<String>,
    /// link to another table in Airtable
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub link_to_page_views: Vec<String>,
    /// The CIO company ID.
    #[serde(default)]
    pub cio_company_id: i32,
}

/// Implement updating the Airtable record for a AuthUser.
#[async_trait]
impl UpdateAirtableRecord<AuthUser> for AuthUser {
    async fn update_airtable_record(&mut self, record: AuthUser) {
        // Set the link_to_people and link_to_auth_user_logins from the original so it stays intact.
        self.link_to_people = record.link_to_people.clone();
        self.link_to_auth_user_logins = record.link_to_auth_user_logins;
        self.link_to_page_views = record.link_to_page_views;
    }
}

impl PartialEq for AuthUser {
    // We implement our own here because Airtable has a different data type for the picture.
    fn eq(&self, other: &Self) -> bool {
        self.user_id == other.user_id
            && self.last_login == other.last_login
            && self.logins_count == other.logins_count
            && self.last_application_accessed == other.last_application_accessed
            && self.company == other.company
    }
}

/// The data type for a NewAuthUserLogin.
#[db {
    new_struct_name = "AuthUserLogin",
    airtable_base = "customer_leads",
    airtable_table = "AIRTABLE_AUTH_USER_LOGINS_TABLE",
    match_on = {
        "user_id" = "String",
        "date" = "DateTime<Utc>",
    },
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, Deserialize, Serialize)]
#[table_name = "auth_user_logins"]
pub struct NewAuthUserLogin {
    pub date: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "type")]
    pub typev: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub connection: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub connection_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub client_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub client_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ip: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub hostname: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub user_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub user_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub audience: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub scope: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub strategy: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub strategy_type: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub log_id: String,
    #[serde(default, alias = "isMobile")]
    pub is_mobile: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub user_agent: String,
    /// link to another table in Airtable
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub link_to_auth_user: Vec<String>,
    /// The CIO company ID.
    #[serde(default)]
    pub cio_company_id: i32,
}

/// Implement updating the Airtable record for a AuthUserLogin.
#[async_trait]
impl UpdateAirtableRecord<AuthUserLogin> for AuthUserLogin {
    async fn update_airtable_record(&mut self, _record: AuthUserLogin) {
        // Get the current auth users in Airtable so we can link to it.
        // TODO: make this more dry so we do not call it every single damn time.
        let db = Database::new();
        let auth_users = AuthUsers::get_from_airtable(&db, self.cio_company_id).await;

        // Iterate over the auth_users and see if we find a match.
        for (_id, auth_user_record) in auth_users {
            if auth_user_record.fields.user_id == self.user_id {
                // Set the link_to_auth_user to the right user.
                self.link_to_auth_user = vec![auth_user_record.id];
                // Break the loop and return early.
                break;
            }
        }
    }
}

/// The data type for an Auth0 user.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct User {
    pub user_id: String,
    pub email: String,
    #[serde(default)]
    pub email_verified: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub username: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub family_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub given_name: String,
    pub name: String,
    pub nickname: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub picture: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub phone_number: String,
    #[serde(default)]
    pub phone_verified: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub locale: String,
    pub identities: Vec<Identity>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_login: DateTime<Utc>,
    pub last_ip: String,
    pub logins_count: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub blog: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub company: String,
}

impl User {
    /// Convert an auth0 user into a NewAuthUser.
    pub fn to_auth_user(&self, c: &Company) -> NewAuthUser {
        let mut company: &str = &self.company;
        // Check if we have an Oxide email address.
        if self.email.ends_with("@oxidecomputer.com") || self.email.ends_with("@oxide.computer") || *self.company.trim() == *"Oxide Computer Company" {
            company = "@oxidecomputer";
        } else if self.email.ends_with("@bench.com") {
            // Check if we have a Benchmark Manufacturing email address.
            company = "@bench";
        } else if *self.company.trim() == *"Algolia" {
            // Cleanup algolia.
            company = "@algolia";
        } else if *self.company.trim() == *"0xF9BA143B95FF6D82" || self.company.trim().is_empty() || *self.company.trim() == *"TBD" {
            // Cleanup David Tolnay and other weird empty parses
            company = "";
        }

        NewAuthUser {
            user_id: self.user_id.to_string(),
            name: self.name.to_string(),
            nickname: self.nickname.to_string(),
            username: self.username.to_string(),
            email: self.email.to_string(),
            email_verified: self.email_verified,
            picture: self.picture.to_string(),
            company: company.trim().to_string(),
            blog: self.blog.to_string(),
            phone: self.phone_number.to_string(),
            phone_verified: self.phone_verified,
            locale: self.locale.to_string(),
            login_provider: self.identities[0].provider.to_string(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            last_login: self.last_login,
            last_ip: self.last_ip.to_string(),
            logins_count: self.logins_count,
            link_to_people: Default::default(),
            last_application_accessed: Default::default(),
            link_to_auth_user_logins: Default::default(),
            link_to_page_views: Default::default(),
            cio_company_id: c.id,
        }
    }
}

/// The data type for an Auth0 identity.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Identity {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub access_token: String,
    pub provider: String,
    pub user_id: String,
    pub connection: String,
    #[serde(rename = "isSocial")]
    pub is_social: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Token {
    pub access_token: String,
    pub token_type: String,
}

/// List users.
pub async fn get_auth_users(domain: String, db: &Database, company: &Company) -> Vec<NewAuthUser> {
    let client = Client::new();
    // Get our token.
    let client_id = env::var("CIO_AUTH0_CLIENT_ID").unwrap();
    let client_secret = env::var("CIO_AUTH0_CLIENT_SECRET").unwrap();

    let mut map = HashMap::new();
    map.insert("client_id", client_id);
    map.insert("client_secret", client_secret);
    map.insert("audience", format!("https://{}.auth0.com/api/v2/", domain));
    map.insert("grant_type", "client_credentials".to_string());

    let resp = client.post(&format!("https://{}.auth0.com/oauth/token", domain)).json(&map).send().await.unwrap();

    let token: Token = resp.json().await.unwrap();

    let mut users: Vec<User> = Default::default();

    let rate_limit_sleep = time::Duration::from_millis(2000);

    let mut i: i32 = 0;
    let mut has_records = true;
    while has_records {
        let mut u = get_auth_users_page(&token.access_token, &domain, &i.to_string()).await;
        // We need to sleep here for a half second so we don't get rate limited.
        // https://auth0.com/docs/policies/rate-limit-policy
        // https://auth0.com/docs/policies/rate-limit-policy/management-api-endpoint-rate-limits
        thread::sleep(rate_limit_sleep);

        has_records = !u.is_empty();
        i += 1;

        users.append(&mut u);
    }

    let mut auth_users: Vec<NewAuthUser> = Default::default();
    for user in users {
        // Convert the user to an AuthUser.
        let mut auth_user = user.to_auth_user(company);

        // Get the application they last accessed.
        let auth_user_logins = get_auth_logs_for_user(&token.access_token, &domain, &user.user_id).await;

        // Get the first result.
        if !auth_user_logins.is_empty() {
            let first_result = auth_user_logins.get(0).unwrap();
            auth_user.last_application_accessed = first_result.client_name.to_string();
        }

        auth_users.push(auth_user);

        // We need to sleep here for a half second so we don't get rate limited.
        // https://auth0.com/docs/policies/rate-limit-policy
        // https://auth0.com/docs/policies/rate-limit-policy/management-api-endpoint-rate-limits
        thread::sleep(rate_limit_sleep);

        // Update our database with all the auth_user_logins.
        for mut auth_user_login in auth_user_logins {
            auth_user_login.email = user.email.to_string();
            auth_user_login.cio_company_id = company.id;
            auth_user_login.upsert(db).await;
        }
    }

    auth_users
}

// TODO: clean this all up to be an auth0 api library.
async fn get_auth_logs_for_user(token: &str, domain: &str, user_id: &str) -> Vec<NewAuthUserLogin> {
    let client = Client::new();
    let resp = client
        .get(&format!("https://{}.auth0.com/api/v2/users/{}/logs", domain, user_id))
        .bearer_auth(token)
        .query(&[("sort", "date:-1"), ("per_page", "100")])
        .send()
        .await
        .unwrap();

    match resp.status() {
        StatusCode::OK => (),
        StatusCode::TOO_MANY_REQUESTS => {
            // Get the rate limit headers.
            let headers = resp.headers();
            let limit = headers.get("x-ratelimit-limit").unwrap().to_str().unwrap();
            let remaining = headers.get("x-ratelimit-remaining").unwrap().to_str().unwrap();
            let reset = headers.get("x-ratelimit-reset").unwrap().to_str().unwrap();
            let reset_int = reset.parse::<i64>().unwrap();

            // Convert the reset to a more sane number.
            let ts = DateTime::from_utc(NaiveDateTime::from_timestamp(reset_int, 0), Utc);
            let mut dur = ts - Utc::now();
            if dur.num_seconds() > 0 {
                dur = -dur;
            }
            let time = HumanTime::from(dur);

            println!("getting auth0 user logs failed because of rate limit: {}, remaining: {}, reset: {}", limit, remaining, time);

            return vec![];
        }
        s => {
            println!("getting auth0 user logs failed, status: {} | resp: {}", s, resp.text().await.unwrap(),);

            return vec![];
        }
    };

    resp.json::<Vec<NewAuthUserLogin>>().await.unwrap()
}

async fn get_auth_users_page(token: &str, domain: &str, page: &str) -> Vec<User> {
    let client = Client::new();
    let resp = client
        .get(&format!("https://{}.auth0.com/api/v2/users", domain))
        .bearer_auth(token)
        .query(&[("per_page", "20"), ("page", page), ("sort", "last_login:-1")])
        .send()
        .await
        .unwrap();

    match resp.status() {
        StatusCode::OK => (),
        s => {
            println!("getting auth0 users failed, status: {} | resp: {}", s, resp.text().await.unwrap());

            return vec![];
        }
    };

    resp.json::<Vec<User>>().await.unwrap()
}

// Sync the auth_users with our database.
pub async fn refresh_auth_users_and_logins(db: &Database, company: &Company) {
    // Get the company id for Oxide.
    let auth_users = get_auth_users("oxide".to_string(), db, company).await;

    // Sync auth users.
    for auth_user in auth_users {
        auth_user.upsert(db).await;
    }
}

#[cfg(test)]
mod tests {
    use crate::auth_logins::{refresh_auth_users_and_logins, AuthUserLogins, AuthUsers};
    use crate::companies::Company;
    use crate::db::Database;

    #[ignore]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_cron_auth_users_and_logins_refresh() {
        // Initialize our database.
        let db = Database::new();

        // TODO: split this out per company.
        let oxide = Company::get_from_db(&db, "Oxide".to_string()).unwrap();

        refresh_auth_users_and_logins(&db, &oxide).await;

        // Update auth user and auth user logins in airtable.
        AuthUserLogins::get_from_db(&db, oxide.id).update_airtable(&db).await;
        AuthUsers::get_from_db(&db, oxide.id).update_airtable(&db).await;
    }
}
