use anyhow::Result;
use async_trait::async_trait;
use dropshot::{Extractor, ExtractorMetadata, HttpError, RequestContext, ServerContext};

use std::{marker::PhantomData, sync::Arc};

use crate::http::{unauthorized, Headers};

#[async_trait]
pub trait BearerProvider {
    async fn token() -> Result<String>;
}

pub struct Bearer<T> {
    _provider: PhantomData<T>,
}

pub struct BearerAudit<T> {
    _provider: PhantomData<T>,
}

#[async_trait]
impl<T> Extractor for Bearer<T>
where
    T: BearerProvider + Send + Sync,
{
    async fn from_request<Context: ServerContext>(rqctx: Arc<RequestContext<Context>>) -> Result<Bearer<T>, HttpError> {
        let headers = Headers::from_request(rqctx.clone()).await.map_err(|_| unauthorized())?;
        let expected_token = T::token().await.map_err(|_| unauthorized())?;

        let header = headers.0.get("Authorization").ok_or_else(unauthorized)?;
        let header_value = header.to_str().map_err(|_| unauthorized())?;
        let mut parts = header_value.split(" ");
        let label = parts.next();
        let user_token = parts.next();

        if let (Some(label), Some(user_token)) = (label, user_token) {
            if label == "Bearer" && expected_token == user_token {
                Ok(Bearer { _provider: PhantomData })
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
impl<T> Extractor for BearerAudit<T>
where
    T: BearerProvider + Send + Sync,
{
    async fn from_request<Context: ServerContext>(
        rqctx: Arc<RequestContext<Context>>,
    ) -> Result<BearerAudit<T>, HttpError> {
        if let Err(_) = Bearer::<T>::from_request(rqctx.clone()).await {
            let uri = &rqctx.request.lock().await.uri().clone();
            log::info!(
                "Bearer authentication check failed. id: {}, uri: {}",
                rqctx.request_id,
                uri
            );
        };

        Ok(BearerAudit { _provider: PhantomData })
    }

    fn metadata() -> ExtractorMetadata {
        ExtractorMetadata {
            paginated: false,
            parameters: vec![],
        }
    }
}
