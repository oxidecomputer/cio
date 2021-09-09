#![allow(clippy::from_over_into)]

use anyhow::Result;
use async_trait::async_trait;
use chrono::{offset::Utc, DateTime};
use macros::db;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    airtable::{AIRTABLE_AUTH_USERS_TABLE, AIRTABLE_AUTH_USER_LOGINS_TABLE},
    core::UpdateAirtableRecord,
    db::Database,
    schema::{auth_user_logins, auth_users},
};

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
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "airtable_api::attachment_format_as_string::deserialize"
    )]
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
    async fn update_airtable_record(&mut self, record: AuthUser) -> Result<()> {
        // Set the link_to_people and link_to_auth_user_logins from the original so it stays intact.
        self.link_to_people = record.link_to_people.clone();
        self.link_to_auth_user_logins = record.link_to_auth_user_logins;
        self.link_to_page_views = record.link_to_page_views;

        Ok(())
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
    async fn update_airtable_record(&mut self, _record: AuthUserLogin) -> Result<()> {
        // Get the current auth users in Airtable so we can link to it.
        // TODO: make this more dry so we do not call it every single damn time.
        let db = Database::new();
        let auth_users = AuthUsers::get_from_airtable(&db, self.cio_company_id).await?;

        // Iterate over the auth_users and see if we find a match.
        for (_id, auth_user_record) in auth_users {
            if auth_user_record.fields.user_id == self.user_id {
                // Set the link_to_auth_user to the right user.
                self.link_to_auth_user = vec![auth_user_record.id];
                // Break the loop and return early.
                break;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        auth_logins::{AuthUserLogins, AuthUsers},
        companies::Company,
        db::Database,
    };

    #[ignore]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_cron_auth_users_and_logins_refresh() {
        // Initialize our database.
        let db = Database::new();

        // TODO: split this out per company.
        let oxide = Company::get_from_db(&db, "Oxide".to_string()).unwrap();

        // Update auth user and auth user logins in airtable.
        AuthUserLogins::get_from_db(&db, oxide.id).update_airtable(&db).await;
        AuthUsers::get_from_db(&db, oxide.id).update_airtable(&db).await;
    }
}
