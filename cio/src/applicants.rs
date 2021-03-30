#![allow(clippy::from_over_into)]
use std::collections::{BTreeMap, HashMap};
use std::env;
use std::fs;
use std::io::{copy, stderr, stdout, Write};
use std::process::Command;
use std::str::FromStr;

use async_trait::async_trait;
use checkr::Checkr;
use chrono::offset::Utc;
use chrono::{DateTime, Duration};
use chrono_humanize::HumanTime;
use google_drive::GoogleDrive;
use html2text::from_read;
use hubcaps::issues::{Issue, IssueListOptions, IssueOptions, State};
use hubcaps::Github;
use macros::db;
use pandoc::OutputKind;
use rand::seq::SliceRandom;
use regex::Regex;
use schemars::JsonSchema;
use sendgrid_api::SendGrid;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sheets::Sheets;
use slack_chat_api::{FormattedMessage, MessageBlock, MessageBlockText, MessageBlockType, MessageType};
use tar::Archive;
use tracing::instrument;
use walkdir::WalkDir;

use crate::airtable::{AIRTABLE_APPLICATIONS_TABLE, AIRTABLE_BASE_ID_RECURITING_APPLICATIONS};
use crate::core::UpdateAirtableRecord;
use crate::db::Database;
use crate::models::get_value;
use crate::schema::applicants;
use crate::slack::{get_hiring_channel_post_url, post_to_channel};
use crate::utils::{authenticate_github_jwt, check_if_github_issue_exists, get_gsuite_token, github_org, DOMAIN, GSUITE_DOMAIN};

// The line breaks that get parsed are weird thats why we have the random asterisks here.
static QUESTION_TECHNICALLY_CHALLENGING: &str = r"W(?s:.*)at work(?s:.*)ave you found mos(?s:.*)challenging(?s:.*)caree(?s:.*)wh(?s:.*)\?";
static QUESTION_WORK_PROUD_OF: &str = r"W(?s:.*)at work(?s:.*)ave you done that you(?s:.*)particularl(?s:.*)proud o(?s:.*)and why\?";
static QUESTION_HAPPIEST_CAREER: &str = r"W(?s:.*)en have you been happiest in your professiona(?s:.*)caree(?s:.*)and why\?";
static QUESTION_UNHAPPIEST_CAREER: &str = r"W(?s:.*)en have you been unhappiest in your professiona(?s:.*)caree(?s:.*)and why\?";
static QUESTION_VALUE_REFLECTED: &str = r"F(?s:.*)r one of Oxide(?s:.*)s values(?s:.*)describe an example of ho(?s:.*)it wa(?s:.*)reflected(?s:.*)particula(?s:.*)body(?s:.*)you(?s:.*)work\.";
static QUESTION_VALUE_VIOLATED: &str = r"F(?s:.*)r one of Oxide(?s:.*)s values(?s:.*)describe an example of ho(?s:.*)it wa(?s:.*)violated(?s:.*)you(?s:.*)organization o(?s:.*)work\.";
static QUESTION_VALUES_IN_TENSION: &str =
    r"F(?s:.*)r a pair of Oxide(?s:.*)s values(?s:.*)describe a time in whic(?s:.*)the tw(?s:.*)values(?s:.*)tensio(?s:.*)for(?s:.*)your(?s:.*)and how yo(?s:.*)resolved it\.";
static QUESTION_WHY_OXIDE: &str = r"W(?s:.*)y do you want to work for Oxide\?";

/// The data type for a NewApplicant.
#[db {
    new_struct_name = "Applicant",
    airtable_base_id = "AIRTABLE_BASE_ID_RECURITING_APPLICATIONS",
    airtable_table = "AIRTABLE_APPLICATIONS_TABLE",
    match_on = {
        "email" = "String",
        "sheet_id" = "String",
    },
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[table_name = "applicants"]
pub struct NewApplicant {
    pub name: String,
    pub role: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sheet_id: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub raw_status: String,
    pub submitted_time: DateTime<Utc>,
    pub email: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub phone: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub country_code: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub location: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub github: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub gitlab: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub linkedin: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub portfolio: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub website: String,
    pub resume: String,
    pub materials: String,
    #[serde(default)]
    pub sent_email_received: bool,
    #[serde(default)]
    pub sent_email_follow_up: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub value_reflected: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub value_violated: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values_in_tension: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub resume_contents: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub materials_contents: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub work_samples: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub writing_samples: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub analysis_samples: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub presentation_samples: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub exploratory_samples: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub question_technically_challenging: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub question_proud_of: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub question_happiest: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub question_unhappiest: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub question_value_reflected: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub question_value_violated: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub question_values_in_tension: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub question_why_oxide: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub interview_packet: String,
    /// Airtable fields.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub interviews: Vec<String>,

    /// The scorers/reviewers assigned to the applicant.
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        serialize_with = "airtable_api::user_format_as_array_of_strings::serialize",
        deserialize_with = "airtable_api::user_format_as_array_of_strings::deserialize"
    )]
    pub scorers: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub scoring_form_id: String,
    /// The form for scoring/evaluating applicants.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub scoring_form_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub scoring_form_responses_url: String,
    /// The number of form responses for the applicant.
    #[serde(default)]
    pub scoring_evaluations_count: i32,
    #[serde(default)]
    pub scoring_enthusiastic_yes_count: i32,
    #[serde(default)]
    pub scoring_yes_count: i32,
    #[serde(default)]
    pub scoring_pass_count: i32,
    #[serde(default)]
    pub scoring_no_count: i32,
    #[serde(default)]
    pub scoring_not_applicable_count: i32,
    #[serde(default)]
    pub scoring_insufficient_experience_count: i32,
    #[serde(default)]
    pub scoring_inapplicable_experience_count: i32,
    #[serde(default)]
    pub scoring_job_function_yet_needed_count: i32,
    #[serde(default)]
    pub scoring_underwhelming_materials_count: i32,

    #[serde(default)]
    pub request_background_check: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub criminal_background_check_status: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub motor_vehicle_background_check_status: String,
}

impl NewApplicant {
    /// Parse the sheet columns from single Google Sheets row values.
    /// This is what we get back from the webhook.
    #[instrument]
    #[inline]
    pub fn parse_from_row(sheet_id: &str, values: &HashMap<String, Vec<String>>) -> Self {
        // Fill in the data we know from what we got from the row.
        let (github, gitlab) = NewApplicant::parse_github_gitlab(&get_value(values, "GitHub Profile URL"));

        NewApplicant {
            submitted_time: NewApplicant::parse_timestamp(&get_value(values, "Timestamp")),
            role: get_role_from_sheet_id(sheet_id),
            sheet_id: sheet_id.to_string(),
            name: get_value(values, "Name"),
            email: get_value(values, "Email Address"),
            location: get_value(values, "Location (City, State or Region)"),
            phone: get_value(values, "Phone Number"),
            country_code: Default::default(),
            github,
            gitlab,
            linkedin: get_value(values, "LinkedIn profile URL"),
            portfolio: get_value(values, "Portfolio"),
            website: get_value(values, "Website"),
            resume: get_value(values, "Submit your resume (or PDF export of LinkedIn profile)"),
            materials: get_value(values, "Submit your Oxide candidate materials"),
            status: crate::applicant_status::Status::NeedsToBeTriaged.to_string(),
            raw_status: get_value(values, "Status"),
            sent_email_received: false,
            sent_email_follow_up: false,
            value_reflected: Default::default(),
            value_violated: Default::default(),
            values_in_tension: Default::default(),
            resume_contents: Default::default(),
            materials_contents: Default::default(),
            work_samples: Default::default(),
            writing_samples: Default::default(),
            analysis_samples: Default::default(),
            presentation_samples: Default::default(),
            exploratory_samples: Default::default(),
            question_technically_challenging: Default::default(),
            question_proud_of: Default::default(),
            question_happiest: Default::default(),
            question_unhappiest: Default::default(),
            question_value_reflected: Default::default(),
            question_value_violated: Default::default(),
            question_values_in_tension: Default::default(),
            question_why_oxide: Default::default(),
            interview_packet: Default::default(),
            interviews: Default::default(),
            scorers: Default::default(),
            scoring_form_id: Default::default(),
            scoring_form_url: Default::default(),
            scoring_form_responses_url: Default::default(),
            scoring_evaluations_count: Default::default(),
            scoring_enthusiastic_yes_count: Default::default(),
            scoring_yes_count: Default::default(),
            scoring_pass_count: Default::default(),
            scoring_no_count: Default::default(),
            scoring_not_applicable_count: Default::default(),
            scoring_insufficient_experience_count: Default::default(),
            scoring_inapplicable_experience_count: Default::default(),
            scoring_job_function_yet_needed_count: Default::default(),
            scoring_underwhelming_materials_count: Default::default(),
            request_background_check: Default::default(),
            criminal_background_check_status: Default::default(),
            motor_vehicle_background_check_status: Default::default(),
        }
    }

    /// Send an email to the applicant that we recieved their application.
    #[instrument]
    #[inline]
    pub async fn send_email_recieved_application_to_applicant(&self) {
        // Initialize the SendGrid client.
        let sendgrid_client = SendGrid::new_from_env();

        // Send the message.
        sendgrid_client
            .send_mail(
                "Oxide Computer Company Application Received!".to_string(),
                format!(
                    "Dear {},

Thank you for submitting your application materials! We really appreciate all
the time and thought everyone puts into their application. We will be in touch
within the next couple weeks with more information.

Sincerely,
  The Oxide Team",
                    self.name
                ),
                vec![self.email.to_string()],
                vec![format!("careers@{}", DOMAIN)],
                vec![],
                format!("careers@{}", DOMAIN),
            )
            .await;
    }

