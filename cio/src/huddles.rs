use std::collections::HashMap;

use airtable_api::{Airtable, Record};
use anyhow::Result;
use chrono::{Duration, NaiveDate, Utc};
use google_calendar::types::Event;
use handlebars::Handlebars;
use log::{info, warn};
use sendgrid_api::{traits::MailOps, Client as SendGrid};

use crate::{
    airtable::{AIRTABLE_DISCUSSION_TOPICS_TABLE, AIRTABLE_MEETING_SCHEDULE_TABLE},
    companies::Company,
    configs::{get_configs_from_repo, User},
    core::{DiscussionTopic, Meeting, MeetingReminderEmailData},
    db::Database,
    utils::create_or_update_file_in_github_repo,
};

/// Make sure if an event is moved in Google Calendar that Airtable is updated.
pub async fn sync_changes_to_google_events(db: &Database, company: &Company) -> Result<()> {
    let github = company.authenticate_github()?;
    let configs = get_configs_from_repo(&github, company).await?;

    if configs.huddles.is_empty() {
        // Return early.
        return Ok(());
    }

    let gcal = company.authenticate_google_calendar(db).await?;

    // Iterate over the huddle meetings.
    for (slug, huddle) in configs.huddles {
        // Initialize the Airtable client.
        let airtable = Airtable::new(&company.airtable_api_key, huddle.airtable_base_id, "");

        // Get the meeting schedule table from airtable.
        let records: Vec<Record<Meeting>> = airtable
            .list_records(AIRTABLE_MEETING_SCHEDULE_TABLE, "All Meetings", vec![])
            .await?;

        // Iterate over the airtable records and update the meeting notes where we have notes.
        for mut record in records {
            if record.fields.calendar_id.is_empty() || record.fields.calendar_event_id.is_empty() {
                // We don't care we don't have the information we need.
                continue;
            }

            // Get the event from Google Calendar.
            if let Ok(event) = gcal
                .events()
                .get(
                    &record.fields.calendar_id,
                    &record.fields.calendar_event_id,
                    0,  // max attendees, 0 to ignore
                    "", // time_zone
                )
                .await
            {
                // If the event is cancelled, we can just carry on our merry way.
                if event.status.to_lowercase().trim() == "cancelled" {
                    // Set the airtable record to cancelled.
                    record.fields.cancelled = true;
                }

                let date = event.start.unwrap().date_time.unwrap();
                let pacific_time = date.with_timezone(&chrono_tz::US::Pacific);
                // Update the date of the meeting based on the calendar event.
                record.fields.date = pacific_time.date().naive_utc();

                // Clear out the fields that are functions since the API cannot take values for those.
                record.fields.name = "".to_string();
                record.fields.week = "".to_string();

                // Update the Airtable
                airtable
                    .update_records(AIRTABLE_MEETING_SCHEDULE_TABLE, vec![record.clone()])
                    .await?;

                // Get the discussion topics for the meeting.
                let mut discussion_topics = String::new();
                for id in &record.fields.proposed_discussion {
                    // Get the topic from Airtable.
                    let topic: Record<DiscussionTopic> =
                        airtable.get_record(AIRTABLE_DISCUSSION_TOPICS_TABLE, id).await?;

                    discussion_topics = format!(
                        "{}\n- {} from {}",
                        discussion_topics, topic.fields.topic, topic.fields.submitter.name
                    );
                }
                discussion_topics = discussion_topics.trim().to_string();
                if !discussion_topics.is_empty() {
                    discussion_topics = format!("Discussion topics:\n{}", discussion_topics);
                }

                let notes = if !huddle.link_to_notes.is_empty() {
                    format!("Notes Doc: {}\n", huddle.link_to_notes)
                } else {
                    String::new()
                };

                // Update the event description.
                let description = format!(
                    r#"This is the event for {} huddles.

You can submit topics at: https://{}-huddle-form.corp.{}

The Airtable workspace lives at: https://{}-huddle.corp.{}

{}
{}"#,
                    slug.replace('-', " "),
                    slug,
                    company.domain,
                    slug,
                    company.domain,
                    notes,
                    discussion_topics
                );

                if event.recurring_event_id != event.id {
                    let organizer_email = event.organizer.unwrap().email.to_string();
                    // Update the calendar event with the new description.
                    // Get the event under the right user.
                    if let Ok(mut event) = gcal
                        .events()
                        .get(
                            &organizer_email,
                            &event.id,
                            0,  // max attendees, 0 to ignore
                            "", // time_zone
                        )
                        .await
                    {
                        // Modify the properties of the event so we can update it.
                        event.description = description.trim().to_string();
                        if !event.recurring_event_id.is_empty() {
                            // Individual instances are similar to single events. Unlike their parent recurring events, instances do not have the recurrence field set.
                            // FROM: https://developers.google.com/calendar/recurringevents#ruby_1
                            event.recurrence = vec![];
                        }

                        match gcal
                            .events()
                            .update(
                                &organizer_email,
                                &event.id,
                                0,     // conference data version
                                0,     // max attendees, 0 to ignore
                                false, // send notifications
                                google_calendar::types::SendUpdates::Noop,
                                true, // supports_attachments
                                &event,
                            )
                            .await
                        {
                            Ok(_) => (),
                            Err(err) => warn!(
                                "could not update event description {}: {}",
                                serde_json::to_string_pretty(&json!(event))?.to_string(),
                                err
                            ),
                        }
                    }
                }

                info!("updated {} huddle meeting {} in Airtable", slug, pacific_time);
            }
        }
    }

    Ok(())
}

