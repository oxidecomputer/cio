use std::time::{Duration, Instant};

// #[ignore]
#[tokio::test]
async fn test_google_drive_reauth() {
    let db = cio_api::db::Database::new().await;
    let company = cio_api::companies::Company::get_from_domain(&db, "oxide.computer")
        .await
        .unwrap();
    let mut drive = company.authenticate_google_drive(&db).await.unwrap();

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

    assert_eq!(std::env::var("REAUTH_TEST_FILE_NAME").unwrap(), test_file.name);

    // Check that the expiration has not changed (we expect that a refresh has not occurred)
    assert!(expires_at == drive.expires_at().await.unwrap());

    // Force set an expiration time in the past so that the next file read requires a refresh
    drive.set_expires_at(Some(Instant::now())).await;

    // Check that the access for this drive is expired
    assert!(drive.is_expired().await.unwrap());

    let files = drive.files();

    let test_file = files
        .get(&std::env::var("REAUTH_TEST_FILE_ID").unwrap(), false, "", false, false)
        .await
        .unwrap();

    // Check that the token has been refreshed and that access is not longer expired
    assert!(!drive.is_expired().await.unwrap());

    assert!(Instant::now() < drive.expires_at().await.unwrap());

    panic!("print");
}

// https://drive.google.com/file/d/1tOTrmYOvxP3vepj00s2W0vCH36n1_h4s/view?usp=sharing

// https://drive.google.com/file/d/1tOTrmYOvxP3vepj00s2W0vCH36n1_h4s/view?usp=sharing