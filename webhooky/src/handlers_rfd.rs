use anyhow::Result;
use cio_api::rfds::{RFDs, RFD};
use dropshot::RequestContext;
use schemars::JsonSchema;
use serde::Serialize;
use std::sync::Arc;

use crate::server::Context;

#[derive(Serialize, JsonSchema)]
pub struct RFDIndex {
    rfds: Vec<RFDIndexEntry>,
    pagination: Pagination,
}

#[derive(Serialize, JsonSchema)]
pub struct Pagination {
    page: u32,
    total_pages: u32,
    has_next: bool,
}

#[derive(Serialize, JsonSchema)]
pub struct RFDIndexEntry {
    number: i32,
    number_string: String,
    title: String,
    link: String,
    short_link: String,
    discussion: String,
}

impl From<RFD> for RFDIndexEntry {
    fn from(rfd: RFD) -> Self {
        Self {
            number: rfd.number,
            number_string: rfd.number_string,
            title: rfd.title,
            link: rfd.link,
            short_link: rfd.short_link,
            discussion: rfd.discussion,
        }
    }
}

pub async fn handle_rfd_index(rqctx: Arc<RequestContext<Context>>) -> Result<RFDIndex> {
    let ctx = rqctx.context();

    // There is only a single company, this is a legacy concept
    let rfds = RFDs::get_from_db(&ctx.db, 1).await?;

    let entries: Vec<RFDIndexEntry> = rfds.into_iter().map(|rfd| rfd.into()).collect();
    let pages = if !entries.is_empty() { 1 } else { 0 };

    Ok(RFDIndex {
        rfds: entries,
        pagination: Pagination {
            page: pages,
            total_pages: pages,
            has_next: false,
        },
    })
}