    /// Send an email to the applicant that they did not provide materials.
    #[instrument]
    #[inline]
    pub async fn send_email_rejection_did_not_provide_materials(&self) {
        // Initialize the SendGrid client.
        let sendgrid_client = SendGrid::new_from_env();

        // Send the message.
        sendgrid_client
            .send_mail(
                "Thank you for your application".to_string(),
                format!(
                    "Dear {},

Unfortunately, we cannot accept it at this time since you failed to provide the
requested materials.

All the best,
The Oxide Team",
                    self.name
                ),
                vec![self.email.to_string()],
                vec![format!("careers@{}", DOMAIN)],
                vec![],
                format!("careers@{}", DOMAIN),
            )
            .await;
    }

    /// Send an email to the applicant about timing.
    #[instrument]
    #[inline]
    pub async fn send_email_rejection_timing(&self) {
        // Initialize the SendGrid client.
        let sendgrid_client = SendGrid::new_from_env();

        // Send the message.
        sendgrid_client
            .send_mail(
                "Thank you for your application".to_string(),
                format!(
                    "Dear {},

We are so humbled by your application to join Oxide Computer Company. At this
stage of the company we are hyper-focused on certain areas of the stack and
when we need specific domain space experience such as yours, please engage
with us. Our roles will be updated as we need them.

We are grateful you took the time to apply and put so much thought into the
candidate materials, we loved reading them. We would absolutely love to work
with you in the future and cannot wait for that stage of the company!

All the best,
The Oxide Team",
                    self.name
                ),
                vec![self.email.to_string()],
                vec![format!("careers@{}", DOMAIN)],
                vec![],
                format!("careers@{}", DOMAIN),
            )
            .await;
    }

    /// Send an email internally that we have a new application.
    #[instrument]
    #[inline]
    pub async fn send_email_internally(&self) {
        // Initialize the SendGrid client.
        let sendgrid_client = SendGrid::new_from_env();

        // Send the message.
        sendgrid_client
            .send_mail(
                format!("New Application: {}", self.name),
                self.as_company_notification_email(),
                vec![format!("all@{}", DOMAIN)],
                vec![],
                vec![],
                format!("applications@{}", DOMAIN),
            )
            .await;
    }

    /// Parse the applicant from a Google Sheets row, where we also happen to know the columns.
    /// This is how we get the spreadsheet back from the API.
    #[instrument]
    #[inline]
    pub async fn parse_from_row_with_columns(sheet_name: &str, sheet_id: &str, columns: &ApplicantSheetColumns, row: &[String]) -> Self {
        // If the length of the row is greater than the status column
        // then we have a status.
        let raw_status = if row.len() > columns.status { row[columns.status].to_string() } else { "".to_string() };
        let mut status = crate::applicant_status::Status::from_str(&raw_status).unwrap_or_default();

        let (github, gitlab) = NewApplicant::parse_github_gitlab(&row[columns.github]);

        // If the length of the row is greater than the linkedin column
        // then we have a linkedin.
        let linkedin = if row.len() > columns.linkedin && columns.linkedin != 0 {
            row[columns.linkedin].trim().to_lowercase()
        } else {
            "".to_string()
        };

        // If the length of the row is greater than the portfolio column
        // then we have a portfolio.
        let portfolio = if row.len() > columns.portfolio && columns.portfolio != 0 {
            row[columns.portfolio].trim().to_string()
        } else {
            "".to_lowercase()
        };

        // If the length of the row is greater than the website column
        // then we have a website.
        let website = if row.len() > columns.website && columns.website != 0 {
            row[columns.website].trim().to_lowercase()
        } else {
            "".to_lowercase()
        };

        // If the length of the row is greater than the value_reflected column
        // then we have a value_reflected.
        let mut value_reflected = if row.len() > columns.value_reflected && columns.value_reflected != 0 {
            row[columns.value_reflected].trim().to_lowercase()
        } else {
            "".to_lowercase()
        };

        // If the length of the row is greater than the value_violated column
        // then we have a value_violated.
        let mut value_violated = if row.len() > columns.value_violated && columns.value_violated != 0 {
            row[columns.value_violated].trim().to_lowercase()
        } else {
            "".to_lowercase()
        };

        let mut values_in_tension: Vec<String> = Default::default();
        // If the length of the row is greater than the value_in_tension1 column
        // then we have a value_in_tension1.
        if row.len() > columns.value_in_tension_1 && columns.value_in_tension_1 != 0 {
            values_in_tension.push(row[columns.value_in_tension_1].trim().to_lowercase());
        }
        // If the length of the row is greater than the value_in_tension2 column
        // then we have a value_in_tension2.
        if row.len() > columns.value_in_tension_2 && columns.value_in_tension_2 != 0 {
            values_in_tension.push(row[columns.value_in_tension_2].trim().to_lowercase());
        }

        // Check if we sent them an email that we received their application.
        let mut sent_email_received = true;
        if row[columns.sent_email_received].to_lowercase().contains("false") {
            sent_email_received = false;
        }

        // Check if we sent them an email to either reject or follow up.
        let mut sent_email_follow_up = true;
        if row[columns.sent_email_follow_up].to_lowercase().contains("false") {
            sent_email_follow_up = false;
        }

        let email = row[columns.email].trim().to_string();
        let location = row[columns.location].trim().to_string();
        let phone = row[columns.phone].trim().to_string();
        let mut country_code = "".to_string();
        let resume = row[columns.resume].to_string();
        let materials = row[columns.materials].to_string();

        let mut resume_contents = String::new();
        let mut materials_contents = String::new();
        let mut work_samples = String::new();
        let mut writing_samples = String::new();
        let mut analysis_samples = String::new();
        let mut presentation_samples = String::new();
        let mut exploratory_samples = String::new();
        let mut question_technically_challenging = String::new();
        let mut question_proud_of = String::new();
        let mut question_happiest = String::new();
        let mut question_unhappiest = String::new();
        let mut question_value_reflected = String::new();
        let mut question_value_violated = String::new();
        let mut question_values_in_tension = String::new();
        let mut question_why_oxide = String::new();

        let mut interviews: Vec<String> = Default::default();
        let mut interview_packet = String::new();

        let mut scorers: Vec<String> = Default::default();
        let mut scoring_form_id = "".to_string();
        let mut scoring_form_url = "".to_string();
        let mut scoring_form_responses_url = "".to_string();

        // Set the defaults.
        let mut scoring_evaluations_count = 0;
        let mut scoring_enthusiastic_yes_count = 0;
        let mut scoring_yes_count = 0;
        let mut scoring_pass_count = 0;
        let mut scoring_no_count = 0;
        let mut scoring_not_applicable_count = 0;
        let mut scoring_insufficient_experience_count = 0;
        let mut scoring_inapplicable_experience_count = 0;
        let mut scoring_job_function_yet_needed_count = 0;
        let mut scoring_underwhelming_materials_count = 0;

        let mut request_background_check = false;
        let mut criminal_background_check_status = "".to_string();
        let mut motor_vehicle_background_check_status = "".to_string();

        // Try to get the applicant, if they exist.
        // This is a way around the stupid magic macro to make sure it
        // doesn't overwrite fields set by other functions on the upsert.
        // TODO: this is gross and disgusting.
        let db = Database::new();
        if let Ok(a) = applicants::dsl::applicants
            .filter(applicants::dsl::email.eq(email.to_string()))
            .filter(applicants::dsl::sheet_id.eq(sheet_id.to_string()))
            .first::<Applicant>(&db.conn())
        {
            // Try to get from airtable.
            // This ensures if we had any one offs added in airtable that they stay intact.
            if let Some(record) = a.get_existing_airtable_record().await {
                scorers = record.fields.scorers;
                request_background_check = record.fields.request_background_check;
                interviews = record.fields.interviews;
            }

            if !a.country_code.is_empty() {
                country_code = a.country_code.to_string();
            }

            if !a.interview_packet.is_empty() {
                interview_packet = a.interview_packet.to_string();
            }

            resume_contents = a.resume_contents.to_string();
            materials_contents = a.materials_contents.to_string();
            work_samples = a.work_samples.to_string();
            writing_samples = a.writing_samples.to_string();
            analysis_samples = a.analysis_samples.to_string();
            presentation_samples = a.presentation_samples.to_string();
            exploratory_samples = a.exploratory_samples.to_string();
            question_technically_challenging = a.question_technically_challenging.to_string();
            question_proud_of = a.question_proud_of.to_string();
            question_happiest = a.question_happiest.to_string();
            question_unhappiest = a.question_unhappiest.to_string();
            question_value_reflected = a.question_value_reflected.to_string();
            question_value_violated = a.question_value_violated.to_string();
            question_values_in_tension = a.question_values_in_tension.to_string();
            question_why_oxide = a.question_why_oxide.to_string();

            if !a.value_reflected.is_empty() {
                value_reflected = a.value_reflected.to_string();
            }
            if !a.value_violated.is_empty() {
                value_violated = a.value_violated.to_string();
            }
            if !a.values_in_tension.is_empty() {
                values_in_tension = a.values_in_tension.clone();
            }
            // Set the scorers data so we don't keep adding new scorers.
            if !a.scorers.is_empty() {
                scoring_form_id = a.scoring_form_id.to_string();
                scoring_form_url = a.scoring_form_url.to_string();
                scoring_form_responses_url = a.scoring_form_responses_url.to_string();

                scoring_evaluations_count = a.scoring_evaluations_count;
                scoring_enthusiastic_yes_count = a.scoring_enthusiastic_yes_count;
                scoring_yes_count = a.scoring_yes_count;
                scoring_pass_count = a.scoring_pass_count;
                scoring_no_count = a.scoring_no_count;
                scoring_not_applicable_count = a.scoring_not_applicable_count;
                scoring_insufficient_experience_count = a.scoring_insufficient_experience_count;
                scoring_inapplicable_experience_count = a.scoring_inapplicable_experience_count;
                scoring_job_function_yet_needed_count = a.scoring_job_function_yet_needed_count;
                scoring_underwhelming_materials_count = a.scoring_underwhelming_materials_count;
            }
            if !a.criminal_background_check_status.is_empty() {
                criminal_background_check_status = a.criminal_background_check_status.to_string();
            }
            if !a.motor_vehicle_background_check_status.is_empty() {
                motor_vehicle_background_check_status = a.criminal_background_check_status.to_string();
            }
        }

        // If we know they have more than 1 interview AND their current status is "next steps",
        // THEN we can mark the applicant as in the "interviewing" state.
        if interviews.len() > 1 && status == crate::applicant_status::Status::NextSteps {
            status = crate::applicant_status::Status::Interviewing;
        }

        NewApplicant {
            submitted_time: NewApplicant::parse_timestamp(&row[columns.timestamp]),
            name: row[columns.name].to_string(),
            email,
            location,
            phone,
            country_code,
            github,
            gitlab,
            linkedin,
            portfolio,
            website,
            resume,
            materials,
            status: status.to_string(),
            raw_status,
            sent_email_received,
            sent_email_follow_up,
            role: sheet_name.to_string(),
            sheet_id: sheet_id.to_string(),
            value_reflected,
            value_violated,
            values_in_tension,
            resume_contents,
            materials_contents,
            work_samples,
            writing_samples,
            analysis_samples,
            presentation_samples,
            exploratory_samples,
            question_technically_challenging,
            question_proud_of,
            question_happiest,
            question_unhappiest,
            question_value_reflected,
            question_value_violated,
            question_values_in_tension,
            question_why_oxide,
            interview_packet,
            interviews,
            scorers,
            scoring_form_id,
            scoring_form_url,
            scoring_form_responses_url,
            scoring_evaluations_count,
            scoring_enthusiastic_yes_count,
            scoring_yes_count,
            scoring_pass_count,
            scoring_no_count,
            scoring_not_applicable_count,
            scoring_insufficient_experience_count,
            scoring_inapplicable_experience_count,
            scoring_job_function_yet_needed_count,
            scoring_underwhelming_materials_count,
            request_background_check,
            criminal_background_check_status,
            motor_vehicle_background_check_status,
        }
    }

