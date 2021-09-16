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
 *     let slack = Slack::new_from_env("", "", "");
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
#![allow(clippy::nonstandard_macro_braces)]
use std::{collections::HashMap, env, sync::Arc};

use anyhow::{bail, Result};
use async_recursion::async_recursion;
use reqwest::{header, Body, Client, Method, Request, StatusCode, Url};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Endpoint for the Slack API.
const ENDPOINT: &str = "https://slack.com/api/";

/// Entrypoint for interacting with the Slack API.
pub struct Slack {
    token: String,
    // This expires in 101 days. It is hardcoded in the GitHub Actions secrets,
    // We might want something a bit better like storing it in the database.
    user_token: String,
    client_id: String,
    client_secret: String,
    redirect_uri: String,
    workspace_id: String,

    client: Arc<Client>,
}

impl Slack {
    /// Create a new Slack client struct. It takes a type that can convert into
    /// an &str (`String` or `Vec<u8>` for example). As long as the function is
    /// given a valid API Token and Workspace ID your requests will work.
    pub fn new<I, K, B, R, T, Q>(
        client_id: I,
        client_secret: K,
        workspace_id: B,
        redirect_uri: R,
        token: T,
        user_token: Q,
    ) -> Self
    where
        I: ToString,
        K: ToString,
        B: ToString,
        R: ToString,
        T: ToString,
        Q: ToString,
    {
        let client = Client::builder().build();
        match client {
            Ok(c) => {
                let s = Slack {
                    client_id: client_id.to_string(),
                    client_secret: client_secret.to_string(),
                    workspace_id: workspace_id.to_string(),
                    redirect_uri: redirect_uri.to_string(),
                    token: token.to_string(),
                    user_token: user_token.to_string(),

                    client: Arc::new(c),
                };

                if s.token.is_empty() || s.user_token.is_empty() {
                    // This is super hacky and a work around since there is no way to
                    // auth without using the browser.
                    println!("slack consent URL: {}", s.user_consent_url());
                }
                // We do not refresh the access token since we leave that up to the
                // user to do so they can re-save it to their database.

                s
            }
            Err(e) => panic!("creating client failed: {:?}", e),
        }
    }

    /// Create a new Slack client struct from environment variables. It
    /// takes a type that can convert into
    /// an &str (`String` or `Vec<u8>` for example). As long as the function is
    /// given a valid API Token and Workspace ID your requests will work.
    pub fn new_from_env<C, T, R>(workspace_id: C, token: T, user_token: R) -> Self
    where
        C: ToString,
        T: ToString,
        R: ToString,
    {
        let client_id = env::var("SLACK_CLIENT_ID").expect("SLACK_CLIENT_ID should be set");
        let client_secret = env::var("SLACK_CLIENT_SECRET").expect("SLACK_CLIENT_SECRET should be set");
        let redirect_uri = env::var("SLACK_REDIRECT_URI").expect("SLACK_REDIRECT_URI should be set");

        Slack::new(client_id, client_secret, workspace_id, redirect_uri, token, user_token)
    }

    fn request<B>(
        &self,
        token: &str,
        method: Method,
        path: &str,
        body: B,
        query: Option<Vec<(&str, String)>>,
    ) -> Result<Request>
    where
        B: Serialize,
    {
        let base = Url::parse(ENDPOINT)?;
        let url = base.join(path)?;

        let bt = format!("Bearer {}", token);
        let bearer = header::HeaderValue::from_str(&bt)?;

        // Set the default headers.
        let mut headers = header::HeaderMap::new();
        headers.append(header::AUTHORIZATION, bearer);
        headers.append(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/json"),
        );

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
        Ok(rb.build()?)
    }

    pub fn user_consent_url(&self) -> String {
        let state = uuid::Uuid::new_v4();
        format!(
            "https://slack.com/oauth/v2/authorize?scope={}&client_id={}&user_scope={}&redirect_uri={}&state={}",
            "commands,incoming-webhook,team:read,users:read,users:read.email,users.profile:read,channels:read,chat:write,channels:join",
            self.client_id,
            "identity.basic,identity.email",
            self.redirect_uri,
            state
        )
    }

