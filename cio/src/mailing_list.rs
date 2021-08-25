#![allow(clippy::from_over_into)]

use async_trait::async_trait;
use chrono::{offset::Utc, DateTime, TimeZone};
use chrono_humanize::HumanTime;
use macros::db;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
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
    fn populate_formatted_address(&mut self) {
        let mut street_address = self.street_1.to_string();
        if !self.street_2.is_empty() {
            street_address = format!("{}\n{}", self.street_1, self.street_2,);
        }
        self.address_formatted = format!(
            "{}\n{}, {} {} {}",
            street_address, self.city, self.state, self.zipcode, self.country
        )
        .trim()
        .trim_matches(',')
        .trim()
        .to_string();
    }

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
    async fn update_airtable_record(&mut self, record: MailingListSubscriber) {
        // Set the link_to_people from the original so it stays intact.
        self.link_to_people = record.link_to_people;
    }
}

/// Sync the mailing_list_subscribers from Mailchimp with our database.
pub async fn refresh_db_mailing_list_subscribers(db: &Database, company: &Company) {
    if company.mailchimp_list_id.is_empty() {
        // Return early.
        return;
    }

    let mailchimp_auth = company.authenticate_mailchimp(db).await;
    if mailchimp_auth.is_none() {
        // Return early.
        return;
    }

    let mailchimp = mailchimp_auth.unwrap();

    let members = mailchimp.get_subscribers(&company.mailchimp_list_id).await.unwrap();

    // Sync subscribers.
    for member in members {
        let mut ns: NewMailingListSubscriber = member.into();
        ns.cio_company_id = company.id;
        ns.upsert(db).await;
    }
}

/// Convert to a signup data type.
pub fn as_mailing_list_subscriber(webhook: mailchimp_api::Webhook, db: &Database) -> NewMailingListSubscriber {
    let mut signup: NewMailingListSubscriber = Default::default();

    let list_id = webhook.data.list_id.as_ref().unwrap();

    // Get the company from the list id.
    let company = Company::get_from_mailchimp_list_id(db, list_id);

    if webhook.data.merges.is_some() {
        let merges = webhook.data.merges.as_ref().unwrap();

        if let Some(e) = &merges.email {
            signup.email = e.trim().to_string();
        }
        if let Some(f) = &merges.first_name {
            signup.first_name = f.trim().to_string();
        }
        if let Some(l) = &merges.last_name {
            signup.last_name = l.trim().to_string();
        }
        if let Some(c) = &merges.company {
            signup.company = c.trim().to_string();
        }
        if let Some(i) = &merges.interest {
            signup.interest = i.trim().to_string();
        }

        if merges.groupings.is_some() {
            let groupings = merges.groupings.as_ref().unwrap();

            signup.wants_podcast_updates = groupings[0].groups.is_some();
            signup.wants_newsletter = groupings[1].groups.is_some();
            signup.wants_product_updates = groupings[2].groups.is_some();
        }
    }

    signup.date_added = webhook.fired_at;
    signup.date_optin = webhook.fired_at;
    signup.date_last_changed = webhook.fired_at;
    signup.name = format!("{} {}", signup.first_name, signup.last_name);

    signup.cio_company_id = company.id;

    signup
}

impl Into<NewMailingListSubscriber> for mailchimp_api::Member {
    fn into(self) -> NewMailingListSubscriber {
        let default_bool = false;

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

        let address: mailchimp_api::Address =
            serde_json::from_str(&self.merge_fields.address.to_string()).unwrap_or_default();

        let mut ns = NewMailingListSubscriber {
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
            date_added: timestamp,
            date_optin: timestamp,
            date_last_changed: self.last_changed,
            notes: self.last_note.note,
            source: self.source.to_string(),
            revenue: self.stats.ecommerce_data.total_revenue as f32,
            street_1: address.addr1.to_string(),
            street_2: address.addr2.to_string(),
            city: address.city.to_string(),
            state: address.state.to_string(),
            zipcode: address.zip.to_string(),
            country: address.country.to_string(),
            address_formatted: Default::default(),
            phone: self.merge_fields.phone.to_string(),
            tags,
            link_to_people: Default::default(),
            cio_company_id: Default::default(),
        };

        ns.populate_formatted_address();

        ns
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        companies::Companys,
        db::Database,
        mailing_list::{refresh_db_mailing_list_subscribers, MailingListSubscribers},
    };

    #[ignore]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_cron_mailing_list_subscribers() {
        // Initialize our database.
        let db = Database::new();
        let companies = Companys::get_from_db(&db, 1);
        // Iterate over the companies and update the mailing list subscribers for both.
        for company in companies {
            refresh_db_mailing_list_subscribers(&db, &company).await;
            MailingListSubscribers::get_from_db(&db, company.id)
                .update_airtable(&db)
                .await;
        }
    }
}
