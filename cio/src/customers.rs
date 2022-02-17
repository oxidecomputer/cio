use std::str::from_utf8;

use airtable_api::Record;
use anyhow::Result;

use log::info;

use crate::{companies::Company, core::CustomerInteraction, utils::get_file_content_from_repo};

/// Sync meeting notes with the content from the notes.
pub async fn sync_customer_meeting_notes(company: &Company) -> Result<()> {
    // Initialize the Airtable client.
    let airtable = company.authenticate_airtable(&company.airtable_base_id_customer_leads);

    let github = company.authenticate_github()?;

    // Get the current customer interactions list from airtable.
    let records: Vec<Record<CustomerInteraction>> = airtable
        .list_records(
            crate::airtable::AIRTABLE_CUSTOMER_INTERACTIONS_TABLE,
            crate::airtable::AIRTABLE_GRID_VIEW,
            vec![],
        )
        .await?;

    // Iterate over the airtable records and update the notes where we have a link to notes in
    // GitHub.
    for mut record in records {
        if record.fields.notes_link.is_empty() {
            // Continue early if we don't have a link to notes.
            continue;
        }

        let notes_path = record.fields.notes_link.replace(
            &format!("https://github.com/{}/reports/blob/master", company.github_org),
            "",
        );

        // Get the reports repo client.
        let (content, _) = get_file_content_from_repo(&github, &company.github_org, "reports", "", &notes_path).await?;
        let decoded = from_utf8(&content)?.trim().to_string();
        // Compare the notes and see if we need to update them.
        if record.fields.notes == decoded {
            // They are the same so we can continue through the loop.
            continue;
        }

        // Update the customer interaction in airtable.
        record.fields.notes = decoded;

        // Send the updated record to the airtable client.
        // Batch can only handle 10 at a time.
        airtable
            .update_records(
                crate::airtable::AIRTABLE_CUSTOMER_INTERACTIONS_TABLE,
                vec![record.clone()],
            )
            .await?;

        info!(
            "updated customer interaction record with notes for {} {} {}",
            record.fields.name, record.fields.company[0], record.fields.date
        );
    }

    Ok(())
}
