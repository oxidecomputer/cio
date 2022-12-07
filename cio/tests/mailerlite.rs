#[ignore]
#[tokio::test]
async fn test_mark_subscriber() {
    let client = cio_api::mailerlite::Mailerlite::new().unwrap();
    let res = client.mark_mailing_list_subscriber("").await.unwrap();
}
