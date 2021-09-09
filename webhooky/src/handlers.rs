use std::{convert::TryInto, str::FromStr, sync::Arc};

use anyhow::Result;
use chrono::{NaiveDate, TimeZone, Utc};
use chrono_humanize::HumanTime;
use cio_api::{
    applicants::{get_docusign_template_id, get_role_from_sheet_id, Applicant, NewApplicant},
    asset_inventory::AssetItem,
    companies::Company,
    configs::User,
    journal_clubs::JournalClubMeeting,
    rfds::RFD,
    schema::{applicants, journal_club_meetings, rfds},
    swag_inventory::SwagInventoryItem,
    utils::merge_json,
};
use diesel::{BoolExpressionMethods, ExpressionMethods, PgTextExpressionMethods, QueryDsl, RunQueryDsl};
use dropshot::{Path, RequestContext, TypedBody, UntypedBody};
use log::{info, warn};
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use sheets::traits::SpreadsheetOps;
use slack_chat_api::{BotCommand, MessageResponse, MessageResponseType};

use crate::{
    slack_commands::SlackCommand, AirtableRowEvent, Context, CounterResponse, GitHubRateLimit,
    GoogleSpreadsheetEditEvent, GoogleSpreadsheetRowCreateEvent, RFDPathParams,
};

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
            .nth(event.event.range.column_start.try_into()?)
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
                .nth(value_column.try_into()?)
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
                .nth(value_column.try_into()?)
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

pub async fn handle_google_sheets_row_create(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<GoogleSpreadsheetRowCreateEvent>,
) -> Result<()> {
    let api_context = rqctx.context();
    let db = &api_context.db;

    // Get the company id for Oxide.
    // TODO: split this out per company.
    let oxide = Company::get_from_db(db, "Oxide".to_string()).unwrap();

    // Initialize the Google Drive client.
    let drive = oxide.authenticate_google_drive(db).await?;

    // Initialize the GSuite sheets client.
    let sheets = oxide.authenticate_google_sheets(db).await?;

    let event = body_param.into_inner();

    // Ensure this was an applicant and not some other google form!!
    let role = get_role_from_sheet_id(&event.spreadsheet.id);
    if role.is_empty() {
        // Return early if not
        info!("event is not for an application spreadsheet: {:?}", event);
        return Ok(());
    }

    // Parse the applicant out of the row information.
    let mut applicant = NewApplicant::parse_from_row(&event.spreadsheet.id, &event.event.named_values).await;

    if applicant.email.is_empty() {
        warn!("applicant has an empty email: {:?}", applicant);
        return Ok(());
    }

    // We do not need to add one to the end of the columns to get the column where the email sent verification is
    // because google sheets index's at 0, so adding one would put us over, we are just right here.
    let sent_email_received_column_index = event.event.range.column_end;
    let sent_email_follow_up_index = event.event.range.column_end + 6;
    applicant
        .expand(
            db,
            &drive,
            &sheets,
            sent_email_received_column_index.try_into()?,
            sent_email_follow_up_index.try_into()?,
            event.event.range.row_start.try_into()?,
        )
        .await?;

    if !applicant.sent_email_received {
        info!("applicant is new, sending internal notifications: {:?}", applicant);

        // Send a company-wide email.
        applicant.send_email_internally(db).await?;

        applicant.sent_email_received = true;
    }

    // Send the applicant to the database and Airtable.
    let a = applicant.upsert(db).await?;

    info!("applicant {} created successfully", a.email);
    Ok(())
}

