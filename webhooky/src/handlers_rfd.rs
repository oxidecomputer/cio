use anyhow::Result;
use cio_api::rfds::{RFDs, NewRFD, RFDIndexEntry};
use dropshot::RequestContext;
use schemars::JsonSchema;
use serde::Serialize;
use std::sync::Arc;

use crate::server::Context;

pub async fn handle_rfd_index(rqctx: Arc<RequestContext<Context>>) -> Result<Vec<RFDIndexEntry>> {
    let ctx = rqctx.context();

    // There is only a single company, this is a legacy concept
    let rfds = RFDs::get_from_db(&ctx.db, 1).await?;

    let entries: Vec<RFDIndexEntry> = rfds.into_iter().map(|rfd| {
        let new_rfd: NewRFD = rfd.into();
        new_rfd.into()
    }).collect();

    Ok(entries)
}