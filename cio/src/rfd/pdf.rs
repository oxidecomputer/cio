use anyhow::Result;
use async_trait::async_trait;
use log::info;

use crate::{companies::Company, db::Database, features::Features};

use super::{GitHubRFDRepo, RFDNumber};

#[async_trait]
pub trait PDFStorage {
    async fn store_rfd_pdf(&self, pdf: &RFDPdf) -> Result<String>;
}

pub struct RFDPdf {
    pub number: RFDNumber,
    pub filename: String,
    pub contents: Vec<u8>,
}

pub struct RFDPdfUpload {
    pub github_url: Option<String>,
    pub google_drive_url: Option<String>,
}

impl RFDPdf {
    /// Upload the PDF to GitHub and/or Google Drive depending on which backends are supported
    pub async fn upload(&self, db: &Database, company: &Company) -> Result<RFDPdfUpload> {
        if Features::is_enabled("RFD_PDFS_IN_GITHUB") || Features::is_enabled("RFD_PDFS_IN_GOOGLE_DRIVE") {
            // Create or update the file in the github repository.
            let github_url = if Features::is_enabled("RFD_PDFS_IN_GITHUB") {
                let repo = GitHubRFDRepo::new(company).await?;
                let branch = repo.branch(repo.default_branch.clone());

                Some(branch.store_rfd_pdf(self).await?)
            } else {
                None
            };

            let google_drive_url = if Features::is_enabled("RFD_PDFS_IN_GOOGLE_DRIVE") {
                Some(company.authenticate_google_drive(db).await?.store_rfd_pdf(self).await?)
            } else {
                None
            };

            Ok(RFDPdfUpload {
                github_url,
                google_drive_url,
            })
        } else {
            info!(
                "No RFD PDF storage locations are configured. Skipping PDF generation for RFD {}.",
                self.number.as_number_string()
            );

            Ok(RFDPdfUpload {
                github_url: None,
                google_drive_url: None,
            })
        }
    }
}
