use std::{convert::TryInto, str::FromStr, sync::Arc};

use anyhow::Result;
use chrono::{NaiveDate, TimeZone, Utc};
use chrono_humanize::HumanTime;
use cio_api::{
    applicants::{get_docusign_template_id, get_role_from_sheet_id, Applicant},
    companies::Company,
    rfds::RFD,
    schema::applicants,
};
use diesel::{BoolExpressionMethods, ExpressionMethods, QueryDsl, RunQueryDsl};
use dropshot::{Path, RequestContext, TypedBody};
use log::{info, warn};
use sheets::traits::SpreadsheetOps;

use crate::{Context, CounterResponse, GitHubRateLimit, GoogleSpreadsheetEditEvent, RFDPathParams};

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

pub async fn handle_github_rate_limit(rqctx: Arc<RequestContext<Context>>) -> Result<GitHubRateLimit> {
    let api_context = rqctx.context();

    let db = &api_context.db;

    // Get the company id for Oxide.
    // TODO: split this out per company.
    let oxide = Company::get_from_db(db, "Oxide".to_string()).unwrap();

    let github = oxide.authenticate_github()?;

    let response = github.rate_limit().get().await?;
    let reset_time = Utc.timestamp(response.resources.core.reset, 0);

    let dur = reset_time - Utc::now();

    Ok(GitHubRateLimit {
        limit: response.resources.core.limit as u32,
        remaining: response.resources.core.remaining as u32,
        reset: HumanTime::from(dur).to_string(),
    })
}

