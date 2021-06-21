/*!
 * A rust library for interacting with the MailChimp API.
 *
 * For more information, the MailChimp API is documented at [docs.mailchimp.com](https://docs.mailchimp.com/).
 *
 * Example:
 *
 * ```
 * use mailchimp_api::MailChimp;
 * use serde::{Deserialize, Serialize};
 *
 * async fn get_subscribers() {
 *     // Initialize the MailChimp client.
 *     let mailchimp = MailChimp::new_from_env("", "", "");
 *
 *     // Get the subscribers for a mailing list.
 *     let subscribers = mailchimp.get_subscribers("some_id").await.unwrap();
 *
 *     println!("{:?}", subscribers);
 * }
 * ```
 */
use std::collections::HashMap;
use std::env;
use std::error;
use std::fmt;
use std::fmt::Debug;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use reqwest::{header, Client, Method, RequestBuilder, StatusCode, Url};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Entrypoint for interacting with the MailChimp API.
pub struct MailChimp {
    token: String,
    // This expires in 101 days. It is hardcoded in the GitHub Actions secrets,
    // We might want something a bit better like storing it in the database.
    refresh_token: String,
    client_id: String,
    client_secret: String,
    redirect_uri: String,
    endpoint: String,

    client: Arc<Client>,
}

impl MailChimp {
    /// Create a new MailChimp client struct. It takes a type that can convert into
    /// an &str (`String` or `Vec<u8>` for example). As long as the function is
    /// given a valid API key your requests will work.
    pub fn new<I, K, R, T, Q, C>(client_id: I, client_secret: K, redirect_uri: R, token: T, refresh_token: Q, endpoint: C) -> Self
    where
        I: ToString,
        K: ToString,
        R: ToString,
        T: ToString,
        Q: ToString,
        C: ToString,
    {
        let client = Client::builder().build();
        match client {
            Ok(c) => {
                let g = MailChimp {
                    client_id: client_id.to_string(),
                    client_secret: client_secret.to_string(),
                    redirect_uri: redirect_uri.to_string(),
                    token: token.to_string(),
                    refresh_token: refresh_token.to_string(),
                    endpoint: endpoint.to_string(),

                    client: Arc::new(c),
                };

                if g.token.is_empty() || g.refresh_token.is_empty() {
                    // This is super hacky and a work around since there is no way to
                    // auth without using the browser.
                    println!("mailchimp consent URL: {}", g.user_consent_url());
                }
                // We do not refresh the access token since we leave that up to the
                // user to do so they can re-save it to their database.

                g
            }
            Err(e) => panic!("creating client failed: {:?}", e),
        }
    }

    /// Create a new MailChimp client struct from environment variables. It
    /// takes a type that can convert into
    /// an &str (`String` or `Vec<u8>` for example). As long as the function is
    /// given a valid API key and your requests will work.
    /// We pass in the token and refresh token to the client so if you are storing
    /// it in a database, you can get it first.
    pub fn new_from_env<T, R, C>(token: T, refresh_token: R, endpoint: C) -> Self
    where
        T: ToString,
        R: ToString,
        C: ToString,
    {
        let client_id = env::var("MAILCHIMP_CLIENT_ID").unwrap();
        let client_secret = env::var("MAILCHIMP_CLIENT_SECRET").unwrap();
        let redirect_uri = env::var("MAILCHIMP_REDIRECT_URI").unwrap();

        MailChimp::new(client_id, client_secret, redirect_uri, token, refresh_token, endpoint)
    }

    fn request<P>(&self, method: Method, path: P) -> RequestBuilder
    where
        P: ToString,
    {
        // Build the url.
        let base = Url::parse(&self.endpoint).unwrap();
        let mut p = path.to_string();
        // Make sure we have the leading "/".
        if !p.starts_with('/') {
            p = format!("/{}", p);
        }
        let url = base.join(&p).unwrap();

        let bt = format!("Bearer {}", self.token);
        let bearer = header::HeaderValue::from_str(&bt).unwrap();

        // Set the default headers.
        let mut headers = header::HeaderMap::new();
        headers.append(header::AUTHORIZATION, bearer);
        headers.append(header::CONTENT_TYPE, header::HeaderValue::from_static("application/json"));

        self.client.request(method, url).headers(headers)
    }

    pub fn user_consent_url(&self) -> String {
        format!(
            "https://login.mailchimp.com/oauth2/authorize?response_type=code&client_id={}&redirect_uri={}",
            self.client_id, self.redirect_uri
        )
    }

    pub async fn refresh_access_token(&mut self) -> Result<AccessToken, APIError> {
        let mut headers = header::HeaderMap::new();
        headers.append(header::ACCEPT, header::HeaderValue::from_static("application/json"));

        let params = [
            ("grant_type", "refresh_token"),
            ("refresh_token", &self.refresh_token),
            ("client_id", &self.client_id),
            ("client_secret", &self.client_secret),
            ("redirect_uri", &self.redirect_uri),
        ];
        let client = reqwest::Client::new();
        let resp = client.post("https://login.mailchimp.com/oauth2/token").headers(headers).form(&params).send().await.unwrap();

        // Unwrap the response.
        let t: AccessToken = resp.json().await.unwrap();

        self.token = t.access_token.to_string();
        self.refresh_token = t.refresh_token.to_string();

        Ok(t)
    }

