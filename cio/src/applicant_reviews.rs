use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, JsonSchema, Deserialize, Serialize)]
pub struct ApplicantReview {
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "Name")]
    pub name: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "Value Reflected (from Questionnaire)"
    )]
    pub value_reflected: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "Value Violated (from Questionnaire)"
    )]
    pub value_violated: String,
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        rename = "Values in Tension (from Questionnaire)"
    )]
    pub values_in_tension: Vec<String>,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "Evaluation"
    )]
    pub evaluation: String,

    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        rename = "If \"Pass\" or \"No\", rationale if applicable (check all that apply)"
    )]
    pub rationable: Vec<String>,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "Any additional evaluation (not to be shared with applicant)"
    )]
    pub notes: String,

    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        serialize_with = "airtable_api::user_format_as_string::serialize",
        deserialize_with = "airtable_api::user_format_as_string::deserialize",
        rename = "Reviewer"
    )]
    pub reviewer: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "Applicant")]
    pub applicant: Vec<String>,
}
