use serde::{Deserialize, Serialize};
use std::fmt;

mod changelog;
mod content;
mod drive;
mod github;
mod new_rfd;
mod pdf;
mod search;

use content::RFDContent;
use github::{
    GitHubRFDRepo,
    GitHubRFDBranch,
    GitHubRFDReadme,
    GitHubRFDPullRequest
};
use pdf::{
    PDFStorage,
    RFDPdf
};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RFDNumber(i32);

impl RFDNumber {
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
