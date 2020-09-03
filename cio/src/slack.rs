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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_type: Option<MessageResponseType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocks: Option<Vec<MessageBlock>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachments: Option<Vec<MessageAttachment>>,
}

/// A Slack message block.
///
/// Docs: https://api.slack.com/messaging/composing/layouts#adding-blocks
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct MessageBlock {
    #[serde(rename = "type")]
    pub block_type: MessageBlockType,
    pub text: MessageBlockText,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accessory: Option<MessageBlockAccessory>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<Vec<MessageBlockText>>,
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
    #[serde(rename = "mkdwn")]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocks: Option<Vec<MessageBlock>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_link: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<Vec<MessageAttachmentField>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub footer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub footer_icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pretext: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_link: Option<String>,
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
