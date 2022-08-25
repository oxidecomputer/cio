use async_trait::async_trait;
use reqwest::{header, Method, Request, Response, Url};
use reqwest_middleware::RequestBuilder;
use std::sync::Arc;

use crate::error::ClientError;

pub type Inner = Arc<dyn ApiClient>;

#[derive(Clone)]
pub struct InnerClient {
    key: String,
    base_id: String,
    enterprise_account_id: String,

    client: reqwest_middleware::ClientWithMiddleware,
}

impl InnerClient {
    pub fn new(
        key: String,
        base_id: String,
        enterprise_account_id: String,
        client: reqwest_middleware::ClientWithMiddleware,
    ) -> Self {
        Self {
            key,
            base_id,
            enterprise_account_id,
            client,
        }
    }
}

#[async_trait]
pub trait ApiClient {
    fn key(&self) -> &str;
    fn base_id(&self) -> &str;
    fn enterprise_account_id(&self) -> &str;
    fn client(&self) -> &reqwest_middleware::ClientWithMiddleware;

    fn request(
        &self,
        method: Method,
        url: Url,
        query: Option<Vec<(&str, String)>>,
    ) -> Result<RequestBuilder, ClientError>;
    async fn execute(&self, request: Request) -> Result<Response, ClientError>;
}

#[async_trait]
impl ApiClient for InnerClient {
    fn key(&self) -> &str {
        &self.key
    }

    fn base_id(&self) -> &str {
        &self.base_id
    }

    fn enterprise_account_id(&self) -> &str {
        &self.enterprise_account_id
    }

    fn client(&self) -> &reqwest_middleware::ClientWithMiddleware {
        &self.client
    }

    fn request(
        &self,
        method: Method,
        url: Url,
        query: Option<Vec<(&str, String)>>,
    ) -> Result<RequestBuilder, ClientError> {
        let bt = format!("Bearer {}", self.key());
        let bearer = header::HeaderValue::from_str(&bt)?;

        // Set the default headers.
        let mut headers = header::HeaderMap::new();
        headers.append(header::AUTHORIZATION, bearer);
        headers.append(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/json"),
        );

        let mut rb = self.client.request(method.clone(), url).headers(headers);

        match query {
            None => (),
            Some(val) => {
                rb = rb.query(&val);
            }
        }

        Ok(rb)
    }

    async fn execute(&self, request: Request) -> Result<Response, ClientError> {
        Ok(self.client.execute(request).await?)
    }
}
