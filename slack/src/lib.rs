/*!
 * A rust library for interacting with the Slack API.
 *
 * For more information, the Slack API is documented at [api.slack.com](https://api.slack.com).
 *
 * Example:
 *
 * ```
 * use serde::{Deserialize, Serialize};
 * use slack_chat_api::Slack;
 *
 * async fn get_users() {
 *     // Initialize the Slack client.
 *     let slack = Slack::new_from_env();
 *
 *     // List the users.
 *     let users = slack.list_users().await.unwrap();
 *
 *     // Iterate over the users.
 *     for user in users {
 *         println!("{:?}", user);
 *     }
 * }
 * ```
 */
#![allow(clippy::field_reassign_with_default)]
use std::collections::HashMap;
use std::env;
use std::error;
use std::fmt;
use std::fmt::Debug;
use std::sync::Arc;

use chrono::offset::Utc;
use chrono::serde::ts_seconds;
use chrono::DateTime;
use reqwest::{header, Client, Method, Request, StatusCode, Url};
use serde::{Deserialize, Serialize};

/// Endpoint for the Slack API.
const ENDPOINT: &str = "https://slack.com/api/";

/// Entrypoint for interacting with the Slack API.
pub struct Slack {
    token: String,
    workspace_id: String,

    client: Arc<Client>,
}

impl Slack {
    /// Create a new Slack client struct. It takes a type that can convert into
    /// an &str (`String` or `Vec<u8>` for example). As long as the function is
    /// given a valid API Token and Workspace ID your requests will work.
    pub fn new<K, B>(token: K, workspace_id: B) -> Self
    where
        K: ToString,
        B: ToString,
    {
        let client = Client::builder().build();
        match client {
            Ok(c) => Self {
                token: token.to_string(),
                workspace_id: workspace_id.to_string(),

                client: Arc::new(c),
            },
            Err(e) => panic!("creating client failed: {:?}", e),
        }
    }

    /// Create a new Slack client struct from environment variables. It
    /// takes a type that can convert into
    /// an &str (`String` or `Vec<u8>` for example). As long as the function is
    /// given a valid API Token and Workspace ID your requests will work.
    pub fn new_from_env() -> Self {
        let token = env::var("SLACK_TOKEN").unwrap();
        let workspace_id = env::var("SLACK_WORKSPACE_ID").unwrap();

        Slack::new(token, workspace_id)
    }

    fn request<B>(&self, method: Method, path: &str, body: B, query: Option<Vec<(&str, String)>>) -> Request
    where
        B: Serialize,
    {
        let base = Url::parse(ENDPOINT).unwrap();
        let url = base.join(path).unwrap();

        let bt = format!("Bearer {}", self.token);
        let bearer = header::HeaderValue::from_str(&bt).unwrap();

        // Set the default headers.
        let mut headers = header::HeaderMap::new();
        headers.append(header::AUTHORIZATION, bearer);
        headers.append(header::CONTENT_TYPE, header::HeaderValue::from_static("application/json"));

        let mut rb = self.client.request(method.clone(), url).headers(headers);

        match query {
            None => (),
            Some(val) => {
                rb = rb.query(&val);
            }
        }

        // Add the body, this is to ensure our GET and DELETE calls succeed.
        if method != Method::GET && method != Method::DELETE {
            rb = rb.json(&body);
        }

        // Build the request.
        rb.build().unwrap()
    }

    /// List users on a workspace.
    /// FROM: https://api.slack.com/methods/admin.users.list
    pub async fn list_users(&self) -> Result<Vec<User>, APIError> {
        // Build the request.
        // TODO: paginate.
        let mut body: HashMap<&str, &str> = HashMap::new();
        body.insert("team_id", &self.workspace_id);
        let request = self.request(Method::POST, "admin.users.list", body, Some(vec![("limit", "100".to_string())]));

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

        let r: APIResponse = resp.json().await.unwrap();

        Ok(r.users)
    }

    /// Invite a user to a workspace.
    /// FROM: https://api.slack.com/methods/admin.users.invite
    pub async fn invite_user(&self, invite: UserInvite) -> Result<(), APIError> {
        // Build the request.
        let request = self.request(Method::POST, "admin.users.invite", invite, None);

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

        Ok(())
    }

    /// Remove users from a workspace.
    /// FROM: https://api.slack.com/methods/admin.users.remove
    pub async fn remove_user(&self, user_id: &str) -> Result<(), APIError> {
        // Build the request.
        let mut body: HashMap<&str, &str> = HashMap::new();
        body.insert("team_id", &self.workspace_id);
        body.insert("user_id", user_id);
        let request = self.request(Method::POST, "admin.users.remove", body, None);

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

        Ok(())
    }

