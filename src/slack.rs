use log::warn;
use reqwest::{Client, StatusCode};
use serde::Serialize;

/// The Slack app webhook URL for our app to post to the #applications channel.
static APPLICATIONS_CHANNEL_POST_URL: &str = "https://hooks.slack.com/services/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ok";

/// Post text to the #applications channel.
pub async fn post_to_applications(text: &str) {
    let client = Client::new();
    let resp = client
        .post(APPLICATIONS_CHANNEL_POST_URL)
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
            return;
        }
    };
}

/// A Message to be sent in Slack.
#[derive(Debug, Serialize)]
pub struct Message {
    text: String,
}
