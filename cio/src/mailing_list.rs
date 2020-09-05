use std::collections::HashMap;
use std::env;

use airtable_api::{Airtable, Record};
use chrono::offset::Utc;
use chrono::DateTime;
use chrono_humanize::HumanTime;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::slack::{
    FormattedMessage, MessageBlock, MessageBlockText, MessageBlockType,
    MessageType,
};

pub static AIRTABLE_BASE_ID_CUSTOMER_LEADS: &str = "appr7imQLcR3pWaNa";
static AIRTABLE_MAILING_LIST_SIGNUPS_TABLE: &str = "Mailing List Signups";

/// The data type for a mailing list signup.
/// This is inline with our Airtable workspace.
#[derive(Debug, Clone, JsonSchema, Deserialize, Serialize)]
pub struct Signup {
    #[serde(rename = "Email Address")]
    pub email: String,
    #[serde(skip_serializing_if = "Option::is_none", rename = "First Name")]
    pub first_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "Last Name")]
    pub last_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "Company")]
    pub company: Option<String>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "What is your interest in Oxide Computer Company?"
    )]
    pub interest: Option<String>,
    #[serde(rename = "Interested in On the Metal podcast updates?")]
    pub wants_podcast_updates: bool,
    #[serde(rename = "Interested in the Oxide newsletter?")]
    pub wants_newsletter: bool,
    #[serde(rename = "Interested in product updates?")]
    pub wants_product_updates: bool,
    #[serde(rename = "Date Added")]
    pub date_added: DateTime<Utc>,
    #[serde(rename = "Opt-in Date")]
    pub optin_date: DateTime<Utc>,
    #[serde(rename = "Last Changed")]
    pub last_changed: DateTime<Utc>,
}

impl Signup {
    /// Push the mailing list signup to our Airtable workspace.
    pub async fn push_to_airtable(&self) {
        let api_key = env::var("AIRTABLE_API_KEY").unwrap();
        // Initialize the Airtable client.
        let airtable =
            Airtable::new(api_key.to_string(), AIRTABLE_BASE_ID_CUSTOMER_LEADS);

        // Create the record.
        let record = Record {
            id: None,
            created_time: None,
            fields: serde_json::to_value(self).unwrap(),
        };

        // Send the new record to the Airtable client.
        // Batch can only handle 10 at a time.
        airtable
            .create_records(AIRTABLE_MAILING_LIST_SIGNUPS_TABLE, vec![record])
            .await
            .unwrap();

        println!("created mailing list record in Airtable: {:?}", self);
    }

    /// Convert the mailing list signup into JSON as Slack message.
    pub fn as_slack_msg(&self) -> Value {
        let dur = self.date_added - Utc::now();
        let time = HumanTime::from(dur);

        let mut name = String::new();
        if self.first_name.is_some() {
            name += self.first_name.as_ref().unwrap();
            if self.last_name.is_some() {
                name += &(" ".to_string() + &self.last_name.as_ref().unwrap());
            }
        } else if self.last_name.is_some() {
            name += self.last_name.as_ref().unwrap();
        }
        let msg = format!("*{}* <mailto:{}|{}>", name, self.email, self.email);

        let mut interest: MessageBlock = Default::default();
        if self.interest.is_some() {
            interest = MessageBlock {
                block_type: MessageBlockType::Section,
                text: Some(MessageBlockText {
                    text_type: MessageType::Markdown,
                    text: format!(
                        "\n>{}",
                        self.interest.as_ref().unwrap().trim()
                    ),
                }),
                elements: None,
                accessory: None,
                block_id: None,
                fields: None,
            };
        }

        let updates = format!(
            "podcast updates: _{}_ | newsletter: _{}_ | product updates: _{}_",
            self.wants_podcast_updates,
            self.wants_newsletter,
            self.wants_product_updates
        );

        let mut context = "".to_string();
        if self.company.is_some() {
            context +=
                &format!("works at {} | ", self.company.as_ref().unwrap());
        }
        context += &format!("subscribed to mailing list {}", time);

        json!(FormattedMessage {
            channel: None,
            attachments: None,
            blocks: Some(vec![
                MessageBlock {
                    block_type: MessageBlockType::Section,
                    text: Some(MessageBlockText {
                        text_type: MessageType::Markdown,
                        text: msg,
                    }),
                    elements: None,
                    accessory: None,
                    block_id: None,
                    fields: None,
                },
                interest,
                MessageBlock {
                    block_type: MessageBlockType::Context,
                    elements: Some(vec![MessageBlockText {
                        text_type: MessageType::Markdown,
                        text: updates,
                    }]),
                    text: None,
                    accessory: None,
                    block_id: None,
                    fields: None,
                },
                MessageBlock {
                    block_type: MessageBlockType::Context,
                    elements: Some(vec![MessageBlockText {
                        text_type: MessageType::Markdown,
                        text: context,
                    }]),
                    text: None,
                    accessory: None,
                    block_id: None,
                    fields: None,
                }
            ]),
        })
    }
}

