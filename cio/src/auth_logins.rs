use std::collections::BTreeMap;
use std::collections::HashMap;
use std::env;

use airtable_api::{Airtable, Record};
use chrono::offset::Utc;
use chrono::DateTime;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};

use crate::airtable::{
    airtable_api_key, AIRTABLE_AUTH0_LOGINS_TABLE,
    AIRTABLE_BASE_ID_CUSTOMER_LEADS, AIRTABLE_GRID_VIEW,
};
use crate::db::Database;
use crate::models::{AuthLogin, NewAuthLogin};

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
        {
            company = "@oxidecomputer";
        }
        // Check if we have a Benchmark Manufacturing email address.
        if self.email.ends_with("@bench.com") {
            company = "@bench";
        }

        NewAuthLogin {
            user_id: self.user_id.to_string(),
            name: self.name.to_string(),
            nickname: self.nickname.to_string(),
            username: self.username.to_string(),
            email: self.email.to_string(),
            email_verified: self.email_verified,
            picture: self.picture.to_string(),
            company: company.to_string(),
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
        }
    }
}

/// The data type for an Auth0 identity.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Identity {
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
pub async fn get_auth_logins(domain: String) -> Vec<NewAuthLogin> {
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

    let mut i: i32 = 0;
    let mut has_records = true;
    while has_records {
        let mut u = get_auth_logins_page(
            token.access_token.to_string(),
            domain.to_string(),
            &i.to_string(),
        )
        .await;

        has_records = !u.is_empty();
        i += 1;

        users.append(&mut u);
    }

    let mut auth_logins: Vec<NewAuthLogin> = Default::default();
    for user in users {
        auth_logins.push(user.to_auth_login());
    }

    auth_logins
}

async fn get_auth_logins_page(
    token: String,
    domain: String,
    page: &str,
) -> Vec<User> {
    let client = Client::new();
    let resp = client
        .get(&format!("https://{}.auth0.com/api/v2/users", domain))
        .bearer_auth(token)
        .query(&[("per_page", "20"), ("page", page), ("last_login", "-1")])
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

pub async fn refresh_airtable_auth_logins() {
    // Initialize the Airtable client.
    let airtable =
        Airtable::new(airtable_api_key(), AIRTABLE_BASE_ID_CUSTOMER_LEADS);

    let records = airtable
        .list_records(
            AIRTABLE_AUTH0_LOGINS_TABLE,
            AIRTABLE_GRID_VIEW,
            vec![
                "id",
                "link_to_people",
                "logins_count",
                "updated_at",
                "created_at",
                "user_id",
                "email_verified",
                "last_login",
            ],
        )
        .await
        .unwrap();

    let mut logins: BTreeMap<i32, (Record, AuthLogin)> = Default::default();
    for record in records {
        let fields: AuthLogin =
            serde_json::from_value(record.fields.clone()).unwrap();

        logins.insert(fields.id, (record, fields));
    }

    // Initialize our database.
    let db = Database::new();
    let auth_logins = db.get_auth_logins();

    let mut updated: i32 = 0;
    for mut auth_login in auth_logins {
        // See if we have it in our fields.
        match logins.get(&auth_login.id) {
            Some((r, in_airtable_fields)) => {
                let mut record = r.clone();

                if in_airtable_fields.user_id == auth_login.user_id
                    && in_airtable_fields.last_login == auth_login.last_login
                    && in_airtable_fields.logins_count
                        == auth_login.logins_count
                {
                    // We do not need to update the record.
                    continue;
                }

                // Set the Link to People from the original so it stays intact.
                auth_login.link_to_people =
                    in_airtable_fields.link_to_people.clone();

                record.fields = json!(auth_login);

                airtable
                    .update_records(
                        AIRTABLE_AUTH0_LOGINS_TABLE,
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

    println!("updated {} users", updated);
}

// Sync the auth_logins with our database.
pub async fn refresh_db_auth_logins() {
    let auth_logins = get_auth_logins("oxide".to_string()).await;

    // Initialize our database.
    let db = Database::new();

    // Sync rfds.
    for auth_login in auth_logins {
        db.upsert_auth_login(&auth_login);
    }
}

#[cfg(test)]
mod tests {
    use crate::auth_logins::{
        refresh_airtable_auth_logins, refresh_db_auth_logins,
    };

    #[tokio::test(threaded_scheduler)]
    async fn test_auth_logins() {
        refresh_db_auth_logins().await;
    }

    #[tokio::test(threaded_scheduler)]
    async fn test_auth_logins_airtable() {
        refresh_airtable_auth_logins().await;
    }
}
