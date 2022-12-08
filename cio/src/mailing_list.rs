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
    airtable::AIRTABLE_MAILING_LIST_SIGNUPS_TABLE, companies::Company, core::UpdateAirtableRecord, db::Database,
    schema::mailing_list_subscribers,
};

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
#[diesel(table_name = mailing_list_subscribers)]
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
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
    #[serde(default)]
    pub revenue: f32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub street_1: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub street_2: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub city: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub state: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub zipcode: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub country: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub address_formatted: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub phone: String,
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
    pub async fn send_slack_notification(&self, db: &Database, company: &Company) -> Result<()> {
        let mut msg: FormattedMessage = self.clone().into();
        // Set the channel.
        msg.channel = company.slack_channel_mailing_lists.to_string();
        // Post the message.
        company.post_to_slack_channel(db, &msg).await?;

        Ok(())
    }

    /// Get the human duration of time since the signup was fired.
    pub fn human_duration(&self) -> HumanTime {
        let mut dur = self.date_added - Utc::now();
        if dur.num_seconds() > 0 {
            dur = -dur;
        }

        HumanTime::from(dur)
    }
}

impl MailingListSubscriber {
    pub async fn send_slack_notification(&self, db: &Database, company: &Company) -> Result<()> {
        let n: NewMailingListSubscriber = self.into();
        n.send_slack_notification(db, company).await
    }
}

/// Convert the mailing list signup into a Slack message.
impl From<NewMailingListSubscriber> for FormattedMessage {
    fn from(item: NewMailingListSubscriber) -> Self {
        let time = item.human_duration();

        let msg: String = Default::default();

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

        let updates = format!(
            "podcast updates: _{}_ | newsletter: _{}_ | product updates: _{}_",
            item.wants_podcast_updates, item.wants_newsletter, item.wants_product_updates,
        );

        let mut context = "".to_string();
        if !item.company.is_empty() {
            context += &format!("works at {} | ", item.company);
        }
        context += &format!("subscribed to mailing list {}", time);

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
                        text: updates,
                    })],
                    text: Default::default(),
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
            source: String::new(),
            revenue: Default::default(),
            street_1: Default::default(),
            street_2: Default::default(),
            city: Default::default(),
            state: Default::default(),
            zipcode: Default::default(),
            country: Default::default(),
            address_formatted: Default::default(),
            phone: Default::default(),
            tags: Default::default(),
            link_to_people: Default::default(),
            cio_company_id: Default::default(),
        }
    }
}

/// Implement updating the Airtable record for a MailingListSubscriber.
#[async_trait]
impl UpdateAirtableRecord<MailingListSubscriber> for MailingListSubscriber {
    async fn update_airtable_record(&mut self, record: MailingListSubscriber) -> Result<()> {
        // Set the link_to_people from the original so it stays intact.
        self.link_to_people = record.link_to_people;

        Ok(())
    }
}

impl From<mailerlite::Subscriber> for NewMailingListSubscriber {
    fn from(subscriber: mailerlite::Subscriber) -> Self {
        let mut new_sub = NewMailingListSubscriber::default();

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
                SubscriberFieldValue::Null => {}
                _ => log::warn!(
                    "Non-string field type found for company field for subscriber {}",
                    subscriber.id
                ),
            }
        }

        if let Some(subscriber_location) = subscriber.get_field("subscribe_location") {
            match subscriber_location {
                SubscriberFieldValue::String(subscriber_location) => new_sub.tags.push(subscriber_location.clone()),
                SubscriberFieldValue::Null => {}
                _ => log::warn!(
                    "Non-string field type found for subscribe_location field for subscriber {}",
                    subscriber.id
                ),
            }
        }

        new_sub.email = subscriber.email;
        new_sub.source = subscriber.source;

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
