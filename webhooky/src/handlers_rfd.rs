use anyhow::Result;
use cio_api::{
    companies::Company,
    rfds::{RFD, RFDs}
};
use dropshot::RequestContext;
use schemars::JsonSchema;
use serde::Serialize;
use std::sync::Arc;

use crate::server::Context;

#[derive(Serialize, JsonSchema)]
pub struct RFDIndex {
    rfds: Vec<RFDIndexEntry>,
    pagination: Pagination
}

#[derive(Serialize, JsonSchema)]
pub struct Pagination {
    page: u32,
    total_pages: u32,
    has_next: bool
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
    let company = Company::get_by_id(&ctx.db, 1).await?;
    let rfds = RFDs::get_from_db(&ctx.db, company.id).await?;

    let entries = rfds.into_iter().map(|rfd| rfd.into()).collect();

    Ok(RFDIndex {
        rfds: entries,
        pagination: Pagination {
            page: 1,
            total_pages: 1,
            has_next: false
        }
    })
}