use anyhow::Result;
use meilisearch_sdk::{
    Client,
    indexes::Index,
    search::{
        SearchResults,
        Selectors
    },
};
use quick_js::{Context, JsValue};
use serde::{Deserialize, Serialize};

use super::{
    RFD,
    RFDNumber
};

pub struct RFDSearchIndex {}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RfdId {
    #[serde(rename = "objectID")]
    object_id: String,
}

pub struct IndexableRfd {

}

impl RFDSearchIndex {
    /// Trigger updating the search index for the RFD.
    pub async fn index_rfd(rfd_number: &RFDNumber, rfd: &RFD) -> Result<()> {
        let client = Client::new(
            std::env::var("MEILISEARCH_URL")?,
            std::env::var("MEILISEARCH_KEY")?,
        );

        let index = client.index("rfd");
        let ids = Self::find_rfd_ids(&index, rfd_number).await?;
        let _ = index.delete_documents(&ids).await?;

        let attributes = parse_rfd::parse(&rfd.content);

        Ok(())
    }

    async fn find_rfd_ids(index: &Index, rfd_number: &RFDNumber) -> Result<Vec<String>> {
        let results: SearchResults<RfdId> = index
            .search()
            .with_filter(&format!("rfd_number = {}", rfd_number.0))
            .with_attributes_to_retrieve(Selectors::Some(&["objectID"]))
            .with_limit(500)
            .execute()
            .await?;

        Ok(results.hits.into_iter().map(|hit| hit.result.object_id).collect::<Vec<_>>())
    }

    async fn parse_document(index: &Index, rfd_number: &RFDNumber) -> String {
        let context = Context::new().unwrap();
        
        // Eval.
        
        let value = context.eval("1 + 2").unwrap();
        assert_eq!(value, JsValue::Int(3));
        
        let value = context.eval_as::<String>(" var x = 100 + 250; x.toString() ").unwrap();
        assert_eq!(&value, "350");

        "out".to_string()
    }
}
