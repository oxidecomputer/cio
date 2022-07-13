#![allow(clippy::from_over_into)]
use std::env;

use anyhow::{bail, Result};
use async_bb8_diesel::AsyncRunQueryDsl;
use async_trait::async_trait;
use chrono::{offset::Utc, DateTime, TimeZone};
use chrono_humanize::HumanTime;
use macros::db;
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

        let msg = format!("*{}* <mailto:{}|{}>", item.name, item.email, item.email);

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

/// Sync the rack_line_subscribers from Mailchimp with our database.
pub async fn refresh_db_rack_line_subscribers(db: &Database, company: &Company) -> Result<()> {
    let mailchimp_auth = company.authenticate_mailchimp().await;
    if let Err(e) = mailchimp_auth {
        bail!("authenticating mailchimp failed: {}", e);
    }

    let mailchimp = mailchimp_auth.unwrap();

    // TODO: remove this env variable.
    let members = mailchimp
        .get_subscribers(
            &env::var("MAILCHIMP_LIST_ID_RACK_LINE")
                .map_err(|e| anyhow::anyhow!("getting env var MAILCHIMP_LIST_ID_RACK_LINE failed: {}", e))?,
        )
        .await?;

    // Sync subscribers.
    for member in members {
        let mut ns: NewRackLineSubscriber = member.into();
        ns.cio_company_id = company.id;
        ns.upsert(db).await?;
    }

    RackLineSubscribers::get_from_db(db, company.id)
        .await?
        .update_airtable(db)
        .await?;

    // Pull down any tags and notes stored remotely, and ensure they are persisted locally
    let airtable_records = RackLineSubscribers::get_from_airtable(db, company.id).await?;

    for (id, record) in airtable_records {
        // Airtable records carry with them the most up to date tags and notes data, so we
        // can easily save them here. Ideally this could be a bulk update
        if let Err(err) = record.fields.update_in_db(db).await {
            log::error!("Failed to store tags and notes from remote for {}. {:?}", id, err);
        }
    }

    Ok(())
}

/// Convert to a signup data type.
pub async fn as_rack_line_subscriber(
    webhook: mailchimp_minimal_api::Webhook,
    db: &Database,
) -> Result<NewRackLineSubscriber> {
    let mut signup: NewRackLineSubscriber = Default::default();

    let _list_id = webhook.data.list_id.as_ref().unwrap();

    // Get the company from the list id.
    // TODO: eventually change this when we have more than one.
    if let Some(company) = Company::get_from_db(db, "Oxide".to_string()).await {
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

        Ok(signup)
    } else {
        bail!("Could not find company with name 'Oxide'")
    }
}

impl Into<NewRackLineSubscriber> for mailchimp_minimal_api::Member {
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
            zoho_lead_id: Default::default(),
            zoho_lead_exclude: false,
        }
    }
}
