use std::env;

use chrono::offset::Utc;
use chrono::DateTime;
use gsuite_api::GSuite;
use macros::db;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::airtable::{AIRTABLE_BASE_ID_MISC, AIRTABLE_RECORDED_MEETINGS_TABLE};
use crate::db::Database;
use crate::schema::recorded_meetings;
use crate::utils::{get_gsuite_token, GSUITE_DOMAIN};

/// The data type for a recorded meeting.
#[db {
    new_struct_name = "RecordedMeeting",
    airtable_base_id = "AIRTABLE_BASE_ID_MISC",
    airtable_table = "AIRTABLE_RECORDED_MEETINGS_TABLE",
    match_on = {
        "google_event_id" = "String",
    },
}]
#[derive(Debug, Insertable, AsChangeset, Default, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[table_name = "recorded_meetings"]
pub struct NewRecordedMeeting {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub video: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub chat_log: String,
    #[serde(default)]
    pub is_recurring: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attendees: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub transcript: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub google_event_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub event_link: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub location: String,
}

/// Sync the recorded meetings.
#[instrument]
#[inline]
pub async fn refresh_recorded_meetings() {
    let db = Database::new();
    let gsuite_customer = env::var("GADMIN_ACCOUNT_ID").unwrap();
    let token = get_gsuite_token().await;
    let gsuite = GSuite::new(&gsuite_customer, GSUITE_DOMAIN, token);

    // Get the list of our calendars.
    let calendars = gsuite.list_calendars().await.unwrap();

    // Iterate over the calendars.
    for calendar in calendars {
        if calendar.id.ends_with(GSUITE_DOMAIN) {
            // Let's get all the events on this calendar and try and see if they
            // have a meeting recorded.
            println!("Getting events for {}", calendar.id);
            let events = gsuite.list_past_calendar_events(&calendar.id).await.unwrap();

            for event in events {
                // Let's check if there are attachments. We only care if there are attachments.
                if event.attachments.is_empty() {
                    // Continue early.
                    continue;
                }

                let mut attendees: Vec<String> = Default::default();
                for attendee in event.attendees {
                    attendees.push(attendee.email.to_string());
                }

                let mut video = "".to_string();
                let mut chat_log = "".to_string();
                for attachment in event.attachments {
                    if attachment.mime_type == "video/mp4" && attachment.title.starts_with(&event.summary) {
                        video = attachment.file_url.to_string();
                    }
                    if attachment.mime_type == "text_plain" && attachment.title.starts_with(&event.summary) {
                        chat_log = attachment.file_url.to_string();
                    }
                }

                let meeting = NewRecordedMeeting {
                    name: event.summary.to_string(),
                    description: event.description.to_string(),
                    start_time: event.start.date_time.unwrap(),
                    end_time: event.end.date_time.unwrap(),
                    video,
                    chat_log,
                    is_recurring: !event.recurring_event_id.is_empty(),
                    attendees,
                    transcript: "".to_string(),
                    location: event.location.to_string(),
                    google_event_id: event.id.to_string(),
                    event_link: event.html_link.to_string(),
                };

                println!("{:?}", meeting);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::recorded_meetings::refresh_recorded_meetings;

    #[ignore]
    #[tokio::test(threaded_scheduler)]
    async fn test_cron_recorded_meetings() {
        refresh_recorded_meetings().await;
    }
}