    #[instrument]
    #[inline]
    fn parse_timestamp(timestamp: &str) -> DateTime<Utc> {
        // Parse the time.
        let time_str = timestamp.to_owned() + " -08:00";
        DateTime::parse_from_str(&time_str, "%m/%d/%Y %H:%M:%S  %:z").unwrap().with_timezone(&Utc)
    }

    #[instrument]
    #[inline]
    fn parse_github_gitlab(s: &str) -> (String, String) {
        let mut github = "".to_string();
        let mut gitlab = "".to_string();
        if !s.trim().is_empty() {
            github = format!(
                "@{}",
                s.trim()
                    .to_lowercase()
                    .trim_start_matches("https://github.com/")
                    .trim_start_matches("http://github.com/")
                    .trim_start_matches("https://www.github.com/")
                    .trim_start_matches("http://www.github.com/")
                    .trim_start_matches('@')
                    .trim_end_matches('/')
            )
            .trim()
            .to_string();

            if github == "@" {
                github = "".to_string();
            }

            // Some people put a gitlab URL in the github form input,
            // parse those accordingly.
            if github.contains("https://gitlab.com") {
                github = "".to_string();

                gitlab = format!("@{}", s.trim().to_lowercase().trim_start_matches("https://gitlab.com/").trim_start_matches('@').trim_end_matches('/'));
            }
        }

        (github, gitlab)
    }

