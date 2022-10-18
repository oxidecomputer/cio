use anyhow::Result;
use async_trait::async_trait;

use crate::{
    cloud_dns::CloudDnsClient,
    cloudflare::CloudFlareClient,
    dns_providers::{DNSProviderOps, DnsRecord, DnsUpdateMode},
};

pub struct DnsProviderProxy {
    cloudflare: CloudFlareClient,
    cloud_dns: CloudDnsClient,
}

impl DnsProviderProxy {
    pub fn new(cloudflare: CloudFlareClient, cloud_dns: CloudDnsClient) -> Self {
        Self { cloudflare, cloud_dns }
    }
}

#[async_trait]
impl DNSProviderOps for DnsProviderProxy {
    /// Ensure the record exists and has the correct information.
    async fn ensure_record(&self, record: DnsRecord, mode: DnsUpdateMode) -> Result<()> {
        self.cloudflare.ensure_record(record.clone(), mode.clone()).await?;
        self.cloud_dns.ensure_record(record, mode).await?;

        Ok(())
    }

    /// Delete the record if it exists.
    async fn delete_record(&self, record: DnsRecord) -> Result<()> {
        self.cloudflare.delete_record(record.clone()).await?;
        self.cloud_dns.delete_record(record).await?;

        Ok(())
    }
}
