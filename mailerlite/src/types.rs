use chrono::{offset::LocalResult, DateTime, NaiveDateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fmt};
use thiserror::Error;

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
    pub subscribed_at: DateTime<Utc>,
    pub unsubscribed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub fields: SubscriberFields,
    pub opted_in_at: Option<DateTime<Utc>>,
    pub optin_ip: Option<String>,
}

impl Subscriber {
    pub fn get_field(&self, field_name: &str) -> Option<&SubscriberFieldValue> {
        self.fields.get(field_name).and_then(|v| v.as_ref())
    }

    pub fn get_field_mut(&mut self, field_name: &str) -> Option<&mut SubscriberFieldValue> {
        self.fields.get_mut(field_name).and_then(|v| v.as_mut())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiSubscriber {
    id: String,
    email: String,
    status: SubscriberStatus,
    source: String,
    sent: u64,
    opens_count: u64,
    clicks_count: u64,
    open_rate: f64,
    click_rate: f64,
    ip_address: Option<String>,
    subscribed_at: FormattedDateTime,
    unsubscribed_at: Option<FormattedDateTime>,
    created_at: FormattedDateTime,
    updated_at: FormattedDateTime,
    fields: SubscriberFields,
    opted_in_at: Option<FormattedDateTime>,
    optin_ip: Option<String>,
}

impl ApiSubscriber {
    pub fn into_subscriber(self, tz: &impl TimeZone) -> Result<Subscriber, FailedToTranslateDateError> {
        Ok(Subscriber {
            id: self.id,
            email: self.email,
            status: self.status,
            source: self.source,
            sent: self.sent,
            opens_count: self.opens_count,
            clicks_count: self.clicks_count,
            open_rate: self.open_rate,
            click_rate: self.click_rate,
            ip_address: self.ip_address,
            subscribed_at: into_utc(self.subscribed_at, tz)?,
            unsubscribed_at: self.unsubscribed_at.map(|t| into_utc(t, tz)).transpose()?,
            created_at: into_utc(self.created_at, tz)?,
            updated_at: into_utc(self.updated_at, tz)?,
            fields: self.fields,
            opted_in_at: self.opted_in_at.map(|t| into_utc(t, tz)).transpose()?,
            optin_ip: self.optin_ip,
        })
    }
}

#[derive(Debug, Clone, Error)]
pub struct FailedToTranslateDateError;

impl fmt::Display for FailedToTranslateDateError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Failed to translate date",)
    }
}

fn into_utc(datetime: FormattedDateTime, from_tz: &impl TimeZone) -> Result<DateTime<Utc>, FailedToTranslateDateError> {
    match datetime.0.and_local_timezone(from_tz.to_owned()) {
        LocalResult::Single(dt) => Ok(dt.with_timezone(&Utc)),
        _ => Err(FailedToTranslateDateError),
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDateTime;
    use chrono_tz::America::Chicago;

    #[test]
    fn test_converts_from_api_datetime_to_utc() {
        let test_date = FormattedDateTime(NaiveDateTime::from_timestamp(1667229189, 0));

        let sub = ApiSubscriber {
            id: "12345".to_string(),
            email: "test@test.org".to_string(),
            status: SubscriberStatus::Active,
            source: "None".to_string(),
            sent: 0,
            opens_count: 0,
            clicks_count: 0,
            open_rate: 0.0,
            click_rate: 0.0,
            ip_address: None,
            subscribed_at: test_date.clone(),
            unsubscribed_at: Some(test_date.clone()),
            created_at: test_date.clone(),
            updated_at: test_date.clone(),
            fields: HashMap::new(),
            opted_in_at: Some(test_date.clone()),
            optin_ip: None,
        };

        let converted = sub.clone().into_subscriber(&Chicago).unwrap();

        // At this point in time the Chicago timezone is five hours behind UTC, so we expect that
        // the timezone post conversion is five hours ahead
        let expected_date = Utc.timestamp(1667229189 + (5 * 60 * 60), 0);

        assert_eq!(converted.subscribed_at, expected_date);
        assert_eq!(converted.unsubscribed_at, Some(expected_date));
        assert_eq!(converted.created_at, expected_date);
        assert_eq!(converted.updated_at, expected_date);
        assert_eq!(converted.opted_in_at, Some(expected_date));
    }
}