    /// Expand the applicants materials and do any automation that needs to be done.
    #[instrument(skip(drive_client, sheets_client))]
    #[inline]
    pub async fn expand(&mut self, drive_client: &GoogleDrive, sheets_client: &Sheets, sent_email_received_column_index: usize, sent_email_follow_up_index: usize, row_index: usize) {
        // Check if we have sent them an email that we received their application.
        if !self.sent_email_received {
            // Send them an email.
            self.send_email_recieved_application_to_applicant().await;

            // Mark the column as true not false.
            let mut colmn = "ABCDEFGHIJKLMNOPQRSTUVWXYZ".chars();
            let rng = format!("{}{}", colmn.nth(sent_email_received_column_index).unwrap().to_string(), row_index);

            sheets_client.update_values(&self.sheet_id, &rng, "TRUE".to_string()).await.unwrap();

            println!("[applicant] sent email to {} that we received their application", self.email);
        }

        // Send an email follow up if we should.
        if !self.sent_email_follow_up {
            // Get the right cell to eventually change in the google sheet.
            let mut colmn = "ABCDEFGHIJKLMNOPQRSTUVWXYZ".chars();
            let rng = format!("{}{}", colmn.nth(sent_email_follow_up_index).unwrap().to_string(), row_index);

            let status = crate::applicant_status::Status::from_str(&self.status).unwrap_or_default();
            if status == crate::applicant_status::Status::Declined || status == crate::applicant_status::Status::Deferred {
                // Check if we have sent the follow up email to them.unwrap_or_default().
                if self.raw_status.contains("did not do materials") {
                    // Send the email.
                    self.send_email_rejection_did_not_provide_materials().await;

                    println!("[applicant] sent email to {} tell them they did not do the materials", self.email);
                } else {
                    // Send the email.
                    self.send_email_rejection_timing().await;

                    println!("[applicant] sent email to {} tell them about timing", self.email);
                }

                // Update the cell in the google sheet so we know we sent the email.
                // Mark the column as true not false.
                sheets_client.update_values(&self.sheet_id, &rng, "TRUE".to_string()).await.unwrap();

                self.sent_email_follow_up = true;
            } else if status != crate::applicant_status::Status::NeedsToBeTriaged {
                // Just set that we have sent the email so that we don't do it again if we move to
                // next steps then interviews etc.
                // Only when it's not in "NeedsToBeTriaged".
                // Update the cell in the google sheet so we know we sent the email.
                // Mark the column as true not false.
                sheets_client.update_values(&self.sheet_id, &rng, "TRUE".to_string()).await.unwrap();

                self.sent_email_follow_up = true;
            }
        }

        // Cleanup and parse the phone number and country code.
        let mut phone = self.phone.replace(" ", "").replace("-", "").replace("+", "").replace("(", "").replace(")", "");

        let location = self.location.to_string();
        let mut country = phonenumber::country::US;
        if (location.to_lowercase().contains("uk")
            || location.to_lowercase().contains("london")
            || location.to_lowercase().contains("ipswich")
            || location.to_lowercase().contains("united kingdom")
            || location.to_lowercase().contains("england"))
            && phone.starts_with("44")
        {
            country = phonenumber::country::GB;
        } else if (location.to_lowercase().contains("czech republic") || location.to_lowercase().contains("prague")) && phone.starts_with("420") {
            country = phonenumber::country::CZ;
        } else if location.to_lowercase().contains("turkey") && phone.starts_with("90") {
            country = phonenumber::country::TR;
        } else if location.to_lowercase().contains("sweden") && phone.starts_with("46") {
            country = phonenumber::country::SE;
        } else if (location.to_lowercase().contains("mumbai") || location.to_lowercase().contains("india") || location.to_lowercase().contains("bangalore")) && phone.starts_with("91") {
            country = phonenumber::country::IN;
        } else if location.to_lowercase().contains("brazil") {
            country = phonenumber::country::BR;
        } else if location.to_lowercase().contains("belgium") {
            country = phonenumber::country::BE;
        } else if location.to_lowercase().contains("romania") && phone.starts_with("40") {
            country = phonenumber::country::RO;
        } else if location.to_lowercase().contains("nigeria") {
            country = phonenumber::country::NG;
        } else if location.to_lowercase().contains("austria") {
            country = phonenumber::country::AT;
        } else if location.to_lowercase().contains("australia") && phone.starts_with("61") {
            country = phonenumber::country::AU;
        } else if location.to_lowercase().contains("sri lanka") && phone.starts_with("94") {
            country = phonenumber::country::LK;
        } else if location.to_lowercase().contains("slovenia") && phone.starts_with("386") {
            country = phonenumber::country::SI;
        } else if location.to_lowercase().contains("france") && phone.starts_with("33") {
            country = phonenumber::country::FR;
        } else if location.to_lowercase().contains("netherlands") && phone.starts_with("31") {
            country = phonenumber::country::NL;
        } else if location.to_lowercase().contains("taiwan") {
            country = phonenumber::country::TW;
        } else if location.to_lowercase().contains("new zealand") {
            country = phonenumber::country::NZ;
        } else if location.to_lowercase().contains("maragno") || location.to_lowercase().contains("italy") {
            country = phonenumber::country::IT;
        } else if location.to_lowercase().contains("nairobi") || location.to_lowercase().contains("kenya") {
            country = phonenumber::country::KE;
        } else if location.to_lowercase().contains("dubai") {
            country = phonenumber::country::AE;
        } else if location.to_lowercase().contains("poland") {
            country = phonenumber::country::PL;
        } else if location.to_lowercase().contains("portugal") {
            country = phonenumber::country::PT;
        } else if location.to_lowercase().contains("berlin") || location.to_lowercase().contains("germany") {
            country = phonenumber::country::DE;
        } else if location.to_lowercase().contains("benin") && phone.starts_with("229") {
            country = phonenumber::country::BJ;
        } else if location.to_lowercase().contains("israel") {
            country = phonenumber::country::IL;
        } else if location.to_lowercase().contains("spain") {
            country = phonenumber::country::ES;
        }

        let db = &phonenumber::metadata::DATABASE;
        let metadata = db.by_id(country.as_ref()).unwrap();
        let country_code = metadata.id().to_string().to_lowercase();

        // Get the last ten character of the string.
        if let Ok(phone_number) = phonenumber::parse(Some(country), phone.to_string()) {
            if !phone_number.is_valid() {
                println!("[applicants] phone number is invalid: {}", phone);
            }

            phone = format!("{}", phone_number.format().mode(phonenumber::Mode::International));
        }
        self.phone = phone;
        self.country_code = country_code;

        // Get the time seven days ago.
        let duration_from_now = Utc::now().signed_duration_since(self.submitted_time);

        // If the application is as new as the last week then parse all the contents.
        // This takes a long time so we skip all the others.
        if duration_from_now < Duration::days(7) {
            // Read the file contents.
            self.resume_contents = get_file_contents(drive_client, &self.resume).await;
            self.materials_contents = get_file_contents(drive_client, &self.materials).await;

            // Parse the samples and materials.
            let materials_contents = self.materials_contents.clone();
            let mut work_samples = parse_question(r"Work sample\(s\)", "Writing samples", &materials_contents);
            if work_samples.is_empty() {
                work_samples = parse_question(
                    r"If(?s:.*)his work is entirely proprietary(?s:.*)please describe it as fully as y(?s:.*)can, providing necessary context\.",
                    "Writing samples",
                    &materials_contents,
                );
                if work_samples.is_empty() {
                    // Try to parse work samples for TPM role.
                    work_samples = parse_question(r"What would you have done differently\?", "Exploratory samples", &materials_contents);

                    if work_samples.is_empty() {
                        work_samples = parse_question(r"Some questions(?s:.*)o have in mind as you describe them:", "Exploratory samples", &materials_contents);

                        if work_samples.is_empty() {
                            work_samples = parse_question(r"Work samples", "Exploratory samples", &materials_contents);

                            if work_samples.is_empty() {
                                work_samples = parse_question(r"design sample\(s\)", "Questionnaire", &materials_contents);
                            }
                        }
                    }
                }
            }
            self.work_samples = work_samples;

            let mut writing_samples = parse_question(r"Writing sample\(s\)", "Analysis samples", &materials_contents);
            if writing_samples.is_empty() {
                writing_samples = parse_question(
                    r"Please submit at least one writing sample \(and no more tha(?s:.*)three\) that you feel represent(?s:.*)you(?s:.*)providin(?s:.*)links if(?s:.*)necessary\.",
                    "Analysis samples",
                    &materials_contents,
                );
                if writing_samples.is_empty() {
                    writing_samples = parse_question(r"Writing samples", "Analysis samples", &materials_contents);

                    if writing_samples.is_empty() {
                        writing_samples = parse_question(r"Writing sample\(s\)", "Code and/or design sample", &materials_contents);
                    }
                }
            }
            self.writing_samples = writing_samples;

            let mut analysis_samples = parse_question(r"Analysis sample\(s\)$", "Presentation samples", &materials_contents);
            if analysis_samples.is_empty() {
                analysis_samples = parse_question(
                    r"please recount a(?s:.*)incident(?s:.*)which you analyzed syste(?s:.*)misbehavior(?s:.*)including as much technical detail as you can recall\.",
                    "Presentation samples",
                    &materials_contents,
                );
                if analysis_samples.is_empty() {
                    analysis_samples = parse_question(r"Analysis samples", "Presentation samples", &materials_contents);
                }
            }
            self.analysis_samples = analysis_samples;

            let mut presentation_samples = parse_question(r"Presentation sample\(s\)", "Questionnaire", &materials_contents);
            if presentation_samples.is_empty() {
                presentation_samples = parse_question(
                    r"I(?s:.*)you don’t have a publicl(?s:.*)available presentation(?s:.*)pleas(?s:.*)describe a topic on which you have presented in th(?s:.*)past\.",
                    "Questionnaire",
                    &materials_contents,
                );
                if presentation_samples.is_empty() {
                    presentation_samples = parse_question(r"Presentation samples", "Questionnaire", &materials_contents);
                }
            }
            self.presentation_samples = presentation_samples;

            let mut exploratory_samples = parse_question(r"Exploratory sample\(s\)", "Questionnaire", &materials_contents);
            if exploratory_samples.is_empty() {
                exploratory_samples = parse_question(
                    r"What’s an example o(?s:.*)something that you needed to explore, reverse engineer, decipher or otherwise figure out a(?s:.*)part of a program or project and how did you do it\? Please provide as much detail as you ca(?s:.*)recall\.",
                    "Questionnaire",
                    &materials_contents,
                );
                if exploratory_samples.is_empty() {
                    exploratory_samples = parse_question(r"Exploratory samples", "Questionnaire", &materials_contents);
                }
            }
            self.exploratory_samples = exploratory_samples;

            self.question_technically_challenging = parse_question(QUESTION_TECHNICALLY_CHALLENGING, QUESTION_WORK_PROUD_OF, &materials_contents);
            self.question_proud_of = parse_question(QUESTION_WORK_PROUD_OF, QUESTION_HAPPIEST_CAREER, &materials_contents);
            self.question_happiest = parse_question(QUESTION_HAPPIEST_CAREER, QUESTION_UNHAPPIEST_CAREER, &materials_contents);
            self.question_unhappiest = parse_question(QUESTION_UNHAPPIEST_CAREER, QUESTION_VALUE_REFLECTED, &materials_contents);
            self.question_value_reflected = parse_question(QUESTION_VALUE_REFLECTED, QUESTION_VALUE_VIOLATED, &materials_contents);
            self.question_value_violated = parse_question(QUESTION_VALUE_VIOLATED, QUESTION_VALUES_IN_TENSION, &materials_contents);
            self.question_values_in_tension = parse_question(QUESTION_VALUES_IN_TENSION, QUESTION_WHY_OXIDE, &materials_contents);
            self.question_why_oxide = parse_question(QUESTION_WHY_OXIDE, "", &materials_contents);
        }
    }

    /// Get the human duration of time since the application was submitted.
    #[instrument]
    #[inline]
    pub fn human_duration(&self) -> HumanTime {
        let mut dur = self.submitted_time - Utc::now();
        if dur.num_seconds() > 0 {
            dur = -dur;
        }

        HumanTime::from(dur)
    }

    /// Convert the applicant into JSON for a Slack message.
    #[instrument]
    #[inline]
    pub fn as_slack_msg(&self) -> Value {
        let time = self.human_duration();

        let mut status_msg = format!("<https://docs.google.com/spreadsheets/d/{}|{}> Applicant | applied {}", self.sheet_id, self.role, time);
        if !self.status.is_empty() {
            status_msg += &format!(" | status: *{}*", self.status);
        }

        let mut values_msg = "".to_string();
        if !self.value_reflected.is_empty() {
            values_msg += &format!("values reflected: *{}*", self.value_reflected);
        }
        if !self.value_violated.is_empty() {
            values_msg += &format!(" | violated: *{}*", self.value_violated);
        }
        for (k, tension) in self.values_in_tension.iter().enumerate() {
            if k == 0 {
                values_msg += &format!(" | in tension: *{}*", tension);
            } else {
                values_msg += &format!(" *& {}*", tension);
            }
        }
        if values_msg.is_empty() {
            values_msg = "values not yet populated".to_string();
        }

        let mut intro_msg = format!("*{}*  <mailto:{}|{}>", self.name, self.email, self.email,);
        if !self.location.is_empty() {
            intro_msg += &format!("  {}", self.location);
        }

        let mut info_msg = format!("<{}|resume> | <{}|materials>", self.resume, self.materials,);
        if !self.phone.is_empty() {
            info_msg += &format!(" | <tel:{}|{}>", self.phone, self.phone);
        }
        if !self.github.is_empty() {
            info_msg += &format!(" | <https://github.com/{}|github:{}>", self.github.trim_start_matches('@'), self.github,);
        }
        if !self.gitlab.is_empty() {
            info_msg += &format!(" | <https://gitlab.com/{}|gitlab:{}>", self.gitlab.trim_start_matches('@'), self.gitlab,);
        }
        if !self.linkedin.is_empty() {
            info_msg += &format!(" | <{}|linkedin>", self.linkedin,);
        }
        if !self.portfolio.is_empty() {
            info_msg += &format!(" | <{}|portfolio>", self.portfolio,);
        }
        if !self.website.is_empty() {
            info_msg += &format!(" | <{}|website>", self.website,);
        }

        json!(FormattedMessage {
            channel: Default::default(),
            attachments: Default::default(),
            blocks: vec![
                MessageBlock {
                    block_type: MessageBlockType::Section,
                    text: Some(MessageBlockText {
                        text_type: MessageType::Markdown,
                        text: intro_msg,
                    }),
                    elements: Default::default(),
                    accessory: Default::default(),
                    block_id: Default::default(),
                    fields: Default::default(),
                },
                MessageBlock {
                    block_type: MessageBlockType::Context,
                    elements: vec![MessageBlockText {
                        text_type: MessageType::Markdown,
                        text: info_msg,
                    }],
                    text: Default::default(),
                    accessory: Default::default(),
                    block_id: Default::default(),
                    fields: Default::default(),
                },
                MessageBlock {
                    block_type: MessageBlockType::Context,
                    elements: vec![MessageBlockText {
                        text_type: MessageType::Markdown,
                        text: values_msg,
                    }],
                    text: Default::default(),
                    accessory: Default::default(),
                    block_id: Default::default(),
                    fields: Default::default(),
                },
                MessageBlock {
                    block_type: MessageBlockType::Context,
                    elements: vec![MessageBlockText {
                        text_type: MessageType::Markdown,
                        text: status_msg,
                    }],
                    text: Default::default(),
                    accessory: Default::default(),
                    block_id: Default::default(),
                    fields: Default::default(),
                }
            ]
        })
    }

