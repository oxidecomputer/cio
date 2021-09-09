use std::sync::Arc;

use anyhow::Result;
use cio_api::companies::Companys;
use dropshot::RequestContext;

use crate::Context;

pub async fn handle_sync_repos_create(rqctx: Arc<RequestContext<Context>>) -> Result<Vec<uuid::Uuid>> {
    let api_context = rqctx.context();
    let db = &api_context.db;

    let mut fns: Vec<uuid::Uuid> = Vec::new();
    let companies = Companys::get_from_db(db, 1)?;
    // Iterate over the companies and update.
    for company in companies {
        let id = uuid::Uuid::new_v4();

        // Run the saga.
        cio_api::sagas::sync_repos(db, &api_context.sec, &id, &company).await?;

        fns.push(id);
    }

    Ok(fns)
}
