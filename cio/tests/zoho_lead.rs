use cio_api::rack_line::RackLineSubscriber;

#[tokio::test]
async fn test_pushes_lead() {
    let db = cio_api::db::Database::new().await;
    let company = cio_api::companies::Company::get_from_domain(&db, "oxide.computer")
        .await
        .expect("Failed to find company");

    let lead_id = std::env::var("LEAD_ID").unwrap().parse::<i32>().unwrap();
    let subscriber = RackLineSubscriber::get_by_id(&db, lead_id).await.unwrap();

    println!("{:#?}", subscriber);

    panic!("exit");
}