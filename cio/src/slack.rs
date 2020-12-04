use std::collections::HashMap;
use std::env;

use chrono::offset::Utc;
use chrono::serde::ts_seconds;
use chrono::DateTime;
use reqwest::{Body, Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The Slack app webhook URL for our app to post to the #hiring channel.
pub fn get_hiring_channel_post_url() -> String {
    env::var("SLACK_HIRING_CHANNEL_POST_URL").unwrap()
}

/// The Slack app webhook URL for our app to post to the #public-relations channel.
pub fn get_public_relations_channel_post_url() -> String {
    env::var("SLACK_PUBLIC_RELATIONS_CHANNEL_POST_URL").unwrap()
}

/// Post text to a channel.
pub async fn post_to_channel(url: String, v: Value) {
    let client = Client::new();
    let resp = client
        .post(&url)
        .body(Body::from(v.to_string()))
        .send()
        .await
        .unwrap();

    match resp.status() {
        StatusCode::OK => (),
        s => {
            println!(
                "posting to slack webhook ({}) failed, status: {} | resp: {}",
                url,
                s,
                resp.text().await.unwrap()
            );
        }
    };
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

#[derive(Clone, Debug, Serialize, Deserialize)]
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
