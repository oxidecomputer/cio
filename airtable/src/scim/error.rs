use serde::{Deserialize, Serialize};

use crate::error::AirtableError;

#[derive(Debug)]
pub enum ScimError {
    Airtable(AirtableError),
    Api(AirtableScimError),
    Client(ClientError),
}

#[derive(Debug)]
pub struct ClientError {
    pub error: Box<dyn std::error::Error>,
}

impl From<AirtableError> for ScimError {
    fn from(err: AirtableError) -> Self {
        Self::Airtable(err)
    }
}

impl From<reqwest::Error> for ScimError {
    fn from(err: reqwest::Error) -> Self {
        Self::Client(ClientError { error: Box::new(err) })
    }
}

impl From<url::ParseError> for ScimError {
    fn from(err: url::ParseError) -> Self {
        Self::Client(ClientError { error: Box::new(err) })
    }
}

impl From<serde_json::Error> for ScimError {
    fn from(err: serde_json::Error) -> Self {
        Self::Client(ClientError { error: Box::new(err) })
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct AirtableScimError {
    pub schemas: Vec<String>,
    pub status: u16,
    pub detail: String,
}
