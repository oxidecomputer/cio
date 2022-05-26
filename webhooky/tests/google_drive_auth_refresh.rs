#[tokio::test]
async fn test_google_drive_reauth() {
    let db = cio_api::db::Database::new().await;
    let company = cio_api::companies::Company::get_from_domain(&db, "oxide.computer").await.unwrap();
    let mut drive = company.authenticate_google_drive(&db).await.unwrap();
    drive.set_expires_at(Some(std::time::Instant::now())).await;

    let files = drive.files();

    let test_file = files.get(
        &std::env::var("REAUTH_TEST_FILE_ID").unwrap(),
        false,
        "",
        false,
        false
    ).await.unwrap();

    assert_eq!(std::env::var("REAUTH_TEST_FILE_NAME").unwrap(), test_file.name);
}