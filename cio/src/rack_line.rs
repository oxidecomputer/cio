#![allow(clippy::from_over_into)]
use std::env;

use anyhow::Result;
use async_trait::async_trait;
use chrono::{offset::Utc, DateTime, TimeZone};
use chrono_humanize::HumanTime;
use macros::db;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use slack_chat_api::{FormattedMessage, MessageBlock, MessageBlockText, MessageBlockType, MessageType};

use crate::{
    airtable::AIRTABLE_RACK_LINE_SIGNUPS_TABLE, companies::Company, core::UpdateAirtableRecord, db::Database,
    schema::rack_line_subscribers,
};

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
    async fn update_airtable_record(&mut self, record: RackLineSubscriber) -> Result<()> {
        // Set the link_to_people from the original so it stays intact.
        self.link_to_people = record.link_to_people;

        Ok(())
    }
}

/// Sync the rack_line_subscribers from Mailchimp with our database.
pub async fn refresh_db_rack_line_subscribers(db: &Database, company: &Company) {
    let mailchimp_auth = company.authenticate_mailchimp(db).await;
    if mailchimp_auth.is_none() {
        // Return early.
        return;
    }

    let mailchimp = mailchimp_auth.unwrap();

    // TODO: remove this env variable.
    let members = mailchimp
        .get_subscribers(&env::var("MAILCHIMP_LIST_ID_RACK_LINE").unwrap_or_default())
        .await
        .unwrap();

    // Sync subscribers.
    for member in members {
        let mut ns: NewRackLineSubscriber = member.into();
        ns.cio_company_id = company.id;
        ns.upsert(db).await;
    }
}

/// Convert to a signup data type.
pub fn as_rack_line_subscriber(webhook: mailchimp_api::Webhook, db: &Database) -> NewRackLineSubscriber {
    let mut signup: NewRackLineSubscriber = Default::default();

    let _list_id = webhook.data.list_id.as_ref().unwrap();

    // Get the company from the list id.
    // TODO: eventually change this when we have more than one.
    let company = Company::get_from_db(db, "Oxide".to_string()).unwrap();

    if webhook.data.merges.is_some() {
        let merges = webhook.data.merges.as_ref().unwrap();

        if let Some(e) = &merges.email {
            signup.email = e.trim().to_string();
        }
        if let Some(f) = &merges.name {
            signup.name = f.trim().to_string();
        }
        if let Some(c) = &merges.company {
            signup.company = c.trim().to_string();
        }
        if let Some(c) = &merges.company_size {
            signup.company_size = c.trim().to_string();
        }
        if let Some(i) = &merges.notes {
            signup.interest = i.trim().to_string();
        }
    }

    signup.date_added = webhook.fired_at;
    signup.date_optin = webhook.fired_at;
    signup.date_last_changed = webhook.fired_at;

    signup.cio_company_id = company.id;

    signup
}

impl Into<NewRackLineSubscriber> for mailchimp_api::Member {
    fn into(self) -> NewRackLineSubscriber {
        let mut tags: Vec<String> = Default::default();
        for t in &self.tags {
            tags.push(t.name.to_string());
        }
        let mut timestamp = Utc::now();

        if !self.timestamp_opt.is_empty() {
            timestamp = Utc.datetime_from_str(&self.timestamp_opt, "%+").unwrap();
        }
        if !self.timestamp_signup.is_empty() {
            timestamp = Utc.datetime_from_str(&self.timestamp_signup, "%+").unwrap();
        }

        NewRackLineSubscriber {
            email: self.email_address,
            name: self.merge_fields.name.to_string(),
            company: self.merge_fields.company,
            company_size: self.merge_fields.company_size,
            interest: self.merge_fields.notes,
            date_added: timestamp,
            date_optin: timestamp,
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
    use crate::{
        companies::Company,
        db::Database,
        rack_line::{refresh_db_rack_line_subscribers, RackLineSubscribers},
    };

    #[ignore]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_cron_rack_line_subscribers() {
        // Initialize our database.
        let db = Database::new();

        // Get the company id for Oxide.
        // TODO: split this out per company.
        let oxide = Company::get_from_db(&db, "Oxide".to_string()).unwrap();

        refresh_db_rack_line_subscribers(&db, &oxide).await;
        RackLineSubscribers::get_from_db(&db, oxide.id)
            .update_airtable(&db)
            .await;
    }
}
