use anyhow::Result;
use chrono::{Duration, Utc};
use sendgrid_api::{traits::MailOps, Client as SendGrid};

use crate::rfds::RFDs;
use crate::companies::Company;
use crate::db::Database;

/// Create a changelog email for the RFDs.
pub async fn send_rfd_changelog(db: &Database, company: &Company) -> Result<()> {
    let rfds = RFDs::get_from_db(db, company.id).await?;

    if rfds.0.is_empty() {
        // Return early.
        return Ok(());
    }

    let github = company.authenticate_github()?;
    let seven_days_ago = Utc::now() - Duration::days(7);
    let week_format = format!(
        "from {} to {}",
        seven_days_ago.format("%m-%d-%Y"),
        Utc::now().format("%m-%d-%Y")
    );

    let mut changelog = format!("Changes to RFDs for the week {}:\n", week_format);

    // Iterate over the RFDs.
    for rfd in rfds {
        let changes = rfd.get_weekly_changelog(&github, seven_days_ago, company).await?;
        if !changes.is_empty() {
            changelog += &format!("\n{} {}\n{}", rfd.name, rfd.short_link, changes);
        }
    }

    // Initialize the SendGrid clVient.
    let sendgrid_client = SendGrid::new_from_env();

    // Send the message.
    sendgrid_client
        .mail_send()
        .send_plain_text(
            &format!("RFD changelog for the week from {}", week_format),
            &changelog,
            &[format!("all@{}", company.gsuite_domain)],
            &[],
            &[],
            &format!("rfds@{}", company.gsuite_domain),
        )
        .await?;

    Ok(())
}