pub async fn send_huddle_reminders(db: &Database, company: &Company) -> Result<()> {
    let github = company.authenticate_github()?;
    let configs = get_configs_from_repo(&github, company).await?;

    if configs.huddles.is_empty() {
        // Return early.
        return Ok(());
    }

    let gcal = company.authenticate_google_calendar(db).await?;

    // Define the date format.
    let date_format = "%A, %-d %B, %C%y";

    // Iterate over the huddle meetings.
    for (slug, huddle) in configs.huddles {
        // Create our email data struct.
        let mut email_data: MeetingReminderEmailData = Default::default();

        // Initialize the Airtable client.
        let airtable = Airtable::new(&company.airtable_api_key, huddle.airtable_base_id, "");

        // Get the meeting schedule table from airtable.
        let records: Vec<Record<Meeting>> = airtable
            .list_records(AIRTABLE_MEETING_SCHEDULE_TABLE, "All Meetings", vec![])
            .await?;

        // Iterate over the airtable records and update the meeting notes where we have notes.
        for record in records {
            if record.fields.calendar_id.is_empty() || record.fields.calendar_event_id.is_empty() {
                // We don't care we don't have the information we need.
                continue;
            }

            // Get the event from Google Calendar.
            if let Ok(event) = gcal
                .events()
                .get(
                    &record.fields.calendar_id,
                    &record.fields.calendar_event_id,
                    0,  // max attendees, 0 to ignore
                    "", // time_zone
                )
                .await
            {
                // If the event is cancelled, we can just carry on our merry way.
                if event.status.to_lowercase().trim() == "cancelled" {
                    // The event was cancelled we want to just continue on our way.
                    continue;
                }
                let date = event.start.unwrap().date_time.unwrap();
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

                if huddle.time_to_cancel > 0 && record.fields.proposed_discussion.is_empty() {
                    // We know that this huddle allows the automation to cancel their
                    // meetings.
                    // We need to check if we are within the threshold to be able to cancel the
                    // meeting.
                    if dur.num_hours() < huddle.time_to_cancel.into() {
                        // We are within the threshold to automatically cancel the meeting.
                        // Let's do it.

                        if event.recurring_event_id != event.id {
                            let organizer_email = event.organizer.unwrap().email.to_string();

                            // We need to impersonate the event owner.
                            // Get the event under the right user.
                            let mut event = gcal
                                .events()
                                .get(
                                    &organizer_email,
                                    &record.fields.calendar_event_id,
                                    0,  // max attendees, 0 to ignore
                                    "", // time_zone
                                )
                                .await?;
                            // We need to update the event instance, not delete it, and set the status to
                            // cancelled.
                            // https://developers.google.com/calendar/recurringevents#modifying_or_deleting_instances
                            event.status = "cancelled".to_string();
                            if !event.recurring_event_id.is_empty() {
                                // Individual instances are similar to single events. Unlike their parent recurring events, instances do not have the recurrence field set.
                                // FROM: https://developers.google.com/calendar/recurringevents#ruby_1
                                event.recurrence = vec![];
                            }

                            gcal.events()
                                .update(
                                    &organizer_email,
                                    &event.id,
                                    0,    // conference data version
                                    0,    // max attendees, 0 to ignore
                                    true, // send notifications
                                    google_calendar::types::SendUpdates::All,
                                    true, // supports_attachments
                                    &event,
                                )
                                .await?;
                            info!(
                                "cancelled calendar event for {} {} since within {} hours, owner {}",
                                slug, date, huddle.time_to_cancel, organizer_email
                            );
                        }

                        // Update Airtable since the meeting was cancelled.
                        let mut r = record.clone();
                        // Clear out the fields that are functions since the API cannot take values for those.
                        r.fields.name = "".to_string();
                        r.fields.week = "".to_string();
                        r.fields.cancelled = true;
                        airtable
                            .update_records(AIRTABLE_MEETING_SCHEDULE_TABLE, vec![r.clone()])
                            .await?;

                        // Continue through our loop.
                        continue;
                    }
                }

                // Check if we should even send the email.
                if record.fields.reminder_email_sent {
                    // If we have already sent the reminder email then break this loop.
                    break;
                }

                // This is our next meeting!
                // Set the email data.
                email_data.huddle_name = slug.to_string();
                email_data.email = huddle.email.to_string();
                email_data.date = pacific_time.date().format(date_format).to_string();

                // Get the discussion topics for the meeting.
                for id in &record.fields.proposed_discussion {
                    // Get the topic from Airtable.
                    let topic: Record<DiscussionTopic> =
                        airtable.get_record(AIRTABLE_DISCUSSION_TOPICS_TABLE, id).await?;
                    // Add it to our list for the email.
                    email_data.topics.push(topic.fields);
                }

                email_data.time = pacific_time.format("%r %Z").to_string();

                // Format the email template.
                // Initialize handlebars.
                let handlebars = Handlebars::new();
                // Render the email template.
                let template = &handlebars.render_template(EMAIL_TEMPLATE, &email_data)?;

                // Send the email.
                // Initialize the SendGrid client.
                let sendgrid = SendGrid::new_from_env();
                // Send the email.
                sendgrid
                    .mail_send()
                    .send_plain_text(
                        &format!("Reminder {} huddle tomorrow", slug),
                        template,
                        &[format!("{}@{}", huddle.email, company.gsuite_domain)],
                        &[],
                        &[],
                        &format!("huddle-reminders@{}", company.gsuite_domain),
                    )
                    .await?;

                info!(
                    "successfully sent {} huddle reminder email to {}@{}",
                    slug, huddle.email, company.gsuite_domain
                );

                // Update the airtable record to show the email was sent.
                // Send the updated record to the airtable client.
                let mut r = record.clone();
                r.fields.reminder_email_sent = true;
                // Clear out the fields that are functions since the API cannot take values for those.
                r.fields.name = "".to_string();
                r.fields.week = "".to_string();
                airtable
                    .update_records(AIRTABLE_MEETING_SCHEDULE_TABLE, vec![r.clone()])
                    .await?;

                info!(
                    "updated {} huddle meeting record to show the reminder email was sent",
                    slug
                );
            }
        }
    }

    Ok(())
}

