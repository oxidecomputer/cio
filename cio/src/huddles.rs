use std::collections::HashMap;
use std::env;

use airtable_api::{Airtable, Record};
use chrono::{Duration, NaiveDate, Utc};
use gsuite_api::{CalendarEvent, GSuite};

use crate::airtable::AIRTABLE_MEETING_SCHEDULE_TABLE;
use crate::configs::{get_configs_from_repo, User};
use crate::core::Meeting;
use crate::db::Database;
use crate::utils::{authenticate_github_jwt, get_gsuite_token, GSUITE_DOMAIN};

pub async fn sync_huddles() {
    let github = authenticate_github_jwt();
    let configs = get_configs_from_repo(&github).await;

    let gsuite_customer = env::var("GADMIN_ACCOUNT_ID").unwrap();
    let token = get_gsuite_token("").await;
    let gsuite = GSuite::new(&gsuite_customer, GSUITE_DOMAIN, token.clone());

    let db = Database::new();

    // Iterate over the huddles.
    for (slug, huddle) in configs.huddles {
        // TODO: create all the shortURLs for the huddle.

        // Collect all the calendar events that match this search string.
        // The first part of the map should match the date field in airtable.
        let mut gcal_events: HashMap<NaiveDate, CalendarEvent> = HashMap::new();

        // Get the list of our calendars.
        // We iterate over all of them since we don't know who owns the event.
        let calendars = gsuite.list_calendars().await.unwrap();

        // Iterate over the calendars.
        for calendar in calendars {
            if !calendar.id.ends_with(GSUITE_DOMAIN) {
                // We don't care about this calendar.
                // Continue early.
                continue;
            }

            // Let's get all the events on this calendar and try and see if they
            // have a meeting recorded.
            println!("Getting events for calendar: {}", calendar.id);
            let events = gsuite.list_calendar_events_query(&calendar.id, &huddle.calendar_event_fuzzy_search).await.unwrap();

            // Iterate over all the events, searching for our search string.
            let mut recurring_events: Vec<String> = Vec::new();
            for event in events {
                if !event.summary.to_lowercase().contains(&huddle.calendar_event_fuzzy_search.to_lowercase()) {
                    // This isn't one of the events we are looking for.
                    // Continue early.
                    continue;
                }

                // Let's add the event to our HashMap.
                let date = event.start.date_time.unwrap().date().naive_utc();
                gcal_events.insert(date, event.clone());

                if event.recurring_event_id.is_empty() || recurring_events.contains(&event.recurring_event_id) {
                    // The event either isnt a recurring event OR we already iterated over
                    // it.
                    continue;
                }

                // Get all the recurring events.
                let instances = gsuite.list_recurring_event_instances(&calendar.id, &event.recurring_event_id).await.unwrap();
                for instance in instances {
                    // Let's add the event to our HashMap.
                    if instance.start.date_time.is_some() {
                        let date = instance.start.date_time.unwrap().date().naive_utc();
                        gcal_events.insert(date, instance.clone());
                    }
                }

                // Add it to our vector.
                // So we don't repeat over it.
                recurring_events.push(event.recurring_event_id);
            }
        }

        println!("found {} events for {}", gcal_events.len(), huddle.calendar_event_fuzzy_search);

        // Now let's get the Airtable records.
        let airtable = Airtable::new(airtable_api::api_key_from_env(), huddle.airtable_base_id, "");
        let records: Vec<Record<Meeting>> = airtable.list_records(AIRTABLE_MEETING_SCHEDULE_TABLE, "All Meetings", vec![]).await.unwrap();

        // Iterate over the records and try to match to the google calendar ID.
        for mut record in records {
            match gcal_events.get(&record.fields.date) {
                Some(event) => {
                    // Set the calendar event id in Airtable.
                    record.fields.calendar_event_id = event.id.to_string();
                    // Set the calendar event id in Airtable.
                    record.fields.calendar_event_link = event.html_link.to_string();

                    // The name, day of week, and week fields are a formula so we need to zero it out.
                    record.fields.name = "".to_string();
                    record.fields.week = "".to_string();

                    // Set the link for the recording.
                    for attachment in event.attachments.clone() {
                        if attachment.mime_type == "video/mp4" && attachment.title.starts_with(&event.summary) {
                            record.fields.recording = attachment.file_url.to_string();
                        }
                    }

                    // Update the attendees.
                    let mut attendees: Vec<String> = Default::default();
                    for attendee in event.attendees.clone() {
                        if !attendee.resource && attendee.email.ends_with(GSUITE_DOMAIN) {
                            // Make sure the person is still a user.
                            if let Some(user) = User::get_from_db(&db, attendee.email.trim_end_matches(GSUITE_DOMAIN).trim_end_matches('@').to_string()) {
                                attendees.push(user.email());
                            }
                        }
                    }
                    record.fields.attendees = attendees;

                    // Send the updated record to Airtable.
                    airtable.update_records(AIRTABLE_MEETING_SCHEDULE_TABLE, vec![record.clone()]).await.unwrap();

                    // Delete it from our hashmap.
                    // We do this so that we only have future dates left over.
                    gcal_events.remove(&record.fields.date);

                    println!("[airtable] huddle {} date={} updated", slug, record.fields.date);
                }
                None => {
                    println!("WARN: no event matches: {}", record.fields.date);
                }
            }
        }

        // Create Airtable records for any future calendar dates.
        for (date, event) in gcal_events {
            // One week from now.
            let in_one_week = Utc::now().checked_add_signed(Duration::weeks(1)).unwrap();
            if date > Utc::now().date().naive_utc() && date <= in_one_week.date().naive_utc() {
                // We are in the future.
                // Create an Airtable record.
                let meeting = Meeting {
                    // This is a function so it needs to be empty.
                    name: String::new(),
                    notes: String::new(),
                    action_items: String::new(),
                    date: date,
                    // This is a function so it needs to be empty.
                    week: String::new(),
                    proposed_discussion: Vec::new(),
                    recording: String::new(),
                    attendees: Vec::new(),
                    reminder_email_sent: false,
                    calendar_event_id: event.id.to_string(),
                    calendar_event_link: event.html_link.to_string(),
                };
                let record: Record<Meeting> = Record {
                    id: "".to_string(),
                    fields: meeting,
                    created_time: None,
                };
                airtable.create_records(AIRTABLE_MEETING_SCHEDULE_TABLE, vec![record]).await.unwrap();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::huddles::sync_huddles;

    #[ignore]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_cron_sync_huddles() {
        sync_huddles().await;
    }
}
