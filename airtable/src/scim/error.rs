use chrono::{DateTime, Utc};
use reqwest::{Method, Response, StatusCode, Url};
use schemars::JsonSchema;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

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

#[derive(Debug, PartialEq, Deserialize)]
pub struct AirtableScimError {
    schemas: Vec<String>,
    status: u16,
    detail: String,
}