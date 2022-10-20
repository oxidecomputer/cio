use anyhow::Result;
use async_trait::async_trait;
use google_dns1::{
    api::{ManagedZone, ResourceRecordSet},
    hyper, hyper_rustls, Dns,
};
use std::{
    collections::HashMap,
    ops::Add,
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};

use crate::dns_providers::{DNSProviderOps, DnsRecord, DnsUpdateMode};

struct ZoneCache {
    zones: Vec<ManagedZone>,
    expires_at: Instant,
}

impl ZoneCache {
    pub fn new(zones: Vec<ManagedZone>, ttl: u64) -> Self {
        ZoneCache {
            zones,
            expires_at: Instant::now().add(Duration::from_secs(ttl)),
        }
    }

    pub fn is_expired(&self) -> bool {
        self.expires_at <= Instant::now()
    }
}

struct RRSetsCache {
    rrsets: HashMap<String, Vec<ResourceRecordSet>>,
    expires_at: Instant,
}

impl RRSetsCache {
    pub fn new(ttl: u64) -> Self {
        RRSetsCache {
            rrsets: HashMap::new(),
            expires_at: Instant::now().add(Duration::from_secs(ttl)),
        }
    }

    pub fn is_expired(&self) -> bool {
        self.expires_at <= Instant::now()
    }
}

type CloudDnsInternalClient = Dns<hyper_rustls::HttpsConnector<hyper::client::HttpConnector>>;

pub struct CloudDnsClient {
    project: String,
    inner: Arc<CloudDnsInternalClient>,
    zone_cache: Arc<RwLock<ZoneCache>>,
    zone_cache_ttl: u64,
    rrsets_cache: Arc<RwLock<RRSetsCache>>,
    rrsets_cache_ttl: u64,
}

impl CloudDnsClient {
    pub fn new(project: String, client: CloudDnsInternalClient) -> Self {
        CloudDnsClient {
            project,
            inner: Arc::new(client),
            zone_cache: Arc::new(RwLock::new(ZoneCache::new(vec![], 0))),
            zone_cache_ttl: 30,
            rrsets_cache: Arc::new(RwLock::new(RRSetsCache::new(0))),
            rrsets_cache_ttl: 30,
        }
    }

    async fn translate_domain_to_zone(&self, domain: &str) -> Result<Option<ManagedZone>> {
        let expired = self.zone_cache.read().unwrap().is_expired();

        if expired {
            let (_, response) = self.inner.managed_zones().list(&self.project).doit().await?;

            if let Some(managed_zones) = response.managed_zones {
                log::info!("[CloudDNS] Updated zone cache with {} zones", managed_zones.len());
                *self.zone_cache.write().unwrap() = ZoneCache::new(managed_zones, self.zone_cache_ttl);
            } else {
                log::info!("[CloudDNS] Zone lookup during cache refresh returned an empty list of zones");
            }
        }

        Ok(self
            .zone_cache
            .read()
            .unwrap()
            .zones
            .iter()
            .find(|managed_zone| {
                // GCP zone DNS names end with a .
                managed_zone
                    .dns_name
                    .as_ref()
                    .map(|dns_name| domain.ends_with(dns_name.trim_end_matches('.')))
                    .unwrap_or(false)
            })
            .cloned())
    }

    async fn find_name_and_type_matches(&self, zone: &str, record: &DnsRecord) -> Result<Vec<ResourceRecordSet>> {
        let expired = self.rrsets_cache.read().unwrap().is_expired();

        if expired {
            *self.rrsets_cache.write().unwrap() = RRSetsCache::new(self.rrsets_cache_ttl);
        }

        let cache_available = self.rrsets_cache.read().unwrap().rrsets.get(zone).is_some();

        if !cache_available {
            let mut rrsets = vec![];
            let mut page_token: Option<String> = None;

            loop {
                let mut req = self
                    .inner
                    .resource_record_sets()
                    .list(&self.project, zone)
                    .max_results(1000);

                if let Some(token) = page_token.take() {
                    req = req.page_token(token.as_str());
                }

                let (_, resp) = req.doit().await?;

                if let Some(mut sets) = resp.rrsets {
                    rrsets.append(&mut sets);
                }

                if resp.next_page_token.is_some() {
                    page_token = resp.next_page_token;
                } else {
                    break;
                }
            }

            log::info!("[CloudDNS] Populating Cloud DNS cache with {} entries", rrsets.len());

            self.rrsets_cache
                .write()
                .unwrap()
                .rrsets
                .insert(zone.to_string(), rrsets);
        }

        let mut matches = vec![];

        if let Some(sets) = self.rrsets_cache.read().unwrap().rrsets.get(zone) {
            for set in sets {
                if set.name_match(record) && set.type_match(record) {
                    matches.push(set.clone());
                }
            }
        }

        Ok(matches)
    }
}

trait RecordMatch<T> {
    fn name_match(&self, other: &T) -> bool;
    fn type_match(&self, other: &T) -> bool;
    fn covers(&self, other: &T) -> bool;
}

impl RecordMatch<DnsRecord> for ResourceRecordSet {
    fn name_match(&self, other: &DnsRecord) -> bool {
        self.name.as_ref().map(|name| name == &other.name).unwrap_or(false)
    }

    fn type_match(&self, other: &DnsRecord) -> bool {
        self.type_
            .as_ref()
            .map(|type_| type_ == &other.type_.to_string())
            .unwrap_or(false)
    }

    fn covers(&self, other: &DnsRecord) -> bool {
        self.name_match(other)
            && self.type_match(other)
            && self
                .rrdatas
                .as_ref()
                .map(|data| data.contains(&other.content))
                .unwrap_or(false)
    }
}

fn to_dns_name(name: &str) -> String {
    name.trim_end_matches('.').to_string() + "."
}

