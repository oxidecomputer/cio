use std::sync::Arc;

use anyhow::Result;
use cio_api::{applicants::Applicant, companies::Company, rfds::RFD, schema::applicants};
use diesel::{BoolExpressionMethods, ExpressionMethods, QueryDsl, RunQueryDsl};
use dropshot::{Path, RequestContext};
use log::{info, warn};

use crate::{Context, CounterResponse, RFDPathParams};

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

pub async fn handle_rfd_update_by_number(
    rqctx: Arc<RequestContext<Context>>,
    path_params: Path<RFDPathParams>,
) -> Result<()> {
    let num = path_params.into_inner().num;
    info!("triggering an update for RFD number `{}`", num);

    let api_context = rqctx.context();
    let db = &api_context.db;

    // Get the company id for Oxide.
    // TODO: split this out per company.
    let oxide = Company::get_from_db(db, "Oxide".to_string()).unwrap();

    let github = oxide.authenticate_github()?;

    let result = RFD::get_from_db(db, num);
    if result.is_none() {
        // Return early, we couldn't find an RFD.
        warn!("no RFD was found with number `{}`", num);
        return Ok(());
    }
    let mut rfd = result.unwrap();

    // Update the RFD.
    rfd.expand(&github, &oxide).await?;
    info!("updated  RFD {}", rfd.number_string);

    rfd.convert_and_upload_pdf(db, &github, &oxide).await?;
    info!("updated pdf `{}` for RFD {}", rfd.get_pdf_filename(), rfd.number_string);

    // Save the rfd back to our database.
    rfd.update(db).await?;

    Ok(())
}