/// Email template for the meeting huddle reminders.
static EMAIL_TEMPLATE: &str = r#"Greetings {{this.email}}@!

This is your automated reminder that our regularly scheduled {{this.huddle_name}} huddle is
happening tomorrow {{this.date}} at {{this.time}}. You can submit discussion topics using this form:
https://{{this.huddle_name}}-huddle-form.corp.oxide.computer. Please submit topics before EOD
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
https://{{this.huddle_name}}-huddle.corp.oxide.computer

You can also view the roadmap in Airtable here:
https://airtable-roadmap.corp.oxide.computer.

See you soon,
The Airtable Huddle Bot"#;

/// Sync the huddle meeting notes with the GitHub reports repository.
pub async fn sync_huddle_meeting_notes(company: &Company) -> Result<()> {
    let github = company.authenticate_github()?;
    let configs = get_configs_from_repo(&github, company).await?;

    if configs.huddles.is_empty() {
        // Return early.
        return Ok(());
    }

    // Define the date format.
    let date_format = "%A, %-d %B, %C%y";

    // Iterate over the huddle meetings.
    for (name, huddle) in configs.huddles {
        // Initialize the Airtable client.
        let airtable = Airtable::new(&company.airtable_api_key, huddle.airtable_base_id, "");

        // Get the meeting schedule table from airtable.
        let records: Vec<Record<Meeting>> = airtable
            .list_records(AIRTABLE_MEETING_SCHEDULE_TABLE, "All Meetings", vec![])
            .await?;

        // Iterate over the airtable records and update the meeting notes where we have notes.
        for mut record in records {
            if record.fields.notes.trim().is_empty() || record.fields.cancelled {
                // Continue early if we have no notes or the meeting was cancelled.
                continue;
            }

            let notes_path = format!("/{}/meetings/{}.txt", name, record.fields.date.format("%Y%m%d"));

            if record.fields.action_items.is_empty() {
                record.fields.action_items = "There were no action items as a result of this meeting".to_string();
            }

            let notes = format!(
                "# {} Huddle on {}\n\n**Meeting Recording:** {}\n\n## Notes\n\n{}\n\n## Action \
                 Items\n\n{}",
                name.replace('-', " ").to_uppercase(),
                record.fields.date.format(date_format),
                record.fields.recording,
                record.fields.notes,
                record.fields.action_items,
            );

            // Create or update the file in the repo.
            create_or_update_file_in_github_repo(
                &github,
                &company.github_org,
                "reports",
                "",
                &notes_path,
                notes.as_bytes().to_vec(),
            )
            .await?;
        }
    }

    Ok(())
}

