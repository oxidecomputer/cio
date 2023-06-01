use std::time::Instant;

#[ignore]
#[tokio::test]
async fn test_google_drive_reauth() {
    let db = cio_api::db::Database::new().await;
    let company = cio_api::companies::Company::get_from_domain(&db, "oxide.computer")
        .await
        .unwrap();
    let drive = company.authenticate_google_drive(&db).await.unwrap();

    // Get the initial expiration assigned to the token
    let mut expires_at = drive.expires_at().await.unwrap();

    // Wait for one second
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    // Refresh the access token, getting a new token and expiration time
    drive.refresh_access_token().await.unwrap();

    // Assert that the new expiration time is further in the future than the
    // previous expiration time
    assert!(expires_at < drive.expires_at().await.unwrap());

    expires_at = drive.expires_at().await.unwrap();

    // Check that we can still access a file
    let files = drive.files();

    let test_file = files
        .get(&std::env::var("REAUTH_TEST_FILE_ID").unwrap(), false, "", false, false)
        .await
        .unwrap();

    assert_eq!(std::env::var("REAUTH_TEST_FILE_NAME").unwrap(), test_file.body.name);

    // Check that the expiration has not changed (we expect that a refresh has not occurred)
    assert!(expires_at == drive.expires_at().await.unwrap());

    // Force set an expiration time in the past so that the next file read requires a refresh
    drive.set_expires_at(Some(Instant::now())).await;

    // Check that the access for this drive is expired
    assert!(drive.is_expired().await.unwrap());

    let files = drive.files();

    let _test_file = files
        .get(&std::env::var("REAUTH_TEST_FILE_ID").unwrap(), false, "", false, false)
        .await
        .unwrap();

    // Check that the token has been refreshed and that access is not longer expired
    assert!(!drive.is_expired().await.unwrap());

    assert!(Instant::now() < drive.expires_at().await.unwrap());
}

#[ignore]
#[tokio::test]
async fn test_google_drive_reauth_invalid_expires_in() {
    let db = cio_api::db::Database::new().await;
    let company = cio_api::companies::Company::get_from_domain(&db, "oxide.computer")
        .await
        .unwrap();
    let drive = company.authenticate_google_drive(&db).await.unwrap();

    // Refresh the access token, getting a new token and expiration time
    drive.refresh_access_token().await.unwrap();

    drive.set_expires_in(-2000).await;

    let expires_at = drive.expires_at().await;

    assert!(expires_at.is_some());

    // If a negative expires in is set, then the expiration time should
    // be at least as old as now
    assert!(expires_at.unwrap() <= Instant::now());
}
