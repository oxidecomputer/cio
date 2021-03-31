use async_trait::async_trait;
use macros::db;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::airtable::{AIRTABLE_BASE_ID_FINANCE, AIRTABLE_SOFTWARE_VENDORS_TABLE};
use crate::core::UpdateAirtableRecord;
use crate::db::Database;
use crate::schema::software_vendors;
use crate::utils::{authenticate_github_jwt, get_github_token};

#[db {
    new_struct_name = "Software Vendor",
    airtable_base_id = "AIRTABLE_BASE_ID_FINANCE",
    airtable_table = "AIRTABLE_SOFTWARE_VENDORS_TABLE",
    match_on = {
        "name" = "String",
    },
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[table_name = "software_vendors"]
pub struct NewApplicantInterview {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub status: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub website: String,
    #[serde(default)]
    pub has_okta_integration: bool,
    #[serde(default)]
    pub used_purely_for_api: bool,
    #[serde(default)]
    pub pay_as_you_go: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pay_as_you_go_pricing_description: String,
    #[serde(default)]
    pub software_licenses: bool,
    #[serde(default)]
    pub cost_per_user_per_month: f32,
    #[serde(default)]
    pub users: i32,
    #[serde(default)]
    pub flat_cost_per_month: f32,
    #[serde(default)]
    pub total_cost_per_month: f32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<String>,
}

/// Implement updating the Airtable record for a ApplicantInterview.
#[async_trait]
impl UpdateAirtableRecord<ApplicantInterview> for ApplicantInterview {
    #[instrument]
    #[inline]
    async fn update_airtable_record(&mut self, _record: ApplicantInterview) {}
}
