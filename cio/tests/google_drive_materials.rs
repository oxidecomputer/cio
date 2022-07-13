use google_drive::traits::FileOps;
use std::io::Cursor;

#[ignore]
#[tokio::test]
async fn test_extract_materials_from_zip() {
    let db = cio_api::db::Database::new().await;
    let company = cio_api::companies::Company::get_from_domain(&db, "oxide.computer")
        .await
        .expect("Failed to find company");

    let drive = company.authenticate_google_drive(&db).await.unwrap();
    let contents = drive
        .files()
        .download_by_id(&std::env::var("ZIP_FILE_ID").unwrap())
        .await
        .unwrap();

    // This will fail when an invalid payload is downloaded
    let _archive = zip::ZipArchive::new(Cursor::new(&contents)).unwrap();
}
