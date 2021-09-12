use std::{collections::HashMap, convert::TryInto, ffi::OsStr, str::FromStr, sync::Arc};

use anyhow::{bail, Result};
use chrono::{NaiveDate, TimeZone, Utc};
use chrono_humanize::HumanTime;
use cio_api::{
    analytics::NewPageView,
    applicants::{get_docusign_template_id, get_role_from_sheet_id, Applicant, NewApplicant},
    asset_inventory::AssetItem,
    certs::Certificate,
    companies::Company,
    configs::User,
    journal_clubs::JournalClubMeeting,
    mailing_list::MailingListSubscriber,
    rack_line::RackLineSubscriber,
    rfds::RFD,
    schema::{applicants, inbound_shipments, journal_club_meetings, outbound_shipments, rfds},
    shipments::{InboundShipment, NewInboundShipment, OutboundShipment, OutboundShipments},
    swag_inventory::SwagInventoryItem,
    swag_store::Order,
    utils::{decode_base64, merge_json},
};
use diesel::{BoolExpressionMethods, ExpressionMethods, PgTextExpressionMethods, QueryDsl, RunQueryDsl};
use dropshot::{Path, RequestContext, TypedBody, UntypedBody};
use google_drive::traits::{DriveOps, FileOps};
use log::{info, warn};
use mailchimp_api::Webhook as MailChimpWebhook;
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use serde_qs::Config as QSConfig;
use sheets::traits::SpreadsheetOps;
use slack_chat_api::{
    BotCommand, FormattedMessage, MessageAttachment, MessageBlock, MessageBlockType, MessageResponse,
    MessageResponseType,
};

