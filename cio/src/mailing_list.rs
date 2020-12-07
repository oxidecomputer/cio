use std::collections::HashMap;
use std::env;

use crate::db::Database;

use chrono::offset::Utc;
use chrono::DateTime;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::models::NewMailingListSubscriber;

/// Returns the response from the Mailchimp API with the list of subscribers.
pub async fn get_all_mailchimp_subscribers() -> Vec<MailchimpMember> {
    let client = reqwest::Client::new();
    let per_page = 500;
    let mut offset = 0;

    let mut members: Vec<MailchimpMember> = Default::default();

    let mut has_more_rows = true;
    while has_more_rows {
        let resp = client
            .get(&format!(
                "https://us20.api.mailchimp.com/3.0/lists/{}/members?count={}&offset={}",
                env::var("MAILCHIMP_LIST_ID").unwrap_or_default(),
                per_page,
                offset,
            ))
            .basic_auth("any_string", Some(env::var("MAILCHIMP_API_KEY").unwrap_or_default()))
            .send()
            .await
            .unwrap();

        let mut r: MailchimpListMembersResponse = resp.json().await.unwrap();

        has_more_rows = !r.members.is_empty();
        offset += r.members.len();

        members.append(&mut r.members);
    }

    members
}

// Sync the mailing_list_subscribers from Mailchimp with our database.
pub async fn refresh_db_mailing_list_subscribers() {
    // Initialize our database.
    let db = Database::new();

    let members = get_all_mailchimp_subscribers().await;

    // Sync subscribers.
    for member in members {
        db.upsert_mailing_list_subscriber(&member.into());
    }
}

/// The data type for the response to Mailchimp's API for listing members
/// of a mailing list.
///
/// FROM: https://mailchimp.com/developer/api/marketing/list-members/list-members-info/
#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct MailchimpListMembersResponse {
    /// An array of objects, each representing a specific list member.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub members: Vec<MailchimpMember>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub list_id: String,
    #[serde(default)]
    pub total_items: i64,
}

/// The data type for a member of a  Mailchimp mailing list.
///
/// FROM: https://mailchimp.com/developer/api/marketing/list-members/get-member-info/
#[derive(Debug, Clone, JsonSchema, Deserialize, Serialize)]
pub struct MailchimpMember {
    /// The MD5 hash of the lowercase version of the list member's email address.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    /// Email address for a subscriber.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email_address: String,
    /// An identifier for the address across all of Mailchimp.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub unique_email_id: String,
    /// The ID used in the Mailchimp web application.
    /// View this member in your Mailchimp account at:
    ///     https://{dc}.admin.mailchimp.com/lists/members/view?id={web_id}.
    #[serde(default)]
    pub web_id: i64,
    /// Type of email this member asked to get ('html' or 'text').
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email_type: String,
    /// Subscriber's current status.
    /// Possible values:
    ///     "subscribed", "unsubscribed", "cleaned", "pending", "transactional", or "archived".
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub status: String,
    /// A subscriber's reason for unsubscribing.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub unsubscribe_reason: String,
    /// An individual merge var and value for a member.
    #[serde(default)]
    pub merge_fields: MailchimpMergeFields,
    /// The key of this object's properties is the ID of the interest in question.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub interests: HashMap<String, bool>,
    /// IP address the subscriber signed up from.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ip_signup: String,
    /*/// The date and time the subscriber signed up for the list in ISO 8601 format.
    #[serde(default)]
    pub timestamp_signup: Option<DateTime<Utc>>,*/
    /// The IP address the subscriber used to confirm their opt-in status.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ip_opt: String,
    /// The date and time the subscribe confirmed their opt-in status in ISO 8601 format.
    //#[serde(alias = "timestamp_signup")]
    pub timestamp_opt: DateTime<Utc>,
    /// Star rating for this member, between 1 and 5.
    #[serde(default)]
    pub star_rating: i32,
    /// The date and time the member's info was last changed in ISO 8601 format.
    pub last_changed: DateTime<Utc>,
    /// If set/detected, the subscriber's language.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub language: String,
    /// VIP status for subscriber.
    #[serde(default)]
    pub vip_status: bool,
    /// The list member's email client.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email_client: String,
    /// Subscriber location information.
    #[serde(default)]
    pub location: MailchimpLocation,
    /// The marketing permissions for the subscriber.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub marketing_permissions: Vec<MailchimpMarketingPermissions>,
    /// The most recent Note added about this member.
    #[serde(default)]
    pub last_note: MailchimpLastNote,
    /// The source from which the subscriber was added to this list.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
    /// The number of tags applied to this member.
    /// Returns up to 50 tags applied to this member. To retrieve all tags see Member Tags.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<MailchimpTag>,
    /// The list id.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub list_id: String,
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
        }
    }
}

