use chrono::{Duration, Utc};
use tailscale_api::Tailscale;
use tracing::instrument;

/// When we generate VMs for the console repo on every branch we get lingering
/// Tailscale devices that need to cleaned up when they are no longer active.
/// This function does that.
#[instrument]
#[inline]
pub async fn cleanup_old_tailscale_devices() {
    // Initialize the Tailscale API.
    let tailscale = Tailscale::new_from_env();
    // Get the devices.
    let devices = tailscale.list_devices().await.unwrap();

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
            println!("Deleting tailscale device {}", device.name);
            tailscale.delete_device(&device.id).await.unwrap();
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::tailscale::cleanup_old_tailscale_devices;

    #[ignore]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_cron_tailscale() {
        cleanup_old_tailscale_devices().await;
    }
}
