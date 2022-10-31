use chrono::Utc;
use mailerlite::{endpoints::ListSegmentSubscribersRequestBuilder, MailerliteClient};

fn client() -> MailerliteClient<Utc> {
    MailerliteClient::new(std::env::var("API_KEY").unwrap(), Utc)
}

#[ignore]
#[tokio::test]
async fn test_segment_subscribers() {
    let _ = client()
        .run(
            ListSegmentSubscribersRequestBuilder::default()
                .segment_id(std::env::var("SEGMENT_ID").unwrap())
                .build()
                .unwrap(),
        )
        .await
        .unwrap();
}
