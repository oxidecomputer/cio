use meilisearch_minimal_api::{MeiliClient, SearchQuery};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RfdId {
    #[serde(rename = "objectID")]
    object_id: String,
    rfd_number: u32,
}

#[tokio::test]
async fn test_search() {
    let client = MeiliClient::new(
        "https://search.rfd.shared.oxide.computer".to_string(),
        "586f8fb6ab373bb7cef698eab38e5a853eafe77db5da1ce63908c8ea228a40ca".to_string(),
    );

    let index = client.index("rfd".to_string());

    let mut search = SearchQuery::default();
    search.filter = Some(vec![format!("rfd_number = {}", 328)]);
    let documents = index.search::<RfdId>(search).await;

    panic!("{:#?}", documents);
}
