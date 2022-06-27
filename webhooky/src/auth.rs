use anyhow::Result;
use async_trait::async_trait;
use dropshot_auth::{bearer::BearerProvider, query::QueryTokenProvider};

pub struct GlobalToken;

#[async_trait]
impl BearerProvider for GlobalToken {
    async fn token() -> Result<String> {
        Ok(std::env::var("GLOBAL_AUTH_BEARER")?)
    }
}

#[async_trait]
impl QueryTokenProvider for GlobalToken {
    async fn token() -> Result<String> {
        Ok(std::env::var("GLOBAL_AUTH_BEARER")?)
    }
}

pub struct AirtableToken;

#[async_trait]
impl BearerProvider for AirtableToken {
    async fn token() -> Result<String> {
        Ok(std::env::var("AIRTABLE_WH_TOKEN")?)
    }
}

#[async_trait]
impl QueryTokenProvider for AirtableToken {
    async fn token() -> Result<String> {
        Ok(std::env::var("AIRTABLE_WH_TOKEN")?)
    }
}