    pub async fn get_access_token(&mut self, code: &str) -> Result<AccessToken> {
        let mut headers = header::HeaderMap::new();
        headers.append(header::ACCEPT, header::HeaderValue::from_static("application/json"));

        let params = [
            ("client_id", self.client_id.to_string()),
            ("client_secret", self.client_secret.to_string()),
            ("code", code.to_string()),
            ("redirect_uri", self.redirect_uri.to_string()),
        ];
        let client = reqwest::Client::new();
        let resp = client
            .post("https://slack.com/api/oauth.v2.access")
            .basic_auth(&self.client_id, Some(&self.client_secret))
            .headers(headers)
            .form(&params)
            .send()
            .await?;

        // Unwrap the response.
        let t: AccessToken = resp.json().await?;

        self.token = t.access_token.to_string();
        if let Some(ref user) = t.authed_user {
            self.user_token = user.access_token.to_string();
        }

        Ok(t)
    }

    /// List users on a workspace.
    /// FROM: https://api.slack.com/methods/admin.users.list
    pub async fn list_users(&self) -> Result<Vec<User>> {
        // Build the request.
        // TODO: paginate.
        let request = self.request(
            &self.token,
            Method::GET,
            "users.list",
            (),
            Some(vec![("limit", "100".to_string())]),
        )?;

        let resp = self.client.execute(request).await?;
        match resp.status() {
            StatusCode::OK => (),
            s => {
                bail!("status code: {}, body: {}", s, resp.text().await?);
            }
        };

        let r: APIResponse = resp.json().await?;

        Ok(r.users)
    }

    /// Get the current user's identity.
    /// FROM: https://api.slack.com/methods/users.identity
    pub async fn current_user(&self) -> Result<CurrentUser> {
        // Build the request.
        let request = self.request(&self.user_token, Method::GET, "users.identity", (), None)?;

        let resp = self.client.execute(request).await?;
        match resp.status() {
            StatusCode::OK => (),
            s => {
                bail!("status code: {}, body: {}", s, resp.text().await?);
            }
        };

        let r: CurrentUserResponse = resp.json().await?;
        Ok(r.user)
    }

    /// Get billable info.
    /// FROM: https://api.slack.com/methods/team.billableInfo
    pub async fn billable_info(&self) -> Result<HashMap<String, BillableInfo>> {
        // Build the request.
        // TODO: paginate.
        let request = self.request(&self.user_token, Method::GET, "team.billableInfo", (), None)?;

        let resp = self.client.execute(request).await?;
        match resp.status() {
            StatusCode::OK => (),
            s => {
                bail!("status code: {}, body: {}", s, resp.text().await?);
            }
        };

        let r: BillableInfoResponse = resp.json().await?;
        Ok(r.billable_info)
    }

    /// Invite a user to a workspace.
    /// FROM: https://api.slack.com/methods/admin.users.invite
    pub async fn invite_user(&self, invite: UserInvite) -> Result<()> {
        // Build the request.
        let request = self.request(&self.user_token, Method::POST, "admin.users.invite", invite, None)?;

        let resp = self.client.execute(request).await?;
        match resp.status() {
            StatusCode::OK => (),
            s => {
                bail!("status code: {}, body: {}", s, resp.text().await?);
            }
        };

        Ok(())
    }

    /// Join a channel.
    /// FROM: https://api.slack.com/methods/conversations.join
    pub async fn join_channel(&self, channel: &str) -> Result<Channel> {
        let mut body: HashMap<&str, &str> = HashMap::new();
        body.insert("channel", channel);

        let request = self.request(&self.token, Method::POST, "conversations.join", body, None)?;

        let resp = self.client.execute(request).await?;
        match resp.status() {
            StatusCode::OK => (),
            s => {
                bail!("status code: {}, body: {}", s, resp.text().await?);
            }
        };

        let f: JoinChannelResponse = resp.json().await?;

        if !f.ok {
            bail!(
                "status code: {}, body: {}",
                StatusCode::OK,
                serde_json::json!(f).to_string()
            );
        }

        Ok(f.channel)
    }

