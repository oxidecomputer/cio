use anyhow::Result;
use async_bb8_diesel::AsyncRunQueryDsl;
use chrono::{Duration, Utc};
use diesel::{BoolExpressionMethods, ExpressionMethods, QueryDsl};
use regex::Regex;
use serde_json::json;
use sf_client::ExternalId;
use zoho_api::{
    client::{ModuleUpdateResponseEntry, ModuleUpdateResponseEntryError},
    modules::{Leads, LeadsInput, Notes, NotesInput},
};

use crate::{companies::Company, db::Database, rack_line::RackLineSubscriber, schema::rack_line_subscribers};

pub async fn refresh_sf_leads(db: &Database, company: &Company) -> Result<()> {
    // Subscribers are only sent to SalesForce once. After that their data is owned by SalesForce.
    // If they are removed from the system, we do not re-create
    let not_yet_processed = rack_line_subscribers::dsl::sf_lead_id.eq("".to_string());

    // Skip any subscribers that are explicitly marked as exclusions
    let not_excluded = rack_line_subscribers::dsl::sf_lead_exclude.eq(false);

    // Only consider subscribers that signed up over 5 minutes ago. While SalesForce should prevent
    // the submission of duplicate records with the same external AirTable record id, we do not
    // need to do work for subscribers that may already be being processed by a hook handler
    let five_min_ago = Utc::now()
        .checked_sub_signed(Duration::minutes(5))
        .expect("Failed to create rack line time window. Is the clock broken?");
    let outside_webhook_time_window = rack_line_subscribers::dsl::date_added.le(five_min_ago);

    let mut subscribers_to_process = rack_line_subscribers::dsl::rack_line_subscribers
        .filter(not_yet_processed.and(not_excluded).and(outside_webhook_time_window))
        .limit(75)
        .load_async::<RackLineSubscriber>(db.pool())
        .await?;

    push_new_rack_line_subscribers_to_sf(&mut subscribers_to_process, db, company).await
}

#[derive(Debug, Serialize)]
struct LeadUpdate {
    #[serde(rename = "FirstName")]
    first_name: String,
    #[serde(rename = "LastName")]
    last_name: String,
    #[serde(rename = "Email")]
    email: String,
    #[serde(rename = "Company")]
    company: String,
    #[serde(rename = "NumberOfEmployees")]
    number_of_employees: Option<i64>,
    #[serde(rename = "LeadSource")]
    lead_source: String,
    #[serde(rename = "Interest__c")]
    interest: String,
}

pub async fn push_new_rack_line_subscribers_to_sf(
    subscribers_to_process: &mut [RackLineSubscriber],
    db: &Database,
    company: &Company,
) -> Result<()> {
    if !subscribers_to_process.is_empty() {
        let initial_req_count = subscribers_to_process.len();
        let sf = company.authenticate_sf()?;

        let no_employees_cleaner = Regex::new(r"[A-Za-z ~.,+<>]").expect("Failed to build employee number regex");

        // Process records individually as there is a strict cap on how many records are processed
        // in a given loop. This makes it easier to record back the ids of those records and update
        // them internally. By the time the batch size is a problem, this method of syncing will be
        // replaced.
        for subscriber in subscribers_to_process.iter_mut() {
            if !subscriber.name.empty() {
                let mut name_parts = subscriber.name.rsplitn(2, ' ').peekable();

                if name_parts.peek().is_some() {
                    let last_name = name_parts.next().map(String::from).expect("Iter unwrap failed after checking that it had at least one element");
    
                    // We can not submit a lead with an empty last name
                    if !last_name.is_empty() {
                        let first_name = name_parts.next().map(String::from);

                        let update = LeadUpdate {
                            first_name,
                            last_name,
                            email: subscriber.email,
                            company: subscriber.company,
                            number_of_employees: no_employees_cleaner.replace_all(&subscriber.company_size, "").parse::<i64>().ok();,
                            lead_source: "Rack Line Waitlist".to_string(),
                            interest: subscriber.interest.clone(),
                        };

                        let lead = sf.upsert_object("Lead", &ExternalId::new("Airtable_Lead_Record_Id__c".to_string(), subscriber.airtable_record_id.clone()), &update).await?;

                        log::info!("Created CRM lead {} => {}", subscriber.id, lead.body.id)

                        subscriber.sf_lead_id = lead.body.id;

                        subscriber.update(&db).await {
                            log::error!(
                                "Failed to write RackLineSubscriber back to database. id: {} airtable_record_id: {} lead_id: {} err: {:?}",
                                update.id,
                                update.airtable_record_id,
                                subscriber.sf_lead_id,
                                err
                            );
                        }
                    }
                }
            }
        }
    }

    Ok(())
}