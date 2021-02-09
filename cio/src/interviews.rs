use std::env;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use gsuite_api::GSuite;
use macros::db;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::airtable::{AIRTABLE_BASE_ID_RECURITING_APPLICATIONS, AIRTABLE_INTERVIEWS_TABLE};
use crate::applicants::{get_sheets_map, Applicant};
use crate::core::UpdateAirtableRecord;
use crate::db::Database;
use crate::schema::applicant_interviews;
use crate::utils::{get_gsuite_token, DOMAIN, GSUITE_DOMAIN};

#[db {
    new_struct_name = "ApplicantInterview",
    airtable_base_id = "AIRTABLE_BASE_ID_RECURITING_APPLICATIONS",
    airtable_table = "AIRTABLE_INTERVIEWS_TABLE",
    match_on = {
        "google_event_id" = "String",
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub interviewers: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub google_event_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub event_link: String,
    /// link to another table in Airtable
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub applicant: Vec<String>,
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
    let gsuite = GSuite::new(&gsuite_customer, GSUITE_DOMAIN, token.clone());

    // Get the list of our calendars.
    let calendars = gsuite.list_calendars().await.unwrap();

    // Iterate over the calendars.
    for calendar in calendars {
        // Ignore any calandar that is not the interviews calendar.
        if calendar.summary != "Interviews" {
            continue;
        }

        // Let's get all the events on this calendar and try and see if they
        // have a meeting recorded.
        println!("Getting events for {}", calendar.id);
        let events = gsuite.list_calendar_events(&calendar.id).await.unwrap();

        for event in events {
            // Create the interview event.
            let mut interview = NewApplicantInterview {
                start_time: event.start.date_time.unwrap(),
                end_time: event.end.date_time.unwrap(),

                name: "".to_string(),
                email: "".to_string(),
                interviewers: Default::default(),

                google_event_id: event.id.to_string(),
                event_link: event.html_link.to_string(),
                applicant: Default::default(),
            };

            for attendee in event.attendees {
                // Skip the organizer, this is the Interviews calendar.
                if attendee.organizer || attendee.email.ends_with("@group.calendar.google.com") {
                    continue;
                }

                let end = &format!("({})", attendee.display_name);
                // TODO: Sometimes Dave and Nils use their personal email, find a better way to do this other than
                // a one-off.
                if attendee.email.ends_with(GSUITE_DOMAIN)
                    || attendee.email.ends_with(DOMAIN)
                    || event.summary.ends_with(end)
                    || attendee.email.starts_with("dave.pacheco")
                    || attendee.email.starts_with("nils.nieuwejaar")
                {
                    // This is the interviewer.
                    let mut email = attendee.email.to_string();
                    if attendee.email.starts_with("dave.pacheco") {
                        email = format!("dave@{}", GSUITE_DOMAIN);
                    } else if attendee.email.starts_with("nils") {
                        email = format!("nils@{}", GSUITE_DOMAIN);
                    }
                    interview.interviewers.push(email.to_string());
                    continue;
                }

                // It must be the person being interviewed.
                // See if we can get the Applicant record ID for them.
                interview.email = attendee.email.to_string();
            }

            for (_, sheet_id) in get_sheets_map() {
                let applicant = Applicant::get_from_db(&db, interview.email.to_string(), sheet_id.to_string());
                if let Some(a) = applicant {
                    interview.applicant = vec![a.airtable_record_id];
                    interview.name = a.name.to_string();
                    break;
                }
            }

            let name = interview.name.to_string();
            let mut interviewers = interview.interviewers.clone();
            interviewers
                .iter_mut()
                .for_each(|x| *x = x.trim_end_matches(GSUITE_DOMAIN).trim_end_matches(DOMAIN).trim_end_matches('@').to_string());

            interview = format!("{} ({})", name, interviewers.join(", "));

            interview.upsert(&db).await;
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
