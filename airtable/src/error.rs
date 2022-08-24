use serde::{Deserialize, Serialize};
use std::{error::Error, fmt};

// General API client errors

#[derive(Debug)]
pub enum AirtableError {
    FailedToConstructRequest,
    FailedToExecute(reqwest_middleware::Error),
}

impl fmt::Display for AirtableError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Error for AirtableError {}

impl From<reqwest_middleware::Error> for AirtableError {
    fn from(err: reqwest_middleware::Error) -> Self {
        Self::FailedToExecute(err)
    }
}

// Modeling for Airtable Enterprise API errors (non-scim)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AirtableEnterpriseError {
    pub error: AirtableEnterpriseErrorInner,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AirtableEnterpriseErrorInner {
    #[serde(rename = "type")]
    pub type_: String,
    pub message: String,
}