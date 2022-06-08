use anyhow::Result;
use async_bb8_diesel::AsyncRunQueryDsl;
use chrono::{Duration, Utc};
use diesel::{BoolExpressionMethods, ExpressionMethods, QueryDsl};
use regex::Regex;
use serde_json::json;
use zoho_api::{client::ModuleUpdateResponseEntryDetails, modules::{Leads, LeadsInput, Notes, NotesInput}};

use crate::{companies::Company, db::Database, rack_line::RackLineSubscriber, schema::rack_line_subscribers};

pub async fn refresh_leads(db: &Database, company: &Company) -> Result<()> {
    // Subscribers are only sent to Zoho once. After that their data is owned by Zoho. If they are
    // removed from the system, we do not re-create
    let not_yet_processed = rack_line_subscribers::dsl::zoho_lead_id.eq("".to_string());

    // Skip any subscribers that are explicitly marked as exclusions
    let not_excluded = rack_line_subscribers::dsl::zoho_lead_exclude.eq(false);

    // Only consider subscribers that signed up over 5 minutes ago. While Zoho should prevent the
    // submission of duplicate records with the same external AirTable record id, we do not need
    // to do work for subscribers that may already be being processed by a hook handler
    let five_min_ago = Utc::now()
        .checked_sub_signed(Duration::minutes(5))
        .expect("Failed to create rack line time window. Is the clock broken?");
    let outside_webhook_time_window = rack_line_subscribers::dsl::date_added.le(five_min_ago);

    let mut subscribers_to_process = rack_line_subscribers::dsl::rack_line_subscribers
        .filter(not_yet_processed.and(not_excluded).and(outside_webhook_time_window))
        .limit(100)
        .load_async::<RackLineSubscriber>(db.pool())
        .await?;

    push_new_rack_line_subscribers_to_zoho(&mut subscribers_to_process, db, company).await
}

