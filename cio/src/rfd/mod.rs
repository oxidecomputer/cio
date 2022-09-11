use serde::{Deserialize, Serialize};
use std::fmt;

mod changelog;
mod content;
mod drive;
mod github;
mod new_rfd;
mod pdf;
mod search;

pub use changelog::send_rfd_changelog;
pub use content::RFDContent;
pub use github::{GitHubRFDBranch, GitHubRFDReadme, GitHubRFDReadmeLocation, GitHubRFDRepo, GitHubRFDUpdate};
pub use pdf::{cleanup_rfd_pdfs, PDFStorage, RFDPdf};
pub use search::RFDSearchIndex;

#[derive(Debug, Copy, Clone, Deserialize, Serialize)]
pub struct RFDNumber(i32);

impl RFDNumber {
    /// Get the path to where the source contents of this RFD exists in the RFD repo.
    pub fn repo_directory(&self) -> String {
        format!("/rfd/{}", self.as_number_string())
    }

    /// Get an RFD number in its expanded form with leading 0s
    pub fn as_number_string(&self) -> String {
        let mut number_string = self.0.to_string();
        while number_string.len() < 4 {
            number_string = format!("0{}", number_string);
        }

        number_string
    }
}

impl fmt::Display for RFDNumber {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<i32> for RFDNumber {
    fn from(num: i32) -> Self {
        Self(num)
    }
}

impl From<&i32> for RFDNumber {
    fn from(num: &i32) -> Self {
        Self(*num)
    }
}

impl From<RFDNumber> for i32 {
    fn from(num: RFDNumber) -> Self {
        num.0
    }
}