    pub async fn get_access_token(&mut self, code: &str) -> Result<AccessToken, APIError> {
        let mut headers = header::HeaderMap::new();
        //headers.append(header::ACCEPT, header::HeaderValue::from_static("application/json"));
        headers.append(header::CONTENT_TYPE, header::HeaderValue::from_static("application/x-www-form-urlencoded"));

        let body = format!(
            "grant_type=authorization_code&client_id={}&client_secret={}&redirect_uri={}&code={}",
            self.client_id,
            self.client_secret,
            urlencoding::encode(&self.redirect_uri),
            code
        );
        println!("mailchimp body {}", body);

        let client = reqwest::Client::new();
        let req = client.post("https://login.mailchimp.com/oauth2/token").headers(headers).body(bytes::Bytes::from(body));
        println!("mailchimp req {:?}", req);
        let resp = req.send().await.unwrap();
        println!("mailchimp resp {}", resp.text().await.unwrap(),);

        // Unwrap the response.
        /*let t: AccessToken = resp.json().await.unwrap();

        self.token = t.access_token.to_string();
        self.refresh_token = t.refresh_token.to_string();*/
        let t: AccessToken = Default::default();

        Ok(t)
    }

    /// Get metadata information.
    pub async fn metadata(&self) -> Result<serde_json::Value, APIError> {
        let mut headers = header::HeaderMap::new();
        headers.append(header::ACCEPT, header::HeaderValue::from_static("application/json"));
        headers.append(header::AUTHORIZATION, header::HeaderValue::from_str(&format!("OAuth {}", self.token)).unwrap());

        // Build the request.
        let client = reqwest::Client::new();
        let resp = client.post("https://login.mailchimp.com/oauth2/metadata").headers(headers).send().await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                })
            }
        };

        // Try to deserialize the response.
        let result: serde_json::Value = resp.json().await.unwrap();

        Ok(result)
    }

    /// Returns a list of subscribers.
    pub async fn get_subscribers(&self, list_id: &str) -> Result<Vec<Member>, APIError> {
        let per_page = 500;
        let mut offset: usize = 0;

        let mut members: Vec<Member> = Default::default();

        let mut has_more_rows = true;

        while has_more_rows {
            // Build the request.
            let rb = self.request(Method::GET, &format!("3.0/lists/{}/members?count={}&offset={}", list_id, per_page, offset,));
            let request = rb.build().unwrap();

            let resp = self.client.execute(request).await.unwrap();
            match resp.status() {
                StatusCode::OK => (),
                s => {
                    return Err(APIError {
                        status_code: s,
                        body: resp.text().await.unwrap(),
                    })
                }
            };

            let mut r: ListMembersResponse = resp.json().await.unwrap();

            has_more_rows = !r.members.is_empty();
            offset += r.members.len();

            members.append(&mut r.members);
        }

        Ok(members)
    }
}

/// Error type returned by our library.
pub struct APIError {
    pub status_code: StatusCode,
    pub body: String,
}

impl fmt::Display for APIError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "APIError: status code -> {}, body -> {}", self.status_code.to_string(), self.body)
    }
}

impl fmt::Debug for APIError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "APIError: status code -> {}, body -> {}", self.status_code.to_string(), self.body)
    }
}

// This is important for other errors to wrap this one.
impl error::Error for APIError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        // Generic error, underlying cause isn't tracked.
        None
    }
}

#[derive(Debug, JsonSchema, Clone, Default, Serialize, Deserialize)]
pub struct AccessToken {
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub access_token: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
    pub token_type: String,
    #[serde(default)]
    pub expires_in: i64,
    #[serde(default)]
    pub x_refresh_token_expires_in: i64,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "deserialize_null_string::deserialize")]
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
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "COMPANY", alias = "CNAME")]
    pub company: String,
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "CSIZE")]
    pub company_size: String,
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "INTEREST")]
    pub interest: String,
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "NOTES")]
    pub notes: String,
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
    #[serde(deserialize_with = "mailchimp_date_format::deserialize", serialize_with = "mailchimp_date_format::serialize")]
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

#[derive(Debug, Clone, JsonSchema, Deserialize, Serialize)]
pub struct Metadata {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub dc: String,
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
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_qs::Config as QSConfig;

    #[test]
    fn test_mailchimp_webhook_parsing() {
        let body = r#"type=subscribe&fired_at=2020-09-07 21:31:09&data[id]=b748506b63&data[email]=example@gmail.com&data[email_type]=html&data[ip_opt]=98.128.229.135&data[web_id]=404947702&data[merges][EMAIL]=example@gmail.com&data[merges][FNAME]=&data[merges][LNAME]=&data[merges][ADDRESS]=&data[merges][PHONE]=&data[merges][BIRTHDAY]=&data[merges][COMPANY]=&data[merges][INTEREST]=8&data[merges][INTERESTS]=Yes&data[merges][GROUPINGS][0][id]=6197&data[merges][GROUPINGS][0][unique_id]=458a556058&data[merges][GROUPINGS][0][name]=Interested in On the Metal podcast updates?&data[merges][GROUPINGS][0][groups]=Yes&data[merges][GROUPINGS][1][id]=6245&data[merges][GROUPINGS][1][unique_id]=f64af23d78&data[merges][GROUPINGS][1][name]=Interested in the Oxide newsletter?&data[merges][GROUPINGS][1][groups]=Yes&data[merges][GROUPINGS][2][id]=7518&data[merges][GROUPINGS][2][unique_id]=a9829c90a6&data[merges][GROUPINGS][2][name]=Interested in product updates?&data[merges][GROUPINGS][2][groups]=Yes&data[list_id]=8a6d823488"#;

        let qs_non_strict = QSConfig::new(10, false);

        // Parse the request body as a MailchimpWebhook.
        let webhook: Webhook = qs_non_strict.deserialize_bytes(body.as_bytes()).unwrap();

        println!("{:#?}", webhook);
    }
}