#[async_trait]
impl DNSProviderOps for CloudDnsClient {
    /// Ensure the record exists and has the correct information.
    async fn ensure_record(&self, record: DnsRecord, mode: DnsUpdateMode) -> Result<()> {
        let zone = self
            .translate_domain_to_zone(&record.name)
            .await?
            .ok_or_else(|| anyhow::anyhow!("[CloudDNS] Failed to find zone for {}", record.name))?;
        let zone_name = zone.name.ok_or_else(|| {
            anyhow::anyhow!(
                "[CloudDNS] Unable to operate on zone that does not have a name for {:?}",
                record
            )
        })?;

        // Find all of the records that match the name and type of the incoming record
        let mut existing_record_sets = self.find_name_and_type_matches(&zone_name, &record).await?;

        log::info!(
            "[CloudDNS] Found {} record sets that match the incoming record {:?}",
            existing_record_sets.len(),
            record
        );

        // The incoming record may be a subset of an existing record, check to see if there are any
        // records that already cover what this incoming record does.
        for existing_record_set in existing_record_sets.iter() {
            // If any existing record set fully covers our incoming record, then there is nothing
            // left to do
            if existing_record_set.covers(&record) {
                log::info!("[CloudDNS] Record for {:?} already exists. No updates needed.", record);
                return Ok(());
            }
        }

        // Ensure the record name is appropriately formatted
        let name = to_dns_name(&record.name);

        // We need to add information to either create a new record set or amend an existing one to
        // handle the incoming record

        // If there are no records matching the (name, type) pair, then we can simply create a new
        // record set
        if existing_record_sets.is_empty() {
            log::info!("[CloudDNS] Could not find an existing record for {:?}", record);

            // Write the new record set to GCP
            let result = self
                .inner
                .resource_record_sets()
                .create(
                    ResourceRecordSet {
                        kind: None,
                        name: Some(name),
                        routing_policy: None,
                        rrdatas: Some(vec![record.content.clone()]),
                        signature_rrdatas: None,
                        ttl: Some(1),
                        type_: Some(record.type_.to_string()),
                    },
                    &self.project,
                    &zone_name,
                )
                .doit()
                .await?;

            log::info!(
                "[CloudDNS] Created {}::{} record : {:?}",
                record.type_,
                record.name,
                result
            );
        } else if existing_record_sets.len() == 1 {
            // We need to determine the record set to add the record to. We expect that for a given
            // (name, type) pair there is at most a single record set. If multiple are found then
            // we fill fail to create. This assumption needs to be tested an verified
            let mut existing_record_set = existing_record_sets.remove(0);

            // Because we checked above that no existing record sets fully covered the incoming
            // record, we know that we can simply add this record to the only existing set

            // This should always be Some, but it is simply to handle both cases
            if let Some(rrdatas) = existing_record_set.rrdatas.as_mut() {
                if mode == DnsUpdateMode::Append {
                    rrdatas.push(record.content);
                } else {
                    *rrdatas = vec![record.content];
                }
            } else {
                existing_record_set.rrdatas = Some(vec![record.content]);
            }

            // Write the updated record set back to GCP
            let result = self
                .inner
                .resource_record_sets()
                .patch(
                    existing_record_set,
                    &self.project,
                    &zone_name,
                    &name,
                    &record.type_.to_string(),
                )
                .doit()
                .await?;

            log::info!(
                "[CloudDNS] Updated {}::{} record : {:?}",
                record.type_,
                record.name,
                result
            );
        } else {
            log::warn!(
                "[CloudDNS] Encountered multiple record sets for {}::{}",
                record.type_,
                record.name
            );
        }

        Ok(())
    }

    /// Delete the record if it exists.
    async fn delete_record(&self, record: DnsRecord) -> Result<()> {
        let zone = self
            .translate_domain_to_zone(&record.name)
            .await?
            .ok_or_else(|| anyhow::anyhow!("[CloudDNS] Failed to find zone for {}", record.name))?;
        let zone_name = zone.name.ok_or_else(|| {
            anyhow::anyhow!(
                "[CloudDNS] Unable to operate on zone that does not have a name for {}",
                record.name
            )
        })?;

        // Find all of the records that match the name and type of the incoming record
        let existing_record_sets = self.find_name_and_type_matches(&zone_name, &record).await?;

        // The incoming record may be a subset of an existing record, check to see if there are any
        // records that already cover what this incoming record does.
        for mut existing_record_set in existing_record_sets.into_iter() {
            if existing_record_set.covers(&record) {
                let name = to_dns_name(&record.name);

                let data_count = if let Some(rrdatas) = existing_record_set.rrdatas.as_mut() {
                    rrdatas.retain(|existing_record| existing_record != &record.content);
                    rrdatas.len()
                } else {
                    // rrdatas should always be returned, but we need a fallback
                    0
                };

                if data_count > 0 {
                    // Write the updated record set back to GCP
                    let result = self
                        .inner
                        .resource_record_sets()
                        .patch(
                            existing_record_set,
                            &self.project,
                            &zone_name,
                            &name,
                            &record.type_.to_string(),
                        )
                        .doit()
                        .await?;

                    log::info!(
                        "[CloudDNS] Updated {}::{} record : {:?}",
                        record.type_,
                        record.name,
                        result
                    );
                } else {
                    // Delete the record from GCP
                    let result = self
                        .inner
                        .resource_record_sets()
                        .delete(&self.project, &zone_name, &name, &record.type_.to_string())
                        .doit()
                        .await?;

                    log::info!(
                        "[CloudDNS] Deleted {}::{} record : {:?}",
                        record.type_,
                        record.name,
                        result
                    );
                }
            }
        }

        Ok(())
    }
}
