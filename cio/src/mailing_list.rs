use chrono::offset::Utc;
use chrono::DateTime;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::models::NewMailingListSubscriber;

/// The data type for the webhook from Mailchimp.
///
/// Docs:
/// https://mailchimp.com/developer/guides/sync-audience-data-with-webhooks/#handling-the-webhook-response-in-your-application
#[derive(Debug, Clone, JsonSchema, Deserialize, Serialize)]
pub struct MailchimpWebhook {
    #[serde(rename = "type")]
    pub webhook_type: String,
    #[serde(deserialize_with = "mailchimp_date_format::deserialize", serialize_with = "mailchimp_date_format::serialize")]
    fired_at: DateTime<Utc>,
    data: MailchimpWebhookData,
}

mod mailchimp_date_format {
    use chrono::{DateTime, TimeZone, Utc};
    use serde::{self, Deserialize, Deserializer, Serializer};

    const FORMAT: &str = "%Y-%m-%d %H:%M:%S";

    // The signature of a serialize_with function must follow the pattern:
    //
    //    fn serialize<S>(&T, S) -> Result<S::Ok, S::Error>
    //    where
    //        S: Serializer
    //
    // although it may also be generic over the input types T.
    pub fn serialize<S>(date: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = format!("{}", date.format(FORMAT));
        serializer.serialize_str(&s)
    }

    // The signature of a deserialize_with function must follow the pattern:
    //
    //    fn deserialize<'de, D>(D) -> Result<T, D::Error>
    //    where
    //        D: Deserializer<'de>
    //
    // although it may also be generic over the output types T.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer).unwrap();
        Utc.datetime_from_str(&s, FORMAT).map_err(serde::de::Error::custom)
    }
}

impl MailchimpWebhook {
    /// Convert to a signup data type.
    pub fn as_subscriber(&self) -> NewMailingListSubscriber {
        let mut signup: NewMailingListSubscriber = Default::default();

        if self.data.merges.is_some() {
            let merges = self.data.merges.as_ref().unwrap();

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

        signup.date_added = self.fired_at;
        signup.date_optin = self.fired_at;
        signup.date_last_changed = self.fired_at;
        signup.name = format!("{} {}", signup.first_name, signup.last_name);

        signup
    }
}

#[derive(Debug, Clone, JsonSchema, Deserialize, Serialize)]
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

#[derive(Debug, Clone, JsonSchema, Deserialize, Serialize)]
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
    pub groupings: Option<Vec<MailchimpWebhookGrouping>>,
}

#[derive(Debug, Clone, JsonSchema, Deserialize, Serialize)]
pub struct MailchimpWebhookGrouping {
    pub id: String,
    pub unique_id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub groups: Option<String>,
}

#[cfg(test)]
mod tests {
    use crate::mailing_list::MailchimpWebhook;
    use serde_qs::Config as QSConfig;

    #[test]
    fn test_mailchimp_webhook_parsing() {
        let body_str = r#"type=subscribe&fired_at=2020-09-07 21:31:09&data[id]=b748506b63&data[email]=example@gmail.com&data[email_type]=html&data[ip_opt]=98.128.229.135&data[web_id]=404947702&data[merges][EMAIL]=example@gmail.com&data[merges][FNAME]=&data[merges][LNAME]=&data[merges][ADDRESS]=&data[merges][PHONE]=&data[merges][BIRTHDAY]=&data[merges][COMPANY]=&data[merges][INTEREST]=8&data[merges][INTERESTS]=Yes&data[merges][GROUPINGS][0][id]=6197&data[merges][GROUPINGS][0][unique_id]=458a556058&data[merges][GROUPINGS][0][name]=Interested in On the Metal podcast updates?&data[merges][GROUPINGS][0][groups]=Yes&data[merges][GROUPINGS][1][id]=6245&data[merges][GROUPINGS][1][unique_id]=f64af23d78&data[merges][GROUPINGS][1][name]=Interested in the Oxide newsletter?&data[merges][GROUPINGS][1][groups]=Yes&data[merges][GROUPINGS][2][id]=7518&data[merges][GROUPINGS][2][unique_id]=a9829c90a6&data[merges][GROUPINGS][2][name]=Interested in product updates?&data[merges][GROUPINGS][2][groups]=Yes&data[list_id]=8a6d823488"#;
        let body = urlencoding::encode(body_str);

        //let body = "type=subscribe&fired_at=2020-09-07+21%3A31%3A09";
        let qs_non_strict = QSConfig::new(10, false);

        // Parse the request body as a MailchimpWebhook.
        let webhook: MailchimpWebhook = qs_non_strict.deserialize_bytes(body.as_bytes()).unwrap();

        println!("{:?}", webhook);
    }
}
