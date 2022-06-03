#[tokio::test]
async fn test_get_enterprise_user() {
    let db = cio_api::db::Database::new().await;
    let company = cio_api::companies::Company::get_from_domain(&db, "oxide.computer")
        .await
        .expect("Failed to find company");

    let airtable = company.authenticate_airtable("");

    let user = airtable
        .get_enterprise_user(&std::env::var("TEST_EMAIL").unwrap())
        .await
        .unwrap();

    assert_eq!(&std::env::var("TEST_EMAIL").unwrap(), &user.email);
}
