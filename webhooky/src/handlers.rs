use std::sync::Arc;

use anyhow::Result;
use cio_api::{applicants::Applicant, companies::Company, schema::applicants};
use diesel::{BoolExpressionMethods, ExpressionMethods, QueryDsl, RunQueryDsl};
use dropshot::RequestContext;

use crate::{Context, CounterResponse};

pub async fn handle_products_sold_count(rqctx: Arc<RequestContext<Context>>) -> Result<CounterResponse> {
    let api_context = rqctx.context();

    // TODO: find a better way to do this.
    let company = Company::get_from_db(&api_context.db, "Oxide".to_string()).unwrap();

    // TODO: change this one day to be the number of racks sold.
    // For now, use it as number of applications that need to be triaged.
    // Get the applicants that need to be triaged.
    let applicants = applicants::dsl::applicants
        .filter(
            applicants::dsl::cio_company_id
                .eq(company.id)
                .and(applicants::dsl::status.eq(cio_api::applicant_status::Status::NeedsToBeTriaged.to_string())),
        )
        .load::<Applicant>(&api_context.db.conn())?;

    Ok(CounterResponse {
        count: applicants.len() as i32,
    })
}
