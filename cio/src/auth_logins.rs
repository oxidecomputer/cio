use std::collections::BTreeMap;
use std::collections::HashMap;
use std::env;
use std::{thread, time};

use airtable_api::{Airtable, Record};
use chrono::naive::NaiveDateTime;
use chrono::offset::Utc;
use chrono::DateTime;
use chrono_humanize::HumanTime;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};

use crate::airtable::{
    airtable_api_key, AIRTABLE_AUTH_USERS_TABLE,
    AIRTABLE_AUTH_USER_LOGINS_TABLE, AIRTABLE_BASE_ID_CUSTOMER_LEADS,
    AIRTABLE_GRID_VIEW,
};
use crate::db::Database;
use crate::models::{AuthLogin, AuthUserLogin, NewAuthLogin, NewAuthUserLogin};

/// The data type for an Auth0 user.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct User {
    pub user_id: String,
    pub email: String,
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
    /// Convert an auth0 user into a NewAuthLogin.
    pub fn to_auth_login(&self) -> NewAuthLogin {
        let mut company: &str = &self.company;
        // Check if we have an Oxide email address.
        if self.email.ends_with("@oxidecomputer.com")
            || self.email.ends_with("@oxide.computer")
            || self.company.trim().to_string()
                == "Oxide Computer Company".to_string()
        {
            company = "@oxidecomputer";
        } else if self.email.ends_with("@bench.com") {
            // Check if we have a Benchmark Manufacturing email address.
            company = "@bench";
        } else if self.company.trim().to_string() == "Algolia".to_string() {
            // Cleanup algolia.
            company = "@algolia";
        } else if self.company.trim().to_string()
            == "0xF9BA143B95FF6D82".to_string()
            || self.company.trim().is_empty()
            || self.company.trim().to_string() == "TBD".to_string()
        {
            // Cleanup David Tolnay and other weird empty parses
            company = "";
        }

        NewAuthLogin {
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
pub async fn get_auth_users(
    domain: String,
    db: &Database,
) -> Vec<NewAuthLogin> {
    let client = Client::new();
    // Get our token.
    let client_id = env::var("CIO_AUTH0_CLIENT_ID").unwrap();
    let client_secret = env::var("CIO_AUTH0_CLIENT_SECRET").unwrap();

    let mut map = HashMap::new();
    map.insert("client_id", client_id);
    map.insert("client_secret", client_secret);
    map.insert("audience", format!("https://{}.auth0.com/api/v2/", domain));
    map.insert("grant_type", "client_credentials".to_string());

    let resp = client
        .post(&format!("https://{}.auth0.com/oauth/token", domain))
        .json(&map)
        .send()
        .await
        .unwrap();

    let token: Token = resp.json().await.unwrap();

    let mut users: Vec<User> = Default::default();

    let rate_limit_sleep = time::Duration::from_millis(2000);

    let mut i: i32 = 0;
    let mut has_records = true;
    while has_records {
        let mut u =
            get_auth_users_page(&token.access_token, &domain, &i.to_string())
                .await;
        // We need to sleep here for a half second so we don't get rate limited.
        // https://auth0.com/docs/policies/rate-limit-policy
        // https://auth0.com/docs/policies/rate-limit-policy/management-api-endpoint-rate-limits
        thread::sleep(rate_limit_sleep);

        has_records = !u.is_empty();
        i += 1;

        users.append(&mut u);
    }

    let mut auth_logins: Vec<NewAuthLogin> = Default::default();
    for user in users {
        // Convert the user to an AuthLogin.
        let mut auth_login = user.to_auth_login();

        // Get the application they last accessed.
        let auth_user_logins =
            get_auth_logs_for_user(&token.access_token, &domain, &user.user_id)
                .await;

        // Get the first result.
        if !auth_user_logins.is_empty() {
            let first_result = auth_user_logins.get(0).unwrap();
            auth_login.last_application_accessed =
                first_result.client_name.to_string();
        }

        auth_logins.push(auth_login);

        // We need to sleep here for a half second so we don't get rate limited.
        // https://auth0.com/docs/policies/rate-limit-policy
        // https://auth0.com/docs/policies/rate-limit-policy/management-api-endpoint-rate-limits
        thread::sleep(rate_limit_sleep);

        // Update our database with all the auth_user_logins.
        for mut auth_user_login in auth_user_logins {
            auth_user_login.email = user.email.to_string();
            db.upsert_auth_user_login(&auth_user_login);
        }
    }

    auth_logins
}

// TODO: clean this all up to be an auth0 api library.
async fn get_auth_logs_for_user(
    token: &str,
    domain: &str,
    user_id: &str,
) -> Vec<NewAuthUserLogin> {
    let client = Client::new();
    let resp = client
        .get(&format!(
            "https://{}.auth0.com/api/v2/users/{}/logs",
            domain, user_id
        ))
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
            let limit =
                headers.get("x-ratelimit-limit").unwrap().to_str().unwrap();
            let remaining = headers
                .get("x-ratelimit-remaining")
                .unwrap()
                .to_str()
                .unwrap();
            let reset =
                headers.get("x-ratelimit-reset").unwrap().to_str().unwrap();
            let reset_int = reset.parse::<i64>().unwrap();

            // Convert the reset to a more sane number.
            let ts = DateTime::from_utc(
                NaiveDateTime::from_timestamp(reset_int, 0),
                Utc,
            );
            let mut dur = ts - Utc::now();
            if dur.num_seconds() > 0 {
                dur = -dur;
            }
            let time = HumanTime::from(dur);

            println!("getting auth0 user logs failed because of rate limit: {}, remaining: {}, reset: {}",limit, remaining, time);

            return vec![];
        }
        s => {
            println!(
                "getting auth0 user logs failed, status: {} | resp: {}",
                s,
                resp.text().await.unwrap(),
            );

            return vec![];
        }
    };

    resp.json::<Vec<NewAuthUserLogin>>().await.unwrap()
}

