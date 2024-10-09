#![allow(clippy::from_over_into)]
use anyhow::Result;
use async_bb8_diesel::AsyncRunQueryDsl;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use macros::db;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    airtable::AIRTABLE_PAGE_VIEWS_TABLE,
    auth_logins::AuthUsers,
    companies::{Company, Companys},
    core::UpdateAirtableRecord,
    db::Database,
    schema::page_views,
};

#[db {
    new_struct_name = "PageView",
    match_on = {
        "time" = "DateTime<Utc>",
        "user_email" = "String",
    },
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[diesel(table_name = page_views)]
pub struct NewPageView {
    pub time: DateTime<Utc>,
    pub domain: String,
    pub path: String,
    pub user_email: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub page_link: String,
    /// link to another table in Airtable
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub link_to_auth_user: Vec<String>,
    /// The CIO company ID.
    #[serde(default)]
    pub cio_company_id: i32,
}

impl NewPageView {
    pub fn set_page_link(&mut self) {
        // Set the link.
        self.page_link = format!("https://{}/{}", self.domain, self.path.trim_start_matches('/'));
    }

    pub async fn set_company_id(&mut self, db: &Database) -> Result<()> {
        // Match the company ID with the link.
        // All the companies are owned by Oxide.
        let companies = Companys::get_from_db(db, 1).await?;
        for company in companies {
            if self.domain.ends_with(&company.domain) {
                self.cio_company_id = company.id;
            }
        }

        Ok(())
    }
}
