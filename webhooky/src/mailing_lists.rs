use anyhow::Result;
use cio_api::{
    db::Database, mailerlite::Mailerlite, mailing_list::MailingListSubscriber, rack_line::RackLineSubscriber,
};

pub async fn sync_pending_mailing_list_subscribers(db: &Database) -> Result<()> {
    let client = Mailerlite::new()?;
    let subscribers = client.pending_mailing_list_subscribers().await?;

    log::info!("Processing {} mailing list subscribers", subscribers.len());

    // For each subscriber, add them to our database if we have not seen them before. Subscribers
    // are considered unique based upon their email address. Independent of if they were previously
    // an existing subscriber, we mark their Mailerlite record as processed.
    for subscriber in subscribers.into_iter() {
        let existing = MailingListSubscriber::get_from_db(db, subscriber.email.clone()).await;

        if existing.is_none() {
            log::info!(
                "Mailerlite subscriber {} needs to be added to mailing list",
                subscriber.id
            );
            // let new_subscriber: NewMailingListSubscriber = subscriber.clone().into();
            // let _ = new_subscriber.upsert(db).await?;
        } else {
            log::info!(
                "Mailerlite subscriber {} already exists in mailing list database",
                subscriber.id
            );
        }

        if let Err(err) = client.mark_mailing_list_subscriber(&subscriber.email).await {
            log::warn!(
                "Failed to mark mailerlite subscriber {} as processed due to {:?}",
                subscriber.id,
                err
            );
        }
    }

    Ok(())
}

pub async fn sync_pending_wait_list_subscribers(db: &Database) -> Result<()> {
    let client = Mailerlite::new()?;
    let subscribers = client.pending_wait_list_subscribers().await?;

    log::info!("Processing {} wait list subscribers", subscribers.len());

    for subscriber in subscribers.into_iter() {
        let existing = RackLineSubscriber::get_from_db(db, subscriber.email.clone()).await;

        if existing.is_none() {
            log::info!("Mailerlite subscriber {} needs to be added to wait list", subscriber.id);
            // let new_subscriber: NewRackLineSubscriber = subscriber.clone().into();
            // let _ = new_subscriber.upsert(db).await?;
        } else {
            log::info!(
                "Mailerlite subscriber {} already exists in wait list database",
                subscriber.id
            );
        }

        if let Err(err) = client.mark_wait_list_subscriber(&subscriber.email).await {
            log::warn!(
                "Failed to mark mailerlite subscriber {} as processed due to {:?}",
                subscriber.id,
                err
            );
        }
    }

    Ok(())
}