pub async fn sync_huddles(db: &Database, company: &Company) -> Result<()> {
    let github = company.authenticate_github()?;
    let configs = get_configs_from_repo(&github, company).await?;

    if configs.huddles.is_empty() {
        // Return early.
        return Ok(());
    }

    let gcal = company.authenticate_google_calendar(db).await?;

    // Iterate over the huddles.
    for (slug, huddle) in configs.huddles {
        // Collect all the calendar events that match this search string.
        // The first part of the map should match the date field in airtable.
        let mut gcal_events: HashMap<NaiveDate, Event> = HashMap::new();

        // Let's get all the events on this calendar and try and see if they
        // have a meeting recorded.
        info!("getting {} events for calendar: {}", huddle.name, huddle.calendar_owner);
        let events = gcal
            .events()
            .list_all(
                &huddle.calendar_id(company),
                "", // iCalID
                0,  // Max attendees, set to 0 to ignore.
                google_calendar::types::OrderBy::Noop,
                &[],                                 // private_extended_property
                &huddle.calendar_event_fuzzy_search, // q
                &[],                                 // shared_extended_property
                true,                                // show_deleted
                true,                                // show_hidden_invitations
                true,                                // single_events
                &Utc::now().checked_add_signed(Duration::weeks(13)).unwrap().to_rfc3339(), // time_max
                "",                                  // time_min
                "",                                  // time_zone
                "",                                  // updated_min
            )
            .await?;

        // Iterate over all the events, searching for our search string.
        let mut recurring_events: Vec<String> = Vec::new();
        for event in events {
            if !event
                .summary
                .to_lowercase()
                .contains(&huddle.calendar_event_fuzzy_search.to_lowercase())
            {
                // This isn't one of the events we are looking for.
                // Continue early.
                continue;
            }

            if event.recurring_event_id.is_empty() || event.recurring_event_id != event.id {
                // Let's add the event to our HashMap.
                let date = event.start.as_ref().unwrap().date_time.unwrap().date().naive_utc();
                gcal_events.insert(date, event.clone());

                continue;
            }

            if recurring_events.contains(&event.recurring_event_id) {
                // We have already iterated over this event.
                continue;
            }

            // Get all the recurring events.
            let instances = gcal
                .events()
                .get_all_instances(
                    &huddle.calendar_id(company),
                    &event.recurring_event_id,
                    0,    // max attendees, 0 to ignore
                    "",   // original_start
                    true, // show_deleted
                    &Utc::now().checked_add_signed(Duration::weeks(13)).unwrap().to_rfc3339(), // time_max
                    "",   // time_min
                    "",   // time_zone
                )
                .await?;
            for instance in instances {
                // Let's add the event to our HashMap.
                if instance.start.as_ref().unwrap().date_time.is_some() {
                    // Let's add the event to our HashMap.
                    let date = instance.start.as_ref().unwrap().date_time.unwrap().date().naive_utc();
                    gcal_events.insert(date, instance.clone());
                }
            }

            // Add it to our vector.
            // So we don't repeat over it.
            recurring_events.push(event.recurring_event_id);
        }

        info!(
            "found {} events for {}",
            gcal_events.len(),
            huddle.calendar_event_fuzzy_search
        );

        // Now let's get the Airtable records.
        let airtable = Airtable::new(&company.airtable_api_key, huddle.airtable_base_id.to_string(), "");
        let records: Vec<Record<Meeting>> = airtable
            .list_records(AIRTABLE_MEETING_SCHEDULE_TABLE, "All Meetings", vec![])
            .await?;

        // Iterate over the records and try to match to the google calendar ID.
        for mut record in records {
            match gcal_events.get(&record.fields.date) {
                Some(event) => {
                    // Set the calendar event id in Airtable.
                    record.fields.calendar_event_id = event.id.to_string();
                    // Set the calendar event id in Airtable.
                    record.fields.calendar_event_link = event.html_link.to_string();
                    // Set the calendar id.
                    record.fields.calendar_id = huddle.calendar_id(company);
                    // Set if the event was cancelled.
                    if event.status.to_lowercase().trim() == "cancelled" {
                        record.fields.cancelled = true;
                    }

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
                        if !attendee.resource && attendee.email.ends_with(&company.gsuite_domain) {
                            // Make sure the person is still a user.
                            if let Some(user) = User::get_from_db(
                                db,
                                company.id,
                                attendee
                                    .email
                                    .trim_end_matches(&company.gsuite_domain)
                                    .trim_end_matches('@')
                                    .to_string(),
                            )
                            .await
                            {
                                attendees.push(user.email);
                            }
                        }
                    }
                    record.fields.attendees = attendees;

                    // Send the updated record to Airtable.
                    match airtable
                        .update_records(AIRTABLE_MEETING_SCHEDULE_TABLE, vec![record.clone()])
                        .await
                    {
                        Ok(_) => (),
                        Err(err) => warn!("error updating record `{}`: {}", json!(record.fields).to_string(), err),
                    }

                    // Delete it from our hashmap.
                    // We do this so that we only have future dates left over.
                    gcal_events.remove(&record.fields.date);

                    info!("huddle {} date={} updated", slug, record.fields.date);
                }
                None => {
                    info!("no huddle event matches: {}", record.fields.date);
                }
            }
        }

        // Create Airtable records for any future calendar dates.
        for (date, event) in gcal_events {
            // Create events up to one quarter in advance.
            let in_range = Utc::now().checked_add_signed(Duration::weeks(13)).unwrap();
            if date > Utc::now().date().naive_utc() && date <= in_range.date().naive_utc() {
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
                    calendar_id: huddle.calendar_id(company),
                    calendar_event_id: event.id.to_string(),
                    calendar_event_link: event.html_link.to_string(),
                    cancelled: event.status == "cancelled",
                };
                let record: Record<Meeting> = Record {
                    id: "".to_string(),
                    fields: meeting,
                    created_time: None,
                };
                airtable
                    .create_records(AIRTABLE_MEETING_SCHEDULE_TABLE, vec![record])
                    .await?;
            }
        }
    }

    Ok(())
}