async fn get_auth_users_page(
    token: &str,
    domain: &str,
    page: &str,
) -> Vec<User> {
    let client = Client::new();
    let resp = client
        .get(&format!("https://{}.auth0.com/api/v2/users", domain))
        .bearer_auth(token)
        .query(&[
            ("per_page", "20"),
            ("page", page),
            ("sort", "last_login:-1"),
        ])
        .send()
        .await
        .unwrap();

    match resp.status() {
        StatusCode::OK => (),
        s => {
            println!(
                "getting auth0 users failed, status: {} | resp: {}",
                s,
                resp.text().await.unwrap()
            );

            return vec![];
        }
    };

    resp.json::<Vec<User>>().await.unwrap()
}

pub async fn refresh_airtable_auth_users() {
    // Initialize the Airtable client.
    let airtable =
        Airtable::new(airtable_api_key(), AIRTABLE_BASE_ID_CUSTOMER_LEADS);

    let records = airtable
        .list_records(
            AIRTABLE_AUTH_USERS_TABLE,
            AIRTABLE_GRID_VIEW,
            vec![
                "id",
                "link_to_people",
                "logins_count",
                "updated_at",
                "created_at",
                "user_id",
                "last_login",
                "email_verified",
            ],
        )
        .await
        .unwrap();

    let mut airtable_auth_logins: BTreeMap<i32, (Record, AuthLogin)> =
        Default::default();
    for record in records {
        let fields: AuthLogin =
            serde_json::from_value(record.fields.clone()).unwrap();

        airtable_auth_logins.insert(fields.id, (record, fields));
    }

    // Initialize our database.
    let db = Database::new();
    let auth_logins = db.get_auth_logins();

    let mut updated: i32 = 0;
    for mut auth_login in auth_logins {
        // See if we have it in our fields.
        match airtable_auth_logins.get(&auth_login.id) {
            Some((r, in_airtable_fields)) => {
                let mut record = r.clone();

                if in_airtable_fields.user_id == auth_login.user_id
                    && in_airtable_fields.last_login == auth_login.last_login
                    && in_airtable_fields.logins_count
                        == auth_login.logins_count
                    && in_airtable_fields.last_application_accessed
                        == auth_login.last_application_accessed
                {
                    // We do not need to update the record.
                    continue;
                }

                // Set the link_to_people and link_to_auth_user_logins from the original so it stays intact.
                auth_login.link_to_people =
                    in_airtable_fields.link_to_people.clone();
                auth_login.link_to_auth_user_logins =
                    in_airtable_fields.link_to_auth_user_logins.clone();

                record.fields = json!(auth_login);

                airtable
                    .update_records(
                        AIRTABLE_AUTH_USERS_TABLE,
                        vec![record.clone()],
                    )
                    .await
                    .unwrap();

                updated += 1;
            }
            None => {
                // Create the record.
                auth_login.push_to_airtable().await;
            }
        }
    }

    println!("updated {} auth_logins", updated);
}

