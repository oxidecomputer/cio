use std::env;
use std::str::from_utf8;

use async_trait::async_trait;
use chrono::offset::Utc;
use chrono::DateTime;
use google_drive::GoogleDrive;
use gsuite_api::GSuite;
use macros::db;
use revai::RevAI;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::airtable::{AIRTABLE_BASE_ID_MISC, AIRTABLE_RECORDED_MEETINGS_TABLE};
use crate::configs::User;
use crate::core::UpdateAirtableRecord;
use crate::db::Database;
use crate::models::truncate;
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
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
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
    pub chat_log_link: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub chat_log: String,
    #[serde(default)]
    pub is_recurring: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attendees: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub transcript: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub transcript_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub google_event_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub event_link: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub location: String,
}

/// Implement updating the Airtable record for a RecordedMeeting.
#[async_trait]
impl UpdateAirtableRecord<RecordedMeeting> for RecordedMeeting {
    async fn update_airtable_record(&mut self, record: RecordedMeeting) {
        if !record.transcript_id.is_empty() {
            self.transcript_id = record.transcript_id;
        }
        if !record.transcript.is_empty() {
            self.transcript = record.transcript;
        }

        self.transcript = truncate(self.transcript, 100000);
    }
}

/// Sync the recorded meetings.
#[instrument]
#[inline]
pub async fn refresh_recorded_meetings() {
    let db = Database::new();
    //RecordedMeetings::get_from_db(&db).update_airtable().await;

    let gsuite_customer = env::var("GADMIN_ACCOUNT_ID").unwrap();
    let token = get_gsuite_token("").await;
    let mut gsuite = GSuite::new(&gsuite_customer, GSUITE_DOMAIN, token.clone());
    let revai = RevAI::new_from_env();

    // Get the list of our calendars.
    let calendars = gsuite.list_calendars().await.unwrap();

    // Iterate over the calendars.
    for calendar in calendars {
        if calendar.id.ends_with(GSUITE_DOMAIN) {
            gsuite = GSuite::new(&gsuite_customer, GSUITE_DOMAIN, get_gsuite_token("").await);

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

                let mut owner = "".to_string();
                let mut attendees: Vec<String> = Default::default();
                for attendee in event.attendees {
                    if !attendee.resource {
                        attendees.push(attendee.email.to_string());
                    }
                    if attendee.organizer && attendee.email.ends_with(GSUITE_DOMAIN) {
                        // Make sure the person is still a user.
                        if let Some(_user) = User::get_from_db(&db, attendee.email.trim_end_matches(GSUITE_DOMAIN).trim_end_matches('@').to_string()) {
                            owner = attendee.email.to_string()
                        } else {
                            owner = "catchall@oxidecomputer.com".to_string();
                        }
                    }
                }

                let mut video = "".to_string();
                let mut chat_log_link = "".to_string();
                for attachment in event.attachments {
                    if attachment.mime_type == "video/mp4" && attachment.title.starts_with(&event.summary) {
                        video = attachment.file_url.to_string();
                    }
                    if attachment.mime_type == "text/plain" && attachment.title.starts_with(&event.summary) {
                        chat_log_link = attachment.file_url.to_string();
                    }
                }

                if video.is_empty() {
                    // Continue early, we don't care.
                    continue;
                }

                let delegated_token = get_gsuite_token(&owner).await;
                let drive_client = GoogleDrive::new(delegated_token);

                // If we have a chat log, we should download it.
                let mut chat_log = "".to_string();
                if !chat_log_link.is_empty() {
                    // Download the file.
                    let contents = drive_client
                        .download_file_by_id(&chat_log_link.trim_start_matches("https://drive.google.com/open?id="))
                        .await
                        .unwrap_or_default();
                    chat_log = from_utf8(&contents).unwrap_or_default().trim().to_string();
                }

                // Try to download the video.
                let video_contents = drive_client
                    .download_file_by_id(&video.trim_start_matches("https://drive.google.com/open?id="))
                    .await
                    .unwrap_or_default();

                // Make sure the contents aren't empty.
                if video_contents.is_empty() {
                    // Continue early.
                    continue;
                }

                let mut meeting = NewRecordedMeeting {
                    name: event.summary.trim().to_string(),
                    description: event.description.trim().to_string(),
                    start_time: event.start.date_time.unwrap(),
                    end_time: event.end.date_time.unwrap(),
                    video,
                    chat_log_link,
                    chat_log,
                    is_recurring: !event.recurring_event_id.is_empty(),
                    attendees,
                    transcript: "".to_string(),
                    transcript_id: "".to_string(),
                    location: event.location.to_string(),
                    google_event_id: event.id.to_string(),
                    event_link: event.html_link.to_string(),
                };

                // Let's try to get the meeting.
                let existing = RecordedMeeting::get_from_db(&db, event.id.to_string());
                if let Some(m) = existing {
                    // Update the meeting.
                    meeting.transcript = m.transcript.to_string();
                    meeting.transcript_id = m.transcript_id.to_string();

                    // Get it from Airtable.
                    let existing_airtable = m.get_existing_airtable_record().await;
                    if meeting.transcript.is_empty() {
                        meeting.transcript = existing_airtable.fields.transcript.to_string();
                    }
                    if meeting.transcript_id.is_empty() {
                        meeting.transcript_id = existing_airtable.fields.transcript_id.to_string();
                    }
                    // You are here.
                    println!("{:?}", meeting);
                }

                // Upsert the meeting in the database.
                let mut db_meeting = meeting.upsert(&db).await;
                // Check if we have a transcript id.
                if db_meeting.transcript_id.is_empty() && db_meeting.transcript.is_empty() {
                    // If we don't have a transcript ID, let's post the video to be
                    // transcribed.
                    // Now let's upload it to rev.ai so it can start a job.
                    let job = revai.create_job(video_contents).await.unwrap();
                    // Set the transcript id.
                    db_meeting.transcript_id = job.id.to_string();
                    db_meeting.update(&db).await;
                } else {
                    // We have a transcript id, let's try and get the transcript if we don't have
                    // it already.
                    if db_meeting.transcript.is_empty() {
                        // Now let's try to get the transcript.
                        let transcript = revai.get_transcript(&db_meeting.transcript_id).await.unwrap_or_default();
                        db_meeting.transcript = transcript.trim().to_string();
                        db_meeting.update(&db).await;
                    }
                }
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
