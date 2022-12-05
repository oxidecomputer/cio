#[ignore]
#[tokio::test]
async fn test_get_pending_mailing_list_users() {
    let client = cio_api::mailerlite::Mailerlite::new().unwrap();

    let subscribers = client.pending_mailing_list_subscribers().await.unwrap();

    println!("{}", subscribers.len());

    for subscriber in &subscribers[..1] {
        println!("{:?}", subscriber.email);
        // client.mark_mailing_list_subscriber(&subscriber.email).await.unwrap();
    }

    panic!("exit");
}

#[tokio::test]
async fn test_update_mailerlite_user() {
    let client = cio_api::mailerlite::Mailerlite::new().unwrap();

    let response = client
        .mark_mailing_list_subscriber(&std::env::var("TEST_EMAIL").unwrap())
        .await;

    println!("{:#?}", response);

    response.unwrap();
}
