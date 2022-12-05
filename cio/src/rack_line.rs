use anyhow::Result;
use async_bb8_diesel::AsyncRunQueryDsl;
use async_trait::async_trait;
use chrono::{offset::Utc, DateTime};
use chrono_humanize::HumanTime;
use macros::db;
use mailerlite::SubscriberFieldValue;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
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
#[diesel(table_name = rack_line_subscribers)]
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
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub zoho_lead_id: String,
    #[serde(default)]
    pub zoho_lead_exclude: bool,
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

    pub async fn send_slack_notification(&self, db: &Database, company: &Company) -> Result<()> {
        let mut msg: FormattedMessage = self.clone().into();
        // Set the channel.
        msg.channel = company.slack_channel_mailing_lists.to_string();
        // Post the message.
        company.post_to_slack_channel(db, &msg).await?;

        Ok(())
    }
}

impl RackLineSubscriber {
    pub async fn send_slack_notification(&self, db: &Database, company: &Company) -> Result<()> {
        let n: NewRackLineSubscriber = self.into();
        n.send_slack_notification(db, company).await
    }
}

/// Convert the mailing list signup into Slack message.
impl From<NewRackLineSubscriber> for FormattedMessage {
    fn from(item: NewRackLineSubscriber) -> Self {
        let time = item.human_duration();

        let msg = String::default();

        let mut interest: MessageBlock = Default::default();
        if !item.interest.is_empty() {
            interest = MessageBlock {
                block_type: MessageBlockType::Section,
                text: Some(MessageBlockText {
                    text_type: MessageType::Markdown,
                    text: format!("\n>{}", item.interest),
                }),
                elements: Default::default(),
                accessory: Default::default(),
                block_id: Default::default(),
                fields: Default::default(),
            };
        }

        let mut context = "".to_string();
        if !item.company.is_empty() {
            context += &format!("works at {} | ", item.company);
        }
        if !item.company_size.is_empty() {
            context += &format!("company size: {} | ", item.company_size);
        }
        context += &format!("subscribed to rack line {}", time);

        let mut message = FormattedMessage {
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
                MessageBlock {
                    block_type: MessageBlockType::Context,
                    elements: vec![slack_chat_api::BlockOption::MessageBlockText(MessageBlockText {
                        text_type: MessageType::Markdown,
                        text: context,
                    })],
                    text: Default::default(),
                    accessory: Default::default(),
                    block_id: Default::default(),
                    fields: Default::default(),
                },
            ],
        };

        if item.interest.is_empty() {
            return message;
        }

        message.blocks.insert(1, interest);

        message
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
            zoho_lead_id: Default::default(),
            zoho_lead_exclude: false,
        }
    }
}

/// Implement updating the Airtable record for a RackLineSubscriber.
#[async_trait]
impl UpdateAirtableRecord<RackLineSubscriber> for RackLineSubscriber {
    async fn update_airtable_record(&mut self, record: RackLineSubscriber) -> Result<()> {
        // Set the link_to_people from the original so it stays intact.
        self.link_to_people = record.link_to_people;

        // Notes and tags are owned by Airtable
        self.notes = record.notes;
        self.tags = record.tags;

        Ok(())
    }
}

impl From<mailerlite::Subscriber> for NewRackLineSubscriber {
    fn from(subscriber: mailerlite::Subscriber) -> Self {
        let mut new_sub = NewRackLineSubscriber::default();

        if let Some(name) = subscriber.get_field("name") {
            match name {
                SubscriberFieldValue::String(name) => new_sub.name = name.clone(),
                _ => log::warn!(
                    "Non-string field type found for name field for subscriber {}",
                    subscriber.id
                ),
            }
        }

        if let Some(company) = subscriber.get_field("company") {
            match company {
                SubscriberFieldValue::String(company) => new_sub.company = company.clone(),
                _ => log::warn!(
                    "Non-string field type found for company field for subscriber {}",
                    subscriber.id
                ),
            }
        }

        if let Some(company_size) = subscriber.get_field("company_size") {
            match company_size {
                SubscriberFieldValue::String(company_size) => new_sub.company_size = company_size.clone(),
                _ => log::warn!(
                    "Non-string field type found for company_size field for subscriber {}",
                    subscriber.id
                ),
            }
        }

        if let Some(notes) = subscriber.get_field("notes") {
            match notes {
                SubscriberFieldValue::String(notes) => new_sub.interest = notes.clone(),
                _ => log::warn!(
                    "Non-string field type found for notes field for subscriber {}",
                    subscriber.id
                ),
            }
        }

        new_sub.email = subscriber.email;

        if let Some(subscribed_at) = subscriber.subscribed_at {
            new_sub.date_added = subscribed_at;
        }

        if let Some(opted_in_at) = subscriber.opted_in_at {
            new_sub.date_optin = opted_in_at;
        }

        new_sub.date_last_changed = subscriber.updated_at;

        // Hack to be removed later
        new_sub.cio_company_id = 1;

        new_sub
    }
}
