use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, JsonSchema, Clone, Default, Serialize, Deserialize)]
pub struct AccessToken {
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "deserialize_null_string::deserialize"
    )]
    pub access_token: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "deserialize_null_string::deserialize"
    )]
    pub token_type: String,
    #[serde(default)]
    pub expires_in: i64,
    #[serde(default)]
    pub x_refresh_token_expires_in: i64,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "deserialize_null_string::deserialize"
    )]
    pub refresh_token: String,
}

pub mod deserialize_null_string {
    use serde::{self, Deserialize, Deserializer};

    // The signature of a deserialize_with function must follow the pattern:
    //
    //    fn deserialize<'de, D>(D) -> Result<T, D::Error>
    //    where
    //        D: Deserializer<'de>
    //
    // although it may also be generic over the output types T.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<String, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer).unwrap_or_default();

        Ok(s)
    }
}

#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct MergeFields {
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "FNAME")]
    pub first_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "LNAME")]
    pub last_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "NAME")]
    pub name: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        alias = "COMPANY",
        alias = "CNAME"
    )]
    pub company: String,
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "CSIZE")]
    pub company_size: String,
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "INTEREST")]
    pub interest: String,
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "NOTES")]
    pub notes: String,
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "BIRTHDAY")]
    pub birthday: String,
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "PHONE")]
    pub phone: String,
    #[serde(default, alias = "ADDRESS")]
    pub address: serde_json::Value,
}

#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct Address {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub addr1: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub addr2: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub city: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub state: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub zip: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub country: String,
}

#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct Location {
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
pub struct MarketingPermissions {
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
pub struct LastNote {
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
pub struct Tag {
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
pub struct Webhook {
    #[serde(rename = "type")]
    pub webhook_type: String,
    #[serde(
        deserialize_with = "mailchimp_date_format::deserialize",
        serialize_with = "mailchimp_date_format::serialize"
    )]
    pub fired_at: DateTime<Utc>,
    pub data: WebhookData,
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

#[derive(Debug, Clone, JsonSchema, Deserialize, Serialize)]
pub struct WebhookData {
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
    pub merges: Option<WebhookMerges>,
}

#[derive(Debug, Clone, JsonSchema, Deserialize, Serialize)]
pub struct WebhookMerges {
    #[serde(skip_serializing_if = "Option::is_none", rename = "FNAME")]
    pub first_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "LNAME")]
    pub last_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "NAME")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "EMAIL")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "ADDRESS")]
    pub address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "PHONE")]
    pub phone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", alias = "COMPANY", alias = "CNAME")]
    pub company: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", alias = "CSIZE")]
    pub company_size: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "INTEREST")]
    pub interest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "NOTES")]
    pub notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "BIRTHDAY")]
    pub birthday: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "GROUPINGS")]
    pub groupings: Option<Vec<WebhookGrouping>>,
}

#[derive(Debug, Clone, JsonSchema, Deserialize, Serialize)]
pub struct WebhookGrouping {
    pub id: String,
    pub unique_id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub groups: Option<String>,
}

#[derive(Debug, Default, Clone, JsonSchema, Deserialize, Serialize)]
pub struct Metadata {
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "deserialize_null_string::deserialize"
    )]
    pub dc: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "deserialize_null_string::deserialize"
    )]
    pub accountname: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "deserialize_null_string::deserialize"
    )]
    pub api_endpoint: String,
    #[serde(default)]
    pub login: Login,
}

#[derive(Debug, Default, Clone, JsonSchema, Deserialize, Serialize)]
pub struct Login {
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "deserialize_null_string::deserialize"
    )]
    pub avatar: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "deserialize_null_string::deserialize"
    )]
    pub email: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "deserialize_null_string::deserialize"
    )]
    pub login_email: String,
    #[serde(default)]
    pub login_id: i64,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "deserialize_null_string::deserialize"
    )]
    pub login_name: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "deserialize_null_string::deserialize"
    )]
    pub login_url: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "deserialize_null_string::deserialize"
    )]
    pub role: String,
    #[serde(default)]
    pub user_id: i64,
}

/// The data type for the response to Mailchimp's API for listing members
/// of a mailing list.
///
/// FROM: https://mailchimp.com/developer/api/marketing/list-members/list-members-info/
#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct ListMembersResponse {
    /// An array of objects, each representing a specific list member.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub members: Vec<Member>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub list_id: String,
    #[serde(default)]
    pub total_items: i64,
}

/// The data type for a member of a  Mailchimp mailing list.
///
/// FROM: https://mailchimp.com/developer/api/marketing/list-members/get-member-info/
#[derive(Debug, Clone, JsonSchema, Deserialize, Serialize)]
pub struct Member {
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
    pub merge_fields: MergeFields,
    /// The key of this object's properties is the ID of the interest in question.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub interests: HashMap<String, bool>,
    /// IP address the subscriber signed up from.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ip_signup: String,
    /// The date and time the subscriber signed up for the list in ISO 8601 format.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub timestamp_signup: String,
    /// The IP address the subscriber used to confirm their opt-in status.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ip_opt: String,
    /// The date and time the subscribe confirmed their opt-in status in ISO 8601 format.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub timestamp_opt: String,
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
    pub location: Location,
    /// The marketing permissions for the subscriber.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub marketing_permissions: Vec<MarketingPermissions>,
    /// The most recent Note added about this member.
    #[serde(default)]
    pub last_note: LastNote,
    /// The source from which the subscriber was added to this list.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
    /// The number of tags applied to this member.
    /// Returns up to 50 tags applied to this member. To retrieve all tags see Member Tags.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<Tag>,
    /// The list id.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub list_id: String,
    #[serde(default)]
    pub stats: Stats,
}

#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct Stats {
    #[serde(default)]
    pub avg_open_rate: f32,
    #[serde(default)]
    pub avg_click_rate: f32,
    #[serde(default)]
    pub ecommerce_data: EcommerceData,
}

#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct EcommerceData {
    #[serde(default)]
    pub total_revenue: f32,
    #[serde(default)]
    pub number_of_orders: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub currency_code: String,
}
