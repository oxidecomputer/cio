use anyhow::{bail, Result};
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

    /// Delete the record if it exists.
    async fn delete_record(&self, domain: &str, content: cloudflare::endpoints::dns::DnsContent) -> Result<()>;
}

#[async_trait]
impl DNSProviderOps for CloudflareClient {
    #[tracing::instrument(skip(self))]
    async fn ensure_record(&self, domain: &str, content: cloudflare::endpoints::dns::DnsContent) -> Result<()> {
        let domain = &domain.to_lowercase();
        let zone_identifier = &get_zone_identifier(self, domain).await?;

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

        for record in &dns_records {
            if record.name == *domain && content_equals(record.content.clone(), content.clone()) {
                info!("dns record for domain `{}` already exists: {:?}", domain, content);

                return Ok(());
            }
        }

        let is_a_record = matches!(content, cloudflare::endpoints::dns::DnsContent::A { content: _ });

        let is_aaaa_record = matches!(content, cloudflare::endpoints::dns::DnsContent::AAAA { content: _ });

        let is_cname_record = matches!(content, cloudflare::endpoints::dns::DnsContent::CNAME { content: _ });

        if domain.starts_with("_acme-challenge.") || is_a_record || is_aaaa_record || is_cname_record {
            if dns_records.len() > 1 {
                // TODO: handle this better, match on the record type.
                bail!(
                    "we don't know which DNS record to update for domain `{}`: {:?}\nexisting records: {:?}",
                    domain,
                    content,
                    dns_records
                );
            }

            // Update the record.
            let _dns_record = self
                .request(&dns::UpdateDnsRecord {
                    zone_identifier,
                    identifier: &dns_records[0].id,
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

            info!("updated dns record for domain `{}`: {:?}", domain, content);
        } else {
            // Create the DNS record.
            // We likely want many of these if we got here.
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
        }

        Ok(())
    }

    #[tracing::instrument(skip(self))]
    async fn delete_record(&self, domain: &str, content: cloudflare::endpoints::dns::DnsContent) -> Result<()> {
        let domain = &domain.to_lowercase();
        let zone_identifier = &get_zone_identifier(self, domain).await?;

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

        for record in dns_records {
            if record.name == *domain && content_equals(record.content.clone(), content.clone()) {
                // TODO: Delete the record.
                info!("deleted dns record for domain `{}`", domain);

                return Ok(());
            }
        }

        Ok(())
    }
}

/// TODO: remove this stupid function when cloudflare has PartialEq on their types...
#[tracing::instrument]
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

#[tracing::instrument]
async fn get_zone_identifier(client: &CloudflareClient, domain: &str) -> Result<String> {
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
    let zones = client
        .request(&zone::ListZones {
            params: zone::ListZonesParams {
                name: Some(root_domain.to_string()),
                ..Default::default()
            },
        })
        .await?
        .result;

    // Our zone identifier should be the first record's ID.
    Ok(zones[0].id.to_string())
}