pub async fn handle_slack_commands(
    rqctx: Arc<RequestContext<Context>>,
    body_param: UntypedBody,
) -> Result<serde_json::Value> {
    let api_context = rqctx.context();
    let db = &api_context.db;

    // We should have a string, which we will then parse into our args.
    // Parse the request body as a Slack BotCommand.
    let bot_command: BotCommand = serde_urlencoded::from_bytes(body_param.as_bytes())?;

    // Get the company from the Slack team id.
    let company = Company::get_from_slack_team_id(db, &bot_command.team_id)?;

    // Get the command type.
    let command = SlackCommand::from_str(&bot_command.command).unwrap();
    let text = bot_command.text.trim();

    // Filter by command type and do the command.
    let response = match command {
        SlackCommand::RFD => {
            let num = text.parse::<i32>().unwrap_or(0);
            if num > 0 {
                if let Ok(rfd) = rfds::dsl::rfds
                    .filter(rfds::dsl::cio_company_id.eq(company.id).and(rfds::dsl::number.eq(num)))
                    .first::<RFD>(&db.conn())
                {
                    json!(MessageResponse {
                        response_type: MessageResponseType::InChannel,
                        text: rfd.as_slack_msg(),
                    })
                } else if let Ok(rfd) = rfds::dsl::rfds
                    .filter(
                        rfds::dsl::cio_company_id
                            .eq(company.id)
                            .and(rfds::dsl::name.ilike(format!("%{}%", text))),
                    )
                    .first::<RFD>(&db.conn())
                {
                    json!(MessageResponse {
                        response_type: MessageResponseType::InChannel,
                        text: rfd.as_slack_msg(),
                    })
                } else {
                    json!(MessageResponse {
                        response_type: MessageResponseType::InChannel,
                        text: format!(
                            "Sorry <@{}> :scream: I could not find an RFD matching `{}`",
                            bot_command.user_id, text
                        ),
                    })
                }
            } else if let Ok(rfd) = rfds::dsl::rfds
                .filter(
                    rfds::dsl::cio_company_id
                        .eq(company.id)
                        .and(rfds::dsl::name.ilike(format!("%{}%", text))),
                )
                .first::<RFD>(&db.conn())
            {
                json!(MessageResponse {
                    response_type: MessageResponseType::InChannel,
                    text: rfd.as_slack_msg(),
                })
            } else {
                json!(MessageResponse {
                    response_type: MessageResponseType::InChannel,
                    text: format!(
                        "Sorry <@{}> :scream: I could not find an RFD matching `{}`",
                        bot_command.user_id, text
                    ),
                })
            }
        }
        SlackCommand::Meet => {
            let mut name = text.replace(" ", "-");
            if name.is_empty() {
                // Generate a new random string.
                name = thread_rng()
                    .sample_iter(&Alphanumeric)
                    .take(6)
                    .map(char::from)
                    .collect();
            }

            json!(MessageResponse {
                response_type: MessageResponseType::InChannel,
                text: format!("https://g.co/meet/oxide-{}", name.to_lowercase()),
            })
        }
        SlackCommand::Applicants => {
            // Get the applicants that need to be triaged.
            let applicants =
                applicants::dsl::applicants
                    .filter(applicants::dsl::cio_company_id.eq(company.id).and(
                        applicants::dsl::status.eq(cio_api::applicant_status::Status::NeedsToBeTriaged.to_string()),
                    ))
                    .load::<Applicant>(&db.conn())?;

            let mut msg: serde_json::Value = Default::default();
            for (i, a) in applicants.into_iter().enumerate() {
                if i > 0 {
                    // Merge a divider onto the stack.
                    let object = json!({
                        "blocks": [{
                            "type": "divider"
                        }]
                    });

                    merge_json(&mut msg, object);
                }

                let obj = a.as_slack_msg();
                merge_json(&mut msg, obj);
            }

            msg
        }
        SlackCommand::Applicant => {
            if let Ok(applicant) = applicants::dsl::applicants
                .filter(
                    applicants::dsl::cio_company_id
                        .eq(company.id)
                        .and(applicants::dsl::name.ilike(format!("%{}%", text))),
                )
                .first::<Applicant>(&db.conn())
            {
                applicant.as_slack_msg()
            } else {
                json!(MessageResponse {
                    response_type: MessageResponseType::InChannel,
                    text: format!(
                        "Sorry <@{}> :scream: I could not find an applicant matching `{}`",
                        bot_command.user_id, text
                    ),
                })
            }
        }
        SlackCommand::Papers => {
            // If we asked for the closed meetings then only show those, otherwise
            // default to the open meetings.
            let mut state = "open";
            if text == "closed" {
                state = "closed";
            }
            let meetings = journal_club_meetings::dsl::journal_club_meetings
                .filter(
                    journal_club_meetings::dsl::cio_company_id
                        .eq(company.id)
                        .and(journal_club_meetings::dsl::state.eq(state.to_string())),
                )
                .load::<JournalClubMeeting>(&db.conn())?;

            let mut msg: serde_json::Value = Default::default();
            for (i, m) in meetings.into_iter().enumerate() {
                if i > 0 {
                    // Merge a divider onto the stack.
                    let object = json!({
                        "blocks": [{
                            "type": "divider"
                        }]
                    });

                    merge_json(&mut msg, object);
                }

                let obj = m.as_slack_msg();
                merge_json(&mut msg, obj);
            }

            msg
        }
        SlackCommand::Paper => {
            if let Ok(meeting) = journal_club_meetings::dsl::journal_club_meetings
                .filter(
                    journal_club_meetings::dsl::cio_company_id
                        .eq(company.id)
                        .and(journal_club_meetings::dsl::title.ilike(format!("%{}%", text))),
                )
                .first::<JournalClubMeeting>(&db.conn())
            {
                meeting.as_slack_msg()
            } else {
                json!(MessageResponse {
                    response_type: MessageResponseType::InChannel,
                    text: format!(
                        "Sorry <@{}> :scream: I could not find a journal club meeting matching \
                         `{}`",
                        bot_command.user_id, text
                    ),
                })
            }
        }
    };

    Ok(response)
}

