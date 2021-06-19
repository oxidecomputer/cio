#![allow(clippy::from_over_into)]

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

use crate::airtable::AIRTABLE_MAILING_LIST_SIGNUPS_TABLE;
use crate::companies::Company;
use crate::db::Database;
use crate::mailchimp::{get_all_mailchimp_subscribers, MailchimpMember};
use crate::schema::mailing_list_subscribers;

/// The data type for a MailingListSubscriber.
#[db {
    new_struct_name = "MailingListSubscriber",
    airtable_base = "customer_leads",
    airtable_table = "AIRTABLE_MAILING_LIST_SIGNUPS_TABLE",
    match_on = {
        "email" = "String",
    },
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[table_name = "mailing_list_subscribers"]
pub struct NewMailingListSubscriber {
    pub email: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub first_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub last_name: String,
    /// (generated) name is a combination of first_name and last_name.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub company: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub interest: String,
    #[serde(default)]
    pub wants_podcast_updates: bool,
    #[serde(default)]
    pub wants_newsletter: bool,
    #[serde(default)]
    pub wants_product_updates: bool,
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

impl NewMailingListSubscriber {
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

        let updates = format!(
            "podcast updates: _{}_ | newsletter: _{}_ | product updates: _{}_",
            self.wants_podcast_updates, self.wants_newsletter, self.wants_product_updates,
        );

        let mut context = "".to_string();
        if !self.company.is_empty() {
            context += &format!("works at {} | ", self.company);
        }
        context += &format!("subscribed to mailing list {}", time);

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
                        text: updates,
                    }],
                    text: Default::default(),
                    accessory: Default::default(),
                    block_id: Default::default(),
                    fields: Default::default(),
                },
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

impl Default for NewMailingListSubscriber {
    fn default() -> Self {
        NewMailingListSubscriber {
            email: String::new(),
            first_name: String::new(),
            last_name: String::new(),
            name: String::new(),
            company: String::new(),
            interest: String::new(),
            wants_podcast_updates: false,
            wants_newsletter: false,
            wants_product_updates: false,
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

/// Implement updating the Airtable record for a MailingListSubscriber.
#[async_trait]
impl UpdateAirtableRecord<MailingListSubscriber> for MailingListSubscriber {
    async fn update_airtable_record(&mut self, record: MailingListSubscriber) {
        // Set the link_to_people from the original so it stays intact.
        self.link_to_people = record.link_to_people;
    }
}

/// Sync the mailing_list_subscribers from Mailchimp with our database.
pub async fn refresh_db_mailing_list_subscribers(db: &Database) {
    // Get the company id for Oxide.
    // TODO: split this out per company.
    let oxide = Company::get_from_db(&db, "Oxide".to_string()).unwrap();

    let members = get_all_mailchimp_subscribers(&oxide.mailchimp_list_id).await;

    // Sync subscribers.
    for member in members {
        let mut ns: NewMailingListSubscriber = member.into();
        ns.cio_company_id = oxide.id;
        ns.upsert(db).await;
    }
}

impl Into<NewMailingListSubscriber> for MailchimpMember {
    fn into(self) -> NewMailingListSubscriber {
        let default_bool = false;

        let mut tags: Vec<String> = Default::default();
        for t in &self.tags {
            tags.push(t.name.to_string());
        }

        NewMailingListSubscriber {
            email: self.email_address,
            first_name: self.merge_fields.first_name.to_string(),
            last_name: self.merge_fields.last_name.to_string(),
            name: format!("{} {}", self.merge_fields.first_name, self.merge_fields.last_name),
            company: self.merge_fields.company,
            interest: self.merge_fields.interest,
            // Note to next person. Finding these numbers means looking at actual records and the
            // API response. Don't know of a better way....
            wants_podcast_updates: *self.interests.get("ff0295f7d1").unwrap_or(&default_bool),
            wants_newsletter: *self.interests.get("7f57718c10").unwrap_or(&default_bool),
            wants_product_updates: *self.interests.get("6a6cb58277").unwrap_or(&default_bool),
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
    use crate::mailing_list::{refresh_db_mailing_list_subscribers, MailingListSubscribers};

    #[ignore]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_cron_mailing_list_subscribers() {
        // Initialize our database.
        let db = Database::new();

        refresh_db_mailing_list_subscribers(&db).await;
        MailingListSubscribers::get_from_db(&db).update_airtable().await;
    }
}
