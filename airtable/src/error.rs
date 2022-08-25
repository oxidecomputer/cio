use reqwest::header::InvalidHeaderValue;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{error::Error, fmt, sync::Arc};

// General API client errors

#[derive(Debug, Clone, JsonSchema, Serialize)]
pub struct ClientError {
    pub kind: ClientErrorKind,
    #[serde(skip)]
    pub error: Arc<dyn std::error::Error + Send + Sync>,
}

#[derive(Debug, Clone, JsonSchema, Serialize, Deserialize)]
pub enum ClientErrorKind {
    InvalidHeaderValue,
    Reqwest,
    ReqwestMiddleware,
    Serde,
    Url,
}

impl From<InvalidHeaderValue> for ClientError {
    fn from(err: InvalidHeaderValue) -> Self {
        ClientError {
            kind: ClientErrorKind::Reqwest,
            error: Arc::new(err),
        }
    }
}

impl From<reqwest::Error> for ClientError {
    fn from(err: reqwest::Error) -> Self {
        ClientError {
            kind: ClientErrorKind::Reqwest,
            error: Arc::new(err),
        }
    }
}

impl From<reqwest_middleware::Error> for ClientError {
    fn from(err: reqwest_middleware::Error) -> Self {
        ClientError {
            kind: ClientErrorKind::ReqwestMiddleware,
            error: Arc::new(err),
        }
    }
}

impl From<url::ParseError> for ClientError {
    fn from(err: url::ParseError) -> Self {
        ClientError {
            kind: ClientErrorKind::Url,
            error: Arc::new(err),
        }
    }
}

impl From<serde_json::Error> for ClientError {
    fn from(err: serde_json::Error) -> Self {
        ClientError {
            kind: ClientErrorKind::Serde,
            error: Arc::new(err),
        }
    }
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.kind)
    }
}

impl fmt::Display for ClientErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let label = match self {
            Self::InvalidHeaderValue => "InvalidHeaderValue",
            Self::Reqwest => "Reqwest",
            Self::ReqwestMiddleware => "ReqwestMiddleware",
            Self::Serde => "Serde",
            Self::Url => "Url",
        };

        write!(f, "{}", label)
    }
}

impl Error for ClientError {}

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
