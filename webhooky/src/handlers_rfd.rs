use anyhow::Result;
use async_bb8_diesel::AsyncRunQueryDsl;
use cio_api::{
    rfds::{NewRFD, RFDIndexEntry, RFD},
    schema::rfds,
};
use diesel::{ExpressionMethods, QueryDsl};
use dropshot::RequestContext;
use schemars::JsonSchema;
use serde::Serialize;
use std::sync::Arc;

use crate::server::Context;

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
        .load_async::<RFD>(ctx.db.pool())
        .await?;

    let entries: Vec<RFDIndexEntry> = rfds
        .into_iter()
        .map(|rfd| {
            let new_rfd: NewRFD = rfd.into();
            new_rfd.into()
        })
        .collect();

    Ok(entries)
}
