#[ignore]
#[tokio::test]
async fn test_mark_subscriber() {
    let client = cio_api::mailerlite::Mailerlite::new().unwrap();
    let res = client.mark_mailing_list_subscriber("").await.unwrap();
}

#[tokio::test]
async fn test_mark_batch() {
    let client = cio_api::mailerlite::Mailerlite::new().unwrap();
    let list = client.pending_mailing_list_subscribers().await.unwrap();
    client.mark_mailing_list_subscribers(list).await.unwrap();
}
