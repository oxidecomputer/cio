use std::collections::HashMap;
use std::env;

use airtable_api::{Airtable, Record, User as AirtableUser};
use chrono::naive::NaiveDate;
use chrono::offset::Utc;
use chrono::DateTime;
use chrono_humanize::HumanTime;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub static BASE_ID_CUSTOMER_LEADS: &str = "appr7imQLcR3pWaNa";
static MAILING_LIST_SIGNUPS_TABLE: &str = "Mailing List Signups";

/// The Airtable fields type for Customer Interactions.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CustomerInteractionFields {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Company")]
    pub company: Vec<String>,
    #[serde(with = "meeting_date_format", rename = "Date")]
    pub date: NaiveDate,
    #[serde(rename = "Type")]
    pub meeting_type: String,
    #[serde(rename = "Phase")]
    pub phase: String,
    #[serde(rename = "People")]
    pub people: Vec<String>,
    #[serde(rename = "Oxide Folks")]
    pub oxide_folks: Vec<AirtableUser>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "Link to Notes")]
    pub notes_link: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "Notes")]
    pub notes: Option<String>,
}

/// The Airtable fields type for discussion topics.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DiscussionFields {
    #[serde(rename = "Topic")]
    pub topic: String,
    #[serde(rename = "Submitter")]
    pub submitter: AirtableUser,
    #[serde(rename = "Priority")]
    pub priority: String,
    #[serde(skip_serializing_if = "Option::is_none", rename = "Notes")]
    pub notes: Option<String>,
    // Never modify this, it is a linked record.
    #[serde(rename = "Associated meetings")]
    pub associated_meetings: Vec<String>,
}

/// The Airtable fields type for a mailing list signup.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MailingListSignupFields {
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

impl MailingListSignupFields {
    pub fn new(params: HashMap<String, String>) -> Self {
        let email = if let Some(e) = params.get("data[email]") {
            e.trim().to_string()
        } else {
            "".to_string()
        };
        let first_name = if let Some(f) = params.get("data[merges][FNAME]") {
            f.trim().to_string()
        } else {
            "".to_string()
        };
        let last_name = if let Some(l) = params.get("data[merges][LNAME]") {
            l.trim().to_string()
        } else {
            "".to_string()
        };
        let company = if let Some(c) = params.get("data[merges][COMPANY]") {
            c.trim().to_string()
        } else {
            "".to_string()
        };
        let interest = if let Some(i) = params.get("data[merges][INTEREST]") {
            i.trim().to_string()
        } else {
            "".to_string()
        };

        let wants_podcast_updates =
            params.get("data[merges][GROUPINGS][0][groups]").is_some();
        let wants_newsletter =
            params.get("data[merges][GROUPINGS][1][groups]").is_some();
        let wants_product_updates =
            params.get("data[merges][GROUPINGS][2][groups]").is_some();

        let time: DateTime<Utc> = if let Some(f) = params.get("fired_at") {
            DateTime::parse_from_str(
                &(f.to_owned() + " +00:00"),
                "%Y-%m-%d %H:%M:%S  %:z",
            )
            .unwrap()
            .with_timezone(&Utc)
        } else {
            println!(
                "could not parse mailchimp date time so defaulting to now"
            );

            Utc::now()
        };

        MailingListSignupFields {
            email,
            first_name,
            last_name,
            company,
            interest,
            wants_podcast_updates,
            wants_newsletter,
            wants_product_updates,
            date_added: time,
            optin_date: time,
            last_changed: time,
        }
    }

    pub async fn push_to_airtable(&self) {
        let api_key = env::var("AIRTABLE_API_KEY").unwrap();
        // Initialize the Airtable client.
        let airtable =
            Airtable::new(api_key.to_string(), BASE_ID_CUSTOMER_LEADS);

        // Create the record.
        let record = Record {
            id: None,
            created_time: None,
            fields: serde_json::to_value(self).unwrap(),
        };

        // Send the new record to the airtable client.
        // Batch can only handle 10 at a time.
        airtable
            .create_records(MAILING_LIST_SIGNUPS_TABLE, vec![record])
            .await
            .unwrap();

        println!("created mailing list record in airtable: {:?}", self);
    }

    pub fn as_slack_msg(&self) -> Value {
        let dur = self.date_added - Utc::now();
        let time = HumanTime::from(dur);

        let mut msg = format!(
            "*{} {}* <mailto:{}|{}>",
            self.first_name, self.last_name, self.email, self.email
        );
        if !self.interest.is_empty() {
            msg += &format!("\n>{}", self.interest.trim());
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

        json!({
            "attachments": [
                {
                    "color": "#F6E05E",
                    "blocks": [
                        {
                            "type": "section",
                            "text": {
                                "type": "mrkdwn",
                                "text": msg
                            }
                        },
                        {
                            "type": "context",
                            "elements": [{
                                "type": "mrkdwn",
                                "text": updates
                            }]
                        },
                        {
                            "type": "context",
                            "elements": [{
                                "type": "mrkdwn",
                                "text": context
                            }]
                        }
                    ]
                }
            ]
        })
    }
}

/// The Airtable fields type for meetings.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MeetingFields {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(with = "meeting_date_format", rename = "Date")]
    pub date: NaiveDate,
    #[serde(rename = "Week")]
    pub week: String,
    #[serde(skip_serializing_if = "Option::is_none", rename = "Notes")]
    pub notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "Action items")]
    pub action_items: Option<String>,
    // Never modify this, it is a linked record.
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "Proposed discussion"
    )]
    pub proposed_discussion: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "Recording")]
    pub recording: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "Attendees")]
    pub attendees: Option<Vec<AirtableUser>>,
}

/// Convert the date format `%Y-%m-%d` to a NaiveDate.
mod meeting_date_format {
    use chrono::naive::NaiveDate;
    use serde::{self, Deserialize, Deserializer, Serializer};

    const FORMAT: &str = "%Y-%m-%d";

    // The signature of a serialize_with function must follow the pattern:
    //
    //    fn serialize<S>(&T, S) -> Result<S::Ok, S::Error>
    //    where
    //        S: Serializer
    //
    // although it may also be generic over the input types T.
    pub fn serialize<S>(
        date: &NaiveDate,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
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
    pub fn deserialize<'de, D>(deserializer: D) -> Result<NaiveDate, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;

        Ok(NaiveDate::parse_from_str(&s, FORMAT).unwrap())
    }
}

/// The data type for sending reminders for the product huddle meetings.
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct ProductEmailData {
    pub date: String,
    pub topics: Vec<DiscussionFields>,
    pub last_meeting_reports_link: String,
    pub meeting_id: String,
    pub should_send: bool,
}
