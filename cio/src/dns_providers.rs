use anyhow::Result;
use async_trait::async_trait;

/// This trait defines how to implement a provider for a vendor that manages DNS records.
#[async_trait]
pub trait DNSProviderOps {
    /// Ensure the record exists and has the correct information.
    async fn ensure_record(&self, domain: &str, content: cloudflare::endpoints::dns::DnsContent) -> Result<()>;

    /// Delete the record if it exists.
    async fn delete_record(&self, domain: &str, content: cloudflare::endpoints::dns::DnsContent) -> Result<()>;
}
