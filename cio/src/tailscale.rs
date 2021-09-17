use anyhow::Result;
use chrono::{Duration, Utc};
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

    info!("devices: {:?}", devices);

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
