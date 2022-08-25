use serde::{Deserialize, Serialize};

use crate::error::ClientError;

#[derive(Debug)]
pub enum ScimClientError {
    Api(AirtableScimApiError),
    Client(ClientError),
}

impl<T> From<T> for ScimClientError where T: Into<ClientError> {
    fn from(err: T) -> Self {
        Self::Client(err.into())
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct AirtableScimApiError {
    pub schemas: Vec<String>,
    pub status: u16,
    pub detail: String,
}
