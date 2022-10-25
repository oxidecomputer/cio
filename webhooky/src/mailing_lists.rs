use anyhow::Result;
use cio_api::mailerlite::Mailerlite;

pub async fn sync_pending_mailing_list_subscribers() -> Result<()> {
    let client = Mailerlite::new()?;
    let subscribers = client.pending_mailing_list_subscribers().await?;

    Ok(())
}

pub async fn sync_pending_wait_list_subscribers() -> Result<()> {
    let client = Mailerlite::new()?;
    let subscribers = client.pending_wait_list_subscribers().await?;

    Ok(())
}