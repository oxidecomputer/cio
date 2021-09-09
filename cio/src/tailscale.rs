use chrono::{Duration, Utc};
use log::info;

use crate::companies::Company;

/// When we generate VMs for the console repo on every branch we get lingering
/// Tailscale devices that need to cleaned up when they are no longer active.
/// This function does that.
pub async fn cleanup_old_tailscale_devices(company: &Company) {
    // Initialize the Tailscale API.
    let tailscale = company.authenticate_tailscale();

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
            info!(
                "deleting tailscale device {}, last seen duration {:?}",
                device.name, last_seen_duration
            );
            tailscale.delete_device(&device.id).await.unwrap();
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{companies::Company, db::Database, tailscale::cleanup_old_tailscale_devices};

    #[ignore]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_cron_tailscale() {
        let db = Database::new();

        // Get the company id for Oxide.
        // TODO: split this out per company.
        let oxide = Company::get_from_db(&db, "Oxide".to_string()).unwrap();

        cleanup_old_tailscale_devices(&oxide).await;
    }
}
