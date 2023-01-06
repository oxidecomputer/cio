use cio_api::rfd::RFDSearchIndex;
use meilisearch_minimal_api::{IndexSettings, MeiliClient};

#[ignore]
#[tokio::test]
async fn test_index_rfd() {
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

=== The Fist Option

First in the list

=== The Second Option

Second in the list

==== Further Nested Details

This options contains further information

=== The Third Option

Third in the list"#;

    let client = MeiliClient::new("http://localhost:7700".to_string(), "dev".to_string());
    let index = client.index("rfd".to_string());

    let mut settings = IndexSettings::default();
    settings.filterable_attributes = Some(vec!["rfd_number".to_string()]);
    let _ = index.settings(settings).await.unwrap();

    RFDSearchIndex::index_rfd(&client, &123.into(), content).await.unwrap();
}
