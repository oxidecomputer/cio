use serde::{Deserialize, Serialize};
use serde_json;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct APICall {
    /// If there are more records, the response will contain an
    /// offset. To fetch the next page of records, include offset
    /// in the next request's parameters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<String>,
    /// The current page number of returned records.
    pub records: Vec<Record>,
    /// The Airtable API will perform best-effort automatic data conversion
    /// from string values if the typecast parameter is passed in. Automatic
    /// conversion is disabled by default to ensure data integrity, but it may
    /// be helpful for integrating with 3rd party data sources.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub typecast: Option<bool>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Record {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub fields: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_time: Option<String>,
}
