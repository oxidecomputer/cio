use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::airtable::{AIRTABLE_BASE_ID_CUSTOMER_LEADS, AIRTABLE_PAGE_VIEWS_TABLE};

#[db_struct {
    new_name = "PageView",
    base_id = "AIRTABLE_BASE_ID_CUSTOMER_LEADS",
    table = "AIRTABLE_PAGE_VIEWS_TABLE",
}]
pub struct NewPageView {
    time: DateTime<Utc>,
    domain: String,
    path: String,
    user_email: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    page_link: String,
    /// link to another table in Airtable
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    link_to_auth_user: Vec<String>,
}

/// Implement updating the Airtable record for a PageView.
#[async_trait]
impl UpdateAirtableRecord<PageView> for PageView {
    #[instrument]
    #[inline]
    async fn update_airtable_record(&mut self, _record: PageView) {
        // Get the current auth users in Airtable so we can link to it.
        // TODO: make this more dry so we do not call it every single damn time.
        let auth_users = AuthUsers::get_from_airtable().await;

        // Iterate over the auth_users and see if we find a match.
        for (_id, auth_user_record) in auth_users {
            if auth_user_record.fields.user_id == self.user_id {
                // Set the link_to_auth_user to the right user.
                self.link_to_auth_user = vec![auth_user_record.id];
                // Break the loop and return early.
                break;
            }
        }
    }
}

impl NewPageView {
    pub fn set_page_link(&mut self) {
        // Set the link.
        self.page_link = format!("https://{}/{}", self.domain, self.path.trim_start_matches('/'));
    }
}
