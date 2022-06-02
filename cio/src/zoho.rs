use anyhow::Result;
use async_bb8_diesel::AsyncRunQueryDsl;
use chrono::{Duration, Utc};
use diesel::{BoolExpressionMethods, ExpressionMethods, QueryDsl};
use regex::Regex;
use zoho_api::{modules::{Leads, LeadsInput}};

use crate::{
    companies::Company,
    db::Database,
    rack_line::RackLineSubscriber,
    schema::rack_line_subscribers,
};

pub async fn refresh_leads(db: &Database, company: &Company) -> Result<()> {

    // Subscribers are only sent to Zoho once. After that their data is owned by Zoho. If they are
    // removed from the system, we do not re-create
    let not_yet_processed = rack_line_subscribers::dsl::zoho_lead_id.eq("".to_string());

    // Skip any subscribers that are explicitly marked as exclusions
    let not_excluded = rack_line_subscribers::dsl::zoho_lead_exclude.eq(false);

    // Only consider subscribers that signed up over 5 minutes ago. While Zoho should prevent the
    // submission of duplicate records with the same external AirTable record id, we do not need
    // to do work for subscribers that may already be being processed by a hook handler
    let five_min_ago = Utc::now().checked_sub_signed(Duration::minutes(5)).expect("Failed to rack line time window. Is the clock broken?");
    let outside_webhook_time_window = rack_line_subscribers::dsl::date_added.le(five_min_ago);

    let mut subscribers_to_process = rack_line_subscribers::dsl::rack_line_subscribers
        .filter(
            not_yet_processed
                .and(not_excluded)
                .and(outside_webhook_time_window)
        )
        .limit(25)
        .load_async::<RackLineSubscriber>(db.pool())
        .await?;

    push_new_rack_line_subscribers_to_zoho(subscribers_to_process.iter_mut().collect(), db, company).await
}

pub async fn push_new_rack_line_subscribers_to_zoho(subscribers_to_process: Vec<&mut RackLineSubscriber>, db: &Database, company: &Company) -> Result<()> {
    if !subscribers_to_process.is_empty() {
        let initial_req_count = subscribers_to_process.len();

        let zoho = company.authenticate_zoho(db).await?;

        let no_employees_cleaner = Regex::new(r"[A-Za-z ~.,+<>]").expect("Failed to build employee number regex");

        // Batch up all of the records that need to be created to be able to submit at once
        let (subscribers, leads): (Vec<&mut RackLineSubscriber>, Vec<LeadsInput>) = subscribers_to_process.into_iter().filter_map(|subscriber| {
            let mut input = LeadsInput::default();

            let mut name_parts = subscriber.name.rsplitn(2, ' ').peekable();

            if name_parts.peek().is_some() {
                let last_name = name_parts.next().map(String::from).expect("Iter unwrap failed after checking that it had at least one element");
                let first_name = name_parts.next().map(String::from);

                input.first_name = first_name;
                input.last_name = last_name;

                input.email = Some(subscriber.email.clone());
                input.company = Some(subscriber.company.clone());
                input.no_of_employees = no_employees_cleaner.replace_all(&subscriber.company_size, "").parse::<i64>().ok();
                input.lead_source = Some("Rack Line Waitlist".to_string());
                input.submitted_interest = Some(subscriber.interest.clone());
                input.airtable_lead_record_id = Some(subscriber.airtable_record_id.clone());

                Some((subscriber, input))
            } else {
                log::info!("Unable to compute a last name for rack line subscriber. This is necessary for pushing to Zoho. id: {} airtable_record_id: {}", subscriber.id, subscriber.airtable_record_id);
                None
            }
        }).unzip();

        // If we have filtered out all of the passed subscribers (due to having insufficient data
        // to store), we can return early. Emit a warning though as this could block other work
        if subscribers.is_empty() {
            log::warn!("{} subscribers were requested for processing, but none of them for sufficient for lead creation", initial_req_count);

            return Ok(())
        } else {
            log::info!("{} subscribers were requested for processing, of them {} are being submitted as leads", initial_req_count, leads.len());
        }

        let client = zoho.module_client::<Leads>();

        let results = client.insert(leads, None).await?;

        // Each lead entry may succeed for fail independently, and we only write back to the database
        // the records that where successfully persisted
        let updates: Vec<&mut RackLineSubscriber> = subscribers.into_iter().zip(results.data.into_iter()).filter_map(|(mut subscriber, lead_result)| {
            match lead_result.status.as_str() {
                "success" => {
                    subscriber.zoho_lead_id = lead_result.details.id;
                    Some(subscriber)
                }
                status => {
                    log::warn!("Failed to write lead to Zoho. id: {} airtable_record_id: {} message: {} status: {}", subscriber.id, subscriber.airtable_record_id, lead_result.message, status);
                    None
                }
            }
        }).collect();

        // TODO: This should be a bulk update of the db, which then async updates AirTable.
        for update in updates {
            if let Err(err) = update.update(&db).await {
                log::error!("Failed to write RackLineSubscriber back to database. id: {} airtable_record_id: {} err: {:?}", update.id, update.airtable_record_id, err);
            }
        }
    }

    Ok(())
}