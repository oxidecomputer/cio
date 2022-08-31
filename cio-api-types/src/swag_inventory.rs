use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A request to print labels.
#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct PrintRequest {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    #[serde(default)]
    pub quantity: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub content: String,
}