#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct MailchimpMergeFields {
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "FNAME")]
    pub first_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "LNAME")]
    pub last_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "COMPANY")]
    pub company: String,
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "INTEREST")]
    pub interest: String,
}

#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct MailchimpLocation {
    /// The location latitude.
    #[serde(default)]
    pub latitude: f64,
    /// The location longitude.
    #[serde(default)]
    pub longitude: f64,
    /// The time difference in hours from GMT.
    #[serde(default)]
    pub gmtoff: i32,
    /// The offset for timezones where daylight saving time is observed.
    #[serde(default)]
    pub dstoff: i32,
    /// The unique code for the location country.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub country_code: String,
    /// The timezone for the location.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub time_zone: String,
}

#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct MailchimpMarketingPermissions {
    /// The id for the marketing permission on the list.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub marketing_permission_id: String,
    /// The text of the marketing permission.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    /// If the subscriber has opted-in to the marketing permission.
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct MailchimpLastNote {
    /// The note id.
    #[serde(default)]
    pub note_id: i64,
    /// The date and time the note was created in ISO 8601 format.
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    /// The author of the note.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub created_by: String,
    /// The content of the note.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct MailchimpTag {
    /// The tag id.
    #[serde(default)]
    pub id: i64,
    /// The name of the tag.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
}

/// The data type for the webhook from Mailchimp.
///
/// FROM: https://mailchimp.com/developer/guides/sync-audience-data-with-webhooks/#handling-the-webhook-response-in-your-application
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
    use crate::db::Database;
    use crate::mailing_list::{refresh_db_mailing_list_subscribers, MailchimpWebhook};
    use crate::models::MailingListSubscribers;

    use serde_qs::Config as QSConfig;

    #[test]
    fn test_mailchimp_webhook_parsing() {
        let body = r#"type=subscribe&fired_at=2020-09-07 21:31:09&data[id]=b748506b63&data[email]=example@gmail.com&data[email_type]=html&data[ip_opt]=98.128.229.135&data[web_id]=404947702&data[merges][EMAIL]=example@gmail.com&data[merges][FNAME]=&data[merges][LNAME]=&data[merges][ADDRESS]=&data[merges][PHONE]=&data[merges][BIRTHDAY]=&data[merges][COMPANY]=&data[merges][INTEREST]=8&data[merges][INTERESTS]=Yes&data[merges][GROUPINGS][0][id]=6197&data[merges][GROUPINGS][0][unique_id]=458a556058&data[merges][GROUPINGS][0][name]=Interested in On the Metal podcast updates?&data[merges][GROUPINGS][0][groups]=Yes&data[merges][GROUPINGS][1][id]=6245&data[merges][GROUPINGS][1][unique_id]=f64af23d78&data[merges][GROUPINGS][1][name]=Interested in the Oxide newsletter?&data[merges][GROUPINGS][1][groups]=Yes&data[merges][GROUPINGS][2][id]=7518&data[merges][GROUPINGS][2][unique_id]=a9829c90a6&data[merges][GROUPINGS][2][name]=Interested in product updates?&data[merges][GROUPINGS][2][groups]=Yes&data[list_id]=8a6d823488"#;

        let qs_non_strict = QSConfig::new(10, false);

        // Parse the request body as a MailchimpWebhook.
        let webhook: MailchimpWebhook = qs_non_strict.deserialize_bytes(body.as_bytes()).unwrap();

        println!("{:#?}", webhook);
    }

    #[ignore]
    #[tokio::test(threaded_scheduler)]
    async fn test_cron_mailing_list_subscribers_refresh_db() {
        refresh_db_mailing_list_subscribers().await;
    }

    #[ignore]
    #[tokio::test(threaded_scheduler)]
    async fn test_cron_mailing_list_subscribers_airtable() {
        // Initialize our database.
        let db = Database::new();

        let mailing_list_subscribers = db.get_mailing_list_subscribers();
        // Update the mailing list subscribers in airtable.
        MailingListSubscribers(mailing_list_subscribers).update_airtable().await;
    }
}
