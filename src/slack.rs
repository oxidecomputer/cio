use log::warn;
use reqwest::{Body, Client, StatusCode};
use serde::Serialize;
use serde_json::Value;

/// The Slack app webhook URL for our app to post to the #hiring channel.
pub static HIRING_CHANNEL_POST_URL: &str = "https://hooks.slack.com/services/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ok";
/// The Slack app webhook URL for our app to post to the #public-relations channel.
pub static PUBLIC_RELATIONS_CHANNEL_POST_URL: &str = "https://hooks.slack.com/services/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";

/// Post text to a channel.
pub async fn post_to_channel(url: &str, v: Value) {
    let client = Client::new();
    let resp = client
        .post(url)
        .body(Body::from(v.to_string()))
        .send()
        .await
        .unwrap();

    match resp.status() {
        StatusCode::OK => (),
        s => {
            warn!(
                "posting to applications failed, status: {} | resp: {}",
                s,
                resp.text().await.unwrap()
            );
        }
    };
}

/// A Message to be sent in Slack.
#[derive(Debug, Serialize)]
pub struct Message {
    text: String,
}
