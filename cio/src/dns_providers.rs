use anyhow::Result;
use async_trait::async_trait;
use cloudflare::{
    endpoints::{dns, zone},
    framework::async_api::{ApiClient, Client as CloudflareClient},
};
use log::info;

/// This trait defines how to implement a provider for a vendor that manages DNS records.
#[async_trait]
pub trait DNSProviderOps {
    /// Ensure the record exists and has the correct information.
    async fn ensure_record(&self, domain: &str, content: cloudflare::endpoints::dns::DnsContent) -> Result<()>;

    async fn delete_record(&self, domain: &str) -> Result<()>;
}

#[async_trait]
impl DNSProviderOps for CloudflareClient {
    async fn ensure_record(&self, domain: &str, content: cloudflare::endpoints::dns::DnsContent) -> Result<()> {
        // We need the root of the domain not a subdomain.
        let domain_parts: Vec<&str> = domain.split('.').collect();
        let root_domain = if domain_parts.len() > 1 {
            // We have a subdomain, get the root part of the domain.
            format!(
                "{}.{}",
                domain_parts[domain_parts.len() - 2],
                domain_parts[domain_parts.len() - 1]
            )
        } else {
            domain.to_string()
        };

        // Get the zone ID for the domain.
        let zones = self
            .request(&zone::ListZones {
                params: zone::ListZonesParams {
                    name: Some(root_domain.to_string()),
                    ..Default::default()
                },
            })
            .await?
            .result;

        // Our zone identifier should be the first record's ID.
        let zone_identifier = &zones[0].id;

        // Check if we already have a record and we need to update it.
        let dns_records = self
            .request(&dns::ListDnsRecords {
                zone_identifier,
                params: dns::ListDnsRecordsParams {
                    name: Some(domain.to_string()),
                    ..Default::default()
                },
            })
            .await?
            .result;

        // If we have a dns record already, update it. If not, create it.
        if dns_records.is_empty() {
            // Create the DNS record.
            let _dns_record = self
                .request(&dns::CreateDnsRecord {
                    zone_identifier,
                    params: dns::CreateDnsRecordParams {
                        name: domain,
                        content: content.clone(),
                        // This is the min.
                        ttl: Some(120),
                        proxied: None,
                        priority: None,
                    },
                })
                .await?
                .result;

            info!("created dns record for domain `{}`: {:?}", domain, content);

            return Ok(());
        }
        let first = dns_records.first().unwrap();

        if first.name == domain && content_equals(first.content.clone(), content.clone()) {
            info!("dns record for domain `{}` already exists: {:?}", domain, content);

            return Ok(());
        }

        // Update the DNS record.
        let _dns_record = self
            .request(&dns::UpdateDnsRecord {
                zone_identifier,
                identifier: &first.id,
                params: dns::UpdateDnsRecordParams {
                    name: domain,
                    content: content.clone(),
                    // This is the min.
                    ttl: Some(120),
                    proxied: None,
                },
            })
            .await?
            .result;

        info!("created dns record for domain `{}`: {:?}", domain, content);

        Ok(())
    }

    async fn delete_record(&self, domain: &str) -> Result<()> {
        // We need the root of the domain not a subdomain.
        let domain_parts: Vec<&str> = domain.split('.').collect();
        let root_domain = if domain_parts.len() > 1 {
            // We have a subdomain, get the root part of the domain.
            format!(
                "{}.{}",
                domain_parts[domain_parts.len() - 2],
                domain_parts[domain_parts.len() - 1]
            )
        } else {
            domain.to_string()
        };

        // Get the zone ID for the domain.
        let zones = self
            .request(&zone::ListZones {
                params: zone::ListZonesParams {
                    name: Some(root_domain.to_string()),
                    ..Default::default()
                },
            })
            .await?
            .result;

        // Our zone identifier should be the first record's ID.
        let zone_identifier = &zones[0].id;

        // Check if we already have a record and we need to update it.
        let dns_records = self
            .request(&dns::ListDnsRecords {
                zone_identifier,
                params: dns::ListDnsRecordsParams {
                    name: Some(domain.to_string()),
                    ..Default::default()
                },
            })
            .await?
            .result;

        if dns_records.is_empty() {
            info!("dns record for domain `{}` does not exist", domain);

            return Ok(());
        }

        // TODO: check anything else about the record...
        // Delete the record.
        info!("deleted dns record for domain `{}`", domain);

        Ok(())
    }
}

/// TODO: remove this stupid function when cloudflare has PartialEq on their types...
fn content_equals(a: cloudflare::endpoints::dns::DnsContent, b: cloudflare::endpoints::dns::DnsContent) -> bool {
    match a {
        cloudflare::endpoints::dns::DnsContent::A { content } => {
            let a_content = content;
            if let cloudflare::endpoints::dns::DnsContent::A { content } = b {
                return a_content == content;
            }
        }
        cloudflare::endpoints::dns::DnsContent::AAAA { content } => {
            let a_content = content;
            if let cloudflare::endpoints::dns::DnsContent::AAAA { content } = b {
                return a_content == content;
            }
        }
        cloudflare::endpoints::dns::DnsContent::CNAME { content } => {
            let a_content = content;
            if let cloudflare::endpoints::dns::DnsContent::CNAME { content } = b {
                return a_content == content;
            }
        }
        cloudflare::endpoints::dns::DnsContent::NS { content } => {
            let a_content = content;
            if let cloudflare::endpoints::dns::DnsContent::NS { content } = b {
                return a_content == content;
            }
        }
        cloudflare::endpoints::dns::DnsContent::MX { content, priority } => {
            let a_content = content;
            let a_priority = priority;
            if let cloudflare::endpoints::dns::DnsContent::MX { content, priority } = b {
                return a_content == content && a_priority == priority;
            }
        }
        cloudflare::endpoints::dns::DnsContent::TXT { content } => {
            let a_content = content;
            if let cloudflare::endpoints::dns::DnsContent::TXT { content } = b {
                return a_content == content;
            }
        }
        cloudflare::endpoints::dns::DnsContent::SRV { content } => {
            let a_content = content;
            if let cloudflare::endpoints::dns::DnsContent::SRV { content } = b {
                return a_content == content;
            }
        }
    }

    false
}
