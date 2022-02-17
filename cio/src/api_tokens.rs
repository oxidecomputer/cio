use anyhow::Result;
use async_bb8_diesel::{AsyncRunQueryDsl};
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use macros::db;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    airtable::AIRTABLE_API_TOKENS_TABLE,
    companies::Company,
    core::UpdateAirtableRecord,
    db::Database,
    schema::{api_tokens as a_p_i_tokens, api_tokens},
};

#[db {
    new_struct_name = "APIToken",
    airtable_base = "cio",
    airtable_table = "AIRTABLE_API_TOKENS_TABLE",
    match_on = {
        "auth_company_id" = "i32",
        "product" = "String",
    },
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[diesel(table_name = api_tokens)]
pub struct NewAPIToken {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub product: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub company_id: String,
    /// This is only relevant for Plaid.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub item_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub user_email: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub token_type: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub access_token: String,
    /// Seconds until the token expires.
    #[serde(default)]
    pub expires_in: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub refresh_token: String,
    /// Seconds until the refresh token expires.
    #[serde(default)]
    pub refresh_token_expires_in: i32,
    /// Date when the token expires.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_date: Option<DateTime<Utc>>,
    /// Date when the refresh token expires.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token_expires_date: Option<DateTime<Utc>>,
    /// The optional endpoint, if the API has one, that is specific to
    /// the user.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub endpoint: String,
    pub last_updated_at: DateTime<Utc>,
    /// The CIO company ID.
    /// This should always be Oxide so it saves to our Airtable.
    #[serde(default)]
    pub cio_company_id: i32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub company: Vec<String>,
    /// This is the actual company that we match on for getting the token.
    #[serde(default)]
    pub auth_company_id: i32,
}

/// Implement updating the Airtable record for a APIToken.
#[async_trait]
impl UpdateAirtableRecord<APIToken> for APIToken {
    async fn update_airtable_record(&mut self, _record: APIToken) -> Result<()> {
        // Link to the correct company.
        let db = Database::new().await;
        let company = Company::get_by_id(&db, self.auth_company_id).await?;
        self.company = vec![company.airtable_record_id];

        Ok(())
    }
}

impl NewAPIToken {
    pub fn expand(&mut self) {
        if self.expires_in > 0 {
            // Set the time the tokens expire.
            self.expires_date = Some(
                self.last_updated_at
                    .checked_add_signed(Duration::seconds(self.expires_in as i64))
                    .unwrap(),
            );
        }

        if self.refresh_token_expires_in > 0 {
            self.refresh_token_expires_date = Some(
                self.last_updated_at
                    .checked_add_signed(Duration::seconds(self.refresh_token_expires_in as i64))
                    .unwrap(),
            );
        }
    }
}

impl APIToken {
    pub fn expand(&mut self) {
        if self.expires_in > 0 {
            // Set the time the tokens expire.
            self.expires_date = Some(
                self.last_updated_at
                    .checked_add_signed(Duration::seconds(self.expires_in as i64))
                    .unwrap(),
            );
        }

        if self.refresh_token_expires_in > 0 {
            self.refresh_token_expires_date = Some(
                self.last_updated_at
                    .checked_add_signed(Duration::seconds(self.refresh_token_expires_in as i64))
                    .unwrap(),
            );
        }
    }

    /// Returns if the token is expired.
    pub fn is_expired(&self) -> bool {
        //if let Some(d) = self.expires_date {
        // To be safe, let's subtract a few hours, since that might be
        // how long it takes for the job to run.
        //Utc::now() < d.checked_sub_signed(Duration::hours(10)).unwrap()
        // true
        //} else {
        // Set to being expired by default if we don't know the date.
        true
        // }
    }
}

pub async fn refresh_api_tokens(db: &Database, company: &Company) -> Result<()> {
    APITokens::get_from_db(db, company.id)
        .await?
        .update_airtable(db)
        .await?;

    Ok(())
}
