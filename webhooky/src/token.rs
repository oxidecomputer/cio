use anyhow::Result;
use async_trait::async_trait;
use dropshot::{Extractor, ExtractorMetadata, HttpError, RequestContext, ServerContext};

use std::{marker::PhantomData, sync::Arc};

use crate::http::{internal_error, unauthorized, Headers};

#[async_trait]
pub trait TokenProvider {
    async fn token() -> Result<String>;
}

pub struct Token<T> {
    _provider: PhantomData<T>,
}

pub struct TokenAudit<T> {
    _provider: PhantomData<T>,
}

#[async_trait]
impl<T> Extractor for Token<T>
where
    T: TokenProvider + Send + Sync,
{
    async fn from_request<Context: ServerContext>(rqctx: Arc<RequestContext<Context>>) -> Result<Token<T>, HttpError> {
        let headers = Headers::from_request(rqctx.clone()).await.map_err(|_| unauthorized())?;
        let expected_token = T::token().await.map_err(|_| internal_error())?;

        let header = headers.0.get("Authorization").ok_or_else(unauthorized)?;
        let header_value = header.to_str().map_err(|_| unauthorized())?;
        let mut parts = header_value.split(" ");
        let label = parts.next();
        let user_token = parts.next();

        if let (Some(label), Some(user_token)) = (label, user_token) {
            if label == "Token" && expected_token == user_token {
                Ok(Token { _provider: PhantomData })
            } else {
                Err(unauthorized())
            }
        } else {
            Err(unauthorized())
        }
    }

    fn metadata() -> ExtractorMetadata {
        ExtractorMetadata {
            paginated: false,
            parameters: vec![],
        }
    }
}

#[async_trait]
impl<T> Extractor for TokenAudit<T>
where
    T: TokenProvider + Send + Sync,
{
    async fn from_request<Context: ServerContext>(
        rqctx: Arc<RequestContext<Context>>,
    ) -> Result<TokenAudit<T>, HttpError> {
        if let Err(_) = Token::<T>::from_request(rqctx.clone()).await {
            let uri = &rqctx.request.lock().await.uri().clone();
            log::info!(
                "Token authentication check failed. id: {}, uri: {}",
                rqctx.request_id,
                uri
            );
        };

        Ok(TokenAudit { _provider: PhantomData })
    }

    fn metadata() -> ExtractorMetadata {
        ExtractorMetadata {
            paginated: false,
            parameters: vec![],
        }
    }
}

