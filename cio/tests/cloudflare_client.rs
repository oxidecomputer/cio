#[tokio::test]
async fn test_zone_identifier_lookup_uses_cache() {
    let db = cio_api::db::Database::new().await;
    let company = cio_api::companies::Company::get_from_domain(&db, "oxide.computer")
        .await
        .expect("Failed to find company");

    let cf = company.authenticate_cloudflare().unwrap();

    let zone_req1 = cf.get_zone_identifier("oxide.computer").await.unwrap();
    let zone_req2 = cf.get_zone_identifier("oxide.computer").await.unwrap();

    assert_eq!(zone_req1.id, zone_req2.id);
    assert_eq!(zone_req1.expires_at, zone_req2.expires_at);

    let dns_records = cf
        .request(&cloudflare::endpoints::dns::ListDnsRecords {
            zone_identifier: &zone_req1.id,
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
