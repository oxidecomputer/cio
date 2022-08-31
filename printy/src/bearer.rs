use anyhow::Result;
use async_trait::async_trait;
use dropshot_verify_request::bearer::BearerProvider;

pub struct EnvToken;

#[async_trait]
impl BearerProvider for EnvToken {
    async fn token() -> Result<String> {
        Ok(std::env::var("AUTH_BEARER")?)
    }
}