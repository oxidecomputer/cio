use anyhow::Result;
use async_trait::async_trait;
use dropshot_auth::{bearer::BearerProvider, query::QueryTokenProvider};

pub struct InternalToken;

#[async_trait]
impl BearerProvider for InternalToken {
    async fn token() -> Result<String> {
        Ok(std::env::var("INTERNAL_AUTH_BEARER")?)
    }
}

#[async_trait]
impl QueryTokenProvider for InternalToken {
    async fn token() -> Result<String> {
        Ok(std::env::var("INTERNAL_AUTH_BEARER")?)
    }
}

pub struct AirtableToken;

#[async_trait]
impl BearerProvider for AirtableToken {
    async fn token() -> Result<String> {
        Ok(std::env::var("AIRTABLE_WH_KEY")?)
    }
}

pub struct ShippoToken;

#[async_trait]
impl QueryTokenProvider for ShippoToken {
    async fn token() -> Result<String> {
        Ok(std::env::var("SHIPPO_WH_KEY")?)
    }
}

pub struct MailChimpToken;

#[async_trait]
impl QueryTokenProvider for MailChimpToken {
    async fn token() -> Result<String> {
        Ok(std::env::var("MAILCHIMP_WH_KEY")?)
    }
}
