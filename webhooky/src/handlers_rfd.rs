use anyhow::Result;
use async_bb8_diesel::AsyncRunQueryDsl;
use cio_api::{
    rfds::{NewRFD, RFDEntry, RFDIndexEntry, RFD},
    schema::rfds,
};
use diesel::{ExpressionMethods, QueryDsl};
use dropshot::RequestContext;
use std::sync::Arc;

use crate::context::Context;

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
