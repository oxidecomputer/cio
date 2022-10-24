use chrono::NaiveDateTime;
use serde::{de, Deserialize, Serialize};
use std::{collections::HashMap, fmt};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscriber {
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
    #[serde(deserialize_with = "deserialize_naivedatetime")]
    subscribed_at: NaiveDateTime,
    #[serde(deserialize_with = "deserialize_optional_naivedatetime")]
    unsubscribed_at: Option<NaiveDateTime>,
    #[serde(deserialize_with = "deserialize_naivedatetime")]
    created_at: NaiveDateTime,
    #[serde(deserialize_with = "deserialize_naivedatetime")]
    updated_at: NaiveDateTime,
    fields: SubscriberFields,
    #[serde(deserialize_with = "deserialize_optional_naivedatetime")]
    opted_in_at: Option<NaiveDateTime>,
    optin_ip: Option<String>,
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
    #[serde(deserialize_with = "deserialize_naivedatetime")]
    Date(NaiveDateTime),
}

struct OptionalNaiveDateTimeVisitor;

impl<'de> de::Visitor<'de> for OptionalNaiveDateTimeVisitor {
    type Value = Option<NaiveDateTime>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a string containing json data")
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(None)
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        NaiveDateTime::parse_from_str(&value, "%Y-%m-%d %H:%M:%S")
            .map(Some)
            .map_err(E::custom)
    }
}

fn deserialize_optional_naivedatetime<'de, D>(deserializer: D) -> Result<Option<NaiveDateTime>, D::Error>
where
    D: de::Deserializer<'de>,
{
    deserializer.deserialize_any(OptionalNaiveDateTimeVisitor)
}

fn deserialize_naivedatetime<'de, D>(deserializer: D) -> Result<NaiveDateTime, D::Error>
where
    D: de::Deserializer<'de>,
{
    match deserializer.deserialize_any(OptionalNaiveDateTimeVisitor) {
        Ok(None) => Err(de::Error::custom("Expected a custom formatted datetime but found null")),
        Ok(Some(datetime)) => Ok(datetime),
        Err(err) => Err(err),
    }
}
