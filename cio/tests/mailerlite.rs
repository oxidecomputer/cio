#[ignore]
#[tokio::test]
async fn test_update_mailerlite_user() {
    let client = cio_api::mailerlite::Mailerlite::new().unwrap();

    let response = client
        .mark_mailing_list_subscriber(&std::env::var("TEST_EMAIL").unwrap())
        .await;
}
