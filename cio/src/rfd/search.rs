use anyhow::Result;

use super::RFDNumber;

pub struct RFDSearchIndex {}

impl RFDSearchIndex {
    /// Trigger updating the search index for the RFD.
    pub async fn index_rfd(rfd_number: &RFDNumber) -> Result<()> {
        let client = reqwest::Client::new();
        let req = client.put(&format!("https://rfd.shared.oxide.computer/api/search/{}", rfd_number));
        req.send().await?;

        Ok(())
    }
}
