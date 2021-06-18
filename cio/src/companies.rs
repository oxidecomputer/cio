use async_trait::async_trait;
use macros::db;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::airtable::{AIRTABLE_BASE_ID_CIO, AIRTABLE_COMPANIES_TABLE};
use crate::core::UpdateAirtableRecord;
use crate::db::Database;
use crate::schema::companys;

#[db {
    new_struct_name = "Company",
    airtable_base_id = "AIRTABLE_BASE_ID_CIO",
    airtable_table = "AIRTABLE_COMPANIES_TABLE",
    match_on = {
        "name" = "String",
    },
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[table_name = "companys"]
pub struct NewCompany {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub gsuite_domain: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub github_org: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub website: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub domain: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub gsuite_account_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub gsuite_subject: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub phone: String,
}

/// Implement updating the Airtable record for a Company.
#[async_trait]
impl UpdateAirtableRecord<Company> for Company {
    async fn update_airtable_record(&mut self, _record: Company) {}
}

pub async fn refresh_companies() {
    let db = Database::new();

    let is: Vec<airtable_api::Record<Company>> = Company::airtable().list_records(&Company::airtable_table(), "Grid view", vec![]).await.unwrap();

    for record in is {
        if record.fields.name.is_empty() || record.fields.website.is_empty() {
            // Ignore it, it's a blank record.
            continue;
        }

        let new_company: NewCompany = record.fields.into();

        let mut company = new_company.upsert_in_db(&db);
        if company.airtable_record_id.is_empty() {
            company.airtable_record_id = record.id;
        }
        company.update(&db).await;
    }
    Companys::get_from_db(&db).update_airtable().await;
}

#[cfg(test)]
mod tests {
    use crate::companies::refresh_companies;

    #[ignore]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_cron_companies() {
        refresh_companies().await;
    }
}