    /// Post message to a channel.
    /// FROM: https://api.slack.com/methods/chat.postMessage
    #[async_recursion]
    pub async fn post_message(&self, body: &FormattedMessage) -> Result<FormattedMessageResponse> {
        let request = self.request(&self.token, Method::POST, "chat.postMessage", body, None)?;

        let resp = self.client.execute(request).await?;
        match resp.status() {
            StatusCode::OK => (),
            s => {
                bail!("status code: {}, body: {}", s, resp.text().await?);
            }
        };

        let f: FormattedMessageResponse = resp.json().await?;

        if !f.ok {
            if f.error.contains("not_in_channel") {
                // Join the channel and try again.
                return self.post_message(body).await;
            }
            bail!(
                "status code: {}, body: {}",
                StatusCode::OK,
                serde_json::json!(f).to_string()
            );
        }

        Ok(f)
    }

    /// Remove users from a workspace.
    /// FROM: https://api.slack.com/methods/admin.users.remove
    pub async fn remove_user(&self, user_id: &str) -> Result<()> {
        // Build the request.
        let mut body: HashMap<&str, &str> = HashMap::new();
        body.insert("team_id", &self.workspace_id);
        body.insert("user_id", user_id);
        let request = self.request(&self.user_token, Method::POST, "admin.users.remove", body, None)?;

        let resp = self.client.execute(request).await?;
        match resp.status() {
            StatusCode::OK => (),
            s => {
                bail!("status code: {}, body: {}", s, resp.text().await?);
            }
        };

        Ok(())
    }

    /// Set a user's profile information, including custom status.
    /// FROM: https://api.slack.com/methods/users.profile.set
    pub async fn update_user_profile(&self, user_id: &str, profile: UserProfile) -> Result<()> {
        // Build the request.
        let request = self.request(
            &self.user_token,
            Method::POST,
            "users.profile.set",
            UpdateUserProfileRequest {
                user: user_id.to_string(),
                profile,
            },
            None,
        )?;

        let resp = self.client.execute(request).await?;
        match resp.status() {
            StatusCode::OK => (),
            s => {
                bail!("status code: {}, body: {}", s, resp.text().await?);
            }
        };

        Ok(())
    }

    /// Post text to a channel.
    pub async fn post_to_channel(url: &str, v: &Value) -> Result<()> {
        let client = Client::new();
        let resp = client.post(url).body(Body::from(v.to_string())).send().await?;

        match resp.status() {
            StatusCode::OK => (),
            s => {
                bail!("status code: {}, body: {}", s, resp.text().await?);
            }
        };

        Ok(())
    }
}

/// A message to be sent in Slack.
///
/// Docs: https://api.slack.com/interactivity/slash-commands#responding_to_commands
#[derive(Debug, Deserialize, JsonSchema, Serialize)]
pub struct MessageResponse {
    pub response_type: MessageResponseType,
    pub text: String,
}

/// A message response type in Slack.
///
/// The `response_type` parameter in the JSON payload controls this visibility,
/// by default it is set to `ephemeral`, but you can specify a value of
/// `in_channel` to post the response into the channel
#[derive(Debug, Deserialize, JsonSchema, Serialize)]
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
#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct BotCommand {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub user_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub command: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub api_app_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub response_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub trigger_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub channel_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub team_domain: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub team_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub token: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub channel_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub user_id: String,
}

/// A formatted message to send to Slack.
///
/// Docs: https://api.slack.com/messaging/composing/layouts
#[derive(Debug, Clone, Deserialize, JsonSchema, Serialize)]
pub struct FormattedMessage {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub channel: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<MessageBlock>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<MessageAttachment>,
}

