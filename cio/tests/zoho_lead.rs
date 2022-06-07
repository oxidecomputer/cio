use cio_api::{rack_line::RackLineSubscriber, zoho::push_new_rack_line_subscribers_to_zoho};

#[ignore]
#[tokio::test]
async fn test_pushes_lead() {
    let db = cio_api::db::Database::new().await;
    let company = cio_api::companies::Company::get_from_domain(&db, "oxide.computer")
        .await
        .expect("Failed to find company");

    let lead_id = std::env::var("LEAD_ID").unwrap().parse::<i32>().unwrap();
    let subscriber = RackLineSubscriber::get_by_id(&db, lead_id).await.unwrap();
    let mut subscribers = vec![subscriber];

    let push_result = push_new_rack_line_subscribers_to_zoho(&mut subscribers).await;

    assert!(push_result.is_ok());
}
