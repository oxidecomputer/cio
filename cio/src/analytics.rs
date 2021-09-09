#![allow(clippy::from_over_into)]
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use macros::db;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    airtable::AIRTABLE_PAGE_VIEWS_TABLE, auth_logins::AuthUsers, companies::Companys, core::UpdateAirtableRecord,
    db::Database, schema::page_views,
};

#[db {
    new_struct_name = "PageView",
    airtable_base = "customer_leads",
    airtable_table = "AIRTABLE_PAGE_VIEWS_TABLE",
    match_on = {
        "time" = "DateTime<Utc>",
        "user_email" = "String",
    },
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[table_name = "page_views"]
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

/// Implement updating the Airtable record for a PageView.
#[async_trait]
impl UpdateAirtableRecord<PageView> for PageView {
    async fn update_airtable_record(&mut self, _record: PageView) -> Result<()> {
        // Get the current auth users in Airtable so we can link to it.
        // TODO: make this more dry so we do not call it every single damn time.
        let db = Database::new();
        let auth_users = AuthUsers::get_from_airtable(&db, self.cio_company_id).await?;

        // Iterate over the auth_users and see if we find a match.
        for (_id, auth_user_record) in auth_users {
            if auth_user_record.fields.email == self.user_email {
                // Set the link_to_auth_user to the right user.
                self.link_to_auth_user = vec![auth_user_record.id];
                // Break the loop and return early.
                break;
            }
        }

        Ok(())
    }
}

impl NewPageView {
    pub fn set_page_link(&mut self) {
        // Set the link.
        self.page_link = format!("https://{}/{}", self.domain, self.path.trim_start_matches('/'));
    }

    pub fn set_company_id(&mut self, db: &Database) {
        // Match the company ID with the link.
        // All the companies are owned by Oxide.
        let companies = Companys::get_from_db(db, 1);
        for company in companies {
            if self.domain.ends_with(&company.domain) {
                self.cio_company_id = company.id;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{analytics::PageViews, companies::Company, db::Database};

    #[ignore]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_cron_page_views_airtable() {
        // Initialize our database.
        let db = Database::new();

        // TODO: iterate over all the companies.
        let oxide = Company::get_from_db(&db, "Oxide".to_string()).unwrap();

        PageViews::get_from_db(&db, oxide.id).update_airtable(&db).await;
    }
}
