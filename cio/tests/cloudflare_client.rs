use std::sync::Once;

static INIT: Once = Once::new();

/// Setup function that is only run once, even if called multiple times.
fn setup() {
    INIT.call_once(|| {
        pretty_env_logger::init();
    });
}

#[tokio::test]
async fn test_inner_client_call() {
    setup();

    let db = cio_api::db::Database::new().await;
    let company = cio_api::companies::Company::get_from_domain(&db, "oxide.computer")
        .await
        .expect("Failed to find company");
    let cf = company.authenticate_cloudflare().unwrap();

    let zone_req = cf.get_zone_identifier("oxide.computer").await.unwrap();

    let dns_records = cf
        .request(&cloudflare::endpoints::dns::ListDnsRecords {
            zone_identifier: &zone_req.id,
            params: cloudflare::endpoints::dns::ListDnsRecordsParams {
                // From: https://api.cloudflare.com/#dns-records-for-a-zone-list-dns-records
                per_page: Some(123),
                ..Default::default()
            },
        })
        .await
        .unwrap()
        .result;

    assert_eq!(123, dns_records.len());
}

#[tokio::test]
async fn test_zone_identifier_lookup_uses_cache() {
    setup();

    let db = cio_api::db::Database::new().await;
    let company = cio_api::companies::Company::get_from_domain(&db, "oxide.computer")
        .await
        .expect("Failed to find company");
    let cf = company.authenticate_cloudflare().unwrap();

    let zone_req1 = cf.get_zone_identifier("oxide.computer").await.unwrap();
    let zone_req2 = cf.get_zone_identifier("oxide.computer").await.unwrap();

    assert_eq!(zone_req1.id, zone_req2.id);
    assert_eq!(zone_req1.expires_at, zone_req2.expires_at);
}

#[tokio::test]
async fn test_populates_zone_cache() {
    setup();

    let db = cio_api::db::Database::new().await;
    let company = cio_api::companies::Company::get_from_domain(&db, "oxide.computer")
        .await
        .expect("Failed to find company");
    let cf = company.authenticate_cloudflare().unwrap();

    let zone_req = cf.get_zone_identifier("oxide.computer").await.unwrap();

    assert_eq!(0, cf.cache_size(&zone_req.id));

    cf.populate_zone_cache(&zone_req.id).await.unwrap();

    assert!(cf.cache_size(&zone_req.id) > 0);

    let records_found = cf
        .with_zone(&zone_req.id, |zone| {
            zone.get_records_for_domain("rfd.shared.oxide.computer").len()
        })
        .await
        .unwrap();

    assert_eq!(1, records_found);
}

#[tokio::test]
async fn test_auto_populates_zone_cache() {
    setup();

    let db = cio_api::db::Database::new().await;
    let company = cio_api::companies::Company::get_from_domain(&db, "oxide.computer")
        .await
        .expect("Failed to find company");
    let cf = company.authenticate_cloudflare().unwrap();

    let zone_req = cf.get_zone_identifier("oxide.computer").await.unwrap();

    assert_eq!(0, cf.cache_size(&zone_req.id));

    let records_found = cf
        .with_zone(&zone_req.id, |zone| {
            zone.get_records_for_domain("rfd.shared.oxide.computer").len()
        })
        .await
        .unwrap();

    assert_eq!(1, records_found);

    assert!(cf.cache_size(&zone_req.id) > 0);
}

#[tokio::test]
async fn test_uses_new_cache_after_expiration() {
    setup();

    let db = cio_api::db::Database::new().await;
    let company = cio_api::companies::Company::get_from_domain(&db, "oxide.computer")
        .await
        .expect("Failed to find company");
    let mut cf = company.authenticate_cloudflare().unwrap();
    cf.set_dns_cache_ttl(5);

    let zone_req = cf.get_zone_identifier("oxide.computer").await.unwrap();

    let records_found = cf
        .with_zone(&zone_req.id, |zone| {
            zone.get_records_for_domain("rfd.shared.oxide.computer").len()
        })
        .await
        .unwrap();

    assert_eq!(1, records_found);

    std::thread::sleep(std::time::Duration::from_secs(10));

    let zone_req = cf.get_zone_identifier("oxide.computer").await.unwrap();

    let records_found = cf
        .with_zone(&zone_req.id, |zone| {
            zone.get_records_for_domain("rfd.shared.oxide.computer").len()
        })
        .await
        .unwrap();

    assert_eq!(1, records_found);
}
