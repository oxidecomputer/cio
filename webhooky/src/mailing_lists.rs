use anyhow::Result;
use cio_api::{
    db::Database,
    mailerlite::Mailerlite,
    mailing_list::{MailingListSubscriber, NewMailingListSubscriber},
    rack_line::{NewRackLineSubscriber, RackLineSubscriber},
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
            let new_subscriber: NewMailingListSubscriber = subscriber.clone().into();
            let _ = new_subscriber.upsert(db).await?;
        }

        client.mark_mailing_list_subscriber(&subscriber.email).await?;
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
            let new_subscriber: NewRackLineSubscriber = subscriber.clone().into();
            let _ = new_subscriber.upsert(db).await?;
        }

        client.mark_wait_list_subscriber(&subscriber.email).await?;
    }

    Ok(())
}
