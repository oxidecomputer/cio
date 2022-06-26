use anyhow::Result;
use async_trait::async_trait;
use dropshot_auth::{bearer::BearerProvider, query::QueryTokenProvider};

pub struct GlobalToken;

#[async_trait]
impl BearerProvider for GlobalToken {
    async fn token() -> Result<String> {
        Ok(std::env::var("AUTH_BEARER")?)
    }
}

#[async_trait]
impl QueryTokenProvider for GlobalToken {
    async fn token() -> Result<String> {
        Ok(std::env::var("AUTH_BEARER")?)
    }
}
