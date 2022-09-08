use anyhow::Result;
use async_trait::async_trait;
use google_drive::{
    traits::{DriveOps, FileOps},
    Client as GoogleDrive,
};

use super::{
    RFDPdf,
    PDFStorage
};

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
