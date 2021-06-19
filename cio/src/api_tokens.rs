use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use macros::db;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::airtable::AIRTABLE_API_TOKENS_TABLE;
use crate::core::UpdateAirtableRecord;
use crate::schema::{api_tokens, api_tokens as a_p_i_tokens};

#[db {
    new_struct_name = "APIToken",
    airtable_base = "cio",
    airtable_table = "AIRTABLE_API_TOKENS_TABLE",
    match_on = {
        "cio_company_id" = "i32",
        "product" = "String",
    },
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[table_name = "api_tokens"]
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
    #[serde(default)]
    pub cio_company_id: i32,
}

/// Implement updating the Airtable record for a APIToken.
#[async_trait]
impl UpdateAirtableRecord<APIToken> for APIToken {
    async fn update_airtable_record(&mut self, _record: APIToken) {}
}

impl NewAPIToken {
    pub fn expand(&mut self) {
        if self.expires_in > 0 {
            // Set the time the tokens expire.
            self.expires_date = Some(self.last_updated_at.checked_add_signed(Duration::seconds(self.expires_in as i64)).unwrap());
        }

        if self.refresh_token_expires_in > 0 {
            self.refresh_token_expires_date = Some(self.last_updated_at.checked_add_signed(Duration::seconds(self.refresh_token_expires_in as i64)).unwrap());
        }
    }
}

impl APIToken {
    pub fn expand(&mut self) {
        if self.expires_in > 0 {
            // Set the time the tokens expire.
            self.expires_date = Some(self.last_updated_at.checked_add_signed(Duration::seconds(self.expires_in as i64)).unwrap());
        }

        if self.refresh_token_expires_in > 0 {
            self.refresh_token_expires_date = Some(self.last_updated_at.checked_add_signed(Duration::seconds(self.refresh_token_expires_in as i64)).unwrap());
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::api_tokens::APITokens;
    use crate::companies::Company;
    use crate::db::Database;

    #[ignore]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_cron_api_tokens() {
        let db = Database::new();

        // This should forever only be oxide.
        let oxide = Company::get_from_db(&db, "Oxide".to_string()).unwrap();

        APITokens::get_from_db(&db).update_airtable(&db, oxide.id).await;
    }
}
