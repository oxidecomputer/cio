use anyhow::Result;
use async_trait::async_trait;
use log::info;

use crate::{
    companies::Company,
    db::Database,
    features::Features,
};

use super::{
    GitHubRFDRepo,
    RFDNumber,
};

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
    pub async fn upload(
        &self,
        db: &Database,
        company: &Company,
    ) -> Result<RFDPdfUpload> {
        if Features::is_enabled("RFD_PDFS_IN_GITHUB") || Features::is_enabled("RFD_PDFS_IN_GOOGLE_DRIVE") {
            // Create or update the file in the github repository.
            let github_url = if Features::is_enabled("RFD_PDFS_IN_GITHUB") {
                let repo = GitHubRFDRepo::new(company).await?;
                let branch = repo.branch(repo.default_branch.clone());

                Some(branch.store_rfd_pdf(&self).await?)
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

pub async fn cleanup_rfd_pdfs(db: &Database, company: &Company) -> Result<()> {
    // // Get all the rfds from the database.
    // let rfds = RFDs::get_from_db(db, company.id).await?;
    // let github = company.authenticate_github()?;

    // // Check if the repo exists, if not exit early.
    // if let Err(e) = github.repos().get(&company.github_org, "rfd").await {
    //     if e.to_string().contains("404") {
    //         return Ok(());
    //     } else {
    //         bail!("checking for rfd repo failed: {}", e);
    //     }
    // }

    // // Get all the PDF files.
    // let result = github
    //     .repos()
    //     .get_content_vec_entries(
    //         &company.github_org,
    //         "rfd",
    //         "/pdfs/",
    //         "", // leaving the branch blank gives us the default branch
    //     )
    //     .await
    //     .map_err(into_octorust_error);

    // match result {
    //     Ok(files) => {
    //         let mut github_pdf_files: BTreeMap<String, String> = Default::default();
    //         for file in files {
    //             // We will store these in github_pdf_files as <{name}, {sha}>. So we can more easily delete
    //             // them.
    //             github_pdf_files.insert(file.name.to_string(), file.sha.to_string());
    //         }

    //         let drive_client = company.authenticate_google_drive(db).await?;

    //         // Figure out where our directory is.
    //         // It should be in the shared drive : "Automated Documents"/"rfds"
    //         let shared_drive = drive_client.drives().get_by_name("Automated Documents").await?;
    //         let drive_id = shared_drive.id.to_string();

    //         // Get the directory by the name.
    //         let parent_id = drive_client.files().create_folder(&drive_id, "", "rfds").await?;

    //         // Iterate over the RFD and cleanup any PDFs with the wrong name.
    //         for rfd in rfds {
    //             let pdf_file_name = rfd.get_pdf_filename();

    //             // First let's do Google Drive.
    //             // Search for files with that rfd number string.
    //             let drive_files = drive_client
    //                 .files()
    //                 .list_all(
    //                     "drive",                                                                           // corpa
    //                     &drive_id,                                                                         // drive id
    //                     true,  // include items from all drives
    //                     "",    // include permissions for view
    //                     false, // include team drive items
    //                     "",    // order by
    //                     &format!("name contains '{}' and '{}' in parents", &rfd.number_string, parent_id), // query
    //                     "",    // spaces
    //                     true,  // supports all drives
    //                     false, // supports team drives
    //                     "",    // team drive id
    //                 )
    //                 .await?;
    //             // Iterate over the files and if the name does not equal our name, then nuke it.
    //             for df in drive_files {
    //                 if df.name == pdf_file_name {
    //                     info!("keeping Google Drive PDF of RFD `{}`: {}", rfd.number_string, df.name);
    //                     continue;
    //                 }

    //                 info!("deleting Google Drive PDF of RFD `{}`: {}", rfd.number_string, df.name);
    //                 // Delete the file from our drive.
    //                 drive_client.files().delete(&df.id, true, true).await?;
    //             }

    //             // Now let's do GitHub.
    //             // Iterate over our github_pdf_files and delete any that do not match.
    //             for (gf_name, sha) in github_pdf_files.clone() {
    //                 if gf_name == pdf_file_name {
    //                     info!("keeping GitHub PDF of RFD `{}`: {}", rfd.number_string, gf_name);
    //                     // Remove it from our btree map.
    //                     github_pdf_files.remove(&gf_name);
    //                     continue;
    //                 }

    //                 if gf_name.contains(&rfd.number_string) {
    //                     // Remove it from GitHub.
    //                     info!("deleting GitHub PDF of RFD `{}`: {}", rfd.number_string, gf_name);
    //                     github
    //                         .repos()
    //                         .delete_file(
    //                             &company.github_org,
    //                             "rfd",
    //                             &format!("pdfs/{}", gf_name),
    //                             &octorust::types::ReposDeleteFileRequest {
    //                                 message: format!(
    //                                     "Deleting file content {} programatically\n\nThis is done from \
    //                                     the cio repo cio::cleanup_rfd_pdfs function.",
    //                                     gf_name
    //                                 ),
    //                                 sha: sha.to_string(),
    //                                 committer: None,
    //                                 author: None,
    //                                 branch: "".to_string(),
    //                             },
    //                         )
    //                         .await?;

    //                     // Remove it from our btree map.
    //                     github_pdf_files.remove(&gf_name);
    //                 }
    //             }
    //         }

    //         Ok(())
    //     }
    //     Err(err) => {
    //         if err.kind == OctorustErrorKind::NotFound {
    //             // If the /pdf directory is not found then there is nothing to do
    //             Ok(())
    //         } else {
    //             // Otherwise something else has gone wrong and we need to return the original error
    //             Err(err.into_inner())
    //         }
    //     }
    // }

    Ok(())
} 