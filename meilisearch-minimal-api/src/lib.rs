use chrono::{DateTime, Utc};
use reqwest::{Client, StatusCode};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use std::sync::Arc;

pub struct MeiliClient {
    inner: Arc<InnerClient>,
}

#[derive(Debug)]
pub enum MeiliError {
    Client(reqwest::Error),
    FailedToParseResponse(serde_json::Error),
}

impl std::fmt::Display for MeiliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MeiliError::Client(inner) => write!(f, "Client error: {}", inner),
            MeiliError::FailedToParseResponse(inner) => write!(f, "Failed to parse response: {}", inner),
        }
    }
}

impl std::error::Error for MeiliError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            MeiliError::Client(inner) => Some(inner),
            MeiliError::FailedToParseResponse(inner) => Some(inner),
        }
    }
}

impl From<reqwest::Error> for MeiliError {
    fn from(value: reqwest::Error) -> MeiliError {
        MeiliError::Client(value)
    }
}

impl From<serde_json::Error> for MeiliError {
    fn from(value: serde_json::Error) -> MeiliError {
        MeiliError::FailedToParseResponse(value)
    }
}

impl MeiliClient {
    pub fn new(url: String, key: String) -> Self {
        Self {
            inner: Arc::new(InnerClient::new(url, key)),
        }
    }

    pub fn index(&self, id: String) -> IndexClient {
        IndexClient {
            inner: self.inner.clone(),
            id,
        }
    }
}

pub struct IndexClient {
    inner: Arc<InnerClient>,
    id: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SearchQuery {
    #[serde(rename = "q", skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributes_to_retrieve: Option<Vec<String>>,
}

impl IndexClient {
    pub async fn settings(&self, settings: IndexSettings) -> Result<serde_json::Value, MeiliError> {
        Ok(self
            .inner
            .client
            .patch(format!("{}/indexes/{}/settings", self.inner.url, self.id))
            .bearer_auth(&self.inner.key)
            .json(&settings)
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?)
    }

    pub async fn search<T>(&self, search: SearchQuery) -> Result<SearchResponse<T>, MeiliError>
    where
        T: DeserializeOwned,
    {
        let response = self
            .inner
            .client
            .post(format!("{}/indexes/{}/search", self.inner.url, self.id))
            .bearer_auth(&self.inner.key)
            .json(&search)
            .send()
            .await?;

        if response.status() == StatusCode::NOT_FOUND {
            Ok(SearchResponse { hits: vec![] })
        } else {
            let content = response.text().await?;
            Ok(serde_json::from_str::<SearchResponse<T>>(&content)?)
        }
    }

    pub async fn index_documents<T>(&self, documents: &[T], primary_key: &str) -> Result<TaskResponse, MeiliError>
    where
        T: Serialize,
    {
        let response = self
            .inner
            .client
            .post(format!(
                "{}/indexes/{}/documents?primaryKey={}",
                self.inner.url, self.id, primary_key
            ))
            .bearer_auth(&self.inner.key)
            .json(documents)
            .send()
            .await?;

        let content = response.text().await?;
        Ok(serde_json::from_str::<TaskResponse>(&content)?)
    }

    pub async fn delete_documents<T>(&self, ids: &[T]) -> Result<TaskResponse, MeiliError>
    where
        T: Serialize,
    {
        Ok(self
            .inner
            .client
            .post(format!("{}/indexes/{}/documents/delete-batch", self.inner.url, self.id))
            .bearer_auth(&self.inner.key)
            .json(ids)
            .send()
            .await?
            .json::<TaskResponse>()
            .await?)
    }

    pub async fn delete(&self) -> Result<TaskResponse, MeiliError> {
        let response = self
            .inner
            .client
            .delete(format!("{}/indexes/{}", self.inner.url, self.id))
            .bearer_auth(&self.inner.key)
            .send()
            .await?;

        let content = response.text().await?;
        Ok(serde_json::from_str::<TaskResponse>(&content)?)
    }
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct IndexSettings {
    #[serde(rename = "filterableAttributes", skip_serializing_if = "Option::is_none")]
    pub filterable_attributes: Option<Vec<String>>,
    #[serde(rename = "sortableAttributes", skip_serializing_if = "Option::is_none")]
    pub sortable_attributes: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SearchResponse<T> {
    pub hits: Vec<T>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TaskResponse {
    #[serde(rename = "taskUid")]
    task_uid: u32,
    #[serde(rename = "indexUid")]
    index_uid: String,
    status: String,
    #[serde(rename = "enqueuedAt")]
    enqueued_at: DateTime<Utc>,
}

struct InnerClient {
    url: String,
    key: String,
    client: Client,
}

impl InnerClient {
    pub fn new(url: String, key: String) -> Self {
        Self {
            url,
            key,
            client: Client::new(),
        }
    }
}
