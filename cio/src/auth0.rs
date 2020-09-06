use std::collections::BTreeMap;
use std::env;

use airtable_api::{Airtable, Record};
use chrono::offset::Utc;
use chrono::DateTime;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::airtable::{
    airtable_api_key, AIRTABLE_AUTH0_LOGINS_TABLE,
    AIRTABLE_BASE_ID_CUSTOMER_LEADS, AIRTABLE_GRID_VIEW,
};

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
            Some(u.to_string())
        } else {
            None
        };
        let company = if let Some(u) = &self.company {
            Some(u.to_string())
        } else {
            None
        };
        let blog = if let Some(u) = &self.blog {
            if u.trim().is_empty() {
                None
            } else {
                Some(u.to_string())
            }
        } else {
            None
        };
        let locale = if let Some(u) = &self.locale {
            Some(u.to_string())
        } else {
            None
        };
        let phone_number = if let Some(u) = &self.phone_number {
            Some(u.to_string())
        } else {
            None
        };
        let phone_verified = if let Some(u) = &self.phone_verified {
            Some(*u)
        } else {
            None
        };

        UserFields {
            user_id: self.user_id.to_string(),
            name: self.name.to_string(),
            link_to_people: None,
            nickname: self.nickname.to_string(),
            username,
            email: self.email.to_string(),
            email_verified: self.email_verified,
            picture: json!(&self.picture),
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
            potential_custmoer: None,
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

    let mut i: i32 = 0;
    let mut has_records = true;
    while has_records {
        let mut u = list_users_raw(domain.to_string(), &i.to_string()).await;

        has_records = !u.is_empty();
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

/// The Airtable fields type for an Auth0 user.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UserFields {
    #[serde(rename = "User ID")]
    pub user_id: String,
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "Link to People"
    )]
    pub link_to_people: Option<Vec<String>>,
    #[serde(rename = "Nickname")]
    pub nickname: String,
    #[serde(skip_serializing_if = "Option::is_none", rename = "Username")]
    pub username: Option<String>,
    #[serde(rename = "Email")]
    pub email: String,
    #[serde(rename = "Email verified?")]
    pub email_verified: bool,
    #[serde(rename = "Picture")]
    pub picture: Value,
    #[serde(skip_serializing_if = "Option::is_none", rename = "Company")]
    pub company: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "Blog")]
    pub blog: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "Phone number")]
    pub phone_number: Option<String>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "Phone verified?"
    )]
    pub phone_verified: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "Locale")]
    pub locale: Option<String>,
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
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "Potential customer?"
    )]
    pub potential_custmoer: Option<String>,
}

impl PartialEq for UserFields {
    fn eq(&self, other: &Self) -> bool {
        self.user_id == other.user_id
            && self.name == other.name
            && self.nickname == other.nickname
            && self.username == other.username
            && self.email == other.email
            && self.email_verified == other.email_verified
            //&& self.company == other.company // Ignore this field since we will modify it
            // remotely.
            && self.blog == other.blog
            && self.phone_number == other.phone_number
            && self.phone_verified == other.phone_verified
            && self.locale == other.locale
            && self.login_provider == other.login_provider
            && self.created_at == other.created_at
            && self.updated_at == other.updated_at
            && self.last_login == other.last_login
            && self.last_ip == other.last_ip
            && self.logins_count == other.logins_count
    }
}

impl UserFields {
    /// Push the auth0 login to our Airtable workspace.
    pub async fn push_to_airtable(&self) {
        // Initialize the Airtable client.
        let airtable =
            Airtable::new(airtable_api_key(), AIRTABLE_BASE_ID_CUSTOMER_LEADS);

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

pub async fn update_users_in_airtable() {
    // Initialize the Airtable client.
    let airtable =
        Airtable::new(airtable_api_key(), AIRTABLE_BASE_ID_CUSTOMER_LEADS);

    let records = airtable
        .list_records(AIRTABLE_AUTH0_LOGINS_TABLE, AIRTABLE_GRID_VIEW)
        .await
        .unwrap();

    let mut logins: BTreeMap<String, (Record, UserFields)> = Default::default();
    for record in records {
        let fields: UserFields =
            serde_json::from_value(record.fields.clone()).unwrap();

        logins.insert(fields.user_id.to_string(), (record, fields));
    }

    let users = list_users("oxide".to_string()).await;

    let mut updated: i32 = 0;
    for user in users {
        // See if we have it in our fields.
        match logins.get(&user.user_id) {
            Some((r, in_airtable_fields)) => {
                let mut record = r.clone();
                let mut fields = user.to_airtable_fields();

                // Check if we even need to update.
                if fields == *in_airtable_fields {
                    // They are the same, don't update.
                    continue;
                }

                // Set the Link to People from the original so it stays intact.
                fields.link_to_people =
                    in_airtable_fields.link_to_people.clone();
                // Set the company so it stays intact.
                fields.company = in_airtable_fields.company.clone();

                record.fields = json!(fields);

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
                user.to_airtable_fields().push_to_airtable().await;
            }
        }
    }

    println!("updated {} users", updated);
}

#[cfg(test)]
mod tests {
    use crate::auth0::update_users_in_airtable;

    #[tokio::test(threaded_scheduler)]
    async fn test_update_users_in_airtable() {
        update_users_in_airtable().await;
    }
}