pub async fn refresh_airtable_auth_user_logins() {
    // Initialize the Airtable client.
    let airtable =
        Airtable::new(airtable_api_key(), AIRTABLE_BASE_ID_CUSTOMER_LEADS);

    let records = airtable
        .list_records(
            AIRTABLE_AUTH_USER_LOGINS_TABLE,
            AIRTABLE_GRID_VIEW,
            vec![],
        )
        .await
        .unwrap();

    let mut airtable_auth_user_logins: BTreeMap<i32, (Record, AuthUserLogin)> =
        Default::default();
    for record in records {
        let fields: AuthUserLogin =
            serde_json::from_value(record.fields.clone()).unwrap();

        airtable_auth_user_logins.insert(fields.id, (record, fields));
    }

    // Initialize our database.
    let db = Database::new();
    let auth_user_logins = db.get_auth_user_logins();

    let mut updated: i32 = 0;
    for mut auth_user_login in auth_user_logins {
        // See if we have it in our fields.
        match airtable_auth_user_logins.get(&auth_user_login.id) {
            Some((r, in_airtable_fields)) => {
                let mut record = r.clone();

                if in_airtable_fields.log_id == auth_user_login.log_id
                    && in_airtable_fields.date == auth_user_login.date
                    && in_airtable_fields.id == auth_user_login.id
                    && in_airtable_fields.email == auth_user_login.email
                {
                    // We do not need to update the record.
                    continue;
                }

                // Set the link_to_auth_user from the original so it stays intact.
                auth_user_login.link_to_auth_user =
                    in_airtable_fields.link_to_auth_user.clone();

                record.fields = json!(auth_user_login);

                airtable
                    .update_records(
                        AIRTABLE_AUTH_USER_LOGINS_TABLE,
                        vec![record.clone()],
                    )
                    .await
                    .unwrap();

                updated += 1;
            }
            None => {
                // Create the record.
                auth_user_login.push_to_airtable().await;
            }
        }
    }

    println!("updated {} auth_user_logins", updated);
}

// Sync the auth_logins with our database.
pub async fn refresh_db_auth() {
    // Initialize our database.
    let db = Database::new();

    let auth_logins = get_auth_users("oxide".to_string(), &db).await;

    // Sync rfds.
    for auth_login in auth_logins {
        db.upsert_auth_login(&auth_login);
    }
}

#[cfg(test)]
mod tests {
    use crate::auth_logins::{
        refresh_airtable_auth_user_logins, refresh_airtable_auth_users,
        refresh_db_auth,
    };

    #[tokio::test(threaded_scheduler)]
    async fn test_auth_refresh_db() {
        refresh_db_auth().await;
    }

    #[tokio::test(threaded_scheduler)]
    async fn test_auth_users_airtable() {
        refresh_airtable_auth_users().await;
    }

    #[tokio::test(threaded_scheduler)]
    async fn test_auth_user_logins_airtable() {
        refresh_airtable_auth_user_logins().await;
    }
}
