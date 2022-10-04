use anyhow::Result;
use async_bb8_diesel::AsyncRunQueryDsl;
use cio_api::{
    rfd::{GitHubRFDRepo, NewRFD, RFDEntry, RFDIndexEntry, RFDs, RFD},
    schema::rfds,
};
use diesel::{ExpressionMethods, QueryDsl};
use dropshot::RequestContext;
use log::warn;
use std::sync::Arc;

use crate::{context::Context, handlers_github::RFDUpdater};

pub async fn handle_rfd_index(
    rqctx: Arc<RequestContext<Context>>,
    offset: i32,
    limit: u32,
) -> Result<Vec<RFDIndexEntry>> {
    let ctx = rqctx.context();

    let rfds = rfds::dsl::rfds
        .order_by(rfds::dsl::number)
        .offset(offset as i64)
        .limit(limit as i64)
        .select((
            rfds::dsl::number,
            rfds::dsl::number_string,
            rfds::dsl::title,
            rfds::dsl::name,
            rfds::dsl::state,
            rfds::dsl::link,
            rfds::dsl::short_link,
            rfds::dsl::rendered_link,
            rfds::dsl::discussion,
            rfds::dsl::authors,
            rfds::dsl::sha,
            rfds::dsl::commit_date,
            rfds::dsl::milestones,
            rfds::dsl::relevant_components,
        ))
        .load_async::<RFDIndexEntry>(ctx.db.pool())
        .await?;

    Ok(rfds)
}

pub async fn handle_rfd_view(rqctx: Arc<RequestContext<Context>>, num: i32) -> Result<Option<RFDEntry>> {
    let ctx = rqctx.context();

    let mut rfd = rfds::dsl::rfds
        .filter(rfds::dsl::number.eq(num))
        .load_async::<RFD>(ctx.db.pool())
        .await?;

    if !rfd.is_empty() {
        let new_rfd: NewRFD = rfd.pop().unwrap().into();
        Ok(Some(new_rfd.into()))
    } else {
        Ok(None)
    }
}

// Sync the rfds with our database.
pub async fn refresh_db_rfds(context: &Context) -> Result<()> {
    let repo = GitHubRFDRepo::new(&context.company).await?;
    let updates = repo.get_rfd_sync_updates().await?;

    let batches = chunk(updates, 3);

    // Iterate over the updates and execute them
    // We should do these concurrently, but limit it to maybe 3 at a time.
    for batch in batches.into_iter() {
        let mut tasks: Vec<tokio::task::JoinHandle<Result<()>>> = vec![];

        for update in batch.into_iter() {
            let task = tokio::spawn(enclose! { (context) async move {
                let updater = RFDUpdater::default();
                updater.handle(&context, &[update]).await?;

                Ok(())
            }});

            tasks.push(task);
        }

        let mut results = vec![];
        for task in tasks {
            results.push(task.await?);
        }

        for result in results {
            if let Err(e) = result {
                warn!("[rfd] Refresh task failed with  {}", e);
            }
        }
    }

    // Update rfds in airtable.
    RFDs::get_from_db(&context.db, context.company.id)
        .await?
        .update_airtable(&context.db)
        .await?;

    Ok(())
}

fn chunk<T>(mut source: Vec<T>, chunk_size: usize) -> Vec<Vec<T>> {
    let mut chunks = vec![];

    while source.len() >= chunk_size {
        chunks.push(source.split_off(chunk_size));
    }

    chunks.push(source);

    chunks
}