    /// Get the applicant's information in the form of the body of an email for a
    /// company wide notification that we received a new application.
    #[instrument]
    #[inline]
    pub fn as_company_notification_email(&self) -> String {
        let time = self.human_duration();

        let mut msg = format!(
            "## Applicant Information for {}

Submitted {}
Name: {}
Email: {}",
            self.role, time, self.name, self.email
        );

        if !self.location.is_empty() {
            msg += &format!("\nLocation: {}", self.location);
        }
        if !self.phone.is_empty() {
            msg += &format!("\nPhone: {}", self.phone);
        }

        if !self.github.is_empty() {
            msg += &format!("\nGitHub: {} (https://github.com/{})", self.github, self.github.trim_start_matches('@'));
        }
        if !self.gitlab.is_empty() {
            msg += &format!("\nGitLab: {} (https://gitlab.com/{})", self.gitlab, self.gitlab.trim_start_matches('@'));
        }
        if !self.linkedin.is_empty() {
            msg += &format!("\nLinkedIn: {}", self.linkedin);
        }
        if !self.portfolio.is_empty() {
            msg += &format!("\nPortfolio: {}", self.portfolio);
        }
        if !self.website.is_empty() {
            msg += &format!("\nWebsite: {}", self.website);
        }

        msg += &format!(
            "\nResume: {}
Oxide Candidate Materials: {}

## Reminder

To view the all the candidates refer to the following Google spreadsheets:

- Engineering Applications: https://applications-engineering.corp.oxide.computer
- Product Engineering and Design Applications: https://applications-product.corp.oxide.computer
- Technical Program Manager Applications: https://applications-tpm.corp.oxide.computer

The applicants Airtable is at: https://airtable-applicants.corp.oxide.computer
",
            self.resume, self.materials,
        );

        msg
    }
}

impl Applicant {
    /// Get the human duration of time since the application was submitted.
    #[instrument]
    #[inline]
    pub fn human_duration(&self) -> HumanTime {
        let mut dur = self.submitted_time - Utc::now();
        if dur.num_seconds() > 0 {
            dur = -dur;
        }

        HumanTime::from(dur)
    }

    /// Send an invite to the applicant to do a background check.
    #[instrument(skip(db))]
    #[inline]
    pub async fn send_background_check_invitation(&mut self, db: &Database) {
        // Initialize the Checker client.
        let checkr = Checkr::new_from_env();

        // Check if we already sent them an invitation.
        let candidates = checkr.list_candidates().await.unwrap();
        for candidate in candidates {
            if candidate.email == self.email {
                // We can return early they already exist as a candidate.
                return;
            }
        }

        // Create a new candidate for the applicant in checkr.
        let candidate = checkr.create_candidate(&self.email).await.unwrap();

        // Create an invitation for the candidate.
        checkr.create_invitation(&candidate.id, "premium_criminal").await.unwrap();

        // Update the database.
        self.request_background_check = true;
        self.criminal_background_check_status = "requested".to_string();

        self.update(db).await;

        println!("[applicant] sent background check invitation to: {}", self.email);
    }

    /// Convert the applicant into JSON for a Slack message.
    #[instrument]
    #[inline]
    pub fn as_slack_msg(&self) -> Value {
        let time = self.human_duration();

        let mut status_msg = format!("<https://docs.google.com/spreadsheets/d/{}|{}> Applicant | applied {}", self.sheet_id, self.role, time);
        if !self.status.is_empty() {
            status_msg += &format!(" | status: *{}*", self.status);
        }

        let mut values_msg = "".to_string();
        if !self.value_reflected.is_empty() {
            values_msg += &format!("values reflected: *{}*", self.value_reflected);
        }
        if !self.value_violated.is_empty() {
            values_msg += &format!(" | violated: *{}*", self.value_violated);
        }
        for (k, tension) in self.values_in_tension.iter().enumerate() {
            if k == 0 {
                values_msg += &format!(" | in tension: *{}*", tension);
            } else {
                values_msg += &format!(" *& {}*", tension);
            }
        }
        if values_msg.is_empty() {
            values_msg = "values not yet populated".to_string();
        }

        let mut intro_msg = format!("*{}*  <mailto:{}|{}>", self.name, self.email, self.email,);
        if !self.location.is_empty() {
            intro_msg += &format!("  {}", self.location);
        }

        let mut info_msg = format!("<{}|resume> | <{}|materials>", self.resume, self.materials,);
        if !self.phone.is_empty() {
            info_msg += &format!(" | <tel:{}|{}>", self.phone, self.phone);
        }
        if !self.github.is_empty() {
            info_msg += &format!(" | <https://github.com/{}|github:{}>", self.github.trim_start_matches('@'), self.github,);
        }
        if !self.gitlab.is_empty() {
            info_msg += &format!(" | <https://gitlab.com/{}|gitlab:{}>", self.gitlab.trim_start_matches('@'), self.gitlab,);
        }
        if !self.linkedin.is_empty() {
            info_msg += &format!(" | <{}|linkedin>", self.linkedin,);
        }
        if !self.portfolio.is_empty() {
            info_msg += &format!(" | <{}|portfolio>", self.portfolio,);
        }
        if !self.website.is_empty() {
            info_msg += &format!(" | <{}|website>", self.website,);
        }

        json!(FormattedMessage {
            channel: Default::default(),
            attachments: Default::default(),
            blocks: vec![
                MessageBlock {
                    block_type: MessageBlockType::Section,
                    text: Some(MessageBlockText {
                        text_type: MessageType::Markdown,
                        text: intro_msg,
                    }),
                    elements: Default::default(),
                    accessory: Default::default(),
                    block_id: Default::default(),
                    fields: Default::default(),
                },
                MessageBlock {
                    block_type: MessageBlockType::Context,
                    elements: vec![MessageBlockText {
                        text_type: MessageType::Markdown,
                        text: info_msg,
                    }],
                    text: Default::default(),
                    accessory: Default::default(),
                    block_id: Default::default(),
                    fields: Default::default(),
                },
                MessageBlock {
                    block_type: MessageBlockType::Context,
                    elements: vec![MessageBlockText {
                        text_type: MessageType::Markdown,
                        text: values_msg,
                    }],
                    text: Default::default(),
                    accessory: Default::default(),
                    block_id: Default::default(),
                    fields: Default::default(),
                },
                MessageBlock {
                    block_type: MessageBlockType::Context,
                    elements: vec![MessageBlockText {
                        text_type: MessageType::Markdown,
                        text: status_msg,
                    }],
                    text: Default::default(),
                    accessory: Default::default(),
                    block_id: Default::default(),
                    fields: Default::default(),
                }
            ]
        })
    }

    /// Send an email to a scorer that they are assigned to an applicant.
    #[instrument]
    #[inline]
    pub async fn send_email_to_scorer(&self, scorer: &str) {
        // Initialize the SendGrid client.
        let sendgrid_client = SendGrid::new_from_env();

        // Send the message.
        sendgrid_client
            .send_mail(
                format!("[applicants] Reviewing applicant {}", self.name),
                self.as_scorer_email(),
                vec![scorer.to_string()],
                vec![],
                vec![],
                format!("careers@{}", DOMAIN),
            )
            .await;
    }

