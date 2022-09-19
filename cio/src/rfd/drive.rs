use anyhow::Result;
use async_trait::async_trait;
use google_drive::{
    traits::{DriveOps, FileOps},
    Client as GoogleDrive,
};
use log::info;

use crate::{companies::Company, db::Database};

use super::{PDFStorage, RFDPdf, RFDs, RFD};

#[async_trait]
impl PDFStorage for GoogleDrive {
    async fn store_rfd_pdf(&self, pdf: &RFDPdf) -> Result<String> {
        // Figure out where our directory is.
        // It should be in the shared drive : "Automated Documents"/"rfds"
        let shared_drive = self.drives().get_by_name("Automated Documents").await?;
        let drive_id = shared_drive.id.to_string();

        // Get the directory by the name.
        let parent_id = self.files().create_folder(&drive_id, "", "rfds").await?;

        // Create or update the file in the google_drive.
        let drive_file = self
            .files()
            .create_or_update(&drive_id, &parent_id, &pdf.filename, "application/pdf", &pdf.contents)
            .await?;

        Ok(format!("https://drive.google.com/open?id={}", drive_file.id))
    }
}

// This code has been broken for a while and is therefore only auditing deletes until we verify it.
pub async fn cleanup_rfd_pdfs(db: &Database, company: &Company) -> Result<()> {
    // Get all the rfds from the database.
    let rfds: Vec<RFD> = RFDs::get_from_db(db, company.id).await?.into();

    // Clippy warns about this collect. We could instead move the rfds.iter() call down to the loop
    // but this form feels clearer and is minimal performance hit for a function executes rarely
    #[allow(clippy::needless_collect)]
    let valid_pdf_filenames = rfds.iter().map(|rfd| rfd.get_pdf_filename()).collect::<Vec<String>>();

    let drive_client = company.authenticate_google_drive(db).await?;

    // Figure out where our directory is.
    // It should be in the shared drive : "Automated Documents"/"rfds"
    let shared_drive = drive_client.drives().get_by_name("Automated Documents").await?;
    let drive_id = shared_drive.id.to_string();

    // Get the directory by the name.
    let parent_id = drive_client.files().create_folder(&drive_id, "", "rfds").await?;

    let drive_files = drive_client
        .files()
        .list_all(
            "drive",                                // corpa
            &drive_id,                              // drive id
            true,                                   // include items from all drives
            "",                                     // include permissions for view
            false,                                  // include team drive items
            "",                                     // order by
            &format!("'{}' in parents", parent_id), // query
            "",                                     // spaces
            true,                                   // supports all drives
            false,                                  // supports team drives
            "",                                     // team drive id
        )
        .await?;

    // Iterate over the files and if the name does not equal our name, then nuke it.
    for df in drive_files {
        if !valid_pdf_filenames.contains(&df.name) {
            info!(
                r#"Planning to delete "{}" from Google Drive as it does not much a valid known RFD pdf name"#,
                df.name
            );
        }
    }

    Ok(())
}
