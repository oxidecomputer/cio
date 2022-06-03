use anyhow::{bail, Result};
use async_trait::async_trait;
use cloudflare::{
    endpoints::{dns, zone},
    framework::{
        async_api::{ApiClient, Client},
        endpoint::Endpoint,
        response::{ApiResponse, ApiResult},
    },
};
use log::info;
use serde::Serialize;

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use crate::dns_providers::DNSProviderOps;

pub struct ZoneEntry {
    id: String,
    expires_at: Instant,
}

pub struct CloudFlareClient {
    client: Client,
    zone_cache: Arc<RwLock<HashMap<String, ZoneEntry>>>,
}

impl From<Client> for CloudFlareClient {
    fn from(client: Client) -> Self {
        Self {
            client,
            zone_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl CloudFlareClient {
    pub async fn request<ResultType, QueryType, BodyType>(
        &self,
        endpoint: &(dyn Endpoint<ResultType, QueryType, BodyType> + Send + Sync),
    ) -> ApiResponse<ResultType>
    where
        ResultType: ApiResult,
        QueryType: Serialize,
        BodyType: Serialize,
    {
        self.client.request_handle(endpoint).await
    }

    pub async fn get_zone_identifier(&self, domain: &str) -> Result<String> {
        // We need the root of the domain not a subdomain.
        let domain_parts: Vec<&str> = domain.split('.').collect();
        let root_domain = if domain_parts.len() > 2 {
            // We have a subdomain, get the root part of the domain.
            format!(
                "{}.{}",
                domain_parts[domain_parts.len() - 2],
                domain_parts[domain_parts.len() - 1]
            )
        } else {
            domain.to_string()
        };

        if let Some(cached) = self.zone_cache.read().unwrap().get(&root_domain) {
            if cached.expires_at > Instant::now() {
                return Ok(cached.id.clone());
            }
        }

        // Get the zone ID for the domain.
        let zones = self
            .client
            .request(&zone::ListZones {
                params: zone::ListZonesParams {
                    name: Some(root_domain.to_string()),
                    ..Default::default()
                },
            })
            .await?
            .result;

        self.zone_cache.write().unwrap().insert(
            root_domain,
            ZoneEntry {
                id: zones[0].id.to_string(),
                expires_at: Instant::now().checked_add(Duration::from_secs(60 * 60)).unwrap(),
            },
        );

        // Our zone identifier should be the first record's ID.
        Ok(zones[0].id.to_string())
    }
}

#[async_trait]
impl DNSProviderOps for CloudFlareClient {
    async fn ensure_record(&self, domain: &str, content: cloudflare::endpoints::dns::DnsContent) -> Result<()> {
        let domain = &domain.to_lowercase();
        let zone_identifier = self.get_zone_identifier(domain).await?;

        // Check if we already have a record and we need to update it.
        let dns_records = self
            .request(&dns::ListDnsRecords {
                zone_identifier: &zone_identifier,
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
                    zone_identifier: &zone_identifier,
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
                    zone_identifier: &zone_identifier,
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
                    zone_identifier: &zone_identifier,
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

    async fn delete_record(&self, domain: &str, content: cloudflare::endpoints::dns::DnsContent) -> Result<()> {
        let domain = &domain.to_lowercase();
        let zone_identifier = self.get_zone_identifier(domain).await?;

        // Check if we already have a record and we need to update it.
        let dns_records = self
            .request(&dns::ListDnsRecords {
                zone_identifier: &zone_identifier,
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
