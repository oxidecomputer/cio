use std::collections::HashMap;
use std::env;

use airtable_api::{Airtable, Record};
use chrono::offset::Utc;
use chrono::DateTime;
use chrono_humanize::HumanTime;
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
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Signup {
    #[serde(rename = "Email Address")]
    pub email: String,
    #[serde(rename = "First Name")]
    pub first_name: String,
    #[serde(rename = "Last Name")]
    pub last_name: String,
    #[serde(rename = "Company")]
    pub company: String,
    #[serde(rename = "What is your interest in Oxide Computer Company?")]
    pub interest: String,
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

        let msg = format!(
            "*{} {}* <mailto:{}|{}>",
            self.first_name, self.last_name, self.email, self.email
        );

        let mut interest: MessageBlock = Default::default();
        if !self.interest.is_empty() {
            interest = MessageBlock {
                block_type: MessageBlockType::Section,
                text: Some(MessageBlockText {
                    text_type: MessageType::Markdown,
                    text: format!("\n>{}", self.interest.trim()),
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
        if !self.company.is_empty() {
            context += &format!("works at {} | ", self.company);
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
            first_name: "".to_string(),
            last_name: "".to_string(),
            company: "".to_string(),
            interest: "".to_string(),
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
                f.trim().to_string()
            } else {
                "".to_string()
            };
            signup.last_name = if let Some(l) = &merges.last_name {
                l.trim().to_string()
            } else {
                "".to_string()
            };
            signup.company = if let Some(c) = &merges.company {
                c.trim().to_string()
            } else {
                "".to_string()
            };
            signup.interest = if let Some(i) = &merges.interest {
                i.trim().to_string()
            } else {
                "".to_string()
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