    /// Get the applicant's information in the form of the body of an email for a
    /// scorer email that they have been assigned to score the applicant.
    #[instrument]
    #[inline]
    pub fn as_scorer_email(&self) -> String {
        let time = self.human_duration();

        let mut msg = format!(
            "You have been assigned to review the applicant: {}

Role: {}
Submitted: {}
Name: {}
Email: {}",
            self.name, self.role, time, self.name, self.email
        );

        if !self.location.is_empty() {
            msg += &format!("\nLocation: {}", self.location);
        }
        if !self.phone.is_empty() {
            msg += &format!("\nPhone: {}", self.phone);
        }

        if !self.github.is_empty() {
            msg += &format!("\nGitHub: {} (https://github.com/{})", self.github, self.github.trim_start_matches('@'));
        }
        if !self.gitlab.is_empty() {
            msg += &format!("\nGitLab: {} (https://gitlab.com/{})", self.gitlab, self.gitlab.trim_start_matches('@'));
        }
        if !self.linkedin.is_empty() {
            msg += &format!("\nLinkedIn: {}", self.linkedin);
        }
        if !self.portfolio.is_empty() {
            msg += &format!("\nPortfolio: {}", self.portfolio);
        }
        if !self.website.is_empty() {
            msg += &format!("\nWebsite: {}", self.website);
        }

        msg += &format!(
            "\nResume: {}
Oxide Candidate Materials: {}
Scoring form: {}
Scoring form responses: {}

## Reminder

The applicants Airtable is at: https://airtable-applicants.corp.oxide.computer

",
            self.resume, self.materials, self.scoring_form_url, self.scoring_form_responses_url,
        );

        msg
    }

    #[instrument]
    #[inline]
    pub async fn create_github_onboarding_issue(&self, github: &Github, configs_issues: &[Issue]) {
        // Check if their status is not hired, we only care about hired applicants.
        if !self.status.contains("Hired") {
            return;
        }

        // Check if we already have an issue for this user.
        let issue = check_if_github_issue_exists(&configs_issues, &self.name);
        if issue.is_some() {
            // Return early we don't want to update the issue because it will overwrite
            // any changes we made.
            return;
        }

        // Create an issue for the applicant.
        let title = format!("Onboarding: {}", self.name);
        let labels = vec!["hiring".to_string()];
        let body = format!(
            "- [ ] Add to users.toml
- [ ] Add to matrix chat
Start Date: [START DATE (ex. Monday, January 20th, 2020)]
Personal Email: {}
Twitter: [TWITTER HANDLE]
GitHub: {}
Phone: {}
Location: {}
cc @jessfraz @sdtuck @bcantrill",
            self.email, self.github, self.phone, self.location,
        );

        // Create the issue.
        github
            .repo(github_org(), "configs")
            .issues()
            .create(&IssueOptions {
                title,
                body: Some(body),
                assignee: Some("jessfraz".to_string()),
                labels,
                milestone: Default::default(),
                state: Default::default(),
            })
            .await
            .unwrap();

        println!("[applicant]: created onboarding issue for {}", self.email);
    }
}

#[instrument]
#[inline]
fn parse_question(q1: &str, q2: &str, materials_contents: &str) -> String {
    if materials_contents.is_empty() {
        Default::default()
    }

    let re = Regex::new(&(q1.to_owned() + r"(?s)(.*)" + q2)).unwrap();
    if let Some(q) = re.captures(materials_contents) {
        let val = q.get(1).unwrap();
        let s = val
            .as_str()
            .replace("________________", "")
            .replace("Oxide Candidate Materials: Technical Program Manager", "")
            .replace("Oxide Candidate Materials", "")
            .replace("Work sample(s)", "")
            .trim_start_matches(':')
            .trim()
            .to_string();

        if s.is_empty() {
            return Default::default();
        }

        return s;
    }

    Default::default()
}

/// Implement updating the Airtable record for an Applicant.
#[async_trait]
impl UpdateAirtableRecord<Applicant> for Applicant {
    async fn update_airtable_record(&mut self, record: Applicant) {
        self.interviews = record.interviews;
    }
}

/// The data type for a Google Sheet applicant columns, we use this when
/// parsing the Google Sheets for applicants.
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct ApplicantSheetColumns {
    pub timestamp: usize,
    pub name: usize,
    pub email: usize,
    pub location: usize,
    pub phone: usize,
    pub github: usize,
    pub portfolio: usize,
    pub website: usize,
    pub linkedin: usize,
    pub resume: usize,
    pub materials: usize,
    pub status: usize,
    pub sent_email_received: usize,
    pub sent_email_follow_up: usize,
    pub value_reflected: usize,
    pub value_violated: usize,
    pub value_in_tension_1: usize,
    pub value_in_tension_2: usize,
}

impl ApplicantSheetColumns {
    /// Parse the sheet columns from Google Sheets values.
    #[instrument]
    #[inline]
    pub fn parse(values: &[Vec<String>]) -> Self {
        // Iterate over the columns.
        // TODO: make this less horrible
        let mut columns: ApplicantSheetColumns = Default::default();

        // Get the first row.
        let row = values.get(0).unwrap();

        for (index, col) in row.iter().enumerate() {
            let c = col.to_lowercase();

            if c.contains("timestamp") {
                columns.timestamp = index;
            }
            if c.contains("name") {
                columns.name = index;
            }
            if c.contains("email address") {
                columns.email = index;
            }
            if c.contains("location") {
                columns.location = index;
            }
            if c.contains("phone") {
                columns.phone = index;
            }
            if c.contains("github") {
                columns.github = index;
            }
            if c.contains("portfolio url") {
                columns.portfolio = index;
            }
            if c.contains("website") {
                columns.website = index;
            }
            if c.contains("linkedin") {
                columns.linkedin = index;
            }
            if c.contains("resume") {
                columns.resume = index;
            }
            if c.contains("materials") {
                columns.materials = index;
            }
            if c.contains("status") {
                columns.status = index;
            }
            if c.contains("value reflected") {
                columns.value_reflected = index;
            }
            if c.contains("value violated") {
                columns.value_violated = index;
            }
            if c.contains("value in tension [1") {
                columns.value_in_tension_1 = index;
            }
            if c.contains("value in tension [2") {
                columns.value_in_tension_2 = index;
            }
            if c.contains("sent email that we received their application") {
                columns.sent_email_received = index;
            }
            if c.contains("have sent follow up email") {
                columns.sent_email_follow_up = index;
            }
        }
        columns
    }
}

/// Get the contexts of a file in Google Drive by it's URL as a text string.
#[instrument(skip(drive_client))]
#[inline]
pub async fn get_file_contents(drive_client: &GoogleDrive, url: &str) -> String {
    let id = url.replace("https://drive.google.com/open?id=", "");

    // Get information about the file.
    let drive_file = drive_client.get_file_by_id(&id).await.unwrap();
    let mime_type = drive_file.mime_type;
    let name = drive_file.name;

    let mut path = env::temp_dir();
    let mut output = env::temp_dir();

    let mut result: String = Default::default();

    if mime_type == "application/pdf" {
        // Get the PDF contents from Drive.
        let contents = drive_client.download_file_by_id(&id).await.unwrap();

        path.push(format!("{}.pdf", id));

        let mut file = fs::File::create(&path).unwrap();
        file.write_all(&contents).unwrap();

        result = read_pdf(&name, path.clone());
    } else if mime_type == "text/html" {
        let contents = drive_client.download_file_by_id(&id).await.unwrap();

        // Wrap lines at 80 characters.
        result = from_read(&contents[..], 80);
    } else if mime_type == "application/vnd.google-apps.document" {
        result = drive_client.get_file_contents_by_id(&id).await.unwrap();
    } else if name.ends_with(".tar") {
        // Get the ip contents from Drive.
        let contents = drive_client.download_file_by_id(&id).await.unwrap();

        path.push(format!("{}.tar", id));

        let mut file = fs::File::create(&path).unwrap();
        file.write_all(&contents).unwrap();

        // Unpack the tarball.
        let mut tar = Archive::new(fs::File::open(&path).unwrap());
        output.push(id);
        println!("unpacking tarball: {:?} -> {:?}", path, output);
        tar.unpack(&output).unwrap();

        // Walk the output directory trying to find our file.
        for entry in WalkDir::new(&output).min_depth(1) {
            let entry = entry.unwrap();
            let path = entry.path();
            if is_materials(path.file_name().unwrap().to_str().unwrap()) {
                // Concatenate all the zip files into our result.
                result += &format!(
                    "====================== tarball file: {} ======================\n\n",
                    path.to_str().unwrap().replace(env::temp_dir().as_path().to_str().unwrap(), "")
                );
                if path.extension().unwrap() == "pdf" {
                    result += &read_pdf(&name, path.to_path_buf());
                } else {
                    result += &fs::read_to_string(&path).unwrap();
                }
                result += "\n\n\n";
            }
        }
    } else if name.ends_with(".zip") {
        // This is patrick :)
        // Get the ip contents from Drive.
        let contents = drive_client.download_file_by_id(&id).await.unwrap();

        path.push(format!("{}.zip", id));

        let mut file = fs::File::create(&path).unwrap();
        file.write_all(&contents).unwrap();
        file = fs::File::open(&path).unwrap();

        // Unzip the file.
        let mut archive = zip::ZipArchive::new(file).unwrap();
        for i in 0..archive.len() {
            let mut file = archive.by_index(i).unwrap();
            output = env::temp_dir();
            output.push("zip/");
            output.push(file.name());

            {
                let comment = file.comment();
                if !comment.is_empty() {
                    println!("[applicants] zip file {} comment: {}", i, comment);
                }
            }

            if (&*file.name()).ends_with('/') {
                println!("[applicants] zip file {} extracted to \"{}\"", i, output.as_path().display());
                fs::create_dir_all(&output).unwrap();
            } else {
                println!("[applicants] zip file {} extracted to \"{}\" ({} bytes)", i, output.as_path().display(), file.size());

                if let Some(p) = output.parent() {
                    if !p.exists() {
                        fs::create_dir_all(&p).unwrap();
                    }
                }
                let mut outfile = fs::File::create(&output).unwrap();
                copy(&mut file, &mut outfile).unwrap();

                let file_name = output.to_str().unwrap();
                if !output.is_dir() && is_materials(file_name) {
                    // Concatenate all the zip files into our result.
                    result += &format!(
                        "====================== zip file: {} ======================\n\n",
                        output.as_path().to_str().unwrap().replace(env::temp_dir().as_path().to_str().unwrap(), "")
                    );
                    if output.as_path().extension().unwrap() == "pdf" {
                        result += &read_pdf(&name, output.clone());
                    } else {
                        result += &fs::read_to_string(&output).unwrap();
                    }
                    result += "\n\n\n";
                }
            }
        }
    } else if name.ends_with(".doc") || name.ends_with(".pptx") || name.ends_with(".jpg")
    // TODO: handle these formats
    {
        println!("[applicants] unsupported doc format -- mime type: {}, name: {}, path: {}", mime_type, name, path.to_str().unwrap());
    } else {
        let contents = drive_client.download_file_by_id(&id).await.unwrap();
        path.push(name.to_string());

        let mut file = fs::File::create(&path).unwrap();
        file.write_all(&contents).unwrap();

        output.push(format!("{}.txt", id));

        let mut pandoc = pandoc::new();
        pandoc.add_input(&path);
        pandoc.set_output(OutputKind::File(output.clone()));
        match pandoc.execute() {
            Ok(_) => (),
            Err(e) => {
                println!("pandoc failed: {}", e);
                return "".to_string();
            }
        }
        result = fs::read_to_string(output.clone()).unwrap();
    }

    // Delete the temporary file, if it exists.
    for p in vec![path, output] {
        if p.exists() && !p.is_dir() {
            fs::remove_file(p).unwrap();
        }
    }

    result.trim().to_string()
}

