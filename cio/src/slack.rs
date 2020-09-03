use std::env;

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
