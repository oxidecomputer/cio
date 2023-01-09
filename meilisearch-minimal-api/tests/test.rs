use meilisearch_minimal_api::{IndexClient, MeiliClient, SearchQuery};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub struct TestDB {
    pub client: MeiliClient,
    pub index_name: String,
}

impl TestDB {
    pub fn new(test_name: &str) -> Option<Self> {
        if let (Ok(db), Ok(key)) = (std::env::var("TEST_MEILI_DB"), std::env::var("TEST_MEILI_KEY")) {
            let client = MeiliClient::new(db, key);
            let index_name = test_name.to_string() + &Uuid::new_v4().to_string();

            Some(Self { client, index_name })
        } else {
            None
        }
    }

    pub fn index_client(&self) -> IndexClient {
        self.client.index(self.index_name.clone())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Document {
    #[serde(rename = "objectID")]
    object_id: String,
}

#[tokio::test]
async fn test_search() {
    if let Some(client) = TestDB::new("test_search_and_index") {
        let index = client.index_client();

        let empty_search = SearchQuery::default();
        let empty_results = index.search::<Document>(empty_search).await;

        assert!(empty_results.is_ok());
        assert_eq!(empty_results.unwrap().hits.len(), 0);

        let document_1 = Document {
            object_id: "doc1".to_string(),
        };

        let document_2 = Document {
            object_id: "doc2".to_string(),
        };

        let index_result = index.index_documents(&[document_1, document_2], "objectID").await;

        assert!(index_result.is_ok());

        // Indexing is asynchronous and can be slow
        std::thread::sleep(std::time::Duration::from_secs(1));

        let mut doc1_search = SearchQuery::default();
        doc1_search.query = Some("doc1".to_string());
        let doc1_result = index.search::<Document>(doc1_search).await;

        assert!(doc1_result.is_ok());
        assert_eq!(doc1_result.as_ref().unwrap().hits.len(), 1);
        assert_eq!(&doc1_result.unwrap().hits[0].object_id, "doc1");

        let mut doc_search = SearchQuery::default();
        doc_search.query = Some("doc".to_string());
        let doc_result = index.search::<Document>(doc_search.clone()).await;

        assert!(doc_result.is_ok());
        assert_eq!(doc_result.as_ref().unwrap().hits.len(), 2);

        let delete_result = index.delete_documents(&["doc1".to_string(), "doc2".to_string()]).await;
        assert!(delete_result.is_ok());

        // Deleting is asynchronous and can be slow
        std::thread::sleep(std::time::Duration::from_secs(1));

        let doc_result = index.search::<Document>(doc_search).await;

        assert!(doc_result.is_ok());
        assert_eq!(doc_result.as_ref().unwrap().hits.len(), 0);

        let delete_index_result = index.delete().await;
        assert!(delete_index_result.is_ok());
    }
}
