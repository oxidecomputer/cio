use anyhow::Result;
use async_trait::async_trait;

use crate::bearer::BearerProvider;
use crate::token::TokenProvider;

pub struct GlobalToken;

#[async_trait]
impl BearerProvider for GlobalToken {
    async fn token() -> Result<String> {
        Ok(std::env::var("AUTH_BEARER")?)
    }
}

#[async_trait]
impl TokenProvider for GlobalToken {
    async fn token() -> Result<String> {
        Ok(std::env::var("AUTH_BEARER")?)
    }
}