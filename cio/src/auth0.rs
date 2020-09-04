use std::env;

use airtable_api::{Airtable, Record};
use chrono::offset::Utc;
use chrono::DateTime;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};

use crate::mailing_list::AIRTABLE_BASE_ID_CUSTOMER_LEADS;

static AIRTABLE_AUTH0_LOGINS_TABLE: &str = "Auth0 Logins to RFD Site";

/// The data type for an Auth0 user.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct User {
    pub user_id: String,
    pub email: String,
    pub email_verified: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub given_name: Option<String>,
    pub name: String,
    pub nickname: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub picture: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phone_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phone_verified: Option<bool>,
    pub locale: Option<String>,
    pub identities: Vec<Identity>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_login: DateTime<Utc>,
    pub last_ip: String,
    pub logins_count: i32,
    pub blog: Option<String>,
    pub company: Option<String>,
}

impl User {
    /// Convert an auth0 user into the format for Airtable.
    pub fn to_airtable_fields(&self) -> UserFields {
        let username = if let Some(u) = &self.username {
            u.to_string()
        } else {
            "".to_string()
        };
        let picture = if let Some(u) = &self.picture {
            u.to_string()
        } else {
            "".to_string()
        };
        let company = if let Some(u) = &self.company {
            u.to_string()
        } else {
            "".to_string()
        };
        let blog = if let Some(u) = &self.blog {
            u.to_string()
        } else {
            "".to_string()
        };
        let locale = if let Some(u) = &self.locale {
            u.to_string()
        } else {
            "".to_string()
        };
        let phone_number = if let Some(u) = &self.phone_number {
            u.to_string()
        } else {
            "".to_string()
        };
        let phone_verified = if let Some(u) = &self.phone_verified {
            *u
        } else {
            false
        };

        UserFields {
            user_id: self.user_id.to_string(),
            name: self.name.to_string(),
            nickname: self.nickname.to_string(),
            username,
            email: self.email.to_string(),
            email_verified: self.email_verified,
            picture,
            company,
            blog,
            phone_number,
            phone_verified,
            locale,
            login_provider: self.identities[0].provider.to_string(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            last_login: self.last_login,
            last_ip: self.last_ip.to_string(),
            logins_count: self.logins_count,
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

/// List users.
pub async fn list_users(domain: String) -> Vec<User> {
    let mut users: Vec<User> = Default::default();

    let mut i: i32 = 1;
    let mut has_records = true;
    while has_records {
        let mut u = list_users_raw(domain.to_string(), &i.to_string()).await;

        has_records = u.len() > 0;
        i += 1;

        users.append(&mut u);
    }

    users
}

async fn list_users_raw(domain: String, page: &str) -> Vec<User> {
    let client = Client::new();
    let resp = client
        .get(&format!("https://{}.auth0.com/api/v2/users", domain))
        .bearer_auth(env::var("AUTH0_TOKEN").unwrap())
        .query(&[("per_page", "50"), ("page", page)])
        .send()
        .await
        .unwrap();

    println!("headers: {:?}", resp.headers());

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

/// The Airtable fields type for an Auth0 user.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UserFields {
    #[serde(rename = "User ID")]
    pub user_id: String,
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Nickname")]
    pub nickname: String,
    #[serde(rename = "Username")]
    pub username: String,
    #[serde(rename = "Email")]
    pub email: String,
    #[serde(rename = "Email verified?")]
    pub email_verified: bool,
    #[serde(rename = "Picture")]
    pub picture: String,
    #[serde(rename = "Company")]
    pub company: String,
    #[serde(rename = "Blog")]
    pub blog: String,
    #[serde(rename = "Phone number")]
    pub phone_number: String,
    #[serde(rename = "Phone verified?")]
    pub phone_verified: bool,
    #[serde(rename = "Locale")]
    pub locale: String,
    #[serde(rename = "Login provider")]
    pub login_provider: String,
    #[serde(rename = "Created at")]
    pub created_at: DateTime<Utc>,
    #[serde(rename = "Updated at")]
    pub updated_at: DateTime<Utc>,
    #[serde(rename = "Last login")]
    pub last_login: DateTime<Utc>,
    #[serde(rename = "Last IP")]
    pub last_ip: String,
    #[serde(rename = "Logins count")]
    pub logins_count: i32,
}

impl UserFields {
    /// Push the auth0 login to our Airtable workspace.
    pub async fn push_to_airtable(&self) {
        let api_key = env::var("AIRTABLE_API_KEY").unwrap();
        // Initialize the Airtable client.
        let airtable =
            Airtable::new(api_key.to_string(), AIRTABLE_BASE_ID_CUSTOMER_LEADS);

        // Create the record.
        let record = Record {
            id: None,
            created_time: None,
            fields: serde_json::to_value(self).unwrap(),
        };

        // Send the new record to the Airtable client.
        // Batch can only handle 10 at a time.
        airtable
            .create_records(AIRTABLE_AUTH0_LOGINS_TABLE, vec![record])
            .await
            .unwrap();

        println!("created auth0 login in Airtable: {:?}", self);
    }
}

/*#[cfg(test)]
mod tests {
    use crate::auth0::list_users;

    #[tokio::test(threaded_scheduler)]
    async fn update_users_in_airtable() {
        let users = list_users("oxide".to_string()).await;

        for user in users {
            user.to_airtable_fields().push_to_airtable().await;
        }
    }
}*/
