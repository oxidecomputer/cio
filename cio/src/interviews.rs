use async_trait::async_trait;
use chrono::{DateTime, Utc};
use gsuite_api::GSuite;
use macros::db;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::airtable::{AIRTABLE_BASE_ID_HIRING, AIRTABLE_INTERVIEWS_TABLE};
use crate::core::UpdateAirtableRecord;
use crate::db::Database;
use crate::schema::applicant_interviews;
use crate::utils::{get_gsuite_token, GSUITE_DOMAIN};

#[db {
    new_struct_name = "ApplicantInterview",
    airtable_base_id = "AIRTABLE_BASE_ID_HIRING",
    airtable_table = "AIRTABLE_INTERVIEWS_TABLE",
    match_on = {
        "start_time" = "DateTime<Utc>",
        "email" = "String",
    },
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[table_name = "applicant_interviews"]
pub struct NewApplicantInterview {
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email: String,
    #[serde(default, skip_serializing_if = "Vec:is_empty")]
    pub interviewers: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub google_event_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub event_link: String,
    /// link to another table in Airtable
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub link_to_applicant: Vec<String>,
}

/// Implement updating the Airtable record for a ApplicantInterview.
#[async_trait]
impl UpdateAirtableRecord<ApplicantInterview> for ApplicantInterview {
    #[instrument]
    #[inline]
    async fn update_airtable_record(&mut self, _record: ApplicantInterview) {}
}

/// Sync interviews.
#[instrument]
#[inline]
pub async fn refresh_interviews() {
    let db = Database::new();

    let gsuite_customer = env::var("GADMIN_ACCOUNT_ID").unwrap();
    let token = get_gsuite_token("").await;
    let mut gsuite = GSuite::new(&gsuite_customer, GSUITE_DOMAIN, token.clone());

    // Get the list of our calendars.
    let calendars = gsuite.list_calendars().await.unwrap();

    // Iterate over the calendars.
    for calendar in calendars {
        // Ignore any calandar that is not the interviews calendar.
        if calendar.name != "Interviews" {
            continue;
        }

        // Let's get all the events on this calendar and try and see if they
        // have a meeting recorded.
        println!("Getting events for {}", calendar.id);
        let events = gsuite.list_past_calendar_events(&calendar.id).await.unwrap();

        for event in events {
            // Create the interview event.
            let mut interview = ApplicantInterview {
                start_time: event.start.date_time.unwrap(),
                end_time: event.end.date_time.unwrap(),

                name: "".to_string(),
                email: "".to_string(),
                interviewers: Default::default(),

                google_event_id: event.id.to_string(),
                event_link: event.html_link.to_string(),
                link_to_applicant: Default::default(),
            };

            for attendee in event.attendees {
                // Skip the organizer, this is the Interviews calendar.
                if attendee.organizer {
                    continue;
                }
                if attendee.email.ends_with(GSUITE_DOMAIN) {
                    // This is the interviewer.
                    interview.interviewers = vec![attendee.email.to_string()];
                    continue;
                }
                // It must be the person being interviewed.
                interview.name = attendee.display_name.to_string();
                interview.email = attendee.email.to_string();
            }
        }
    }

    ApplicantInterviews::get_from_db(&db).update_airtable().await;
}

#[cfg(test)]
mod tests {
    use crate::interviews::refresh_interviews;

    #[ignore]
    #[tokio::test(threaded_scheduler)]
    async fn test_cron_interviews() {
        refresh_interviews().await;
    }
}