/// A formatted message response from Slack.
///
/// Docs: https://api.slack.com/methods/chat.postMessage
#[derive(Debug, Clone, Deserialize, JsonSchema, Serialize)]
pub struct FormattedMessageResponse {
    #[serde(default)]
    pub ok: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub channel: String,
    #[serde(default)]
    pub message: serde_json::Value,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub error: String,
}

/// A channel join response.
#[derive(Debug, Clone, Deserialize, JsonSchema, Serialize)]
pub struct JoinChannelResponse {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub channel: Channel,
    #[serde(default)]
    pub response_metadata: serde_json::Value,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub error: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub warning: String,
}

/// A channel.
#[derive(Debug, Clone, Default, Deserialize, JsonSchema, Serialize)]
pub struct Channel {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub accepted_user: String,
    #[serde(default)]
    pub created: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub creator: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    #[serde(default)]
    pub is_archived: bool,
    #[serde(default)]
    pub is_channel: bool,
    #[serde(default)]
    pub is_general: bool,
    #[serde(default)]
    pub is_member: bool,
    #[serde(default)]
    pub is_moved: i32,
    #[serde(default)]
    pub is_mpim: bool,
    #[serde(default)]
    pub is_org_shared: bool,
    #[serde(default)]
    pub is_pending_ext_shared: bool,
    #[serde(default)]
    pub is_private: bool,
    #[serde(default)]
    pub is_read_only: bool,
    #[serde(default)]
    pub is_shared: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub last_read: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub members: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name_normalized: String,
    #[serde(default)]
    pub num_members: i32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default)]
    pub priority: f32,
    #[serde(default)]
    pub purpose: ChannelValue,
    #[serde(default)]
    pub topic: ChannelValue,
    #[serde(default)]
    pub unlinked: i32,
    #[serde(default)]
    pub unread_count: i32,
    #[serde(default)]
    pub unread_count_display: i32,
}

#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct ChannelValue {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub creator: String,
    #[serde(default)]
    pub last_set: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub value: String,
}

/// A Slack message block.
///
/// Docs: https://api.slack.com/messaging/composing/layouts#adding-blocks
#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct MessageBlock {
    #[serde(rename = "type")]
    pub block_type: MessageBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<MessageBlockText>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub elements: Vec<BlockOption>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub block_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accessory: Option<MessageBlockAccessory>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<MessageBlockText>,
}

/// A message block type in Slack.
#[derive(Debug, Clone, Deserialize, JsonSchema, Serialize)]
pub enum MessageBlockType {
    #[serde(rename = "header")]
    Header,
    #[serde(rename = "section")]
    Section,
    #[serde(rename = "context")]
    Context,
    #[serde(rename = "divider")]
    Divider,
    #[serde(rename = "actions")]
    Actions,
}

impl Default for MessageBlockType {
    fn default() -> Self {
        MessageBlockType::Section
    }
}

/// Action block in Slack.
#[derive(Debug, Clone, JsonSchema, Deserialize, Serialize)]
#[serde(untagged)]
pub enum BlockOption {
    MessageBlockText(MessageBlockText),
    ActionBlock(ActionBlock),
}

/// Message block text in Slack.
#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct MessageBlockText {
    #[serde(rename = "type")]
    pub text_type: MessageType,
    pub text: String,
}

/// Action block in Slack.
#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct ActionBlock {
    #[serde(rename = "type")]
    pub text_type: MessageType,
    pub text: MessageBlockText,

    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub value: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub action_id: String,
}

/// Message type in Slack.
#[derive(Debug, Clone, Deserialize, JsonSchema, Serialize)]
pub enum MessageType {
    #[serde(rename = "plain_text")]
    PlainText,
    #[serde(rename = "mrkdwn")]
    Markdown,
    #[serde(rename = "image")]
    Image,
    #[serde(rename = "button")]
    Button,
}