pub async fn handle_google_sheets_edit(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<GoogleSpreadsheetEditEvent>,
) -> Result<()> {
    let api_context = rqctx.context();
    let db = &api_context.db;

    // Get the company id for Oxide.
    // TODO: split this out per company.
    let oxide = Company::get_from_db(db, "Oxide".to_string()).unwrap();

    let github = oxide.authenticate_github()?;

    // Initialize the GSuite sheets client.
    let sheets = oxide.authenticate_google_sheets(db).await?;

    let event = body_param.into_inner();

    // Ensure this was an applicant and not some other google form!!
    let role = get_role_from_sheet_id(&event.spreadsheet.id);
    if role.is_empty() {
        info!("event is not for an application spreadsheet: {:?}", event);
        return Ok(());
    }

    // Some value was changed. We need to get two things to update the airtable
    // and the database:
    //  - The applicant's email
    //  - The name of the column that was updated.
    // Let's first get the email for this applicant. This is always in column B.
    let mut cell_name = format!("B{}", event.event.range.row_start);
    let email = sheets
        .spreadsheets()
        .cell_get(&event.spreadsheet.id, &cell_name)
        .await?;

    if email.is_empty() {
        // We can return early, the row does not have an email.
        warn!("email cell returned empty for event: {:?}", event);
        return Ok(());
    }

    // Now let's get the header for the column of the cell that changed.
    // This is always in row 1.
    // These should be zero indexed.
    let column_letters = "0ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    cell_name = format!(
        "{}1",
        column_letters
            .chars()
            .nth(event.event.range.column_start.try_into().unwrap())
            .unwrap()
            .to_string()
    );
    let column_header = sheets
        .spreadsheets()
        .cell_get(&event.spreadsheet.id, &cell_name)
        .await?
        .to_lowercase();

    // Now let's get the applicant from the database so we can update it.
    let mut a = applicants::dsl::applicants
        .filter(applicants::dsl::email.eq(email.to_string()))
        .filter(applicants::dsl::sheet_id.eq(event.spreadsheet.id.to_string()))
        .first::<Applicant>(&db.conn())?;

    // Now let's update the correct item for them.
    if column_header.contains("have sent email that we received their application?") {
        // Parse the boolean.
        if event.event.value.to_lowercase() == "true" {
            a.sent_email_received = true;
        }
    } else if column_header.contains("have sent follow up email?") {
        // Parse the boolean.
        if event.event.value.to_lowercase() == "true" {
            a.sent_email_follow_up = true;
        }
    } else if column_header.contains("status") {
        // Parse the new status.
        let mut status = cio_api::applicant_status::Status::from_str(&event.event.value)
            .unwrap_or_default()
            .to_string();
        status = status.trim().to_string();
        if !status.is_empty() {
            a.status = status;
            a.raw_status = event.event.value.to_string();

            // If they changed their status to OnBoarding let's do the docusign updates.
            if a.status == cio_api::applicant_status::Status::Onboarding.to_string() {
                // First let's update the applicant.
                a.update(db).await?;

                // Create our docusign client.
                let dsa = oxide.authenticate_docusign(db).await;
                if let Ok(ds) = dsa {
                    // Get the template we need.
                    let offer_template_id =
                        get_docusign_template_id(&ds, cio_api::applicants::DOCUSIGN_OFFER_TEMPLATE).await;

                    a.do_docusign_offer(db, &ds, &offer_template_id, &oxide).await?;

                    let piia_template_id =
                        get_docusign_template_id(&ds, cio_api::applicants::DOCUSIGN_PIIA_TEMPLATE).await;
                    a.do_docusign_piia(db, &ds, &piia_template_id, &oxide).await?;
                }
            }
        }
    } else if column_header.contains("start date") {
        if event.event.value.trim().is_empty() {
            a.start_date = None;
        } else {
            match NaiveDate::parse_from_str(event.event.value.trim(), "%m/%d/%Y") {
                Ok(v) => a.start_date = Some(v),
                Err(e) => {
                    warn!(
                        "error parsing start date from spreadsheet {}: {}",
                        event.event.value.trim(),
                        e
                    );
                    a.start_date = None
                }
            }
        }
    } else if column_header.contains("value reflected") {
        // Update the value reflected.
        a.value_reflected = event.event.value.to_lowercase();
    } else if column_header.contains("value violated") {
        // Update the value violated.
        a.value_violated = event.event.value.to_lowercase();
    } else if column_header.contains("value in tension [1]") {
        // The person updated the values in tension.
        // We need to get the other value in tension in the next column to the right.
        let value_column = event.event.range.column_start + 1;
        cell_name = format!(
            "{}{}",
            column_letters
                .chars()
                .nth(value_column.try_into().unwrap())
                .unwrap()
                .to_string(),
            event.event.range.row_start
        );
        let value_in_tension_2 = sheets
            .spreadsheets()
            .cell_get(&event.spreadsheet.id, &cell_name)
            .await?
            .to_lowercase();
        a.values_in_tension = vec![value_in_tension_2, event.event.value.to_lowercase()];
    } else if column_header.contains("value in tension [2]") {
        // The person updated the values in tension.
        // We need to get the other value in tension in the next column to the left.
        let value_column = event.event.range.column_start - 1;
        cell_name = format!(
            "{}{}",
            column_letters
                .chars()
                .nth(value_column.try_into().unwrap())
                .unwrap()
                .to_string(),
            event.event.range.row_start
        );
        let value_in_tension_1 = sheets
            .spreadsheets()
            .cell_get(&event.spreadsheet.id, &cell_name)
            .await?
            .to_lowercase();
        a.values_in_tension = vec![value_in_tension_1, event.event.value.to_lowercase()];
    } else {
        // If this is a field wehipmentdon't care about, return early.
        info!(
            "column updated was `{}`, no automations set up for that column yet",
            column_header
        );
        return Ok(());
    }

    // Update the applicant in the database and Airtable.
    let new_applicant = a.update(db).await?;
    let company = Company::get_by_id(db, new_applicant.cio_company_id).unwrap();

    // Get all the hiring issues on the configs repository.
    let configs_issues = github
        .issues()
        .list_all_for_repo(
            &company.github_org,
            "configs",
            // milestone
            "",
            octorust::types::IssuesListState::All,
            // assignee
            "",
            // creator
            "",
            // mentioned
            "",
            // labels
            "hiring",
            // sort
            Default::default(),
            // direction
            Default::default(),
            // since
            None,
        )
        .await?;

    new_applicant
        .create_github_onboarding_issue(db, &github, &configs_issues)
        .await?;

    info!("applicant {} updated successfully", new_applicant.email);
    Ok(())
}
