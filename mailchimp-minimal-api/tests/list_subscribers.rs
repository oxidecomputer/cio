use mailchimp_minimal_api::{AuthMode, MailChimp};

#[ignore]
#[tokio::test]
async fn test_get_subscriber_list() {
    let auth = AuthMode::new_basic_auth(std::env::var("MAILCHIMP_API_KEY").unwrap()).unwrap();
    let client = MailChimp::new(auth);

    let list = client
        .get_subscribers(std::env::var("MAILCHIMP_LIST_ID").unwrap())
        .await
        .unwrap();

    assert!(0 < list.len());
}