/// Get all the mailing list subscribers from Airtable.
pub async fn get_all_subscribers() -> Vec<Signup> {
    let api_key = env::var("AIRTABLE_API_KEY").unwrap();
    // Initialize the Airtable client.
    let airtable =
        Airtable::new(api_key.to_string(), AIRTABLE_BASE_ID_CUSTOMER_LEADS);

    let records = airtable
        .list_records(AIRTABLE_MAILING_LIST_SIGNUPS_TABLE, "Grid view")
        .await
        .unwrap();

    let mut subscribers: Vec<Signup> = Default::default();
    for record in records {
        let fields: Signup =
            serde_json::from_value(record.fields.clone()).unwrap();

        subscribers.push(fields);
    }
    subscribers
}

/// The data type for the webhook from Mailchimp.
///
/// Docs:
/// https://mailchimp.com/developer/guides/sync-audience-data-with-webhooks/#handling-the-webhook-response-in-your-application
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MailchimpWebhook {
    #[serde(rename = "type")]
    pub webhook_type: String,
    fired_at: DateTime<Utc>,
    data: MailchimpWebhookData,
}

impl MailchimpWebhook {
    /// Convert to a signup data type.
    pub fn as_signup(&self) -> Signup {
        let mut signup: Signup = Signup {
            email: "".to_string(),
            first_name: None,
            last_name: None,
            company: None,
            interest: None,
            wants_podcast_updates: false,
            wants_newsletter: false,
            wants_product_updates: false,
            date_added: Utc::now(),
            optin_date: Utc::now(),
            last_changed: Utc::now(),
        };

        if self.data.merges.is_some() {
            let merges = self.data.merges.as_ref().unwrap();

            signup.email = if let Some(e) = &merges.email {
                e.trim().to_string()
            } else {
                "".to_string()
            };
            signup.first_name = if let Some(f) = &merges.first_name {
                Some(f.trim().to_string())
            } else {
                None
            };
            signup.last_name = if let Some(l) = &merges.last_name {
                Some(l.trim().to_string())
            } else {
                None
            };
            signup.company = if let Some(c) = &merges.company {
                Some(c.trim().to_string())
            } else {
                None
            };
            signup.interest = if let Some(i) = &merges.interest {
                Some(i.trim().to_string())
            } else {
                None
            };

            if merges.groupings.is_some() {
                let groupings = merges.groupings.as_ref().unwrap();

                signup.wants_podcast_updates =
                    groupings.get(&0).unwrap().groups.is_some();
                signup.wants_newsletter =
                    groupings.get(&1).unwrap().groups.is_some();
                signup.wants_product_updates =
                    groupings.get(&2).unwrap().groups.is_some();
            }
        }

        signup.date_added = self.fired_at;
        signup.optin_date = self.fired_at;
        signup.last_changed = self.fired_at;

        signup
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MailchimpWebhookData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_opt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_signup: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merges: Option<MailchimpWebhookMerges>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MailchimpWebhookMerges {
    #[serde(skip_serializing_if = "Option::is_none", rename = "FNAME")]
    pub first_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "LNAME")]
    pub last_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "EMAIL")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "ADDRESS")]
    pub address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "PHONE")]
    pub phone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "COMPANY")]
    pub company: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "INTEREST")]
    pub interest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "BIRTHDAY")]
    pub birthday: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "GROUPINGS")]
    pub groupings: Option<HashMap<i32, MailchimpWebhookGrouping>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MailchimpWebhookGrouping {
    pub id: String,
    pub unique_id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub groups: Option<String>,
}
