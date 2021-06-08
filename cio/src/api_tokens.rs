use async_trait::async_trait;
use chrono::{DateTime, Utc};
use macros::db;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::airtable::{AIRTABLE_API_TOKENS_TABLE, AIRTABLE_BASE_ID_CIO};
use crate::core::UpdateAirtableRecord;
use crate::schema::{api_tokens, api_tokens as a_p_i_tokens};

#[db {
    new_struct_name = "APIToken",
    airtable_base_id = "AIRTABLE_BASE_ID_CIO",
    airtable_table = "AIRTABLE_API_TOKENS_TABLE",
    match_on = {
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
    pub last_updated_at: DateTime<Utc>,
}

/// Implement updating the Airtable record for a APIToken.
#[async_trait]
impl UpdateAirtableRecord<APIToken> for APIToken {
    async fn update_airtable_record(&mut self, _record: APIToken) {}
}

#[cfg(test)]
mod tests {
    use crate::api_tokens::APITokens;
    use crate::db::Database;

    #[ignore]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_cron_api_tokens() {
        let db = Database::new();
        APITokens::get_from_db(&db).update_airtable().await;
    }
}