pub async fn handle_airtable_employees_print_home_address_label(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<()> {
    let api_context = rqctx.context();

    let event = body_param.into_inner();

    if event.record_id.is_empty() {
        warn!("record id is empty");
        return Ok(());
    }

    // Get the row from airtable.
    let user = User::get_from_airtable(&event.record_id, &api_context.db, event.cio_company_id).await?;

    // Create a new shipment for the employee and print the label.
    user.create_shipment_to_home_address(&api_context.db).await?;

    Ok(())
}

pub async fn handle_airtable_assets_items_print_barcode_label(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<()> {
    let api_context = rqctx.context();

    let event = body_param.into_inner();

    if event.record_id.is_empty() {
        warn!("record id is empty");
        return Ok(());
    }

    // Get the row from airtable.
    let asset_item = AssetItem::get_from_airtable(&event.record_id, &api_context.db, event.cio_company_id).await?;

    // Print the barcode label(s).
    asset_item.print_label(&api_context.db).await?;
    info!("asset item {} printed label", asset_item.name);

    Ok(())
}

pub async fn handle_airtable_swag_inventory_items_print_barcode_labels(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<()> {
    let api_context = rqctx.context();

    let event = body_param.into_inner();

    if event.record_id.is_empty() {
        warn!("record id is empty");
        return Ok(());
    }

    // Get the row from airtable.
    let swag_inventory_item =
        SwagInventoryItem::get_from_airtable(&event.record_id, &api_context.db, event.cio_company_id).await?;

    // Print the barcode label(s).
    swag_inventory_item.print_label(&api_context.db).await?;
    info!("swag inventory item {} printed label", swag_inventory_item.name);

    Ok(())
}

pub async fn handle_airtable_applicants_request_background_check(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<()> {
    let api_context = rqctx.context();

    let event = body_param.into_inner();

    if event.record_id.is_empty() {
        warn!("record id is empty");
        return Ok(());
    }

    // Get the row from airtable.
    let mut applicant = Applicant::get_from_airtable(&event.record_id, &api_context.db, event.cio_company_id).await?;
    if applicant.criminal_background_check_status.is_empty() {
        // Request the background check, since we previously have not requested one.
        applicant.send_background_check_invitation(&api_context.db).await?;
        info!("sent background check invitation to applicant: {}", applicant.email);
    }

    Ok(())
}

pub async fn handle_airtable_applicants_update(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<()> {
    let event = body_param.into_inner();

    let api_context = rqctx.context();

    if event.record_id.is_empty() {
        warn!("record id is empty");
        return Ok(());
    }

    // Get the row from airtable.
    let applicant = Applicant::get_from_airtable(&event.record_id, &api_context.db, event.cio_company_id).await?;

    if applicant.status.is_empty() {
        warn!("got an empty applicant status for row: {}", applicant.email);
        return Ok(());
    }

    // Grab our old applicant from the database.
    let mut db_applicant = Applicant::get_by_id(&api_context.db, applicant.id)?;

    // Grab the status and the status raw.
    let status = cio_api::applicant_status::Status::from_str(&applicant.status).unwrap();
    db_applicant.status = status.to_string();
    if !applicant.raw_status.is_empty() {
        // Update the raw status if it had changed.
        db_applicant.raw_status = applicant.raw_status.to_string();
    }

    // TODO: should we also update the start date if set in airtable?
    // If we do this, we need to update the airtable webhook settings to include it as
    // well.

    // Update the row in our database.
    db_applicant.update(&api_context.db).await?;

    info!("applicant {} updated successfully", applicant.email);
    Ok(())
}

pub async fn listen_airtable_shipments_outbound_create_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<()> {
    let event = body_param.into_inner();

    let api_context = rqctx.context();

    if event.record_id.is_empty() {
        warn!("record id is empty");
        return Ok(());
    }

    // Get the row from airtable.
    let shipment = OutboundShipment::get_from_airtable(&event.record_id, &api_context.db, event.cio_company_id).await?;

    // If it is a row we created from our internal store do nothing.
    if shipment.notes.contains("Oxide store")
        || shipment.notes.contains("Google sheet")
        || shipment.notes.contains("Internal")
        || !shipment.shippo_id.is_empty()
    {
        return Ok(());
    }

    if shipment.email.is_empty() {
        warn!("got an empty email for row");
        return Ok(());
    }

    // Update the row in our database.
    let mut new_shipment = shipment.update(&api_context.db).await?;
    // Create the shipment in shippo.
    new_shipment.create_or_get_shippo_shipment(&api_context.db).await?;
    // Update airtable again.
    new_shipment.update(&api_context.db).await?;

    info!("shipment {} created successfully", shipment.email);
    Ok(())
}
