use log::warn;
use reqwest::{Client, StatusCode};
use serde::Serialize;

/// The Slack app webhook URL for our app to post to the #hiring channel.
pub static HIRING_CHANNEL_POST_URL: &str = "https://hooks.slack.com/services/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ok";
/// The Slack app webhook URL for our app to post to the #public-relations channel.
pub static PUBLIC_RELATIONS_CHANNEL_POST_URL: &str = "https://hooks.slack.com/services/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";

/// Post text to a channel.
pub async fn post_to_channel(url: &str, text: &str) {
    let client = Client::new();
    let resp = client
        .post(url)
        .json(&Message {
            text: text.to_string(),
        })
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