#[instrument]
#[inline]
fn read_pdf(name: &str, path: std::path::PathBuf) -> String {
    let mut output = env::temp_dir();
    output.push("tempfile.txt");

    // Extract the text from the PDF
    let cmd_output = Command::new("pdftotext").args(&["-enc", "UTF-8", path.to_str().unwrap(), output.to_str().unwrap()]).output().unwrap();

    let result = match fs::read_to_string(output.clone()) {
        Ok(r) => r,
        Err(e) => {
            println!("[applicants] running pdf2text failed: {} | name: {}, path: {}", e, name, path.as_path().display());
            stdout().write_all(&cmd_output.stdout).unwrap();
            stderr().write_all(&cmd_output.stderr).unwrap();

            "".to_string()
        }
    };

    // Delete the temporary file, if it exists.
    for p in vec![path, output] {
        if p.exists() && !p.is_dir() {
            fs::remove_file(p).unwrap();
        }
    }

    result
}

#[instrument]
#[inline]
pub fn get_sheets_map() -> BTreeMap<&'static str, &'static str> {
    let mut sheets: BTreeMap<&str, &str> = BTreeMap::new();
    sheets.insert("Engineering", "1FHA-otHCGwe5fCRpcl89MWI7GHiFfN3EWjO6K943rYA");
    sheets.insert("Product Engineering and Design", "1VkRgmr_ZdR-y_1NJc8L0Iv6UVqKaZapt3T_Bq_gqPiI");
    sheets.insert("Technical Program Management", "1Z9sNUBW2z-Tlie0ci8xiet4Nryh-F0O82TFmQ1rQqlU");

    sheets
}

#[instrument]
#[inline]
pub fn get_role_from_sheet_id(sheet_id: &str) -> String {
    for (name, id) in get_sheets_map() {
        if *id == *sheet_id {
            return name.to_string();
        }
    }

    String::new()
}

/// Return a vector of all the raw applicants and add all the metadata.
#[instrument]
#[inline]
pub async fn get_raw_applicants() -> Vec<NewApplicant> {
    // Get the GSuite token.
    let token = get_gsuite_token("").await;

    // Initialize the GSuite sheets client.
    let sheets_client = Sheets::new(token.clone());

    // Initialize the GSuite sheets client.
    let drive_client = GoogleDrive::new(token.clone());

    // Iterate over the Google sheets and create or update GitHub issues
    // depending on the application status.
    let mut applicants: Vec<NewApplicant> = Default::default();
    for (sheet_name, sheet_id) in get_sheets_map() {
        // Get the values in the sheet.
        let sheet_values = sheets_client.get_values(&sheet_id, "Form Responses 1!A1:S1000".to_string()).await.unwrap();
        let values = sheet_values.values.unwrap();

        if values.is_empty() {
            panic!("unable to retrieve any data values from Google sheet {} {}", sheet_id, sheet_name);
        }

        // Parse the sheet columns.
        let columns = ApplicantSheetColumns::parse(&values);

        // Iterate over the rows.
        for (row_index, row) in values.iter().enumerate() {
            if row_index == 0 {
                // Continue the loop since we were on the header row.
                continue;
            } // End get header information.

            // Break the loop early if we reached an empty row.
            if row[columns.email].is_empty() {
                break;
            }

            // Parse the applicant out of the row information.
            let mut applicant = NewApplicant::parse_from_row_with_columns(sheet_name, sheet_id, &columns, &row).await;
            applicant
                .expand(&drive_client, &sheets_client, columns.sent_email_received, columns.sent_email_follow_up, row_index + 1)
                .await;

            if !applicant.sent_email_received {
                // Post to Slack.
                post_to_channel(get_hiring_channel_post_url(), applicant.as_slack_msg()).await;

                // Send a company-wide email.
                applicant.send_email_internally().await;
            }

            applicants.push(applicant);
        }
    }

    applicants
}

// Sync the applicants with our database.
#[instrument(skip(db))]
#[inline]
pub async fn refresh_db_applicants(db: &Database) {
    let applicants = get_raw_applicants().await;

    let github = authenticate_github_jwt();

    // Get all the hiring issues on the configs repository.
    let configs_issues = github
        .repo(github_org(), "configs")
        .issues()
        .list(&IssueListOptions::builder().per_page(100).state(State::All).labels(vec!["hiring"]).build())
        .await
        .unwrap();

    // Sync applicants.
    for applicant in applicants {
        let new_applicant = applicant.upsert(db).await;

        new_applicant.create_github_onboarding_issue(&github, &configs_issues).await;
    }
}

/// The data type for a Google Sheet applicant form columns, we use this when
/// parsing the Google Sheets for applicant forms where we leave our voting.
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct ApplicantFormSheetColumns {
    pub name: usize,
    pub email: usize,
    pub form_id: usize,
    pub form_url: usize,
    pub form_responses_url: usize,
}

impl ApplicantFormSheetColumns {
    fn new() -> Self {
        ApplicantFormSheetColumns {
            name: 0,
            email: 1,
            form_id: 2,
            form_url: 3,
            form_responses_url: 4,
        }
    }
}

async fn get_reviewer_pool() -> Vec<String> {
    // Get the GSuite token.
    let token = get_gsuite_token("").await;

    // Initialize the GSuite sheets client.
    let sheets_client = Sheets::new(token.clone());
    let sheet_id = "1BOeZTdSNixkJsVHwf3Z0LMVlaXsc_0J8Fsy9BkCa7XM";

    // Get the values in the sheet.
    let sheet_values = sheets_client.get_values(&sheet_id, "Reviewer pool!A1:G1000".to_string()).await.unwrap();
    let values = sheet_values.values.unwrap();

    if values.is_empty() {
        panic!("unable to retrieve any data values from Google sheet for applicant forms {}", sheet_id);
    }

    // Iterate over the rows.
    let mut reviewers: Vec<String> = vec![];
    for (_, row) in values.iter().enumerate() {
        if row[0].is_empty() {
            // Break our loop we are in an empty row.
            break;
        }
        // The email is the second column.
        reviewers.push(row[1].replace(DOMAIN, GSUITE_DOMAIN).to_string());
    }

    reviewers
}

#[instrument(skip(db))]
#[inline]
pub async fn update_applications_with_scoring_forms(db: &Database) {
    // Get the GSuite token.
    let token = get_gsuite_token("").await;

    // Initialize the GSuite sheets client.
    let sheets_client = Sheets::new(token.clone());
    let sheet_id = "1BOeZTdSNixkJsVHwf3Z0LMVlaXsc_0J8Fsy9BkCa7XM";

    // Get the values in the sheet.
    let sheet_values = sheets_client.get_values(&sheet_id, "Applicants to review!A1:G1000".to_string()).await.unwrap();
    let values = sheet_values.values.unwrap();

    if values.is_empty() {
        panic!("unable to retrieve any data values from Google sheet for applicant forms {}", sheet_id);
    }

    // Parse the sheet columns.
    let columns = ApplicantFormSheetColumns::new();

    let mut reviewer_pool = get_reviewer_pool().await;

    // We'll assign reviewers randomly but attempt to produce roughly even loads
    // across reviewers. To do this, we shuffle the list of reviewers, and then
    // create a cycling iterator that produces that shuffled sequence forever.
    // Whenever we need a group of 5 interviewers, we'll just take the next 5.
    let mut rng = rand::thread_rng();
    reviewer_pool.shuffle(&mut rng);
    let mut reviewer_pool = reviewer_pool.iter().cloned().cycle();

    // Iterate over the rows.
    for (_, row) in values.iter().enumerate() {
        if row[columns.email].is_empty() {
            // Break our loop we are in an empty row.
            break;
        }

        let email = row[columns.email].to_string();
        let form_id = row[columns.form_id].to_string();
        let form_url = row[columns.form_url].to_string();
        let form_responses_url = row[columns.form_responses_url].to_string();

        // Update each of the applicants.
        for (_, sheet_id) in get_sheets_map() {
            if let Ok(mut applicant) = applicants::dsl::applicants
                .filter(applicants::dsl::email.eq(email.to_string()))
                .filter(applicants::dsl::sheet_id.eq(sheet_id.to_string()))
                .first::<Applicant>(&db.conn())
            {
                // Make sure the status is "Needs to be triaged".
                let status = crate::applicant_status::Status::from_str(&applicant.status);
                if status != Ok(crate::applicant_status::Status::NeedsToBeTriaged) {
                    // Continue we don't care.
                    continue;
                }

                applicant.scoring_form_id = form_id.to_string();
                applicant.scoring_form_url = form_url.to_string();
                applicant.scoring_form_responses_url = form_responses_url.to_string();

                // See if we already have scorers assigned.
                if applicant.scorers.is_empty() || applicant.scorers.len() < 5 {
                    // Assign scorers and send email.
                    // Choose next five reviewers.
                    applicant.scorers = reviewer_pool.by_ref().take(5).collect();

                    // Send emails to the scorers.
                    // We don't need to do this since we will use airtable
                    // for the emails.
                    //for s in &applicant.scorers {
                    //applicant.send_email_to_scorer(&s).await;
                    //}
                }

                // Update the applicant in the database.
                applicant.update(db).await;
            }
        }
    }
}

