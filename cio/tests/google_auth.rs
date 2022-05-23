#[tokio::test]
async fn test_admin() {
    let db = cio_api::db::Database::new().await;
    let company = cio_api::companies::Company::get_from_domain(&db, "oxide.computer").await.expect("Failed to find company");

    assert!(company.authenticate_google_admin(&db).await.is_ok());
}

#[tokio::test]
async fn test_calendar() {
    let db = cio_api::db::Database::new().await;
    let company = cio_api::companies::Company::get_from_domain(&db, "oxide.computer").await.expect("Failed to find company");

    assert!(company.authenticate_google_calendar(&db).await.is_ok());
}

#[tokio::test]
async fn test_calendar_service_account() {
    let db = cio_api::db::Database::new().await;
    let company = cio_api::companies::Company::get_from_domain(&db, "oxide.computer").await.expect("Failed to find company");

    assert!(company.authenticate_google_calendar_with_service_account("").await.is_ok());
}

#[tokio::test]
async fn test_drive() {
    let db = cio_api::db::Database::new().await;
    let company = cio_api::companies::Company::get_from_domain(&db, "oxide.computer").await.expect("Failed to find company");

    assert!(company.authenticate_google_drive(&db).await.is_ok());
}

#[tokio::test]
async fn test_drive_service_account() {
    let db = cio_api::db::Database::new().await;
    let company = cio_api::companies::Company::get_from_domain(&db, "oxide.computer").await.expect("Failed to find company");

    assert!(company.authenticate_google_drive_with_service_account("").await.is_ok());
}

#[tokio::test]
async fn test_group_settings() {
    let db = cio_api::db::Database::new().await;
    let company = cio_api::companies::Company::get_from_domain(&db, "oxide.computer").await.expect("Failed to find company");

    assert!(company.authenticate_google_groups_settings(&db).await.is_ok());
}

#[tokio::test]
async fn test_sheets() {
    let db = cio_api::db::Database::new().await;
    let company = cio_api::companies::Company::get_from_domain(&db, "oxide.computer").await.expect("Failed to find company");

    assert!(company.authenticate_google_sheets(&db).await.is_ok());
}