    /// Set a user's profile information, including custom status.
    /// FROM: https://api.slack.com/methods/users.profile.set
    pub async fn update_user_profile(&self, user_id: &str, profile: UserProfile) -> Result<(), APIError> {
        // Build the request.
        let request = self.request(Method::POST, "users.profile.set", UpdateUserProfileRequest { user: user_id.to_string(), profile }, None);

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

        Ok(())
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

/// A message to be sent in Slack.
///
/// Docs: https://api.slack.com/interactivity/slash-commands#responding_to_commands
#[derive(Debug, Deserialize, Serialize)]
pub struct MessageResponse {
    pub response_type: MessageResponseType,
    pub text: String,
}

/// A message response type in Slack.
///
/// The `response_type` parameter in the JSON payload controls this visibility,
/// by default it is set to `ephemeral`, but you can specify a value of
/// `in_channel` to post the response into the channel
#[derive(Debug, Deserialize, Serialize)]
pub enum MessageResponseType {
    #[serde(rename = "ephemeral")]
    Ephemeral,
    #[serde(rename = "in_channel")]
    InChannel,
}

impl Default for MessageResponseType {
    fn default() -> Self {
        // This is the default in Slack.
        MessageResponseType::Ephemeral
    }
}

/// A bot command to be run and sent back to Slack.
///
/// Docs: https://api.slack.com/interactivity/slash-commands#app_command_handling
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct BotCommand {
    pub user_name: String,
    pub command: String,
    pub text: String,
    pub api_app_id: String,
    pub response_url: String,
    pub trigger_id: String,
    pub channel_name: String,
    pub team_domain: String,
    pub team_id: String,
    pub token: String,
    pub channel_id: String,
    pub user_id: String,
}

/// A formatted message to send to Slack.
///
/// Docs: https://api.slack.com/messaging/composing/layouts
#[derive(Debug, Deserialize, Serialize)]
pub struct FormattedMessage {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub channel: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<MessageBlock>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<MessageAttachment>,
}

/// A Slack message block.
///
/// Docs: https://api.slack.com/messaging/composing/layouts#adding-blocks
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct MessageBlock {
    #[serde(rename = "type")]
    pub block_type: MessageBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<MessageBlockText>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub elements: Vec<MessageBlockText>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub block_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accessory: Option<MessageBlockAccessory>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<MessageBlockText>,
}

/// A message block type in Slack.
#[derive(Debug, Deserialize, Serialize)]
pub enum MessageBlockType {
    #[serde(rename = "section")]
    Section,
    #[serde(rename = "context")]
    Context,
    #[serde(rename = "divider")]
    Divider,
}

impl Default for MessageBlockType {
    fn default() -> Self {
        MessageBlockType::Section
    }
}

/// Message block text in Slack.
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct MessageBlockText {
    #[serde(rename = "type")]
    pub text_type: MessageType,
    pub text: String,
}

/// Message type in Slack.
#[derive(Debug, Deserialize, Serialize)]
pub enum MessageType {
    #[serde(rename = "mrkdwn")]
    Markdown,
    #[serde(rename = "image")]
    Image,
}

impl Default for MessageType {
    fn default() -> Self {
        MessageType::Markdown
    }
}

/// Message block accessory in Slack.
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct MessageBlockAccessory {
    #[serde(rename = "type")]
    pub accessory_type: MessageType,
    pub image_url: String,
    pub alt_text: String,
}

/// A message attachment in Slack.
///
/// Docs: https://api.slack.com/messaging/composing/layouts#building-attachments
#[derive(Debug, Deserialize, Serialize)]
pub struct MessageAttachment {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<MessageBlock>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub author_icon: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub author_link: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub author_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub color: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub fallback: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<MessageAttachmentField>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub footer: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub footer_icon: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub image_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pretext: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub thumb_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title_link: String,
    #[serde(with = "ts_seconds")]
    pub ts: DateTime<Utc>,
}

/// A message attachment field in Slack.
#[derive(Debug, Deserialize, Serialize)]
pub struct MessageAttachmentField {
    pub short: bool,
    pub title: String,
    pub value: String,
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct UserProfile {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub avatar_hash: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub display_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub display_name_normalized: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email: String,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub fields: HashMap<String, UserProfileFields>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub first_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub guest_channels: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub image_192: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub image_24: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub image_32: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub image_48: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub image_512: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub image_72: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub image_original: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub last_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub phone: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub real_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub real_name_normalized: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub skype: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub status_emoji: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub status_text: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub team: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserProfileFields {
    pub alt: String,
    pub label: String,
    pub value: String,
}

/// The data type for an invited user.
/// FROM: https://api.slack.com/methods/admin.users.invite
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UserInvite {
    /// A comma-separated list of channel_ids for this user to join. At least one channel is required.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub channel_ids: Vec<String>,
    /// The email address of the person to invite.
    pub email: String,
    /// The ID of the workspace.
    pub team_id: String,
    /// An optional message to send to the user in the invite email.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub custom_message: String,
    /// Is this user a multi-channel guest user? (default: false)
    pub is_restricted: bool,
    /// Is this user a single channel guest user? (default: false)
    pub is_ultra_restricted: bool,
    /// Full name of the user.
    pub real_name: String,
    /// Allow this invite to be resent in the future if a user has not signed up yet. (default: false)
    pub resend: bool,
}

/// The data type for an API response.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct APIResponse {
    pub ok: bool,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub users: Vec<User>,
}

/// The data type for a User.
/// FROM: https://api.slack.com/types/user
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub team_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email: String,
    #[serde(default)]
    pub is_admin: bool,
    #[serde(default)]
    pub is_owner: bool,
    #[serde(default)]
    pub is_primary_owner: bool,
    #[serde(default)]
    pub is_restricted: bool,
    #[serde(default)]
    pub is_ultra_restricted: bool,
    #[serde(default)]
    pub is_bot: bool,
    #[serde(default)]
    pub deleted: bool,
    #[serde(default)]
    pub is_stranger: bool,
    #[serde(default)]
    pub is_app_user: bool,
    #[serde(default)]
    pub is_invited_user: bool,
    #[serde(default)]
    pub has_2fa: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub real_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tz: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tz_label: String,
    #[serde(default)]
    pub tz_offset: i64,
    #[serde(default)]
    profile: UserProfile,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub locale: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UpdateUserProfileRequest {
    pub user: String,
    pub profile: UserProfile,
}
