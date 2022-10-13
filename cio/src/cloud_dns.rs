use anyhow::Result;
use async_trait::async_trait;
use cloudflare::endpoints::dns::DnsContent;
use google_dns1::api::ResourceRecordSet;
use log::info;
use std::{
    collections::HashSet,
    sync::Arc,
};

use crate::dns_providers::DNSProviderOps;

pub struct CloudDNSClient {
    project: String,
    inner: Arc<dyn google_dns1::client::Hub + Send + Sync>
}

impl CloudDNSClient {
    fn translate_domain_to_zone(domain: &str) -> String {
        unimplemented!()
    }

    fn resource_record_set(name: String, type_: String, value: String) -> ResourceRecordSet {
        ResourceRecordSet {
            kind: None,
            name: Some(name),
            routing_policy: None,
            rrdatas: Some(vec![value]),
            signature_rrdatas: None,
            ttl: Some(1),
            type_: Some(type_),
        }
    }

    fn create_record_set_from_cloudflare_content(domain: &str, cf_record: &DnsContent) -> ResourceRecordSet {
        let extracted = match cf_record {
            DnsContent::A { content } => ("A", content.to_string()),
            DnsContent::AAAA { content } => ("AAAA", content.to_string()),
            DnsContent::CNAME { content } => ("CNAME", content),
            DnsContent::NS { content } => ("NS", content),
            DnsContent::MX { content, priority } => ("MX", priority.to_string() + " " + content),
            DnsContent::TXT { content } => ("TXT", content),
            DnsContent::SRV { content } => ("SRV", content),
        };

        Self::resource_record_set(domain.to_string(), extracted.0.to_string(), extracted.1.to_string())
    }
}

trait Covers {
    fn covers(self, other: &Self) -> bool;
}

// Note that "covers" is not reflexive
impl Covers for ResourceRecordSet {
    fn covers(self, other: &Self) -> bool {
        let type_match = self.type_.and_then(|type_| other.type_.map(|other_type| type_ == other_type)).unwrap_or(false);

        let data_lines = self.rrdatas.map(|data| data.split("\n").iter().collect::<HashSet<&str>>()).unwrap_or_else(|| HashSet::default());
        let other_data_lines = other.rrdatas.map(|data| data.split("\n").iter().collect::<HashSet<&str>>()).unwrap_or_else(|| HashSet::default());

        true
    }
}

#[async_trait]
impl DNSProviderOps for CloudDNSClient {
    /// Ensure the record exists and has the correct information.
    async fn ensure_record(&self, domain: &str, content: cloudflare::endpoints::dns::DnsContent) -> Result<()> {
        let zone_name = self.translate_domain_to_zone(&domain);

        // Fetch all of the record sets
        let response = self.inner.resource_record_sets().list(&self.project, &zone_name).doit().await;

        CloudDNSClient::create_record_set_from_cloudflare_content(domain, &content);

        unimplemented!()
        // // Determine if our current Cloud DNS configuration covers this RecordSet. Our RecordSet may
        // // only be a partial record, and as such matches may only be partial

        // let zone_identifier = self.get_zone_identifier(domain).await?.id;

        // // Populate the zone cache for this zone if needed
        // self.populate_zone_cache(&zone_identifier).await?;

        // let lookup_result = {
        //     // `populate_zone_cache` guarantees that the `zones` has at worst an empty zone set
        //     let guard = self.zones.read().unwrap();
        //     let zone = guard.get(&zone_identifier).unwrap();

        //     let dns_records = zone.get_records_for_domain(domain);

        //     // If any of the records found for the domain actually match, then return early
        //     for record in &dns_records {
        //         if record.name == *domain && content_equals(record.content.clone(), content.clone()) {
        //             info!("dns record for domain `{}` already exists: {:?}", domain, content);

        //             return Ok(());
        //         }
        //     }

        //     LookupResult {
        //         first_non_match_id: if !dns_records.is_empty() {
        //             Some(dns_records[0].id.clone())
        //         } else {
        //             None
        //         },
        //         response_count: dns_records.len(),
        //     }
        // };

        // log::debug!(
        //     "Ensuring  {:?}. Found records count: {} First id found: {:?}",
        //     content,
        //     lookup_result.response_count,
        //     lookup_result.first_non_match_id
        // );

        // if let Some(first_non_match_id) = &lookup_result.first_non_match_id {
        //     let is_a_record = matches!(content, cloudflare::endpoints::dns::DnsContent::A { content: _ });
        //     let is_aaaa_record = matches!(content, cloudflare::endpoints::dns::DnsContent::AAAA { content: _ });
        //     let is_cname_record = matches!(content, cloudflare::endpoints::dns::DnsContent::CNAME { content: _ });

        //     if domain.starts_with("_acme-challenge.") || is_a_record || is_aaaa_record || is_cname_record {
        //         if lookup_result.response_count > 1 {
        //             // TODO: handle this better, match on the record type.
        //             bail!(
        //                 "we don't know which DNS record to update for domain `{}`: {:?}",
        //                 domain,
        //                 content
        //             );
        //         }

        //         // Update the record.
        //         let _dns_record = self
        //             .request(&dns::UpdateDnsRecord {
        //                 zone_identifier: &zone_identifier,
        //                 identifier: first_non_match_id,
        //                 params: dns::UpdateDnsRecordParams {
        //                     name: domain,
        //                     content: content.clone(),
        //                     // This is the min.
        //                     ttl: Some(120),
        //                     proxied: None,
        //                 },
        //             })
        //             .await?
        //             .result;

        //         info!("updated dns record for domain `{}`: {:?}", domain, content);
        //     } else {
        //         // Create the DNS record.
        //         // We likely want many of these if we got here.
        //         let _dns_record = self
        //             .request(&dns::CreateDnsRecord {
        //                 zone_identifier: &zone_identifier,
        //                 params: dns::CreateDnsRecordParams {
        //                     name: domain,
        //                     content: content.clone(),
        //                     // This is the min.
        //                     ttl: Some(120),
        //                     proxied: None,
        //                     priority: None,
        //                 },
        //             })
        //             .await?
        //             .result;

        //         info!("created dns record for existing domain `{}`: {:?}", domain, content);
        //     }
        // } else {
        //     // If do not have a DNS record create it.
        //     // Create the DNS record.
        //     let _dns_record = self
        //         .request(&dns::CreateDnsRecord {
        //             zone_identifier: &zone_identifier,
        //             params: dns::CreateDnsRecordParams {
        //                 name: domain,
        //                 content: content.clone(),
        //                 // This is the min.
        //                 ttl: Some(120),
        //                 proxied: None,
        //                 priority: None,
        //             },
        //         })
        //         .await?
        //         .result;

        //     info!("created dns record for domain `{}`: {:?}", domain, content);
        // }

        // Ok(())
    }

    /// Delete the record if it exists.
    async fn delete_record(&self, domain: &str, content: cloudflare::endpoints::dns::DnsContent) -> Result<()> {
        unimplemented!()
    }
}
