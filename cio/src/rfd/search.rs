use anyhow::Result;
use hmac::{Hmac, Mac};
use md5::Md5;
use meilisearch_minimal_api::{IndexClient, MeiliClient, SearchQuery};
use parse_rfd::{parse, ParsedDoc, Section};
use serde::{Deserialize, Serialize};
use std::{cmp::min, collections::HashMap};

use super::RFDNumber;

pub struct RFDSearchIndex {}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RfdId {
    #[serde(rename = "objectID")]
    object_id: String,
}

type HmacMd5 = Hmac<Md5>;

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct IndexDocument {
    #[serde(rename = "objectID")]
    pub object_id: String,
    pub name: String,
    pub level: usize,
    pub url: String,
    pub anchor: String,
    pub content: String,
    pub rfd_number: i32,
    #[serde(flatten)]
    pub hierarchy: HashMap<String, String>,
    #[serde(flatten)]
    pub hierarchy_radio: HashMap<String, String>,
}

impl IndexDocument {
    pub fn new(section: Section, rfd_number: &RFDNumber, title: &str) -> Self {
        let level = section.parents.len() + 1;

        let mut hierarchy_radio = HashMap::new();
        if level == 1 {
            hierarchy_radio.insert("hierarchy_radio_lvl1".to_string(), section.name.clone());
        } else {
            hierarchy_radio.insert(
                format!("hierarchy_radio_lvl{}", min(5, level)),
                section.parents[section.parents.len() - 1].clone(),
            );
        }

        let mut hierarchy = HashMap::new();
        hierarchy.insert("hierarchy_lvl0".to_string(), title.to_string());
        hierarchy.insert("hierarchy_lvl1".to_string(), section.name.to_string());

        for (i, section_name) in section.parents.into_iter().enumerate() {
            hierarchy.insert(format!("hierarchy_lvl{}", i + 2), section_name);
        }

        let url = format!(
            "https://rfd.shared.oxide.computer/rfd/{}#{}",
            rfd_number.as_number_string(),
            section.section_id
        );

        // The hash here is only intended to enforce uniqueness amongst documents. md5 and the
        // statically defined key is being used to maintain backward compatibility with previous
        // implementations. None of the key, the ids, nor hash are required to be kept secret
        let mut mac =
            HmacMd5::new_from_slice("dsflkajsdf".as_bytes()).expect("Statically defined key should always be valid");
        mac.update(rfd_number.as_number_string().as_bytes());
        mac.update(section.section_id.as_bytes());
        let object_id = hex::encode(&mac.finalize().into_bytes()[..]);

        Self {
            object_id,
            name: section.name,
            level,
            url,
            anchor: section.section_id,
            content: section.content,
            rfd_number: rfd_number.into(),
            hierarchy,
            hierarchy_radio,
        }
    }
}

impl RFDSearchIndex {
    pub fn default_client() -> Result<MeiliClient> {
        Ok(MeiliClient::new(
            std::env::var("MEILI_URL")?,
            std::env::var("MEILI_KEY")?,
        ))
    }

    /// Trigger updating the search index for the RFD.
    pub async fn index_rfd(client: &MeiliClient, index: String, rfd_number: &RFDNumber, content: &str) -> Result<()> {
        let index = client.index(index);
        let ids_to_delete = Self::find_rfd_ids(&index, rfd_number).await?;

        log::info!("Deleting documents for RFD {}: {:?}", rfd_number.0, ids_to_delete);
        index.delete_documents(&ids_to_delete).await?;

        let parsed = Self::parse_document(rfd_number, content)?;
        index.index_documents(&parsed, "objectID").await?;

        Ok(())
    }

    pub async fn find_rfd_ids(index: &IndexClient, rfd_number: &RFDNumber) -> Result<Vec<String>> {
        let mut query = SearchQuery::default();
        query.filter = Some(vec![format!("rfd_number = {}", rfd_number.0)]);

        let results = index.search::<RfdId>(query).await?;

        Ok(results.hits.into_iter().map(|hit| hit.object_id).collect::<Vec<_>>())
    }

    pub fn parse_document(rfd_number: &RFDNumber, content: &str) -> Result<Vec<IndexDocument>> {
        let ParsedDoc { title, sections } = parse(content)?;
        Ok(sections
            .into_iter()
            .map(|section| IndexDocument::new(section, rfd_number, &title))
            .collect::<Vec<_>>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_indexable_documents() {
        let documents = RFDSearchIndex::parse_document(
            &123.into(),
            r#":showtitle:
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

Third in the list"#,
        )
        .unwrap();

        let expected: serde_json::Value = serde_json::from_str(r#"[{"objectID":"d4cb86c0f047968689bfb31b3b0e8777","anchor":"_background","url":"https://rfd.shared.oxide.computer/rfd/0123#_background","name":"Background","level":1,"content":"A paragraph about background topics","rfd_number":123,"hierarchy_lvl0":"On Parsing Documents","hierarchy_lvl1":"Background","hierarchy_radio_lvl1":"Background"},{"objectID":"78f5e7630699137ab79f8ebc28f1f969","anchor":"_possibilities","url":"https://rfd.shared.oxide.computer/rfd/0123#_possibilities","name":"Possibilities","level":1,"content":"Nested sections describing possible options","rfd_number":123,"hierarchy_lvl0":"On Parsing Documents","hierarchy_lvl1":"Possibilities","hierarchy_radio_lvl1":"Possibilities"},{"objectID":"ae183605f83b74f65d9b8581d62df7a4","anchor":"_the_first_option","url":"https://rfd.shared.oxide.computer/rfd/0123#_the_first_option","name":"The First Option","level":2,"content":"First in the list","rfd_number":123,"hierarchy_lvl0":"On Parsing Documents","hierarchy_lvl1":"The First Option","hierarchy_lvl2":"Possibilities","hierarchy_radio_lvl2":"Possibilities"},{"objectID":"2cc8b5223efebcc9688249fcbbc513a3","anchor":"_the_second_option","url":"https://rfd.shared.oxide.computer/rfd/0123#_the_second_option","name":"The Second Option","level":2,"content":"Second in the list","rfd_number":123,"hierarchy_lvl0":"On Parsing Documents","hierarchy_lvl1":"The Second Option","hierarchy_lvl2":"Possibilities","hierarchy_radio_lvl2":"Possibilities"},{"objectID":"1c37370ab346614df6e78a5003eb11b1","anchor":"_further_nested_details","url":"https://rfd.shared.oxide.computer/rfd/0123#_further_nested_details","name":"Further Nested Details","level":3,"content":"This options contains further information","rfd_number":123,"hierarchy_lvl0":"On Parsing Documents","hierarchy_lvl1":"Further Nested Details","hierarchy_lvl2":"The Second Option","hierarchy_lvl3":"Possibilities","hierarchy_radio_lvl3":"Possibilities"},{"objectID":"476fe6d1ff7a522859fc71bbc146fd60","anchor":"_the_third_option","url":"https://rfd.shared.oxide.computer/rfd/0123#_the_third_option","name":"The Third Option","level":2,"content":"Third in the list","rfd_number":123,"hierarchy_lvl0":"On Parsing Documents","hierarchy_lvl1":"The Third Option","hierarchy_lvl2":"Possibilities","hierarchy_radio_lvl2":"Possibilities"}]"#).unwrap();
        let deser: serde_json::Value = serde_json::from_str(&serde_json::to_string(&documents).unwrap()).unwrap();

        assert_eq!(expected, deser);
    }
}
