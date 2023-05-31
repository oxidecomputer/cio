use anyhow::Result;
use async_trait::async_trait;
use dropshot::{
    ApiEndpointBodyContentType, SharedExtractor, ExtractorMetadata, HttpError, Query, RequestContext, ServerContext, ExtensionMode
};
use schemars::JsonSchema;
use serde::Deserialize;

use std::{marker::PhantomData};

use crate::http::{internal_error, unauthorized};

#[async_trait]
pub trait QueryTokenProvider {
    async fn token() -> Result<String>;
}

pub struct QueryToken<T> {
    _provider: PhantomData<T>,
}

pub struct QueryTokenAudit<T> {
    verified: bool,
    _provider: PhantomData<T>,
}

#[derive(Deserialize, JsonSchema)]
struct Token {
    token: String,
}

#[async_trait]
impl<T> SharedExtractor for QueryToken<T>
where
    T: QueryTokenProvider + Send + Sync,
{
    async fn from_request<Context: ServerContext>(
        rqctx: &RequestContext<Context>,
    ) -> Result<QueryToken<T>, HttpError> {
        let audit = QueryTokenAudit::<T>::from_request(rqctx).await?;

        if audit.verified {
            Ok(QueryToken { _provider: PhantomData })
        } else {
            Err(unauthorized())
        }
    }

    fn metadata(_body_content_type: ApiEndpointBodyContentType) -> ExtractorMetadata {
        ExtractorMetadata {
            extension_mode: ExtensionMode::None,
            parameters: vec![],
        }
    }
}

#[async_trait]
impl<T> SharedExtractor for QueryTokenAudit<T>
where
    T: QueryTokenProvider + Send + Sync,
{
    async fn from_request<Context: ServerContext>(
        rqctx: &RequestContext<Context>,
    ) -> Result<QueryTokenAudit<T>, HttpError> {
        let req_token = Query::<Token>::from_request(rqctx.clone())
            .await
            .map(|token| token.into_inner().token)
            .ok();
        let expected_token = T::token().await.map_err(|_| internal_error())?;

        let verified = Some(expected_token) == req_token;

        if verified {
            log::info!(
                "Successfully verified request via url token. req_id: {} uri: {}",
                rqctx.request_id,
                rqctx.request.uri()
            );
        } else {
            log::info!(
                "Failed to verify request via url token. req_id: {} uri: {}",
                rqctx.request_id,
                rqctx.request.uri()
            );
        }

        Ok(QueryTokenAudit {
            verified,
            _provider: PhantomData,
        })
    }

    fn metadata(_body_content_type: ApiEndpointBodyContentType) -> ExtractorMetadata {
        ExtractorMetadata {
            extension_mode: ExtensionMode::None,
            parameters: vec![],
        }
    }
}
