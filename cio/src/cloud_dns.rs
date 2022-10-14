use anyhow::Result;
use async_trait::async_trait;
use cloudflare::endpoints::dns::DnsContent;
use google_dns1::{api::ResourceRecordSet, Dns};
use log::info;
use std::{
    collections::HashSet,
    sync::Arc,
};

use crate::dns_providers::{
    DNSProviderOps,
    DnsRecord,
    DnsRecordType,
};

pub struct CloudDnsClient {
    project: String,
    inner: Arc<Dns>,
}

impl CloudDnsClient {
    fn translate_domain_to_zone(&self, domain: &str) -> String {
        unimplemented!()
    }

    fn resource_record_set(name: String, type_: DnsRecordType, value: String) -> ResourceRecordSet {
        ResourceRecordSet {
            kind: None,
            name: Some(name),
            rrdatas: Some(vec![value]),
            signature_rrdatas: None,
            ttl: Some(1),
            type_: Some(type_.to_string()),
        }
    }

    async fn find_name_and_type_matches(&self, zone: &str, record: &DnsRecord) -> Result<Vec<ResourceRecordSet>> {
        let mut matches = vec![];

        let (_, response) = self.inner.resource_record_sets().list(&self.project, &zone).doit().await?;
        
        if let Some(sets) = response.rrsets {
            for set in sets {
                if set.name_match(record) && set.type_match(record) {
                    matches.push(set);
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
        self.type_.as_ref().map(|type_| type_ == &other.type_.to_string()).unwrap_or(false)
    }

    fn covers(&self, other: &DnsRecord) -> bool {
        if self.name_match(other) && self.type_match(other) {
            let data_lines = self.rrdatas.as_ref().map(|data| data.iter().map(|s| s.as_str()).collect::<HashSet<&str>>()).unwrap_or_else(|| HashSet::default());
            let other_data_lines = other.content.split("\n").collect::<HashSet<&str>>();

            data_lines.is_superset(&other_data_lines)
        } else {
            false
        }
    }
}