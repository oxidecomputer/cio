use anyhow::{bail, Result};
use async_trait::async_trait;
use cloudflare::{
    endpoints::{dns, dns::DnsRecord, zone},
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

#[derive(Debug, Clone)]
pub struct ZoneEntry {
    pub id: String,
    pub expires_at: Instant,
}

pub struct CloudFlareClient {
    client: Client,
    zones: Arc<RwLock<HashMap<String, Zone>>>,
    zone_cache: Arc<RwLock<HashMap<String, ZoneEntry>>>,
}

impl From<Client> for CloudFlareClient {
    fn from(client: Client) -> Self {
        Self {
            client,
            zones: Arc::new(RwLock::new(HashMap::new())),
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
        log::debug!("Sending req to CloudFlare {:?}", endpoint.path());
        self.client.request_handle(endpoint).await
    }

    pub async fn get_zone_identifier(&self, domain: &str) -> Result<ZoneEntry> {
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
                return Ok(cached.clone());
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

        let entry = ZoneEntry {
            id: zones[0].id.to_string(),
            expires_at: Instant::now().checked_add(Duration::from_secs(60 * 60)).unwrap(),
        };

        self.zone_cache.write().unwrap().insert(root_domain, entry.clone());

        // Our zone identifier should be the first record's ID.
        Ok(entry)
    }

    async fn get_dns_records_in_zone(&self, zone_identifier: &str, page: u32) -> ApiResponse<Vec<DnsRecord>> {
        self.client
            .request_handle(&dns::ListDnsRecords {
                zone_identifier,
                params: dns::ListDnsRecordsParams {
                    // From: https://api.cloudflare.com/#dns-records-for-a-zone-list-dns-records
                    per_page: Some(5000),
                    page: Some(page),
                    ..Default::default()
                },
            })
            .await
    }

    pub async fn populate_zone_cache(&self, zone_identifier: &str) -> Result<()> {
        if self.zones.read().unwrap().get(zone_identifier).is_none() {
            self.zones
                .write()
                .unwrap()
                .insert(zone_identifier.to_string(), Zone::new(zone_identifier));
        }

        // Because we initialize the zone entry above (if it did not already exist), we can be
        // assured that it is safe to unwrap here
        if self.zones.read().unwrap().get(zone_identifier).unwrap().is_expired() {
            let mut records = vec![];
            let mut page = 1;

            loop {
                let mut response = self.get_dns_records_in_zone(zone_identifier, page).await?;
                records.append(&mut response.result);

                let total_pages = response
                    .result_info
                    .and_then(|info| info.get("total_pages").and_then(|total_pages| total_pages.as_u64()))
                    .unwrap_or(0);

                if (page as u64) < total_pages {
                    page += 1;
                } else {
                    break;
                }
            }

            self.zones
                .write()
                .unwrap()
                .get_mut(zone_identifier)
                .unwrap()
                .populate(records);
        }

        Ok(())
    }

    pub fn cache_size(&self, zone_identifier: &str) -> usize {
        self.zones
            .read()
            .unwrap()
            .get(zone_identifier)
            .map(|zone| zone.dns_cache.dns_records.len())
            .unwrap_or(0)
    }

    pub async fn with_zone<F, R>(&self, zone_identifier: &str, f: F) -> Result<R>
    where
        F: FnOnce(&Zone) -> R,
    {
        self.populate_zone_cache(zone_identifier).await?;

        let guard = self.zones.read().unwrap();
        let zone = guard.get(zone_identifier).unwrap();

        Ok(f(zone))
    }
}

#[derive(Debug)]
pub struct DnsCache {
    domain_to_ids: HashMap<String, Vec<String>>,
    dns_records: HashMap<String, DnsRecord>,
    expires_at: Instant,
}

impl DnsCache {}

#[derive(Debug)]
pub struct Zone {
    identifier: String,
    dns_cache: DnsCache,
}

impl Zone {
    pub fn new(identifier: &str) -> Self {
        Self {
            identifier: identifier.to_string(),
            dns_cache: DnsCache {
                domain_to_ids: HashMap::new(),
                dns_records: HashMap::new(),
                expires_at: Instant::now(),
            },
        }
    }

    pub fn is_expired(&self) -> bool {
        self.dns_cache.expires_at <= Instant::now()
    }

    pub fn get_record_for_id(&self, id: &str) -> Option<&DnsRecord> {
        if !self.is_expired() {
            self.dns_cache.dns_records.get(id)
        } else {
            None
        }
    }

    pub fn get_records_for_domain(&self, domain: &str) -> Vec<&DnsRecord> {
        self.dns_cache
            .domain_to_ids
            .get(domain)
            .map(|ids| ids.iter().filter_map(|id| self.get_record_for_id(id)).collect())
            .unwrap_or_else(Vec::new)
    }

    pub fn populate(&mut self, records: Vec<DnsRecord>) {
        for record in records.into_iter() {
            if let Some(ids) = self.dns_cache.domain_to_ids.get_mut(&record.name) {
                ids.push(record.id.clone());
            } else {
                self.dns_cache
                    .domain_to_ids
                    .insert(record.name.clone(), vec![record.id.clone()]);
            }

            self.dns_cache.dns_records.insert(record.id.clone(), record);
        }

        self.dns_cache.expires_at = Instant::now().checked_add(Duration::from_secs(60)).unwrap();
    }
}

#[async_trait]
impl DNSProviderOps for CloudFlareClient {
    async fn ensure_record(&self, domain: &str, content: cloudflare::endpoints::dns::DnsContent) -> Result<()> {
        let domain = &domain.to_lowercase();
        let zone_identifier = self.get_zone_identifier(domain).await?.id;

        // Populate the zone cache for this zone if needed
        self.populate_zone_cache(&zone_identifier).await?;

        let (found_records, found_id) = {
            // `populate_zone_cache` guarantees that the `zones` has at worst an empty zone set
            let guard = self.zones.read().unwrap();
            let zone = guard.get(&zone_identifier).unwrap();

            let dns_records = zone.get_records_for_domain(domain);

            // If any of the records found for the domain actually match, then return early
            for record in &dns_records {
                if record.name == *domain && content_equals(record.content.clone(), content.clone()) {
                    info!("dns record for domain `{}` already exists: {:?}", domain, content);

                    return Ok(());
                }
            }

            let found_records = !dns_records.is_empty();
            let found_id = dns_records[0].id.clone();

            (found_records, found_id)
        };

        log::debug!("Ensuring  {:?}. Found records count: {} First id found: {}", content, found_records, found_id);

        // If do not have a DNS record create it.
        if !found_records {
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

        let is_a_record = matches!(content, cloudflare::endpoints::dns::DnsContent::A { content: _ });
        let is_aaaa_record = matches!(content, cloudflare::endpoints::dns::DnsContent::AAAA { content: _ });
        let is_cname_record = matches!(content, cloudflare::endpoints::dns::DnsContent::CNAME { content: _ });

        if domain.starts_with("_acme-challenge.") || is_a_record || is_aaaa_record || is_cname_record {
            if found_records {
                // TODO: handle this better, match on the record type.
                bail!(
                    "we don't know which DNS record to update for domain `{}`: {:?}",
                    domain,
                    content
                );
            }

            // Update the record.
            let _dns_record = self
                .request(&dns::UpdateDnsRecord {
                    zone_identifier: &zone_identifier,
                    identifier: &found_id,
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
        let zone_identifier = self.get_zone_identifier(domain).await?.id;

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