use crate::{
    server::{
        AirtableRowEvent, ApplicationFileUploadData, Context, CounterResponse, GitHubRateLimit,
        GoogleSpreadsheetEditEvent, GoogleSpreadsheetRowCreateEvent, RFDPathParams, ShippoTrackingUpdateEvent,
    },
    slack_commands::SlackCommand,
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
        bail!("no RFD was found with number `{}`", num);
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
        bail!("email cell returned empty for event: {:?}", event);
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
        bail!("applicant has an empty email: {:?}", applicant);
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

    // Create a basic divider we can use as a reference.
    let divider = MessageAttachment {
        color: Default::default(),
        author_icon: Default::default(),
        author_link: Default::default(),
        author_name: Default::default(),
        fallback: Default::default(),
        fields: Default::default(),
        footer: Default::default(),
        footer_icon: Default::default(),
        image_url: Default::default(),
        pretext: Default::default(),
        text: Default::default(),
        thumb_url: Default::default(),
        title: Default::default(),
        title_link: Default::default(),
        ts: Default::default(),
        blocks: vec![MessageBlock {
            block_type: MessageBlockType::Divider,
            text: None,
            elements: Default::default(),
            accessory: Default::default(),
            block_id: Default::default(),
            fields: Default::default(),
        }],
    };

    // Filter by command type and do the command.
    let response = match command {
        SlackCommand::RFD => {
            let num = text.parse::<i32>().unwrap_or(0);
            if num > 0 {
                if let Ok(rfd) = rfds::dsl::rfds
                    .filter(rfds::dsl::cio_company_id.eq(company.id).and(rfds::dsl::number.eq(num)))
                    .first::<RFD>(&db.conn())
                {
                    let r: FormattedMessage = rfd.into();
                    json!(r)
                } else if let Ok(rfd) = rfds::dsl::rfds
                    .filter(
                        rfds::dsl::cio_company_id
                            .eq(company.id)
                            .and(rfds::dsl::name.ilike(format!("%{}%", text))),
                    )
                    .first::<RFD>(&db.conn())
                {
                    let r: FormattedMessage = rfd.into();
                    json!(r)
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
                let r: FormattedMessage = rfd.into();
                json!(r)
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
            let mut status = cio_api::applicant_status::Status::NeedsToBeTriaged;

            if text.to_lowercase() == "onboarding" {
                status = cio_api::applicant_status::Status::Onboarding;
            } else if text.to_lowercase() == "interviewing" {
                status = cio_api::applicant_status::Status::Interviewing;
            } else if text.to_lowercase() == "giving offer" {
                status = cio_api::applicant_status::Status::GivingOffer;
            } else if text.to_lowercase() == "next steps" {
                status = cio_api::applicant_status::Status::NextSteps;
            } else if text.to_lowercase() == "hired" {
                status = cio_api::applicant_status::Status::Hired;
            } else if text.to_lowercase() == "deferred" {
                status = cio_api::applicant_status::Status::Deferred;
            } else if text.to_lowercase() == "declined" {
                status = cio_api::applicant_status::Status::Declined;
            }

            // Get the applicants that need to be triaged.
            let applicants = applicants::dsl::applicants
                .filter(
                    applicants::dsl::cio_company_id
                        .eq(company.id)
                        .and(applicants::dsl::status.eq(status.to_string())),
                )
                .load::<Applicant>(&db.conn())?;

            if applicants.len() > 10 {
                json!(MessageResponse {
                    response_type: MessageResponseType::InChannel,
                    text: format!(
                        "Found `{}` applicants with status `{}`. Sorry, that's too many to return at once.",
                        applicants.len(),
                        status.to_string()
                    ),
                })
            } else if applicants.is_empty() {
                json!(MessageResponse {
                    response_type: MessageResponseType::InChannel,
                    text: format!(
                        "Sorry <@{}> :scream: I could not find any applicants with status `{}`",
                        bot_command.user_id,
                        status.to_string()
                    ),
                })
            } else {
                // We know we have at least one item, lets add it.
                let mut msg: FormattedMessage = applicants.get(0).unwrap().clone().into();
                for (i, a) in applicants.into_iter().enumerate() {
                    if i == 0 {
                        continue;
                    }

                    if i > 0 {
                        // Add our divider.
                        msg.attachments.push(divider.clone());
                    }

                    // Add the rest of the blocks.
                    let mut m: FormattedMessage = a.into();
                    msg.attachments.append(&mut m.attachments);
                }

                json!(msg)
            }
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
                let r: FormattedMessage = applicant.into();
                json!(r)
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
        SlackCommand::Shipments => {
            let msg = if !text.is_empty() && text != "outbound" && text != "inbound" {
                json!(MessageResponse {
                    response_type: MessageResponseType::InChannel,
                    text: format!(
                        "Sorry <@{}> :scream: `{}` is valid, try `outbound` or `inbound` or leave blank for both",
                        bot_command.user_id, text
                    ),
                })
            } else {
                let outbound = if text.is_empty() || text == "outbound" {
                    outbound_shipments::dsl::outbound_shipments
                        .filter(
                            outbound_shipments::dsl::cio_company_id
                                .eq(company.id)
                                .and(outbound_shipments::dsl::tracking_status.ne("DELIVERED".to_string()))
                                .and(
                                    outbound_shipments::dsl::status
                                        .ne(cio_api::shipment_status::Status::PickedUp.to_string()),
                                ),
                        )
                        .load::<OutboundShipment>(&db.conn())?
                } else {
                    Default::default()
                };

                let inbound = if text.is_empty() || text == "inbound" {
                    inbound_shipments::dsl::inbound_shipments
                        .filter(
                            inbound_shipments::dsl::cio_company_id
                                .eq(company.id)
                                .and(inbound_shipments::dsl::tracking_status.ne("DELIVERED".to_string()))
                                .and(inbound_shipments::dsl::delivered_time.is_null()),
                        )
                        .load::<InboundShipment>(&db.conn())?
                } else {
                    Default::default()
                };

                if outbound.is_empty() && text == "outbound" {
                    json!(MessageResponse {
                        response_type: MessageResponseType::InChannel,
                        text: format!(
                            "Sorry <@{}> :scream: I could not find any `outbound` shipments",
                            bot_command.user_id,
                        ),
                    })
                } else if inbound.is_empty() && text == "inbound" {
                    json!(MessageResponse {
                        response_type: MessageResponseType::InChannel,
                        text: format!(
                            "Sorry <@{}> :scream: I could not find any `inbound` shipments",
                            bot_command.user_id,
                        ),
                    })
                } else if inbound.is_empty() && outbound.is_empty() {
                    json!(MessageResponse {
                        response_type: MessageResponseType::InChannel,
                        text: format!(
                            "Sorry <@{}> :scream: I could not find any shipments that had not been delivered",
                            bot_command.user_id,
                        ),
                    })
                } else {
                    let mut fm: FormattedMessage = if (text.is_empty() || text == "outbound") && !outbound.is_empty() {
                        outbound.get(0).unwrap().clone().into()
                    } else {
                        inbound.get(0).unwrap().clone().into()
                    };

                    for (i, a) in outbound.clone().into_iter().enumerate() {
                        if i == 0 {
                            continue;
                        }

                        if i > 0 {
                            // Add our divider.
                            fm.attachments.push(divider.clone());
                        }

                        // Add the rest of the blocks.
                        let mut m: FormattedMessage = a.into();
                        fm.attachments.append(&mut m.attachments);
                    }

                    for (i, a) in inbound.into_iter().enumerate() {
                        if i == 0 && ((text == "inbound" || text.is_empty()) && outbound.is_empty()) {
                            continue;
                        }

                        if i > 0 {
                            // Add our divider.
                            fm.attachments.push(divider.clone());
                        }

                        // Add the rest of the blocks.
                        let mut m: FormattedMessage = a.into();
                        fm.attachments.append(&mut m.attachments);
                    }

                    json!(fm)
                }
            };

            msg
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

                let obj: FormattedMessage = m.into();
                merge_json(&mut msg, json!(obj));
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
                let r: FormattedMessage = meeting.into();
                json!(r)
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
        bail!("record id is empty");
    }

    // Get the row from airtable.
    let user = User::get_from_airtable(&event.record_id, &api_context.db, event.cio_company_id).await?;

    // Create a new shipment for the employee and print the label.
    user.create_shipment_to_home_address(&api_context.db).await?;

    Ok(())
}

pub async fn handle_airtable_certificates_renew(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<()> {
    let api_context = rqctx.context();

    let event = body_param.into_inner();

    if event.record_id.is_empty() {
        bail!("record id is empty");
    }

    // Get the row from airtable.
    let cert = Certificate::get_from_airtable(&event.record_id, &api_context.db, event.cio_company_id).await?;

    let company = cert.company(&api_context.db)?;

    let github = company.authenticate_github()?;

    // Renew the cert.
    cert.renew(&api_context.db, &github, &company).await?;

    Ok(())
}

pub async fn handle_airtable_assets_items_print_barcode_label(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<()> {
    let api_context = rqctx.context();

    let event = body_param.into_inner();

    if event.record_id.is_empty() {
        bail!("record id is empty");
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
        bail!("record id is empty");
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
        bail!("record id is empty");
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
        bail!("record id is empty");
    }

    // Get the row from airtable.
    let applicant = Applicant::get_from_airtable(&event.record_id, &api_context.db, event.cio_company_id).await?;

    if applicant.status.is_empty() {
        bail!("got an empty applicant status for row: {}", applicant.email);
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

pub async fn handle_airtable_shipments_outbound_create(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<()> {
    let event = body_param.into_inner();

    let api_context = rqctx.context();

    if event.record_id.is_empty() {
        bail!("record id is empty");
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
        bail!("got an empty email for row");
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

pub async fn handle_airtable_shipments_outbound_reprint_label(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<()> {
    let event = body_param.into_inner();

    if event.record_id.is_empty() {
        bail!("got an empty email for row");
    }

    let api_context = rqctx.context();

    // Get the row from airtable.
    let mut shipment =
        OutboundShipment::get_from_airtable(&event.record_id, &api_context.db, event.cio_company_id).await?;

    // Reprint the label.
    shipment.print_label(&api_context.db).await?;
    info!("shipment {} reprinted label", shipment.email);

    // Update the field.
    shipment.status = "Label printed".to_string();

    // Update Airtable.
    shipment.update(&api_context.db).await?;

    Ok(())
}

pub async fn handle_airtable_shipments_outbound_reprint_receipt(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<()> {
    let event = body_param.into_inner();

    if event.record_id.is_empty() {
        bail!("got an empty email for row");
    }

    let api_context = rqctx.context();

    // Get the row from airtable.
    let shipment = OutboundShipment::get_from_airtable(&event.record_id, &api_context.db, event.cio_company_id).await?;

    // Reprint the receipt.
    shipment.print_receipt(&api_context.db).await?;
    info!("shipment {} reprinted receipt", shipment.email);

    // Update Airtable.
    shipment.update(&api_context.db).await?;

    Ok(())
}

pub async fn handle_airtable_shipments_outbound_resend_shipment_status_email_to_recipient(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<()> {
    let event = body_param.into_inner();

    if event.record_id.is_empty() {
        bail!("record id is empty");
    }

    let api_context = rqctx.context();

    // Get the row from airtable.
    let shipment = OutboundShipment::get_from_airtable(&event.record_id, &api_context.db, event.cio_company_id).await?;

    // Resend the email to the recipient.
    shipment.send_email_to_recipient(&api_context.db).await?;
    info!("resent the shipment email to the recipient {}", shipment.email);

    Ok(())
}

pub async fn handle_airtable_shipments_outbound_schedule_pickup(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<()> {
    let event = body_param.into_inner();

    if event.record_id.is_empty() {
        bail!("record id is empty");
    }

    // Schedule the pickup.
    let api_context = rqctx.context();
    let company = Company::get_by_id(&api_context.db, event.cio_company_id)?;
    OutboundShipments::create_pickup(&api_context.db, &company).await?;

    Ok(())
}

pub async fn handle_emails_incoming_sendgrid_parse(
    rqctx: Arc<RequestContext<Context>>,
    body_param: UntypedBody,
) -> Result<()> {
    // Parse the body as bytes.
    let mut b = body_param.as_bytes();

    // Get the headers and parse the form data.
    let headers = rqctx.request.lock().await.headers().clone();

    let content_type = headers.get("content-type").unwrap();
    let content_length = headers.get("content-length").unwrap();
    let mut h = hyper::header::Headers::new();
    h.set_raw("content-type", vec![content_type.as_bytes().to_vec()]);
    h.set_raw("content-length", vec![content_length.as_bytes().to_vec()]);

    let form_data = formdata::read_formdata(&mut b, &h)?;

    // Start creating the new shipment.
    let mut i: NewInboundShipment = Default::default();
    let mut from = "".to_string();
    // Parse the form body.
    for (name, value) in &form_data.fields {
        if i.carrier.is_empty() && (name == "html" || name == "text" || name == "email") {
            let (carrier, tracking_number) = crate::tracking_numbers::parse_tracking_information(value);
            if !carrier.is_empty() {
                i.carrier = carrier.to_string();
                i.tracking_number = tracking_number.to_string();
                i.notes = value.to_string();
            }
        }

        if name == "subject" {
            if value.contains("from Mouser Electronics") {
                i.name = "Mouser".to_string();
                i.order_number = value
                    .replace("Fwd:", "")
                    .replace("Shipment Notification on Your Purchase Order", "")
                    .replace("from Mouser Electronics, Inc. Invoice Attached", "")
                    .trim()
                    .to_string();
            } else if value.contains("Arrow Order") {
                i.name = "Arrow".to_string();
                i.order_number = value
                    .replace("Fwd:", "")
                    .replace("Arrow Order #", "")
                    .trim()
                    .to_string();
            } else if value.contains("Microchip Order #") {
                i.name = "Microchip".to_string();
                i.order_number = value
                    .replace("Fwd:", "")
                    .replace("Your Microchip Order #", "")
                    .replace("Has Been Shipped", "")
                    .trim()
                    .to_string();
            } else if value.contains("TI.com order") {
                i.name = "Texas Instruments".to_string();
                i.order_number = value
                    .replace("Fwd:", "")
                    .replace("TI.com order", "")
                    .replace("- DO NOT REPLY - Order", "")
                    .replace("fulfilled", "")
                    .replace("status update", "")
                    .trim()
                    .to_string();
            } else if value.contains("Coilcraft") {
                i.name = "Coilcraft".to_string();
            } else {
                i.name = format!("Email: {}", value);
            }
        }

        if name == "from" {
            from = value.to_string();
        }
    }

    i.notes = format!("Parsed email from {}:\n{}", from, i.notes);
    i.cio_company_id = 1;

    if i.carrier.is_empty() {
        bail!(
            "could not find shipment for email:shipment: {:?}\nfields: {:?}\nfiles: {:?}",
            i,
            form_data.fields,
            form_data.files
        );
    }

    // Add the shipment to our database.
    let api_context = rqctx.context();
    i.upsert(&api_context.db).await?;

    Ok(())
}

pub async fn handle_applicant_review(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<cio_api::applicant_reviews::NewApplicantReview>,
) -> Result<()> {
    let api_context = rqctx.context();
    let event = body_param.into_inner();

    if event.name.is_empty() || event.applicant.is_empty() || event.reviewer.is_empty() || event.evaluation.is_empty() {
        bail!("review is empty");
    }

    // Add them to the database.
    event.upsert(&api_context.db).await?;

    info!("applicant review created successfully: {:?}", event);

    Ok(())
}

pub async fn handle_application_submit(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<cio_api::application_form::ApplicationForm>,
) -> Result<()> {
    let api_context = rqctx.context();
    let event = body_param.into_inner();

    event.do_form(&api_context.db).await?;

    info!("application for {} {} created successfully", event.email, event.role);

    Ok(())
}

pub async fn handle_application_files_upload(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<ApplicationFileUploadData>,
) -> Result<HashMap<String, String>> {
    let data = body_param.into_inner();

    // We will return a key value of the name of file and the link in google drive.
    let mut response: HashMap<String, String> = Default::default();

    if data.email.is_empty()
        || data.role.is_empty()
        || data.cio_company_id <= 0
        || data.materials.is_empty()
        || data.resume.is_empty()
        || data.materials_contents.is_empty()
        || data.resume_contents.is_empty()
        || data.user_name.is_empty()
    {
        bail!("could not get applicant information for: {:?}", data);
    }

    // TODO: Add the files to google drive.
    let api_context = rqctx.context();
    let db = &api_context.db;

    let company = Company::get_by_id(db, data.cio_company_id)?;

    // Initialize the Google Drive client.
    let drive = company.authenticate_google_drive(db).await?;

    // Figure out where our directory is.
    // It should be in the shared drive : "Automated Documents"/"application_content"
    let shared_drive = drive.drives().get_by_name("Automated Documents").await?;

    // Get the directory by the name.
    let parent_id = drive
        .files()
        .create_folder(&shared_drive.id, "", "application_content")
        .await?;

    // Create the folder for our candidate with their email.
    let email_folder_id = drive
        .files()
        .create_folder(&shared_drive.id, &parent_id, &data.email)
        .await?;

    // Create the folder for our candidate with the role.
    let role_folder_id = drive
        .files()
        .create_folder(&shared_drive.id, &email_folder_id, &data.role)
        .await?;

    let mut files: HashMap<String, (String, String)> = HashMap::new();
    files.insert(
        "resume".to_string(),
        (data.resume.to_string(), data.resume_contents.to_string()),
    );
    files.insert(
        "materials".to_string(),
        (data.materials.to_string(), data.materials_contents.to_string()),
    );

    // Iterate over our files and create them in google drive.
    // Create or update the file in the google_drive.
    for (name, (file_path, contents)) in files {
        // Get the extension from the content type.
        let ext = get_extension_from_filename(&file_path).unwrap();
        let ct = mime_guess::from_ext(ext).first().unwrap();
        let content_type = ct.essence_str().to_string();
        let file_name = format!("{} - {}.{}", data.user_name, name, ext);

        // Upload our file to drive.
        let drive_file = drive
            .files()
            .create_or_update(
                &shared_drive.id,
                &role_folder_id,
                &file_name,
                &content_type,
                &decode_base64(&contents),
            )
            .await?;
        // Add the file to our links.
        response.insert(
            name.to_string(),
            format!("https://drive.google.com/open?id={}", drive_file.id),
        );
    }

    Ok(response)
}

fn get_extension_from_filename(filename: &str) -> Option<&str> {
    std::path::Path::new(filename).extension().and_then(OsStr::to_str)
}

pub async fn handle_airtable_shipments_inbound_create(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<()> {
    let event = body_param.into_inner();

    if event.record_id.is_empty() {
        bail!("record id is empty");
    }

    let api_context = rqctx.context();
    let db = &api_context.db;

    // Get the row from airtable.
    let record = InboundShipment::get_from_airtable(&event.record_id, db, event.cio_company_id).await?;

    if record.tracking_number.is_empty() || record.carrier.is_empty() {
        // Return early, we don't care.
        bail!("tracking_number and carrier are empty, ignoring");
    }

    let company = record.company(db)?;

    let mut new_shipment: NewInboundShipment = record.into();

    new_shipment.expand(db, &company).await?;
    let mut shipment = new_shipment.upsert_in_db(db)?;
    if shipment.airtable_record_id.is_empty() {
        shipment.airtable_record_id = event.record_id;
    }
    shipment.cio_company_id = event.cio_company_id;
    shipment.update(db).await?;

    info!("inbound shipment {} updated successfully", shipment.tracking_number);
    Ok(())
}

pub async fn handle_store_order_create(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<Order>,
) -> Result<()> {
    let api_context = rqctx.context();

    let event = body_param.into_inner();
    event.do_order(&api_context.db).await?;

    info!("order for {} created successfully", event.email);
    Ok(())
}

pub async fn handle_shippo_tracking_update(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<serde_json::Value>,
) -> Result<()> {
    let api_context = rqctx.context();

    let event = body_param.into_inner();
    let body: ShippoTrackingUpdateEvent = match serde_json::from_str(&event.to_string()) {
        Ok(b) => b,
        Err(e) => bail!("decoding event body for shippo `{}` failed: {}", event.to_string(), e),
    };

    let ts = body.data;
    if ts.tracking_number.is_empty() || ts.carrier.is_empty() {
        // We can reaturn early.
        // It's too early to get anything good from this event.
        bail!("tracking_number and carrier are empty, ignoring");
    }

    // Update the inbound shipment, if it exists.
    if let Some(mut shipment) =
        InboundShipment::get_from_db(&api_context.db, ts.carrier.to_string(), ts.tracking_number.to_string())
    {
        let company = shipment.company(&api_context.db)?;

        // Get the tracking status for the shipment and fill in the details.
        shipment.tracking_number = ts.tracking_number.to_string();
        let tracking_status = ts.tracking_status.unwrap_or_default();
        shipment.tracking_link();
        shipment.eta = ts.eta;

        shipment.oxide_tracking_link = shipment.oxide_tracking_link();

        shipment.messages = tracking_status.status_details;

        // Iterate over the tracking history and set the shipped_time.
        // Get the first date it was maked as in transit and use that as the shipped
        // time.
        for h in ts.tracking_history {
            if h.status == *"TRANSIT" {
                if let Some(shipped_time) = h.status_date {
                    let current_shipped_time = if let Some(s) = shipment.shipped_time {
                        s
                    } else {
                        Utc::now()
                    };

                    if shipped_time < current_shipped_time {
                        shipment.shipped_time = Some(shipped_time);
                    }
                }
            }
        }

        if tracking_status.status == *"DELIVERED" {
            shipment.delivered_time = tracking_status.status_date;
        }

        let send_notification = shipment.tracking_status != tracking_status.status;

        shipment.tracking_status = tracking_status.status.to_string();

        if send_notification {
            // Send a slack notification since it changed.
            let msg: FormattedMessage = shipment.clone().into();
            company.post_to_slack_channel(&api_context.db, &msg).await?;
        }

        shipment.update(&api_context.db).await?;
    }

    // Update the outbound shipment if it exists.
    if let Some(mut shipment) =
        OutboundShipment::get_from_db(&api_context.db, ts.carrier.to_string(), ts.tracking_number.to_string())
    {
        // Update the shipment in shippo.
        // TODO: we likely don't need the extra request here, but it makes the code more DRY.
        // Clean this up eventually.
        shipment.create_or_get_shippo_shipment(&api_context.db).await?;
        shipment.update(&api_context.db).await?;
    }

    info!("shipment {} tracking status updated successfully", ts.tracking_number);
    Ok(())
}

pub async fn handle_checkr_background_update(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<checkr::WebhookEvent>,
) -> Result<()> {
    let api_context = rqctx.context();
    let event = body_param.into_inner();

    // Run the update of the background checks.
    // If we have a candidate ID let's get them from checkr.
    if event.data.object.candidate_id.is_empty()
        || event.data.object.package.is_empty()
        || event.data.object.status.is_empty()
    {
        // Return early we don't care.
        info!("checkr candidate id is empty for event: {:?}", event);
        return Ok(());
    }

    // TODO: change this to the real company name.
    let oxide = Company::get_from_db(&api_context.db, "Oxide".to_string()).unwrap();

    let checkr_auth = oxide.authenticate_checkr();
    if checkr_auth.is_none() {
        // Return early.
        bail!("this company {:?} does not have a checkr api key: {:?}", oxide, event);
    }

    let checkr = checkr_auth.unwrap();
    let candidate = checkr.get_candidate(&event.data.object.candidate_id).await?;
    let result = applicants::dsl::applicants
        .filter(
            applicants::dsl::email
                .eq(candidate.email.to_string())
                // TODO: matching on name might be a bad idea here.
                .or(applicants::dsl::name.eq(format!("{} {}", candidate.first_name, candidate.last_name))),
        )
        .filter(applicants::dsl::status.eq(cio_api::applicant_status::Status::Onboarding.to_string()))
        .first::<Applicant>(&api_context.db.conn());
    if result.is_ok() {
        let mut applicant = result?;
        // Keep the fields from Airtable we need just in case they changed.
        applicant.keep_fields_from_airtable(&api_context.db).await;

        // Set the status for the report.
        if event.data.object.package.contains("premium_criminal") {
            applicant.criminal_background_check_status = event.data.object.status.to_string();
        }
        if event.data.object.package.contains("motor_vehicle") {
            applicant.motor_vehicle_background_check_status = event.data.object.status.to_string();
        }

        // Update the applicant.
        applicant.update(&api_context.db).await?;
    }

    Ok(())
}

pub async fn handle_docusign_envelope_update(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<docusign::Envelope>,
) -> Result<()> {
    let api_context = rqctx.context();
    let db = &api_context.db;

    let event = body_param.into_inner();

    // We need to get the applicant for the envelope.
    // Check their offer first.
    let result = applicants::dsl::applicants
        .filter(applicants::dsl::docusign_envelope_id.eq(event.envelope_id.to_string()))
        .first::<Applicant>(&db.conn());
    match result {
        Ok(mut applicant) => {
            let company = applicant.company(db)?;

            // Create our docusign client.
            let dsa = company.authenticate_docusign(db).await;
            if let Ok(ds) = dsa {
                applicant
                    .update_applicant_from_docusign_offer_envelope(db, &ds, event.clone())
                    .await?;
            }
        }
        Err(e) => {
            bail!(
                "database could not find applicant with docusign offer envelope id {}: {}",
                event.envelope_id,
                e
            );
        }
    }

    // We need to get the applicant for the envelope.
    // Now do PIIA.
    let result = applicants::dsl::applicants
        .filter(applicants::dsl::docusign_piia_envelope_id.eq(event.envelope_id.to_string()))
        .first::<Applicant>(&db.conn());
    match result {
        Ok(mut applicant) => {
            let company = applicant.company(db)?;

            // Create our docusign client.
            let dsa = company.authenticate_docusign(db).await;
            if let Ok(ds) = dsa {
                applicant
                    .update_applicant_from_docusign_piia_envelope(db, &ds, event)
                    .await?;
            }
        }
        Err(e) => {
            bail!(
                "database could not find applicant with docusign piia envelope id {}: {}",
                event.envelope_id,
                e
            );
        }
    }

    Ok(())
}

pub async fn handle_analytics_page_view(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<NewPageView>,
) -> Result<()> {
    let api_context = rqctx.context();
    let db = &api_context.db;

    let mut event = body_param.into_inner();

    // Expand the page_view.
    event.set_page_link();
    event.set_company_id(db).unwrap();

    // Add the page_view to the database and Airttable.
    let pv = event.create(db).await?;

    info!("page_view `{} | {}` created successfully", pv.page_link, pv.user_email);
    Ok(())
}

pub async fn handle_mailchimp_mailing_list(rqctx: Arc<RequestContext<Context>>, body_param: UntypedBody) -> Result<()> {
    let api_context = rqctx.context();
    let db = &api_context.db;

    // We should have a string, which we will then parse into our args.
    let event_string = body_param.as_str().unwrap().to_string();
    let qs_non_strict = QSConfig::new(10, false);

    let event: MailChimpWebhook = qs_non_strict.deserialize_str(&event_string)?;

    if event.webhook_type != *"subscribe" {
        info!("not a `subscribe` event, got `{}`", event.webhook_type);
        return Ok(());
    }

    // Parse the webhook as a new mailing list subscriber.
    let new_subscriber = cio_api::mailing_list::as_mailing_list_subscriber(event, db)?;

    let existing = MailingListSubscriber::get_from_db(db, new_subscriber.email.to_string());
    if existing.is_none() {
        // Update the subscriber in the database.
        let subscriber = new_subscriber.upsert(db).await?;

        // Parse the signup into a slack message.
        // Send the message to the slack channel.
        let company = Company::get_by_id(db, new_subscriber.cio_company_id)?;
        company.post_to_slack_channel(db, &new_subscriber.into()).await?;
        info!("subscriber {} posted to Slack", subscriber.email);

        info!("subscriber {} created successfully", subscriber.email);
    } else {
        info!("subscriber {} already exists", new_subscriber.email);
    }

    Ok(())
}

pub async fn handle_mailchimp_rack_line(rqctx: Arc<RequestContext<Context>>, body_param: UntypedBody) -> Result<()> {
    let api_context = rqctx.context();
    let db = &api_context.db;

    // We should have a string, which we will then parse into our args.
    let event_string = body_param.as_str().unwrap().to_string();
    let qs_non_strict = QSConfig::new(10, false);

    let event: MailChimpWebhook = qs_non_strict.deserialize_str(&event_string)?;

    if event.webhook_type != *"subscribe" {
        info!("not a `subscribe` event, got `{}`", event.webhook_type);
        return Ok(());
    }

    // Parse the webhook as a new rack line subscriber.
    let new_subscriber = cio_api::rack_line::as_rack_line_subscriber(event, db);

    let existing = RackLineSubscriber::get_from_db(db, new_subscriber.email.to_string());
    if existing.is_none() {
        // Update the subscriber in the database.
        let subscriber = new_subscriber.upsert(db).await?;

        // Parse the signup into a slack message.
        // Send the message to the slack channel.
        let company = Company::get_by_id(db, new_subscriber.cio_company_id)?;
        company.post_to_slack_channel(db, &new_subscriber.into()).await?;
        info!("subscriber {} posted to Slack", subscriber.email);

        info!("subscriber {} created successfully", subscriber.email);
    } else {
        info!("subscriber {} already exists", new_subscriber.email);
    }

    Ok(())
}
