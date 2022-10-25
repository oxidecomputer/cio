use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscriber {
    pub id: String,
    pub email: String,
    pub status: SubscriberStatus,
    pub source: String,
    pub sent: u64,
    pub opens_count: u64,
    pub clicks_count: u64,
    pub open_rate: f64,
    pub click_rate: f64,
    pub ip_address: Option<String>,
    pub subscribed_at: FormattedDateTime,
    pub unsubscribed_at: Option<FormattedDateTime>,
    pub created_at: FormattedDateTime,
    pub updated_at: FormattedDateTime,
    pub fields: SubscriberFields,
    pub opted_in_at: Option<FormattedDateTime>,
    pub optin_ip: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SubscriberStatus {
    Active,
    Bounced,
    Junk,
    Unconfirmed,
    Unsubscribed,
}

pub type SubscriberFields = HashMap<String, Option<SubscriberFieldValue>>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SubscriberFieldValue {
    String(String),
    Number(i64),
    Date(FormattedDateTime),
}

impl From<String> for SubscriberFieldValue {
    fn from(value: String) -> SubscriberFieldValue {
        SubscriberFieldValue::String(value)
    }
}

impl From<i64> for SubscriberFieldValue {
    fn from(value: i64) -> SubscriberFieldValue {
        SubscriberFieldValue::Number(value)
    }
}

impl From<FormattedDateTime> for SubscriberFieldValue {
    fn from(value: FormattedDateTime) -> SubscriberFieldValue {
        SubscriberFieldValue::Date(value)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FormattedDateTime(pub NaiveDateTime);

impl From<NaiveDateTime> for FormattedDateTime {
    fn from(naive: NaiveDateTime) -> Self {
        Self(naive)
    }
}