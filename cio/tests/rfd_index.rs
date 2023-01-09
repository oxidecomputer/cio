use cio_api::rfd::{IndexDocument, RFDSearchIndex};
use meilisearch_minimal_api::{IndexClient, IndexSettings, MeiliClient, SearchQuery};
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

#[tokio::test]
async fn test_index_rfd() {
    if let Some(db) = TestDB::new("test_index_rfd") {
        let content = r#":showtitle:
:toc: left
:numbered:
:icons: font
:state: published
:discussion: https://github.com/organization/repo/pull/123
:revremark: State: {state} | {discussion}
:authors: Firstname Lastname <author@organization.com>

= RFD 123 On Parsing Documents
{authors}

An introductory line about the document

== Background

A paragraph about background topics

== Possibilities

Nested sections describing possible options

=== The First Option

First in the list

=== The Second Option

Second in the list

==== Further Nested Details

This options contains further information

=== The Third Option

Third in the list"#;

        let index = db.index_client();

        let mut settings = IndexSettings::default();
        settings.filterable_attributes = Some(vec!["rfd_number".to_string()]);
        let _ = index.settings(settings).await.unwrap();

        RFDSearchIndex::index_rfd(&db.client, db.index_name.to_string(), &123.into(), content)
            .await
            .unwrap();

        // Wait for indexing to complete
        std::thread::sleep(std::time::Duration::from_secs(1));

        let mut search = SearchQuery::default();
        search.filter = Some(vec!["rfd_number = 123".to_string()]);

        let results = index.search::<IndexDocument>(search).await;

        assert!(results.is_ok());
        assert_eq!(results.as_ref().unwrap().hits.len(), 6);

        let section_names = results
            .unwrap()
            .hits
            .into_iter()
            .map(|hit| hit.name)
            .collect::<Vec<_>>();

        assert!(section_names.contains(&"Background".to_string()));
        assert!(section_names.contains(&"Possibilities".to_string()));
        assert!(section_names.contains(&"The First Option".to_string()));
        assert!(section_names.contains(&"The Second Option".to_string()));
        assert!(section_names.contains(&"Further Nested Details".to_string()));
        assert!(section_names.contains(&"The Third Option".to_string()));
    }
}
