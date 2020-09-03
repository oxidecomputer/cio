use std::env;

use reqwest::{Body, Client, StatusCode};
use serde::Serialize;
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
#[derive(Debug, Serialize)]
pub struct Message {
    text: String,
}
