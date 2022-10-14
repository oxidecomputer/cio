use anyhow::Result;
use async_trait::async_trait;
use google_dns1::{api::ResourceRecordSet, Dns, hyper_rustls, hyper};
use std::sync::Arc;

use crate::dns_providers::{
    DNSProviderOps,
    DnsRecord,
};

pub struct CloudDnsClient {
    project: String,
    inner: Arc<Dns<hyper_rustls::HttpsConnector<hyper::client::HttpConnector>>>,
}

impl CloudDnsClient {
    fn translate_domain_to_zone(&self, _domain: &str) -> String {
        unimplemented!()
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
        self.name_match(other) &&
        self.type_match(other) &&
        self.rrdatas.as_ref().map(|data| data.contains(&other.content)).unwrap_or(false)
    }
}

#[async_trait]
impl DNSProviderOps for CloudDnsClient {
    /// Ensure the record exists and has the correct information.
    async fn ensure_record(&self, record: DnsRecord) -> Result<()> {
        let zone_name = self.translate_domain_to_zone(&record.name);

        // Find all of the records that match the name and type of the incoming record
        let mut existing_record_sets = self.find_name_and_type_matches(&zone_name, &record).await?;

        // The incoming record may be a subset of an existing record, check to see if there are any
        // records that already cover what this incoming record does.
        for existing_record_set in existing_record_sets.iter() {

            // If any existing record set fully covers our incoming record, then there is nothing
            // left to do
            if existing_record_set.covers(&record) {
                return Ok(())
            }
        }

        // We need to add information to either create a new record set or amend an existing one to
        // handle the incoming record

        // If there are no records matching the (name, type) pair, then we can simply create a new
        // record set
        if existing_record_sets.is_empty() {
            // TODO: Create record set
        } else {

            // We need to determine the record set to add the record to. We expect that for a given
            // (name, type) pair there is at most a single record set. If multiple are found then
            // we fill fail to create. This assumption needs to be tested an verified
            if existing_record_sets.len() == 1 {
                let mut existing_record_set = &mut existing_record_sets[0];

                // Because we checked above that no existing record sets fully covered the incoming
                // record, we know that we can simply add this record to the only existing set

                // This should always be Some, but it is simply to handle both cases
                if let Some(rrdatas) = existing_record_set.rrdatas.as_mut() {
                    rrdatas.push(record.content);
                } else {
                    existing_record_set.rrdatas = Some(vec![record.content]);
                }
            } else {
                log::warn!("Encountered multiple record sets for {}::{}", record.type_, record.name);
            }
        }

        Ok(())
    }

    /// Delete the record if it exists.
    async fn delete_record(&self, record: DnsRecord) -> Result<()> {
        let zone_name = self.translate_domain_to_zone(&record.name);

        // Find all of the records that match the name and type of the incoming record
        let existing_record_sets = self.find_name_and_type_matches(&zone_name, &record).await?;

        // The incoming record may be a subset of an existing record, check to see if there are any
        // records that already cover what this incoming record does.
        for mut existing_record_set in existing_record_sets.into_iter() {

            // If any existing record set fully covers our incoming record, then there is nothing
            // left to do
            if existing_record_set.covers(&record) {

                if let Some(rrdatas) = existing_record_set.rrdatas.as_mut() {
                    rrdatas.retain(|existing_record| existing_record != &record.content);
                }

                // Write the updated record set back to GCP
                let result = self.inner.resource_record_sets().patch(
                    existing_record_set,
                    &self.project,
                    &zone_name,
                    &record.name,
                    &record.type_.to_string(),
                ).doit().await?;

                log::info!("Updated {}::{} record : {:?}", record.type_, record.name, result);
            }
        }

        Ok(())
    }
}
