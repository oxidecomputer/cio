#[ignore]
#[tokio::test]
async fn test_mark_subscriber() {
    let client = cio_api::mailerlite::Mailerlite::new().unwrap();
    let res = client.mark_mailing_list_subscriber("").await.unwrap();
}

#[ignore]
#[tokio::test]
async fn test_mark_batch() {
    let client = cio_api::mailerlite::Mailerlite::new().unwrap();
    let list = client.pending_mailing_list_subscribers().await.unwrap();

    let batch = list.chunks(10).next().unwrap();

    let res = client.mark_mailing_list_subscribers(batch.to_vec()).await.unwrap();
}