#[instrument(skip(db))]
#[inline]
pub async fn update_applications_with_scoring_results(db: &Database) {
    // Get the GSuite token.
    let token = get_gsuite_token("").await;

    // Initialize the GSuite sheets client.
    let sheets_client = Sheets::new(token.clone());
    let sheet_id = "1BOeZTdSNixkJsVHwf3Z0LMVlaXsc_0J8Fsy9BkCa7XM";

    // Get the values in the sheet.
    let sheet_values = sheets_client.get_values(&sheet_id, "Responses!A1:R1000".to_string()).await.unwrap();
    let values = sheet_values.values.unwrap();

    if values.is_empty() {
        panic!("unable to retrieve any data values from Google sheet for applicant form responses {}", sheet_id);
    }

    // Iterate over the rows.
    for (row_index, row) in values.iter().enumerate() {
        if row_index == 0 {
            // We are on the header row.
            continue;
        }
        if row[0].is_empty() {
            // Break our loop we are in an empty row.
            break;
        }

        let email = row[1].to_string();

        // Parse the scoring results.
        let scoring_evaluations_count = row[2].parse::<i32>().unwrap_or(0);
        let scoring_enthusiastic_yes_count = row[3].parse::<i32>().unwrap_or(0);
        let scoring_yes_count = row[4].parse::<i32>().unwrap_or(0);
        let scoring_pass_count = row[5].parse::<i32>().unwrap_or(0);
        let scoring_no_count = row[6].parse::<i32>().unwrap_or(0);
        let scoring_not_applicable_count = row[7].parse::<i32>().unwrap_or(0);
        let mut scoring_insufficient_experience_count = 0;
        let mut scoring_inapplicable_experience_count = 0;
        let mut scoring_job_function_yet_needed_count = 0;
        let mut scoring_underwhelming_materials_count = 0;
        let mut value_reflected = "".to_string();
        let mut value_violated = "".to_string();
        let mut values_in_tension: Vec<String> = vec![];
        if row.len() >= 10 {
            scoring_insufficient_experience_count = row[9].parse::<i32>().unwrap_or(0);
            scoring_inapplicable_experience_count = row[10].parse::<i32>().unwrap_or(0);
            scoring_job_function_yet_needed_count = row[11].parse::<i32>().unwrap_or(0);
            scoring_underwhelming_materials_count = row[12].parse::<i32>().unwrap_or(0);

            // Parse the values.
            value_reflected = row[14].to_lowercase().to_string();
            if value_reflected == "n/a" {
                value_reflected = "".to_string();
            }
            value_violated = row[15].to_lowercase().to_string();
            if value_violated == "n/a" {
                value_violated = "".to_string();
            }
            let value_in_tension_1 = row[16].to_lowercase().to_string();
            if value_in_tension_1 != "n/a" && !value_in_tension_1.trim().is_empty() {
                values_in_tension.push(value_in_tension_1);
            }
            let value_in_tension_2 = row[17].to_lowercase().to_string();
            if value_in_tension_2 != "n/a" && !value_in_tension_2.trim().is_empty() {
                values_in_tension.push(value_in_tension_2);
            }
        }

        // Update each of the applicants.
        for (_, sheet_id) in get_sheets_map() {
            if let Ok(mut applicant) = applicants::dsl::applicants
                .filter(applicants::dsl::email.eq(email.to_string()))
                .filter(applicants::dsl::sheet_id.eq(sheet_id.to_string()))
                .first::<Applicant>(&db.conn())
            {
                applicant.scoring_evaluations_count = scoring_evaluations_count;
                applicant.scoring_enthusiastic_yes_count = scoring_enthusiastic_yes_count;
                applicant.scoring_yes_count = scoring_yes_count;
                applicant.scoring_pass_count = scoring_pass_count;
                applicant.scoring_no_count = scoring_no_count;
                applicant.scoring_not_applicable_count = scoring_not_applicable_count;
                applicant.scoring_insufficient_experience_count = scoring_insufficient_experience_count;
                applicant.scoring_inapplicable_experience_count = scoring_inapplicable_experience_count;
                applicant.scoring_job_function_yet_needed_count = scoring_job_function_yet_needed_count;
                applicant.scoring_underwhelming_materials_count = scoring_underwhelming_materials_count;

                applicant.value_reflected = value_reflected.to_string();
                applicant.value_violated = value_violated.to_string();
                applicant.values_in_tension = values_in_tension.clone();

                // Update the applicant in the database.
                applicant.update(db).await;
            }
        }
    }
}

#[instrument]
#[inline]
fn is_materials(file_name: &str) -> bool {
    file_name.ends_with("responses.pdf")
        || (file_name.starts_with("Oxide Candidate Materials") && file_name.ends_with(".pdf"))
        || file_name.ends_with("Oxide Candidate Materials.pdf")
        || file_name.ends_with("OxideQuestions.pdf")
        || file_name.ends_with("oxide-computer-candidate-materials.pdf")
        || file_name.ends_with("Questionnaire.pdf")
}

#[instrument(skip(db))]
#[inline]
pub async fn refresh_background_checks(db: &Database) {
    // Initialize the Checker client.
    let checkr = Checkr::new_from_env();

    // List the candidates.
    let candidates = checkr.list_candidates().await.unwrap();
    for candidate in candidates {
        // Try to match the candidate based on their email.
        // Try for all the sheet_ids.
        for (_, sheet_id) in get_sheets_map() {
            if let Ok(mut applicant) = applicants::dsl::applicants
                .filter(applicants::dsl::email.eq(candidate.email.to_string()))
                .filter(applicants::dsl::sheet_id.eq(sheet_id.to_string()))
                .first::<Applicant>(&db.conn())
            {
                for report_id in &candidate.report_ids {
                    // Get the report for the candidate.
                    let report = checkr.get_report(&report_id).await.unwrap();
                    println!("report: {:?}", report);

                    // Set the status for the report.
                    if report.package.contains("premium_criminal") {
                        applicant.criminal_background_check_status = report.status.to_string();
                    }
                    if report.package.contains("motor_vehicle") {
                        applicant.motor_vehicle_background_check_status = report.status.to_string();
                    }

                    // Update the applicant.
                    applicant.update(db).await;
                }
            } else {
                println!("[checkr] could not find applicant with email {} in sheet_id {}", candidate.email, sheet_id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::applicants::{refresh_background_checks, refresh_db_applicants, update_applications_with_scoring_forms, update_applications_with_scoring_results, Applicant, Applicants};
    use crate::db::Database;
    use crate::schema::applicants;

    use diesel::prelude::*;
    use serde_json::json;

    #[test]
    fn test_serialize_deserialize_applicants() {
        let db = Database::new();
        let applicant = applicants::dsl::applicants.filter(applicants::dsl::id.eq(318)).first::<Applicant>(&db.conn()).unwrap();

        // Let's test that serializing this is going to give us an array of Airtable users.
        let scorers = json!(applicant).to_string();
        // Let's assert in the string are the scorers formatted as Airtable users.
        assert_eq!(scorers.contains("\"scorers\":[{\"email\":\""), true);

        // Let's test that deserializing a string will give us the same applicant we had
        // originally.
        let a: Applicant = serde_json::from_str(&scorers).unwrap();
        assert_eq!(applicant, a);
    }

    #[ignore]
    #[tokio::test(threaded_scheduler)]
    async fn test_applicants_background_checks() {
        let db = Database::new();

        refresh_background_checks(&db).await;
    }

    #[ignore]
    #[tokio::test(threaded_scheduler)]
    async fn test_applicants() {
        let db = Database::new();
        refresh_db_applicants(&db).await;

        // Update Airtable.
        Applicants::get_from_db(&db).update_airtable().await;

        // These come from the sheet at:
        // https://docs.google.com/spreadsheets/d/1BOeZTdSNixkJsVHwf3Z0LMVlaXsc_0J8Fsy9BkCa7XM/edit#gid=2017435653
        update_applications_with_scoring_forms(&db).await;

        update_applications_with_scoring_results(&db).await;
    }
}
