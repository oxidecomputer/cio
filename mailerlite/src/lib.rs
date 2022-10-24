use reqwest::{Client, Error as ReqwestError, RequestBuilder};
use secrecy::{ExposeSecret, SecretString};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{str::FromStr, sync::Arc};
use thiserror::Error;

pub mod endpoints;
pub mod types;

use endpoints::MailerliteEndpoint;
use types::*;

#[derive(Debug)]
pub struct MailerliteClient {
    base_url: String,
    bearer: SecretString,
    inner: Client,
}

#[derive(Debug, Clone, Error)]
pub enum MailerliteError {
    #[error("Inner request failed")]
    Inner(Arc<ReqwestError>),
}

impl From<ReqwestError> for MailerliteError {
    fn from(inner: ReqwestError) -> Self {
        Self::Inner(Arc::new(inner))
    }
}

/// A partial Mailerlite client that implements only the necessary functionality
impl MailerliteClient {
    pub fn new<S>(bearer: S) -> Self
    where
        S: AsRef<str>,
    {
        Self {
            base_url: "https://connect.mailerlite.com/api".to_string(),
            // SecretString returns an infallible error, and can be unwrapped without error
            bearer: SecretString::from_str(bearer.as_ref()).unwrap(),
            inner: Client::new(),
        }
    }

    pub fn set_base_url(&mut self, base_url: String) {
        self.base_url = base_url;
    }

    fn auth(&self, builder: RequestBuilder) -> RequestBuilder {
        builder.bearer_auth(self.bearer.expose_secret())
    }

    pub async fn run<T>(
        &self,
        endpoint: impl MailerliteEndpoint<Response = T> + Sync,
    ) -> Result<MailerliteResponse<T>, MailerliteError>
    where
        T: DeserializeOwned,
    {
        let request = self.auth(endpoint.to_request_builder(&self.base_url, &self.inner));
        let response = request.send().await?;

        // Handle general case errors like failed authentication. Afterwards, individual endpoints
        // are responsible for parsing their own errors
        if response.status() == 401 {
            Ok(response.json::<MailerliteResponse<T>>().await?)
        } else {
            endpoint
                .handle_response(response)
                .await
                .map(MailerliteResponse::EndpointResponse)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MailerliteResponse<T> {
    AuthenticationError { message: String },
    EndpointResponse(T),
}