impl Default for MessageType {
    fn default() -> Self {
        MessageType::Markdown
    }
}

/// Message block accessory in Slack.
#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct MessageBlockAccessory {
    #[serde(rename = "type")]
    pub accessory_type: MessageType,

    /// These are for an image.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub image_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub alt_text: String,

    /// These are for a button.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<MessageBlockText>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub value: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub action_id: String,
}

/// A message attachment in Slack.
///
/// Docs: https://api.slack.com/messaging/composing/layouts#building-attachments
#[derive(Debug, Clone, Deserialize, JsonSchema, Serialize)]
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
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ts: String,
}

/// A message attachment field in Slack.
#[derive(Debug, Clone, Deserialize, JsonSchema, Serialize)]
pub struct MessageAttachmentField {
    pub short: bool,
    pub title: String,
    pub value: String,
}

#[derive(Clone, Default, Debug, JsonSchema, Serialize, Deserialize)]
pub struct UserProfile {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub avatar_hash: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub display_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub display_name_normalized: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fields: Option<HashMap<String, UserProfileFields>>,
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

#[derive(Clone, Debug, Serialize, JsonSchema, Deserialize)]
pub struct UserProfileFields {
    pub alt: String,
    pub label: String,
    pub value: String,
}

/// The data type for an invited user.
/// FROM: https://api.slack.com/methods/admin.users.invite
#[derive(Clone, Debug, Default, JsonSchema, Serialize, Deserialize)]
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
#[derive(Clone, Debug, Default, JsonSchema, Serialize, Deserialize)]
pub struct APIResponse {
    pub ok: bool,

    #[serde(default, skip_serializing_if = "Vec::is_empty", alias = "members")]
    pub users: Vec<User>,
}

/// The data type for a User.
/// FROM: https://api.slack.com/types/user
#[derive(Clone, Debug, Default, JsonSchema, Serialize, Deserialize)]
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
    pub profile: UserProfile,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub locale: String,
}

#[derive(Clone, Debug, Default, JsonSchema, Serialize, Deserialize)]
pub struct UpdateUserProfileRequest {
    pub user: String,
    pub profile: UserProfile,
}

#[derive(Clone, Debug, Default, JsonSchema, Serialize, Deserialize)]
pub struct BillableInfoResponse {
    #[serde(default)]
    pub ok: bool,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub billable_info: HashMap<String, BillableInfo>,
}

#[derive(Clone, Debug, Default, JsonSchema, Serialize, Deserialize)]
pub struct BillableInfo {
    #[serde(default)]
    pub billing_active: bool,
}

#[derive(Debug, JsonSchema, Clone, Default, Serialize, Deserialize)]
pub struct AccessToken {
    #[serde(default)]
    pub ok: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub access_token: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub token_type: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub scope: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub bot_user_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub app_id: String,
    #[serde(default)]
    pub team: Team,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enterprise: Option<Enterprise>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authed_user: Option<AuthedUser>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub incoming_webhook: Option<IncomingWebhook>,
}

#[derive(Debug, JsonSchema, Clone, Default, Serialize, Deserialize)]
pub struct Team {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
}

#[derive(Debug, JsonSchema, Clone, Default, Serialize, Deserialize)]
pub struct IncomingWebhook {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub channel: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub channel_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub configuration_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
}

#[derive(Debug, JsonSchema, Clone, Default, Serialize, Deserialize)]
pub struct Enterprise {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
}

#[derive(Debug, JsonSchema, Clone, Default, Serialize, Deserialize)]
pub struct AuthedUser {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub access_token: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub token_type: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub scope: String,
}

#[derive(Debug, JsonSchema, Clone, Default, Serialize, Deserialize)]
pub struct CurrentUserResponse {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub user: CurrentUser,
    #[serde(default)]
    pub team: Team,
}

#[derive(Debug, JsonSchema, Clone, Default, Serialize, Deserialize)]
pub struct CurrentUser {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email: String,
}
