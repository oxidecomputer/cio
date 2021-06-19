#![allow(clippy::from_over_into)]
use std::env;

use crate::core::UpdateAirtableRecord;
use async_trait::async_trait;
use chrono::offset::Utc;
use chrono::DateTime;
use chrono_humanize::HumanTime;
use macros::db;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use slack_chat_api::{FormattedMessage, MessageBlock, MessageBlockText, MessageBlockType, MessageType};

use crate::airtable::AIRTABLE_RACK_LINE_SIGNUPS_TABLE;
use crate::companies::Company;
use crate::db::Database;
use crate::mailchimp::{get_all_mailchimp_subscribers, MailchimpMember};
use crate::schema::rack_line_subscribers;

/// The data type for a RackLineSubscriber.
#[db {
    new_struct_name = "RackLineSubscriber",
    airtable_base = "customer_leads",
    airtable_table = "AIRTABLE_RACK_LINE_SIGNUPS_TABLE",
    match_on = {
        "email" = "String",
    },
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[table_name = "rack_line_subscribers"]
pub struct NewRackLineSubscriber {
    pub email: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub company: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub company_size: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub interest: String,
    pub date_added: DateTime<Utc>,
    pub date_optin: DateTime<Utc>,
    pub date_last_changed: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// link to another table in Airtable
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub link_to_people: Vec<String>,
    /// The CIO company ID.
    #[serde(default)]
    pub cio_company_id: i32,
}

impl NewRackLineSubscriber {
    /// Get the human duration of time since the signup was fired.
    pub fn human_duration(&self) -> HumanTime {
        let mut dur = self.date_added - Utc::now();
        if dur.num_seconds() > 0 {
            dur = -dur;
        }

        HumanTime::from(dur)
    }

    /// Convert the mailing list signup into JSON as Slack message.
    pub fn as_slack_msg(&self) -> Value {
        let time = self.human_duration();

        let msg = format!("*{}* <mailto:{}|{}>", self.name, self.email, self.email);

        let mut interest: MessageBlock = Default::default();
        if !self.interest.is_empty() {
            interest = MessageBlock {
                block_type: MessageBlockType::Section,
                text: Some(MessageBlockText {
                    text_type: MessageType::Markdown,
                    text: format!("\n>{}", self.interest),
                }),
                elements: Default::default(),
                accessory: Default::default(),
                block_id: Default::default(),
                fields: Default::default(),
            };
        }

        let mut context = "".to_string();
        if !self.company.is_empty() {
            context += &format!("works at {} | ", self.company);
        }
        if !self.company_size.is_empty() {
            context += &format!("company size: {} | ", self.company_size);
        }
        context += &format!("subscribed to rack line {}", time);

        json!(FormattedMessage {
            channel: Default::default(),
            attachments: Default::default(),
            blocks: vec![
                MessageBlock {
                    block_type: MessageBlockType::Section,
                    text: Some(MessageBlockText {
                        text_type: MessageType::Markdown,
                        text: msg,
                    }),
                    elements: Default::default(),
                    accessory: Default::default(),
                    block_id: Default::default(),
                    fields: Default::default(),
                },
                interest,
                MessageBlock {
                    block_type: MessageBlockType::Context,
                    elements: vec![MessageBlockText {
                        text_type: MessageType::Markdown,
                        text: context,
                    }],
                    text: Default::default(),
                    accessory: Default::default(),
                    block_id: Default::default(),
                    fields: Default::default(),
                }
            ],
        })
    }
}

impl Default for NewRackLineSubscriber {
    fn default() -> Self {
        NewRackLineSubscriber {
            email: String::new(),
            name: String::new(),
            company: String::new(),
            company_size: String::new(),
            interest: String::new(),
            date_added: Utc::now(),
            date_optin: Utc::now(),
            date_last_changed: Utc::now(),
            notes: String::new(),
            tags: Default::default(),
            link_to_people: Default::default(),
            cio_company_id: Default::default(),
        }
    }
}

/// Implement updating the Airtable record for a RackLineSubscriber.
#[async_trait]
impl UpdateAirtableRecord<RackLineSubscriber> for RackLineSubscriber {
    async fn update_airtable_record(&mut self, record: RackLineSubscriber) {
        // Set the link_to_people from the original so it stays intact.
        self.link_to_people = record.link_to_people;
    }
}

/// Sync the rack_line_subscribers from Mailchimp with our database.
pub async fn refresh_db_rack_line_subscribers(db: &Database) {
    // Get the company id for Oxide.
    // TODO: split this out per company.
    let oxide = Company::get_from_db(&db, "Oxide".to_string()).unwrap();

    // TODO: remove this env variable.
    let members = get_all_mailchimp_subscribers(&env::var("MAILCHIMP_LIST_ID_RACK_LINE").unwrap_or_default()).await;

    // Sync subscribers.
    for member in members {
        let mut ns: NewRackLineSubscriber = member.into();
        ns.cio_company_id = oxide.id;
        ns.upsert(db).await;
    }
}

impl Into<NewRackLineSubscriber> for MailchimpMember {
    fn into(self) -> NewRackLineSubscriber {
        let mut tags: Vec<String> = Default::default();
        for t in &self.tags {
            tags.push(t.name.to_string());
        }

        NewRackLineSubscriber {
            email: self.email_address,
            name: self.merge_fields.name.to_string(),
            company: self.merge_fields.company,
            company_size: self.merge_fields.company_size,
            interest: self.merge_fields.notes,
            date_added: self.timestamp_opt,
            date_optin: self.timestamp_opt,
            date_last_changed: self.last_changed,
            notes: self.last_note.note,
            tags,
            link_to_people: Default::default(),
            cio_company_id: Default::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::db::Database;
    use crate::rack_line::{refresh_db_rack_line_subscribers, RackLineSubscribers};

    #[ignore]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_cron_rack_line_subscribers() {
        // Initialize our database.
        let db = Database::new();

        refresh_db_rack_line_subscribers(&db).await;
        RackLineSubscribers::get_from_db(&db).update_airtable().await;
    }
}
