use std::collections::HashMap;
use std::env;

use airtable_api::{Airtable, Record};
use chrono::{Duration, NaiveDate, Utc};
use gsuite_api::{CalendarEvent, GSuite};
use handlebars::Handlebars;
use sendgrid_api::SendGrid;

use crate::airtable::{AIRTABLE_DISCUSSION_TOPICS_TABLE, AIRTABLE_MEETING_SCHEDULE_TABLE};
use crate::configs::{get_configs_from_repo, User};
use crate::core::{DiscussionTopic, Meeting, MeetingReminderEmailData};
use crate::db::Database;
use crate::utils::{authenticate_github_jwt, get_gsuite_token, GSUITE_DOMAIN};

pub async fn send_huddle_reminders() {
    let github = authenticate_github_jwt();
    let configs = get_configs_from_repo(&github).await;

    let gsuite_customer = env::var("GADMIN_ACCOUNT_ID").unwrap();
    let token = get_gsuite_token("").await;
    let gsuite = GSuite::new(&gsuite_customer, GSUITE_DOMAIN, token.clone());

    // Define the date format.
    let date_format = "%A, %-d %B, %C%y";

    // Iterate over the huddle meetings.
    for (name, huddle) in configs.huddles {
        // Create our email data struct.
        let mut email_data: MeetingReminderEmailData = Default::default();

        // Initialize the Airtable client.
        let airtable = Airtable::new(airtable_api::api_key_from_env(), huddle.airtable_base_id, "");

        // Get the meeting schedule table from airtable.
        let records: Vec<Record<Meeting>> = airtable.list_records(AIRTABLE_MEETING_SCHEDULE_TABLE, "All Meetings", vec![]).await.unwrap();

        // Iterate over the airtable records and update the meeting notes where we have notes.
        for record in records {
            if record.fields.calendar_id.is_empty() || record.fields.calendar_event_id.is_empty() {
                // We don't care we don't have the information we need.
                continue;
            }

            // Get the event from Google Calendar.
            let event = gsuite.get_calendar_event(&record.fields.calendar_id, &record.fields.calendar_event_id).await.unwrap();
            let date = event.start.date_time.unwrap();
            let pacific_time = date.with_timezone(&chrono_tz::US::Pacific);

            // Compare the dates.
            let dur = date.signed_duration_since(Utc::now());

            if dur.num_seconds() <= 0 || dur.num_days() >= 2 {
                // Continue our loop since we don't care if it's in the past or way out in the
                // future.
                continue;
            }

            if dur.num_days() < 0 || dur.num_hours() >= 23 {
                // Continue our loop since we don't care if it's in the past or way out in the
                // future.
                continue;
            }

            // Check if we should even send the email.
            if record.fields.reminder_email_sent {
                // If we have already sent the reminder email then break this loop.
                break;
            }

            // This is our next meeting!
            // Set the email data.
            email_data.huddle_name = name.to_string();
            email_data.email = huddle.email.to_string();
            email_data.date = pacific_time.date().format(date_format).to_string();

            // Get the discussion topics for the meeting.
            for id in &record.fields.proposed_discussion {
                // Get the topic from Airtable.
                let topic: Record<DiscussionTopic> = airtable.get_record(AIRTABLE_DISCUSSION_TOPICS_TABLE, &id).await.unwrap();
                // Add it to our list for the email.
                email_data.topics.push(topic.fields);
            }

            email_data.time = pacific_time.time().format("%r %Z").to_string();

            // Format the email template.
            // Initialize handlebars.
            let handlebars = Handlebars::new();
            // Render the email template.
            let template = &handlebars.render_template(EMAIL_TEMPLATE, &email_data).unwrap();

            // Send the email.
            // Initialize the SendGrid client.
            let sendgrid = SendGrid::new_from_env();
            // Send the email.
            // TODO: pass in the domain like the other tools.
            sendgrid
                .send_mail(
                    format!("Reminder {} huddle tomorrow", name),
                    template.to_string(),
                    vec![format!("{}@oxidecomputer.com", huddle.email)],
                    vec![],
                    vec![],
                    "huddle-reminders@oxidecomputer.com".to_string(),
                )
                .await;

            println!("successfully sent {} huddle reminder email to {}@oxidecomputer.com", name, huddle.email);

            // Update the airtable record to show the email was sent.
            // Send the updated record to the airtable client.
            let mut r = record.clone();
            r.fields.reminder_email_sent = true;
            // Clear out the fields that are functions since the API cannot take values for those.
            r.fields.name = "".to_string();
            r.fields.week = "".to_string();
            airtable.update_records(AIRTABLE_MEETING_SCHEDULE_TABLE, vec![r.clone()]).await.unwrap();

            println!("updated {} huddle meeting record to show the reminder email was sent", name);
        }
    }
}

/// Email template for the meeting huddle reminders.
static EMAIL_TEMPLATE: &str = r#"Greetings {{this.email}}@!

This is your automated reminder that our regularly scheduled {{this.huddle_name}} huddle is
happening tomorrow {{this.date}} at {{this.time}}. You can submit discussion topics using this form:
https://{{this.huddle_name}}-huddle-form.corp.oxide.computer. Please submit topics before 12 PM PT
today so people can do any pre-reading and come prepared tomorrow.

{{#if this.topics}}The following topics have already been proposed, but it is not too late to add something
you have been working on or thinking about as well.
# Discussion topics for {{this.date}}
{{#each this.topics}}
- Topic: {{this.Topic}}
  Submitted by: {{this.Submitter.name}}
  Priority: {{this.Priority}}
  Notes: {{this.Notes}}
{{/each}}{{else}}There are no topics on the agenda yet!{{/if}}

Past meeting notes are archived in GitHub:
https://github.com/oxidecomputer/reports/blob/master/{{this.huddle_name}}/meetings

You can also view the Airtable base for agenda, notes, action items here:
https://airtable-{{this.huddle_name}}-huddle.corp.oxide.computer

You can also view the roadmap in Airtable here:
https://airtable-roadmap.corp.oxide.computer.

See you soon,
The Oxide Airtable Huddle Bot"#;

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
            for mut event in events {
                if !event.summary.to_lowercase().contains(&huddle.calendar_event_fuzzy_search.to_lowercase()) {
                    // This isn't one of the events we are looking for.
                    // Continue early.
                    continue;
                }

                // Let's add the event to our HashMap.
                event.calendar_id = calendar.id.to_string();
                let date = event.start.date_time.unwrap().date().naive_utc();
                gcal_events.insert(date, event.clone());

                if event.recurring_event_id.is_empty() || recurring_events.contains(&event.recurring_event_id) {
                    // The event either isnt a recurring event OR we already iterated over
                    // it.
                    continue;
                }

                // Get all the recurring events.
                let instances = gsuite.list_recurring_event_instances(&calendar.id, &event.recurring_event_id).await.unwrap();
                for mut instance in instances {
                    // Let's add the event to our HashMap.
                    if instance.start.date_time.is_some() {
                        instance.calendar_id = calendar.id.to_string();
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
                    // Set the calendar id.
                    record.fields.calendar_id = event.calendar_id.to_string();

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
                    date,
                    // This is a function so it needs to be empty.
                    week: String::new(),
                    proposed_discussion: Vec::new(),
                    recording: String::new(),
                    attendees: Vec::new(),
                    reminder_email_sent: false,
                    calendar_id: event.calendar_id.to_string(),
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
    use crate::huddles::{send_huddle_reminders, sync_huddles};

    #[ignore]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_cron_sync_huddles() {
        sync_huddles().await;

        send_huddle_reminders().await;
    }
}
