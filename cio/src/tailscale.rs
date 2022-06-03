use std::collections::BTreeMap;

use anyhow::Result;
use chrono::{Duration, Utc};
use cloudflare::endpoints::dns;
use log::info;

use crate::companies::Company;

/// When we generate VMs for the console repo on every branch we get lingering
/// Tailscale devices that need to cleaned up when they are no longer active.
/// This function does that.
pub async fn cleanup_old_tailscale_devices(company: &Company) -> Result<()> {
    if company.tailscale_api_key.is_empty() {
        info!(
            "skipping `cleanup_old_tailscale_devices` for company `{}`",
            company.name
        );

        // Return early.
        return Ok(());
    }

    // Initialize the Tailscale API.
    let tailscale = company.authenticate_tailscale();

    // Get the devices.
    let devices = tailscale.list_devices().await?;

    // Create the array of links.
    for device in devices {
        if !device.hostname.starts_with("console-git-") {
            // Continue early.
            // We only care about the hostnames we have from the console generated
            // VMs.
            continue;
        }

        let last_seen_duration = Utc::now() - device.last_seen;
        if last_seen_duration > Duration::days(1) {
            info!(
                "deleting tailscale device {}, last seen duration {:?}",
                device.name, last_seen_duration
            );
            tailscale.delete_device(&device.id).await?;
        }
    }

    info!("cleaned up old tailscale devices successfully");

    Ok(())
}

/// When we generate VMs for the console repo, we leave behind a lot of DNS records
/// in Cloudflare. This function cleans these up when the tailscale device is no longer
/// active.
pub async fn cleanup_old_tailscale_cloudflare_dns(company: &Company) -> Result<()> {
    if company.tailscale_api_key.is_empty() || company.name != "Oxide" {
        info!(
            "skipping `cleanup_old_tailscale_cloudflare_dns` for company `{}`",
            company.name
        );

        // Return early.
        return Ok(());
    }

    // Initialize the Tailscale API.
    let tailscale = company.authenticate_tailscale();

    // Get the devices.
    let devices = tailscale.list_devices().await?;

    // Create the array of links.
    let tailscale_devices: BTreeMap<String, String> = devices
        .iter()
        .map(|device| {
            (
                device.hostname.trim_end_matches("-2").to_string(),
                device.id.to_string(),
            )
        })
        .collect();

    // Initialize the Cloudflare API.
    let cloudflare = company.authenticate_cloudflare()?;

    // List the DNS records.
    let domain = "oxide.computer";
    let zone_identifier = &cloudflare.get_zone_identifier(domain).await?;
    let dns_records = cloudflare
        .request(&dns::ListDnsRecords {
            zone_identifier,
            params: dns::ListDnsRecordsParams {
                // From: https://api.cloudflare.com/#dns-records-for-a-zone-list-dns-records
                per_page: Some(5000),
                ..Default::default()
            },
        })
        .await?
        .result;

    for dns_record in dns_records {
        if !dns_record.name.starts_with("console-git-") {
            continue;
        }

        if !dns_record.name.ends_with(".internal.oxide.computer") {
            continue;
        }

        let name = dns_record.name.replace(".internal.oxide.computer", "");

        // If it does not exist in Tailscale, delete it.
        if !tailscale_devices.contains_key(&name) {
            info!("deleting dns record {}", name);
            cloudflare
                .request(&dns::DeleteDnsRecord {
                    zone_identifier,
                    identifier: &dns_record.id,
                })
                .await?;
        }
    }

    info!("cleaned up old tailscale dns records in cloudflare successfully");

    Ok(())
}