pub async fn push_new_rack_line_subscribers_to_zoho(
    subscribers_to_process: &mut [RackLineSubscriber],
    db: &Database,
    company: &Company,
) -> Result<()> {
    if !subscribers_to_process.is_empty() {
        let initial_req_count = subscribers_to_process.len();

        let zoho = company.authenticate_zoho(db).await?;

        let no_employees_cleaner = Regex::new(r"[A-Za-z ~.,+<>]").expect("Failed to build employee number regex");

        // Batch up all of the records that need to be created to be able to submit at once
        let (subscribers, leads): (Vec<&mut RackLineSubscriber>, Vec<LeadsInput>) = subscribers_to_process.iter_mut().filter_map(|subscriber| {
            let mut input = LeadsInput::default();

            let mut name_parts = subscriber.name.rsplitn(2, ' ').peekable();

            if name_parts.peek().is_some() {
                let last_name = name_parts.next().map(String::from).expect("Iter unwrap failed after checking that it had at least one element");

                // We can not submit a lead with an empty last name
                if !last_name.is_empty() {
                    let first_name = name_parts.next().map(String::from);

                    input.first_name = first_name;
                    input.last_name = last_name;

                    input.email = Some(subscriber.email.clone());
                    input.company = Some(subscriber.company.clone());
                    input.no_of_employees = no_employees_cleaner.replace_all(&subscriber.company_size, "").parse::<i64>().ok();
                    input.lead_source = Some("Rack Line Waitlist".to_string());
                    input.submitted_interest = Some(subscriber.interest.clone());
                    input.airtable_lead_record_id = Some(subscriber.airtable_record_id.clone());
                    input.tag = Some(subscriber.tags.iter().map(|tag| json!({ "name": tag })).collect());

                    Some((subscriber, input))
                } else {
                    log::info!("Dropping rack line subscriber that would have an empty last name. This is necessary for pushing to Zoho. id: {} airtable_record_id: {}", subscriber.id, subscriber.airtable_record_id);

                    None
                }
            } else {
                log::info!("Unable to compute a last name for rack line subscriber. This is necessary for pushing to Zoho. id: {} airtable_record_id: {}", subscriber.id, subscriber.airtable_record_id);
                None
            }
        }).unzip();

        // If we have filtered out all of the passed subscribers (due to having insufficient data
        // to store), we can return early. Emit a warning though as this could block other work
        if subscribers.is_empty() {
            log::warn!(
                "{} subscribers were requested for processing, but none of them for sufficient for lead creation",
                initial_req_count
            );

            return Ok(());
        } else {
            log::info!(
                "{} subscribers were requested for processing, of them {} are being submitted as leads",
                initial_req_count,
                leads.len()
            );
        }

        let leads_client = zoho.module_client::<Leads>();

        let results = leads_client.insert(leads, None).await?;

        // Each lead entry may succeed for fail independently, and we only write back to the database
        // the records that where successfully persisted

        // TODO: Refactor this as we better understand how Zoho responses can actually be shaped
        let updates: Vec<&mut RackLineSubscriber> = subscribers
            .into_iter()
            .zip(results.data.into_iter())
            .filter_map(|(mut subscriber, lead_result)| match lead_result.status.as_str() {
                "success" => {
                    match lead_result.details {
                        ModuleUpdateResponseEntryDetails::Success(details) => {
                            subscriber.zoho_lead_id = details.id;
                            Some(subscriber)
                        },
                        failure => {
                            log::warn!(
                                "Zoho returned success but did not include success response details. id: {} airtable_record_id: {} message: {} details: {:?}",
                                subscriber.id,
                                subscriber.airtable_record_id,
                                lead_result.message,
                                failure
                            );

                            None
                        }
                    }
                }
                status => {

                    // In the case that we receive a duplicate error from Zoho, we can instead take the id
                    // of the duplicate and apply it to our internal record
                    match lead_result.message.as_str() {
                        "duplicate data" => {
                            match lead_result.details {
                                ModuleUpdateResponseEntryDetails::Failure(ref details) => {
                                    if let Some(id) = &details.id {
                                        subscriber.zoho_lead_id = id.to_string();
                                        Some(subscriber)
                                    } else {
                                        log::warn!(
                                            "Zoho did not return an id in the duplicate data response details. id: {} airtable_record_id: {} response: {:?}",
                                            subscriber.id,
                                            subscriber.airtable_record_id,
                                            lead_result
                                        );

                                        None
                                    }
                                },
                                _ => {
                                    log::warn!(
                                        "Zoho returned a duplicate data error, but details did not match expectations. id: {} airtable_record_id: {} response: {:?}",
                                        subscriber.id,
                                        subscriber.airtable_record_id,
                                        lead_result
                                    );

                                    None
                                }
                            }
                        }
                        other => {
                            log::warn!(
                                "Failed to write lead to Zoho. id: {} airtable_record_id: {} details: {:?}",
                                subscriber.id,
                                subscriber.airtable_record_id,
                                lead_result
                            );

                            None
                        }
                    }
                }
            })
            .collect();

        // TODO: This should be a bulk update of the db, which then async updates AirTable.
        for update in updates.iter() {
            if let Err(err) = update.update(db).await {
                log::error!(
                    "Failed to write RackLineSubscriber back to database. id: {} airtable_record_id: {} err: {:?}",
                    update.id,
                    update.airtable_record_id,
                    err
                );
            }
        }

        let notes_client = zoho.module_client::<Notes>();

        let notes: Vec<NotesInput> = updates
            .iter()
            .filter_map(|subscriber_update| {
                // For each subscriber, attempt to also send their note data (if they have any)
                if !subscriber_update.notes.is_empty() {
                    let mut note_input = NotesInput::default();

                    note_input.note_content = Some(subscriber_update.notes.clone());
                    note_input.parent_id = serde_json::Value::String(subscriber_update.zoho_lead_id.clone());
                    note_input.se_module = "Leads".to_string();

                    Some(note_input)
                } else {
                    None
                }
            })
            .collect();

        // Only do work if there are notes to insert
        if !notes.is_empty() {
            let notes_results = notes_client.insert(notes, None).await?;

            for note_result in notes_results.data {
                match note_result.status.as_str() {
                    "success" => (),
                    status => {
                        log::warn!(
                            "Failed to write note to Zoho. message: {} status: {}",
                            note_result.message,
                            status
                        )
                    }
                }
            }
        }
    }

    Ok(())
}
