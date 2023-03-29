#![allow(clippy::from_over_into)]
use std::{env, process::Command, str::FromStr};

use anyhow::{bail, Result};
use async_bb8_diesel::AsyncRunQueryDsl;
use async_trait::async_trait;
use chrono::{offset::Utc, DateTime, Duration, NaiveDate};
use chrono_humanize::HumanTime;
use docusign::DocuSign;
use google_drive::{
    traits::{DriveOps, FileOps},
    Client as GoogleDrive,
};
use google_geocode::Geocode;
use log::{info, warn};
use macros::db;
use regex::Regex;
use schemars::JsonSchema;
use sendgrid_api::{traits::MailOps, Client as SendGrid};
use serde::{Deserialize, Serialize};
use slack_chat_api::{
    FormattedMessage, MessageAttachment, MessageBlock, MessageBlockText, MessageBlockType, MessageType,
};
use tokio::fs;
use tokio::io::AsyncWriteExt;

use crate::{
    airtable::{AIRTABLE_APPLICATIONS_TABLE, AIRTABLE_REVIEWER_LEADERBOARD_TABLE},
    app_config::{AppConfig, ApplyConfig, Letter, NewHireIssue},
    applicant_reviews::ApplicantReview,
    companies::Company,
    configs::User,
    core::UpdateAirtableRecord,
    db::Database,
    enclose,
    interviews::ApplicantInterview,
    schema::{applicant_interviews, applicant_reviewers, applicants, users},
    utils::{check_if_github_issue_exists, truncate},
};

// The line breaks that get parsed are weird thats why we have the random asterisks here.
static QUESTION_TECHNICALLY_CHALLENGING: &str =
    r"W(?s:.*)at work(?s:.*)ave you found mos(?s:.*)challenging(?s:.*)caree(?s:.*)wh(?s:.*)\?";
static QUESTION_WORK_PROUD_OF: &str =
    r"W(?s:.*)at work(?s:.*)ave you done that you(?s:.*)particularl(?s:.*)proud o(?s:.*)and why\?";
static QUESTION_HAPPIEST_CAREER: &str =
    r"W(?s:.*)en have you been happiest in your professiona(?s:.*)caree(?s:.*)and why\?";
static QUESTION_UNHAPPIEST_CAREER: &str =
    r"W(?s:.*)en have you been unhappiest in your professiona(?s:.*)caree(?s:.*)and why\?";
static QUESTION_VALUE_REFLECTED: &str = r"F(?s:.*)r one of Oxide(?s:.*)s values(?s:.*)describe an example of ho(?s:.*)it wa(?s:.*)reflected(?s:.*)particula(?s:.*)body(?s:.*)you(?s:.*)work\.";
static QUESTION_VALUE_VIOLATED: &str = r"F(?s:.*)r one of Oxide(?s:.*)s values(?s:.*)describe an example of ho(?s:.*)it wa(?s:.*)violated(?s:.*)you(?s:.*)organization o(?s:.*)work\.";
static QUESTION_VALUES_IN_TENSION: &str = r"F(?s:.*)r a pair of Oxide(?s:.*)s values(?s:.*)describe a time in whic(?s:.*)the tw(?s:.*)values(?s:.*)tensio(?s:.*)for(?s:.*)your(?s:.*)and how yo(?s:.*)resolved it\.";
static QUESTION_WHY_OXIDE: &str =
    r"W(?s:.*)y(?s:.*)do(?s:.*)you(?s:.*)want(?s:.*)to(?s:.*)work(?s:.*)for(?s:.*)Oxide\?";

struct OnboardingAssignees([&'static str; 2]);

impl From<&OnboardingAssignees> for Vec<String> {
    fn from(assignees: &OnboardingAssignees) -> Self {
        assignees.0.iter().map(|assignee| assignee.to_string()).collect()
    }
}

/// The data type for a NewApplicant.
#[db {
    new_struct_name = "Applicant",
    airtable_base = "hiring",
    airtable_table = "AIRTABLE_APPLICATIONS_TABLE",
    match_on = {
        "email" = "String",
        "sheet_id" = "String",
    },
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[diesel(table_name = applicants)]
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
    #[serde(default)]
    pub latitude: f32,
    #[serde(default)]
    pub longitude: f32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub github: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub gitlab: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub linkedin: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub portfolio: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub portfolio_pdf: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub website: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub resume: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub materials: String,
    #[serde(default)]
    pub sent_email_received: bool,
    #[serde(default)]
    pub sent_email_follow_up: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rejection_sent_date_time: Option<DateTime<Utc>>,

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interviews_started: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interviews_completed: Option<DateTime<Utc>>,

    /// The scorers/reviewers assigned to the applicant.
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        serialize_with = "airtable_api::user_format_as_array_of_strings::serialize",
        deserialize_with = "airtable_api::user_format_as_array_of_strings::deserialize"
    )]
    pub scorers: Vec<String>,
    /// The scorers_completed field means the person has already reviewed the applicant.
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        serialize_with = "airtable_api::user_format_as_array_of_strings::serialize",
        deserialize_with = "airtable_api::user_format_as_array_of_strings::deserialize"
    )]
    pub scorers_completed: Vec<String>,
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

    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub criminal_background_check_status: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub motor_vehicle_background_check_status: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_date: Option<NaiveDate>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub interested_in: Vec<String>,

    /// This field is used by Airtable for mapping the location data.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub geocode_cache: String,

    /// These fields are used by the DocuSign integration.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub docusign_envelope_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub docusign_envelope_status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offer_created: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offer_completed: Option<DateTime<Utc>>,

    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub docusign_piia_envelope_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub docusign_piia_envelope_status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub piia_envelope_created: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub piia_envelope_completed: Option<DateTime<Utc>>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub link_to_reviews: Vec<String>,

    /// The CIO company ID.
    #[serde(default)]
    pub cio_company_id: i32,
}

pub fn clean_interested_in(st: &str) -> String {
    let s = st.trim().to_lowercase();

    if s == "product security engineer" || s == "security engineer" || s == "software engineer - security" {
        return "Product Security Engineer".to_string();
    } else if s == "software engineer: web" {
        return "Software Engineer: Web".to_string();
    } else if s == "software engineer: embedded systems" {
        return "Software Engineer: Embedded Systems".to_string();
    } else if s == "software engineer: control plane" {
        return "Software Engineer: Control Plane".to_string();
    } else if s == "hardware engineer" {
        return "Hardware Engineer".to_string();
    }

    st.to_string()
}

impl NewApplicant {
    /// Get the human duration of time since the application was submitted.
    fn human_duration(&self) -> HumanTime {
        let mut dur = self.submitted_time - Utc::now();
        if dur.num_seconds() > 0 {
            dur = -dur;
        }

        HumanTime::from(dur)
    }
}

fn get_color_based_on_status(s: &str) -> String {
    let status = crate::applicant_status::Status::from_str(s).unwrap();

    let color = match status {
        crate::applicant_status::Status::NextSteps => crate::colors::Colors::Blue,
        crate::applicant_status::Status::Deferred => crate::colors::Colors::Red,
        crate::applicant_status::Status::Declined => crate::colors::Colors::Red,
        crate::applicant_status::Status::Hired => crate::colors::Colors::Green,
        crate::applicant_status::Status::GivingOffer => crate::colors::Colors::Green,
        crate::applicant_status::Status::Contractor => crate::colors::Colors::Green,
        crate::applicant_status::Status::NeedsToBeTriaged => crate::colors::Colors::Yellow,
        crate::applicant_status::Status::Interviewing => crate::colors::Colors::Blue,
        crate::applicant_status::Status::Onboarding => crate::colors::Colors::Green,
        crate::applicant_status::Status::Withdrawn => crate::colors::Colors::Red,
    };

    color.to_string()
}

/// Convert the applicant into a Slack message.
impl From<NewApplicant> for FormattedMessage {
    fn from(item: NewApplicant) -> Self {
        let time = item.human_duration();

        let mut status_msg = item.role.to_string();

        if !item.interested_in.is_empty() {
            // Make sure we don't repeat the same string as the role for no reason.
            let joined = item.interested_in.join(",");
            if joined != item.role {
                status_msg += &format!(" | {}", joined);
            }
        }

        if !item.status.is_empty() {
            status_msg += &format!(" | *{}*", item.status);
        }

        status_msg += &format!(" | applied {}", time);

        let mut values_msg = "".to_string();
        if !item.value_reflected.is_empty() {
            values_msg += &format!("values reflected: *{}*", item.value_reflected);
        }
        if !item.value_violated.is_empty() {
            values_msg += &format!(" | violated: *{}*", item.value_violated);
        }
        for (k, tension) in item.values_in_tension.iter().enumerate() {
            if k == 0 {
                values_msg += &format!(" | in tension: *{}*", tension);
            } else {
                values_msg += &format!(" *& {}*", tension);
            }
        }
        if values_msg.is_empty() {
            values_msg = "values not yet populated".to_string();
        }

        let mut intro_msg = format!("*{}*  <mailto:{}|{}>", item.name, item.email, item.email,);
        if !item.location.is_empty() {
            intro_msg += &format!("  {}", item.location);
        }

        let mut info_msg = format!("<{}|resume> | <{}|materials>", item.resume, item.materials,);
        if !item.phone.is_empty() {
            info_msg += &format!(" | <tel:{}|{}>", item.phone, item.phone);
        }
        if !item.github.is_empty() {
            info_msg += &format!(
                " | <https://github.com/{}|github:{}>",
                item.github.trim_start_matches('@'),
                item.github,
            );
        }
        if !item.gitlab.is_empty() {
            info_msg += &format!(
                " | <https://gitlab.com/{}|gitlab:{}>",
                item.gitlab.trim_start_matches('@'),
                item.gitlab,
            );
        }
        if !item.linkedin.is_empty() {
            info_msg += &format!(" | <{}|linkedin>", item.linkedin,);
        }
        if !item.portfolio.is_empty() {
            info_msg += &format!(" | <{}|portfolio>", item.portfolio,);
        }
        if !item.portfolio_pdf.is_empty() {
            info_msg += &format!(" | <{}|portfolio pdf>", item.portfolio_pdf,);
        }
        if !item.website.is_empty() {
            info_msg += &format!(" | <{}|website>", item.website,);
        }

        FormattedMessage {
            channel: Default::default(),
            blocks: Default::default(),
            attachments: vec![MessageAttachment {
                color: get_color_based_on_status(&item.status),
                author_icon: Default::default(),
                author_link: Default::default(),
                author_name: Default::default(),
                fallback: Default::default(),
                fields: Default::default(),
                footer: Default::default(),
                footer_icon: Default::default(),
                image_url: Default::default(),
                pretext: Default::default(),
                text: Default::default(),
                thumb_url: Default::default(),
                title: Default::default(),
                title_link: Default::default(),
                ts: Default::default(),
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
                        elements: vec![slack_chat_api::BlockOption::MessageBlockText(MessageBlockText {
                            text_type: MessageType::Markdown,
                            text: info_msg,
                        })],
                        text: Default::default(),
                        accessory: Default::default(),
                        block_id: Default::default(),
                        fields: Default::default(),
                    },
                    MessageBlock {
                        block_type: MessageBlockType::Context,
                        elements: vec![slack_chat_api::BlockOption::MessageBlockText(MessageBlockText {
                            text_type: MessageType::Markdown,
                            text: values_msg,
                        })],
                        text: Default::default(),
                        accessory: Default::default(),
                        block_id: Default::default(),
                        fields: Default::default(),
                    },
                    MessageBlock {
                        block_type: MessageBlockType::Context,
                        elements: vec![slack_chat_api::BlockOption::MessageBlockText(MessageBlockText {
                            text_type: MessageType::Markdown,
                            text: status_msg,
                        })],
                        text: Default::default(),
                        accessory: Default::default(),
                        block_id: Default::default(),
                        fields: Default::default(),
                    },
                ],
            }],
        }
    }
}

impl From<Applicant> for FormattedMessage {
    fn from(item: Applicant) -> Self {
        let new: NewApplicant = item.into();
        new.into()
    }
}

impl Applicant {
    pub async fn refresh(
        &mut self,
        db: &Database,
        company: &Company,
        github: &octorust::Client,
        configs_issues: &[octorust::types::IssueSimple],
        app_config: AppConfig,
    ) -> Result<()> {
        // Initialize the GSuite sheets client.
        let drive_client = company.authenticate_google_drive(db).await?;

        self.keep_fields_from_airtable(db).await;

        // Expand the application.
        if let Err(e) = self.expand(db, &drive_client, &app_config.apply).await {
            warn!("expanding applicant `{}` failed: {}", self.email, e);

            // Return early.
            return Ok(());
        }

        // Update the applicant's status based on other criteria.
        self.update_status().await?;

        // Update airtable and the database again, we want to save our status just in
        // case there is an error.
        self.update(db).await?;

        // Send the follow up email if we need to, this will also update the database.
        self.send_email_follow_up_if_necessary(db, app_config.apply).await?;

        // Create the GitHub onboarding issue if we need to.
        self.create_github_onboarding_issue(db, github, configs_issues, &app_config.onboarding.new_hire_issue)
            .await?;

        // Update the interviews start and end time if we have interviews.
        self.update_interviews_start_end_time(db).await;

        // Update airtable and the database again, we want to save our status just in
        // case there is an error.
        self.update(db).await?;

        // Update the reviews for the self.
        // This function will update the database so we don't have to.
        self.update_reviews_scoring(db).await?;

        // TODO: we could move docusign stuff here as well, and out of its own function.
        Ok(())
    }

    /// Update an applicant's status based on dates, interviews, etc.
    pub async fn update_status(&mut self) -> Result<()> {
        // If we know they have more than 1 interview AND their current status is "next steps",
        // THEN we can mark the applicant as in the "interviewing" state.
        if self.interviews.len() > 1
            && (self.status == crate::applicant_status::Status::NextSteps.to_string()
                || self.status == crate::applicant_status::Status::NeedsToBeTriaged.to_string())
        {
            self.status = crate::applicant_status::Status::Interviewing.to_string();
        }

        // If their status is "Onboarding" and it is after their start date.
        // Set their status to "Hired".
        if (self.status == crate::applicant_status::Status::Onboarding.to_string()
            || self.status == crate::applicant_status::Status::GivingOffer.to_string())
            && self.start_date.is_some()
            && self.start_date.unwrap() <= Utc::now().date().naive_utc()
        {
            // We shouldn't also check if we have an employee for the user, only if the employee had
            // been hired and left.
            // TODO: Have a status for if the employee was hired but then left the company.
            self.status = crate::applicant_status::Status::Hired.to_string();
        }

        Ok(())
    }

    /// Update the interviews start and end time, if we have it.
    pub async fn update_interviews_start_end_time(&mut self, db: &Database) {
        // If we have interviews for them, let's update the interviews_started and
        // interviews_completed times.
        if self.interviews.is_empty() || self.airtable_record_id.is_empty() {
            // Return early we don't care.
            return;
        }

        // Since our interviews length is at least one, we must have at least one interview.
        // Let's query the interviews for this candidate.
        let data = applicant_interviews::dsl::applicant_interviews
            .filter(applicant_interviews::dsl::applicant.contains(vec![self.airtable_record_id.to_string()]))
            .order_by(applicant_interviews::dsl::start_time.asc())
            .load_async::<ApplicantInterview>(db.pool())
            .await
            .unwrap();
        // Probably a better way to do this using first and last, but whatever.
        for (index, r) in data.iter().enumerate() {
            if index == 0 {
                // We have the first record.
                // Let's update the started time.
                self.interviews_started = Some(r.start_time);
                // We continue here so we don't accidentally set the
                // completed_time if we only have one record.
                continue;
            }
            if index == data.len() - 1 {
                // We are on the last record.
                // Let's update the completed time.
                self.interviews_completed = Some(r.end_time);
                break;
            }
        }
    }

    /// Update applicant reviews counts.
    pub async fn update_reviews_scoring(&mut self, db: &Database) -> Result<()> {
        self.keep_fields_from_airtable(db).await;

        // Create the Airtable client.
        let company = Company::get_by_id(db, self.cio_company_id).await?;
        let airtable = company.authenticate_airtable(&company.airtable_base_id_hiring);

        // We need to capture the existing score count prior to mutations to ensure that
        // we can properly detect when we need to zero out onboarding and hired employees
        let previous_scoring_evaluations_count = self.scoring_evaluations_count;

        // Zero out the values for the scores.
        self.scoring_evaluations_count = 0;
        self.scoring_enthusiastic_yes_count = 0;
        self.scoring_yes_count = 0;
        self.scoring_pass_count = 0;
        self.scoring_no_count = 0;
        self.scoring_not_applicable_count = 0;
        self.scoring_insufficient_experience_count = 0;
        self.scoring_inapplicable_experience_count = 0;
        self.scoring_job_function_yet_needed_count = 0;
        self.scoring_underwhelming_materials_count = 0;

        if self.status == crate::applicant_status::Status::Onboarding.to_string()
            || self.status == crate::applicant_status::Status::Hired.to_string()
        {
            // There are two cases in which we need to perform an update on a hired employee:
            //   1. There are links to reviews that need to be deleted
            //   2. There exist lingering scores that have not been zeroed
            if !self.link_to_reviews.is_empty() || previous_scoring_evaluations_count > 0 {
                // Let's iterate over the reviews.
                for record_id in &self.link_to_reviews {
                    // Get the record.
                    // TODO: get these from the database.
                    let record: airtable_api::Record<crate::applicant_reviews::ApplicantReview> = airtable
                        .get_record(crate::airtable::AIRTABLE_REVIEWS_TABLE, record_id)
                        .await?;

                    // Set the values if they are not empty.
                    // TODO: actually do the majority if they differ in value but for now YOLO.
                    if !record.fields.value_reflected.is_empty() {
                        self.value_reflected = record.fields.value_reflected.to_string();
                    }
                    if !record.fields.value_violated.is_empty() {
                        self.value_violated = record.fields.value_violated.to_string();
                    }
                    if !record.fields.values_in_tension.is_empty() {
                        self.values_in_tension = record.fields.values_in_tension.clone();
                    }

                    // Delete the record from the reviews Airtable.
                    airtable
                        .delete_record(crate::airtable::AIRTABLE_REVIEWS_TABLE, record_id)
                        .await?;

                    // Delete the record if it exists in the Database.
                    let r = ApplicantReview::get_by_id(db, record.fields.id).await?;

                    // Delete it.
                    r.delete(db).await?;

                    log::info!("Deleted review record {} {}", self.id, record_id);
                }

                log::info!("Resetting score values to 0 {}", self.id);

                // We already zero-ed out the values for the scores, now we return early.
                // We don't want people who join to know their scores.
                self.update(db).await?;
            }

            return Ok(());
        }

        // We have now handled people that are either in the process of onboarding or hired.
        // For everyone else we only have work to do if they actually have reviews
        if !self.link_to_reviews.is_empty() {
            for record_id in &self.link_to_reviews {
                // Get the record.
                // TODO: get these from the database.
                let record: airtable_api::Record<crate::applicant_reviews::ApplicantReview> = airtable
                    .get_record(crate::airtable::AIRTABLE_REVIEWS_TABLE, record_id)
                    .await
                    .unwrap();

                // Set the values if they are not empty.
                // TODO: actually do the majority if they differ in value but for now YOLO.
                if !record.fields.value_reflected.is_empty() {
                    self.value_reflected = record.fields.value_reflected.to_string();
                }
                if !record.fields.value_violated.is_empty() {
                    self.value_violated = record.fields.value_violated.to_string();
                }
                if !record.fields.values_in_tension.is_empty() {
                    self.values_in_tension = record.fields.values_in_tension.clone();
                }

                // Add the scoring count.
                self.scoring_evaluations_count += 1;

                // Up the scores for the relevant evaluations.
                if record.fields.evaluation.to_lowercase().starts_with("emphatic yes:") {
                    self.scoring_enthusiastic_yes_count += 1;
                }
                if record.fields.evaluation.to_lowercase().starts_with("yes:") {
                    self.scoring_yes_count += 1;
                }
                if record.fields.evaluation.to_lowercase().starts_with("pass:") {
                    self.scoring_pass_count += 1;
                }
                if record.fields.evaluation.to_lowercase().starts_with("no:") {
                    self.scoring_no_count += 1;
                }
                if record.fields.evaluation.to_lowercase().starts_with("n/a:") {
                    self.scoring_not_applicable_count += 1;
                }

                // Add in the rationale.
                if record
                    .fields
                    .evaluation
                    .to_lowercase()
                    .starts_with("insufficient experience")
                {
                    self.scoring_insufficient_experience_count += 1;
                }
                if record
                    .fields
                    .evaluation
                    .to_lowercase()
                    .starts_with("inapplicable experience")
                {
                    self.scoring_inapplicable_experience_count += 1;
                }
                if record
                    .fields
                    .evaluation
                    .to_lowercase()
                    .starts_with("job function not yet needed")
                {
                    self.scoring_job_function_yet_needed_count += 1;
                }
                if record
                    .fields
                    .evaluation
                    .to_lowercase()
                    .starts_with("underwhelming materials")
                {
                    self.scoring_underwhelming_materials_count += 1;
                }

                // If we don't already have the review in reviewers completed,
                // add them.
                if !self.scorers_completed.contains(&record.fields.reviewer) {
                    self.scorers_completed.push(record.fields.reviewer.to_string());
                }

                // If this reviewer was assigned, remove them since they completed scoring.
                if self.scorers.contains(&record.fields.reviewer) {
                    let index = self.scorers.iter().position(|r| *r == record.fields.reviewer).unwrap();
                    self.scorers.remove(index);
                }
            }

            log::info!("Updating scores for applicant {}", self.id);

            // Update the record.
            self.update(db).await?;
        }

        Ok(())
    }

    /// Get the human duration of time since the application was submitted.
    pub fn human_duration(&self) -> HumanTime {
        let mut dur = self.submitted_time - Utc::now();
        if dur.num_seconds() > 0 {
            dur = -dur;
        }

        HumanTime::from(dur)
    }

    /// Send an invite to the applicant to do a background check.
    pub async fn send_background_check_invitation(&mut self, db: &Database) -> Result<()> {
        // Keep the fields from Airtable we need just in case they changed.
        self.keep_fields_from_airtable(db).await;

        let company = self.company(db).await?;
        let checkr_auth = company.authenticate_checkr();
        if checkr_auth.is_none() {
            // Return early.
            return Ok(());
        }

        let checkr = checkr_auth.unwrap();

        // Check if we already sent them an invitation.
        let candidates = checkr.list_candidates().await?;
        for candidate in candidates {
            if candidate.email == self.email {
                // Check if we already have sent their invitation.
                if self.criminal_background_check_status.is_empty() {
                    // Create an invitation for the candidate.
                    checkr.create_invitation(&candidate.id, "premium_criminal").await?;

                    // Update the database.
                    self.criminal_background_check_status = "requested".to_string();

                    self.update(db).await?;

                    info!("sent background check invitation to: {}", self.email);
                }
                // We can return early they already exist as a candidate and we have sent them an
                // invite.
                return Ok(());
            }
        }

        // Create a new candidate for the applicant in checkr.
        let candidate = checkr.create_candidate(&self.email).await?;

        // Create an invitation for the candidate.
        checkr.create_invitation(&candidate.id, "premium_criminal").await?;

        // Update the database.
        self.criminal_background_check_status = "requested".to_string();

        self.update(db).await?;

        info!("sent background check invitation to: {}", self.email);

        Ok(())
    }

    /// Send an email to a scorer that they are assigned to an applicant.
    pub async fn send_email_to_scorer(&self, scorer: &str, company: &Company) {
        // Initialize the SendGrid client.
        let sendgrid_client = SendGrid::new_from_env();

        // Send the message.
        sendgrid_client
            .mail_send()
            .send_plain_text(
                &format!("[applicants] Reviewing applicant {}", self.name),
                &self.as_scorer_email(),
                &[scorer.to_string()],
                &[],
                &[],
                &format!("careers@{}", company.gsuite_domain),
            )
            .await
            .unwrap();
    }

    /// Get the applicant's information in the form of the body of an email for a
    /// scorer email that they have been assigned to score the applicant.
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
            msg += &format!(
                "\nGitHub: {} (https://github.com/{})",
                self.github,
                self.github.trim_start_matches('@')
            );
        }
        if !self.gitlab.is_empty() {
            msg += &format!(
                "\nGitLab: {} (https://gitlab.com/{})",
                self.gitlab,
                self.gitlab.trim_start_matches('@')
            );
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

## \
             Reminder

The applicants Airtable is at: https://airtable-applicants.corp.oxide.computer\
             ",
            self.resume, self.materials, self.scoring_form_url, self.scoring_form_responses_url,
        );

        msg
    }

    fn first_name(&self) -> String {
        let split = self.name.splitn(2, ' ');
        let parts: Vec<&str> = split.collect();
        parts[0].to_string()
    }

    fn last_name(&self) -> String {
        let split = self.name.splitn(2, ' ');
        let parts: Vec<&str> = split.collect();
        parts[1].to_string()
    }

    async fn compute_username(&self, db: &Database, company: &Company) -> String {
        let first_name = self.first_name();
        let last_name = self.last_name();

        // Let's check the user's database to see if we can give this person the
        // {first_name}@ email.
        let mut username = first_name.to_lowercase().to_string();
        let existing_user = User::get_from_db(db, company.id, username.to_string()).await;
        if existing_user.is_some() {
            username = format!("{}.{}", first_name.replace(' ', "-"), last_name.replace(' ', "-"));
        }
        // Make sure it's lowercase.
        username = username.to_lowercase();

        username
    }

    pub async fn create_github_onboarding_issue(
        &self,
        db: &Database,
        github: &octorust::Client,
        configs_issues: &[octorust::types::IssueSimple],
        new_hire_issue: &NewHireIssue,
    ) -> Result<()> {
        let company = self.company(db).await?;

        // Make sure they have a start date.
        if self.start_date.is_none() {
            // Return early.
            return Ok(());
        }

        let owner = &company.github_org;
        let repo = "configs";

        let label = "hiring".to_string();
        let title = format!("Onboarding: {}", self.name);
        let username = self.compute_username(db, &company).await;

        let body = self.create_new_hire_issue_body(username.as_str(), new_hire_issue);

        // Check if we already have an issue for this user.
        let issue = check_if_github_issue_exists(configs_issues, &self.name);

        // Check if their status is not onboarding, we only care about onboarding applicants.
        if self.status != crate::applicant_status::Status::Onboarding.to_string() {
            // If the issue exists and is opened, we need to close it.
            if let Some(i) = issue {
                if i.state != "open" {
                    // We only care if the issue is still opened.
                    return Ok(());
                }

                // Comment on the issue that this person is now set to a different status and we no
                // longer need the issue.
                github
                    .issues()
                    .create_comment(
                        owner,
                        repo,
                        i.number,
                        &octorust::types::PullsUpdateReviewRequest {
                            body: format!(
                                "Closing issue automatically since the applicant is now status: \
                                 `{}`
Notes:
> {}",
                                self.status, self.raw_status
                            ),
                        },
                    )
                    .await?;

                // Close the issue.
                github
                    .issues()
                    .update(
                        owner,
                        repo,
                        i.number,
                        &octorust::types::IssuesUpdateRequest {
                            title: Some(title.into()),
                            body: Default::default(),
                            assignee: "".to_string(),
                            assignees: new_hire_issue.assignees.clone(),
                            labels: vec![label.into()],
                            milestone: Default::default(),
                            state: Some(octorust::types::State::Closed),
                        },
                    )
                    .await?;
            }

            // Return early.
            return Ok(());
        }

        // If we don't have a start date, return early.
        if self.start_date.is_none() {
            return Ok(());
        }

        // Create an issue for the applicant.
        if let Some(i) = issue {
            if i.state != "open" {
                // Make sure the issue is in the state of "open".
                github
                    .issues()
                    .update(
                        owner,
                        repo,
                        i.number,
                        &octorust::types::IssuesUpdateRequest {
                            title: Some(title.into()),
                            body: body.to_string(),
                            assignee: "".to_string(),
                            assignees: new_hire_issue.assignees.clone(),
                            labels: vec![label.into()],
                            milestone: Default::default(),
                            state: Some(octorust::types::State::Open),
                        },
                    )
                    .await?;
            } else {
                // If the issue does not have any check marks.
                // Update it.
                let checkmark = "[x]".to_string();
                if !i.body.contains(&checkmark) {
                    github
                        .issues()
                        .update(
                            owner,
                            repo,
                            i.number,
                            &octorust::types::IssuesUpdateRequest {
                                title: Some(title.into()),
                                body: body.to_string(),
                                assignee: "".to_string(),
                                assignees: new_hire_issue.assignees.clone(),
                                labels: vec![label.into()],
                                milestone: Default::default(),
                                state: Some(octorust::types::State::Open),
                            },
                        )
                        .await?;
                }
            }

            // Return early we don't want to update the issue because it will overwrite
            // any changes we made.
            return Ok(());
        }

        // Create the issue.
        github
            .issues()
            .create(
                owner,
                repo,
                &octorust::types::IssuesCreateRequest {
                    title: title.into(),
                    body,
                    assignee: "".to_string(),
                    assignees: new_hire_issue.assignees.clone(),
                    labels: vec![label.into()],
                    milestone: Default::default(),
                },
            )
            .await?;

        info!("created onboarding issue for {}", self.email);

        Ok(())
    }

    fn create_new_hire_issue_body(&self, username: &str, config: &NewHireIssue) -> String {
        let first_name = self.first_name();
        let last_name = self.last_name();

        let alerts = config
            .alerts
            .iter()
            .map(|a| format!("cc @{}", a))
            .collect::<Vec<String>>()
            .join("\n");
        let default_groups = config
            .default_groups
            .iter()
            .map(|g| format!("    '{}'", g))
            .collect::<Vec<String>>()
            .join(",\n");
        let aws_role = config.aws_roles.join(",");

        format!(
            r#"- [ ] Add to users.toml
- [ ] Provision user in Airtable
- [ ] Add to matrix chat

Start Date: {}
Personal Email: {}
Twitter: [TWITTER HANDLE]
GitHub: {}
Phone: {}
Location: {}
{}

```
[users.{}]
first_name = '{}'
last_name = '{}'
username = '{}'
aliases = []
groups = [
{}
]
recovery_email = '{}'
recovery_phone = '{}'
gender = ''
github = '{}'
chat = ''
aws_role = '{}'
department = ''
manager = ''
```"#,
            self.start_date.unwrap().format("%A, %B %-d, %C%y"),
            self.email,
            self.github,
            self.phone,
            self.location,
            alerts,
            username.replace('.', "-"),
            first_name,
            last_name,
            username,
            default_groups,
            self.email,
            self.phone.replace(['-', ' '], ""),
            self.github.replace('@', ""),
            aws_role,
        )
    }
}

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
    async fn update_airtable_record(&mut self, record: Applicant) -> Result<()> {
        self.interviews = record.interviews;
        self.geocode_cache = record.geocode_cache;
        self.link_to_reviews = record.link_to_reviews;
        self.resume_contents = truncate(&self.resume_contents, 100000);
        self.materials_contents = truncate(&self.materials_contents, 100000);
        self.question_why_oxide = truncate(&self.question_why_oxide, 100000);

        Ok(())
    }
}

/// Get the contexts of a file in Google Drive by it's URL as a text string.
pub async fn get_file_contents(drive_client: &GoogleDrive, url: &str) -> Result<String> {
    let id = url
        .replace("https://drive.google.com/open?id=", "")
        .replace("https://drive.google.com/file/d/", "")
        .replace("/view", "");

    // Get information about the file.
    let drive_file = drive_client
        .files()
        .get(
            &id, false, // acknowledge_abuse
            "",    // include_permissions_for_view
            true,  // supports_all_drives
            true,  // supports_team_drives
        )
        .await?;
    let mime_type = drive_file.mime_type;
    let name = drive_file.name;

    let mut path = env::temp_dir();
    let mut output = env::temp_dir();

    let result: String = if mime_type == "application/pdf" {
        // Get the PDF contents from Drive.
        let contents = drive_client.files().download_by_id(&id).await?;

        path.push(format!("{}.pdf", id));

        let mut file = fs::File::create(&path).await?;
        file.write_all(&contents).await?;

        read_pdf(&name, path.clone()).await?
    } else {
        let contents = drive_client.files().download_by_id(&id).await?;
        path.push(&name);

        let mut file = fs::File::create(&path).await?;
        file.write_all(&contents).await?;

        output.push(format!("{}.txt", id));

        match tokio::task::spawn_blocking(enclose! { (output, path) move || {Command::new("pandoc")
        .args(["-o", output.clone().to_str().unwrap(), path.to_str().unwrap()])
        .output()}})
        .await?
        {
            Ok(_) => (),
            Err(e) => {
                warn!("pandoc failed: {}", e);
                return Ok("".to_string());
            }
        }
        fs::read_to_string(output.clone()).await?
    };

    // Delete the temporary file, if it exists.
    for p in vec![path, output] {
        if p.exists() && !p.is_dir() {
            fs::remove_file(p).await?;
        }
    }

    Ok(result.trim().to_string())
}

async fn read_pdf(name: &str, path: std::path::PathBuf) -> Result<String> {
    let mut output = env::temp_dir();
    output.push(&format!("tempfile-{}.txt", name));

    // Extract the text from the PDF
    let cmd_output = tokio::task::spawn_blocking(enclose! { (output, path) move || {Command::new("pdftotext")
    .args(["-enc", "UTF-8", path.to_str().unwrap(), output.to_str().unwrap()])
    .output()}})
    .await??;

    let result = match fs::read_to_string(output.clone()).await {
        Ok(r) => r,
        Err(e) => {
            warn!(
                "running pdf2text failed: {} | name: {}, path: {}\nstdout: {}\nstderr: {}",
                e,
                name,
                path.as_path().display(),
                String::from_utf8(cmd_output.stdout)?,
                String::from_utf8(cmd_output.stderr)?,
            );

            "".to_string()
        }
    };

    // Delete the temporary file, if it exists.
    for p in vec![path, output] {
        if p.exists() && !p.is_dir() {
            fs::remove_file(p).await?;
        }
    }

    Ok(result)
}

/// The data type for a ApplicantReviewer.
#[db {
    new_struct_name = "ApplicantReviewer",
    airtable_base = "hiring",
    airtable_table = "AIRTABLE_REVIEWER_LEADERBOARD_TABLE",
    match_on = {
        "email" = "String",
    },
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[diesel(table_name = applicant_reviewers)]
pub struct NewApplicantReviewer {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        serialize_with = "airtable_api::user_format_as_string::serialize",
        deserialize_with = "airtable_api::user_format_as_string::deserialize"
    )]
    pub email: String,
    #[serde(default)]
    pub evaluations: i32,
    #[serde(default)]
    pub emphatic_yes: i32,
    #[serde(default)]
    pub yes: i32,
    #[serde(default)]
    pub pass: i32,
    #[serde(default)]
    pub no: i32,
    #[serde(default)]
    pub not_applicable: i32,
    /// The CIO company ID.
    #[serde(default)]
    pub cio_company_id: i32,
}

/// Implement updating the Airtable record for an ApplicantReviewer.
#[async_trait]
impl UpdateAirtableRecord<ApplicantReviewer> for ApplicantReviewer {
    async fn update_airtable_record(&mut self, _record: ApplicantReviewer) -> Result<()> {
        Ok(())
    }
}

pub async fn refresh_docusign_for_applicants(db: &Database, company: &Company, config: &AppConfig) -> Result<()> {
    if company.airtable_base_id_hiring.is_empty() {
        // Return early.
        return Ok(());
    }

    // Authenticate DocuSign.
    let dsa = company.authenticate_docusign(db).await;
    if let Err(e) = dsa {
        if e.to_string().contains("no token") {
            // Return early, this company does not use Zoom.
            return Ok(());
        }

        bail!("authenticating docusign failed: {}", e);
    }
    let ds = dsa.unwrap();

    // TODO: we could actually query the DB by status, but whatever.
    let applicants = Applicants::get_from_db(db, company.id).await?;

    // Iterate over the applicants and find any that have the status: giving offer.
    for mut applicant in applicants {
        applicant
            .do_docusign_offer(db, &ds, config.envelopes.create_offer_letter(&applicant))
            .await?;

        applicant
            .do_docusign_piia(db, &ds, config.envelopes.create_piia_letter(&applicant))
            .await?;
    }

    Ok(())
}

pub async fn get_docusign_template_id(ds: &DocuSign, name: &str) -> String {
    let templates = ds.list_templates().await.unwrap();
    for template in templates {
        if template.name == name {
            return template.template_id;
        }
    }

    "".to_string()
}

impl Applicant {
    pub fn cleanup_linkedin(&mut self) {
        if self.linkedin.trim().is_empty() {
            self.linkedin = "".to_string();
            return;
        }

        // Cleanup linkedin link.
        self.linkedin = format!(
            "https://linkedin.com/{}",
            self.linkedin
                .trim_start_matches("https://linkedin.com/")
                .trim_start_matches("https://uk.linkedin.com/")
                .trim_start_matches("https://www.linkedin.com/")
                .trim_start_matches("http://linkedin.com/")
                .trim_start_matches("http://www.linkedin.com/")
                .trim_start_matches("www.linkedin.com/")
                .trim_start_matches("linkedin.com/")
                .trim()
        );
    }

    pub async fn set_lat_long(&mut self) {
        // Get the latitude and longitude if we don't already have it.
        if self.latitude != 0.0 && self.longitude != 0.0 {
            // Return early we alreaedy have lat and long set.
            return;
        }

        // Create the geocode client.
        let geocode = Geocode::new_from_env();
        // Attempt to get the lat and lng.
        match geocode.get(&self.location).await {
            Ok(result) => {
                let location = result.geometry.location;
                self.latitude = location.lat as f32;
                self.longitude = location.lng as f32;
            }
            Err(e) => {
                if !self.location.is_empty() {
                    warn!("could not get lat lng for location `{}`: {}", self.location, e);
                }
            }
        }
    }

    /// Send a rejection email if we need to.
    pub async fn send_email_follow_up_if_necessary(&mut self, db: &Database, config: ApplyConfig) -> Result<()> {
        // Send an email follow up if we should.
        if self.sent_email_follow_up {
            log::info!(
                "Applicant {} in {} has already been sent a follow up. Skipping.",
                self.id,
                self.raw_status
            );

            // We have already followed up with the candidate.
            // Let's return early.
            return Ok(());
        }

        // Get the status for the applicant.
        let status = crate::applicant_status::Status::from_str(&self.status).unwrap_or_default();

        if status != crate::applicant_status::Status::NeedsToBeTriaged
            && status != crate::applicant_status::Status::Declined
            && status != crate::applicant_status::Status::Deferred
        {
            // Just set that we have sent the email so that we don't do it again if we move to
            // next steps then interviews etc.
            // Only when it's not in "NeedsToBeTriaged", or we are about to defer or decline.
            // Mark the column as true not false.

            log::info!(
                "Applicant {} in {:?} is in a state that assumes a manual followup has been sent. Marking record as sent. Skipping.",
                self.id,
                status
            );

            self.sent_email_follow_up = true;
            // Update the database.
            self.update(db).await?;
            // Return early, we don't actually want to send something, likely a member
            // of the Oxide team reached out directly.
            return Ok(());
        }

        if status != crate::applicant_status::Status::Declined && status != crate::applicant_status::Status::Deferred {
            // We want to return early, we only care about people who were deferred or declined.
            // So sent the folks in the triage home.
            // Above we sent home everyone else.
            log::info!(
                "Applicant {} in {:?} is not in a state where an email needs to be sent. Skipping.",
                self.id,
                status
            );

            return Ok(());
        }

        // Check if we have sent the follow up email to them.unwrap_or_default().
        let letter_key = if self.raw_status.contains("did not do materials") {
            "no-materials"
        } else if self.raw_status.contains("junior") {
            "junior"
        } else {
            "timing"
        };

        if let Some(letter) = config.create_rejection_letter(letter_key, self) {
            log::info!(
                "Applicant {} in {:?} needs a rejection letter to be created. Creating.",
                self.id,
                status
            );

            // Initialize the SendGrid client.
            let sendgrid_client = SendGrid::new_from_env();

            // Send the message.
            sendgrid_client
                .mail_send()
                .send_plain_text(
                    &letter.subject,
                    &letter.body,
                    &[self.email.to_string()],
                    &letter.cc,
                    &letter.bcc,
                    &letter.from,
                )
                .await?;

            log::info!(
                "Applicant {} in {:?} has been sent a rejection letter.",
                self.id,
                status
            );

            // Mark the time we sent the email.
            self.rejection_sent_date_time = Some(Utc::now());

            self.sent_email_follow_up = true;
            // Update the database.
            self.update(db).await?;

            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "Failed to send rejection letter to {} with status {}",
                self.id,
                self.raw_status
            ))
        }
    }

    /// Expand the applicants materials and do any automation that needs to be done.
    pub async fn expand(&mut self, db: &Database, drive_client: &GoogleDrive, config: &ApplyConfig) -> Result<()> {
        self.cleanup_phone();
        self.parse_github_gitlab();
        self.cleanup_linkedin();

        // Add the scoring url since now we should have an Airtable record id.
        // Since we are an Applicant.
        if !self.airtable_record_id.is_empty() {
            // We could URL-encode the whole thing, but we don't need to, just the + is fine.
            self.scoring_form_url = format!("https://apply.oxide.computer/review/{}", self.email.replace('+', "%2B"));
        }

        // Check if we have sent them an email that we received their application.
        if !self.sent_email_received {
            let letter = config.create_received_letter(self);

            // Send them an email.
            self.send_email_recieved_application_to_applicant(&letter).await?;
            self.sent_email_received = true;
            // Update it in the database just in case.
            self.update(db).await?;

            info!("sent email to {} that we received their application", self.email);
            // Send the email internally.
            self.send_email_internally(db).await?;
        }

        // Set the latitude and longitude if we don't already have it.
        self.set_lat_long().await;

        // Get the time seven days ago.
        let duration_from_now = Utc::now().signed_duration_since(self.submitted_time);

        // If the application is as new as the last week then parse all the contents.
        // This takes a long time so we skip all the others.
        if (duration_from_now < Duration::days(2)
            || (duration_from_now < Duration::days(20) && self.question_why_oxide.is_empty()))
            && self.status != crate::applicant_status::Status::Declined.to_string()
        {
            // Read the file contents.
            match get_file_contents(drive_client, &self.resume).await {
                Ok(r) => self.resume_contents = r,
                Err(e) => {
                    warn!("getting resume contents for applicant `{}` failed: {}", self.email, e);
                }
            }

            match get_file_contents(drive_client, &self.materials).await {
                Ok(r) => self.materials_contents = r,
                Err(e) => {
                    warn!(
                        "getting materials contents for applicant `{}` failed: {}",
                        self.email, e
                    );
                }
            }

            self.parse_materials();
        }

        Ok(())
    }

    /// Get the applicant's information in the form of the body of an email for a
    /// company wide notification that we received a new application.
    fn as_company_notification_email(&self) -> String {
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
            msg += &format!(
                "\nGitHub: {} (https://github.com/{})",
                self.github,
                self.github.trim_start_matches('@')
            );
        }
        if !self.gitlab.is_empty() {
            msg += &format!(
                "\nGitLab: {} (https://gitlab.com/{})",
                self.gitlab,
                self.gitlab.trim_start_matches('@')
            );
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

        if !self.scoring_form_url.is_empty() {
            msg += &format!("\n\nScoring form url: {}\n", self.scoring_form_url);
        }

        msg += &format!(
            "\nResume: {}
Oxide Candidate Materials: {}
Interested in: {}

## Reminder

The applicants Airtable \
             is at: https://airtable-applicants.corp.oxide.computer
",
            self.resume,
            self.materials,
            self.interested_in.join(", ")
        );

        msg
    }

    /// Send an email internally that we have a new application.
    async fn send_email_internally(&self, db: &Database) -> Result<()> {
        let company = self.company(db).await?;
        // Initialize the SendGrid client.
        let sendgrid_client = SendGrid::new_from_env();

        // Send the message.
        sendgrid_client
            .mail_send()
            .send_plain_text(
                &format!("New {} Application: {}", self.role, self.name),
                &self.as_company_notification_email(),
                &[format!("applications@{}", company.gsuite_domain)],
                &[],
                &[],
                &format!("applications@{}", company.gsuite_domain),
            )
            .await?;

        Ok(())
    }

    /// Send an email to the applicant that we recieved their application.
    async fn send_email_recieved_application_to_applicant(&self, letter: &Letter) -> Result<()> {
        // Initialize the SendGrid client.
        let sendgrid_client = SendGrid::new_from_env();

        // Send the message.
        sendgrid_client
            .mail_send()
            .send_plain_text(
                &letter.subject,
                &letter.body,
                &[self.email.to_string()],
                &letter.cc,
                &letter.bcc,
                &letter.from,
            )
            .await?;

        Ok(())
    }

    /// Parse the questions from the materials.
    fn parse_materials(&mut self) {
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
                work_samples = parse_question(
                    r"What would you have done differently\?",
                    "Exploratory samples",
                    &materials_contents,
                );

                if work_samples.is_empty() {
                    work_samples = parse_question(
                        r"Some questions(?s:.*)o have in mind as you describe them:",
                        "Exploratory samples",
                        &materials_contents,
                    );

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
                    writing_samples =
                        parse_question(r"Writing sample\(s\)", "Code and/or design sample", &materials_contents);
                }
            }
        }
        self.writing_samples = writing_samples;

        let mut analysis_samples =
            parse_question(r"Analysis sample\(s\)$", "Presentation samples", &materials_contents);
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

        let mut presentation_samples =
            parse_question(r"Presentation sample\(s\)", "Questionnaire", &materials_contents);
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

        self.question_technically_challenging = parse_question(
            QUESTION_TECHNICALLY_CHALLENGING,
            QUESTION_WORK_PROUD_OF,
            &materials_contents,
        );
        self.question_proud_of = parse_question(QUESTION_WORK_PROUD_OF, QUESTION_HAPPIEST_CAREER, &materials_contents);
        self.question_happiest = parse_question(
            QUESTION_HAPPIEST_CAREER,
            QUESTION_UNHAPPIEST_CAREER,
            &materials_contents,
        );
        self.question_unhappiest = parse_question(
            QUESTION_UNHAPPIEST_CAREER,
            QUESTION_VALUE_REFLECTED,
            &materials_contents,
        );
        self.question_value_reflected =
            parse_question(QUESTION_VALUE_REFLECTED, QUESTION_VALUE_VIOLATED, &materials_contents);
        self.question_value_violated =
            parse_question(QUESTION_VALUE_VIOLATED, QUESTION_VALUES_IN_TENSION, &materials_contents);
        self.question_values_in_tension =
            parse_question(QUESTION_VALUES_IN_TENSION, QUESTION_WHY_OXIDE, &materials_contents);
        self.question_why_oxide = parse_question(QUESTION_WHY_OXIDE, "", &materials_contents);
    }

    fn parse_github_gitlab(&mut self) {
        let mut github = "".to_string();
        let mut gitlab = "".to_string();
        if !self.github.trim().is_empty() {
            github = format!(
                "@{}",
                self.github
                    .trim()
                    .to_lowercase()
                    .trim_start_matches("https://github.com/")
                    .trim_start_matches("http://github.com/")
                    .trim_start_matches("https://www.github.com/")
                    .trim_start_matches("http://www.github.com/")
                    .trim_start_matches("www.github.com/")
                    .trim_start_matches("github.com/")
                    .trim_start_matches('@')
                    .replace("github.com/", "")
                    .trim_end_matches('/')
                    .trim_start_matches('/')
            )
            .trim()
            .to_string();

            if github == "@" || github == "@n/a" || github.contains("linkedin.com") {
                github = "".to_string();
            }

            // Some people put a gitlab URL in the github form input,
            // parse those accordingly.
            if github.contains("https://gitlab.com") {
                github = "".to_string();

                gitlab = format!(
                    "@{}",
                    self.github
                        .trim()
                        .to_lowercase()
                        .trim_start_matches("https://gitlab.com/")
                        .trim_start_matches('@')
                        .trim_end_matches('/')
                );
            }
        }

        self.github = github;
        self.gitlab = gitlab;
    }

    /// Cleanup the applicants phone.
    fn cleanup_phone(&mut self) {
        // Cleanup and parse the phone number and country code.
        let mut phone = self.phone.replace([' ', '-', '+', '(', ')'], "");

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
        } else if (location.to_lowercase().contains("czech republic") || location.to_lowercase().contains("prague"))
            && phone.starts_with("420")
        {
            country = phonenumber::country::CZ;
        } else if location.to_lowercase().contains("turkey") && phone.starts_with("90") {
            country = phonenumber::country::TR;
        } else if location.to_lowercase().contains("sweden") && phone.starts_with("46") {
            country = phonenumber::country::SE;
        } else if (location.to_lowercase().contains("mumbai")
            || location.to_lowercase().contains("india")
            || location.to_lowercase().contains("bangalore"))
            && phone.starts_with("91")
        {
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
        if let Ok(phone_number) = phonenumber::parse(Some(country), &phone) {
            if !phone_number.is_valid() {
                info!("phone number is invalid: {}", self.id);
            }

            phone = format!("{}", phone_number.format().mode(phonenumber::Mode::International));
        }
        self.phone = phone;
        self.country_code = country_code;
    }

    pub async fn do_docusign_offer(
        &mut self,
        db: &Database,
        ds: &DocuSign,
        new_envelope: docusign::Envelope,
    ) -> Result<()> {
        // Keep the fields from Airtable we need just in case they changed.
        self.keep_fields_from_airtable(db).await;

        // We look for "Onboarding" here as well since we want to make sure we can actually update
        // the data for the user.
        if self.status != crate::applicant_status::Status::GivingOffer.to_string()
            && self.status != crate::applicant_status::Status::Onboarding.to_string()
            && self.status != crate::applicant_status::Status::Hired.to_string()
        {
            // We can return early.
            return Ok(());
        }

        if self.docusign_envelope_id.is_empty()
            && self.status == crate::applicant_status::Status::GivingOffer.to_string()
        {
            info!(
                "applicant has status giving offer: {}, generating offer in docusign for them!",
                self.name
            );

            // Let's create the envelope.
            let envelope = ds.create_envelope(new_envelope).await?;

            // Set the id of the envelope.
            self.docusign_envelope_id = envelope.envelope_id.to_string();
            // Set the status of the envelope.
            self.docusign_envelope_status = envelope.status.to_string();

            // Update the applicant in the database.
            self.update(db).await?;
        } else if !self.docusign_envelope_id.is_empty() {
            // We have sent their offer.
            // Let's get the status of the envelope in Docusign.
            let envelope = ds.get_envelope(&self.docusign_envelope_id).await?;

            self.update_applicant_from_docusign_offer_envelope(db, ds, envelope)
                .await?;
        }

        Ok(())
    }

    pub async fn update_applicant_from_docusign_offer_envelope(
        &mut self,
        db: &Database,
        ds: &DocuSign,
        envelope: docusign::Envelope,
    ) -> Result<()> {
        // Keep the fields from Airtable we need just in case they changed.
        self.keep_fields_from_airtable(db).await;

        let company = self.company(db).await?;

        // Set the status in the database and airtable.
        self.docusign_envelope_status = envelope.status.to_string();
        self.offer_created = envelope.created_date_time;

        // If the document is completed, let's save it to Google Drive.
        if envelope.status != "completed" {
            // We will skip to the end and return early, only updating the status.
            self.update(db).await?;

            return Ok(());
        }

        // Set the completed time.
        self.offer_completed = envelope.completed_date_time;
        if self.status == crate::applicant_status::Status::GivingOffer.to_string() {
            // Since the status of the envelope is completed, let's set their status to "Onboarding".
            // Only do this if they are not already hired.
            self.status = crate::applicant_status::Status::Onboarding.to_string();
            // Update them in case something fails.
            self.update(db).await?;

            // Request their background check, if we have not already.
            if self.criminal_background_check_status.is_empty() {
                // Request the background check, since we previously have not requested one.
                self.send_background_check_invitation(db).await?;
            }
        }

        // Initialize the Google Drive client.
        let drive_client = company.authenticate_google_drive(db).await?;
        // Figure out where our directory is.
        // It should be in the shared drive : "Offer Letters"
        let shared_drive = drive_client.drives().get_by_name("Offer Letters").await?;
        let drive_id = shared_drive.id.to_string();

        // TODO: only save the documents if we don't already have them.
        for document in &envelope.documents {
            let mut bytes = base64::decode(&document.pdf_bytes).unwrap_or_default();
            // Check if we already have bytes to the data.
            if document.pdf_bytes.is_empty() {
                // Get the document from docusign.
                // In order to not "over excessively poll the API here, we need to sleep for 15
                // min before getting each of the documents.
                // https://developers.docusign.com/docs/esign-rest-api/esign101/rules-and-limits/
                //thread::sleep(std::time::Duration::from_secs(15));
                bytes = ds.get_document(&envelope.envelope_id, &document.id).await?.to_vec();
            }

            // Create the folder for our applicant with their name.
            let name_folder_id = drive_client
                .files()
                .create_folder(&shared_drive.id, "", &self.name)
                .await?;

            let mut filename = format!("{} - {}.pdf", self.name, document.name);
            if document.name.contains("Offer Letter") {
                filename = format!("{} - Offer.pdf", self.name);
            } else if document.name.contains("Summary") {
                filename = format!("{} - Offer - DocuSign Summary.pdf", self.name);
            } else if document.name.contains("Employee Mediation") || document.name.contains("Employee_Mediation") {
                filename = format!("{} - Mediation Agreement.pdf", self.name);
            } else if document.name.contains("Employee Proprietary") || document.name.contains("Employee_Proprietary") {
                filename = format!("{} - PIIA.pdf", self.name);
            }

            // Create or update the file in the google_drive.
            drive_client
                .files()
                .create_or_update(&drive_id, &name_folder_id, &filename, "application/pdf", &bytes)
                .await?;
            info!(
                "uploaded completed `{}` file for user {} to drive",
                document.name, self.id
            );
        }

        // In order to not "over excessively poll the API here, we need to sleep for 15
        // min before getting each of the documents.
        // https://developers.docusign.com/docs/esign-rest-api/esign101/rules-and-limits/
        //thread::sleep(std::time::Duration::from_secs(900));
        let form_data = ds.get_envelope_form_data(&self.docusign_envelope_id).await?;

        // Let's get the employee for the applicant.
        // We will match on their recovery email.
        let result = users::dsl::users
            .filter(
                users::dsl::recovery_email
                    .eq(self.email.to_string())
                    .and(users::dsl::cio_company_id.eq(company.id)),
            )
            .first_async::<User>(db.pool())
            .await;
        if result.is_ok() {
            let mut employee = result?;
            // Only do this if we don't have the employee's home address or start date.
            // This will help us to not override any changes then that are later made in gusto.
            if employee.home_address_street_1.is_empty() || employee.start_date == crate::utils::default_date() {
                // We have an employee, so we can update their data from the data in Docusign.

                for fd in form_data.clone() {
                    // Save the data to the employee who matches this applicant.
                    if fd.name == "Applicant's Street Address" {
                        employee.home_address_street_1 = fd.value.trim().to_string();
                    }
                    if fd.name == "Applicant's City" {
                        employee.home_address_city = fd.value.trim().to_string();
                    }
                    if fd.name == "Applicant's State" {
                        employee.home_address_state =
                            crate::states::StatesMap::match_abreev_or_return_existing(&fd.value);
                    }
                    if fd.name == "Applicant's Postal Code" {
                        employee.home_address_zipcode = fd.value.trim().to_string();
                    }
                    if fd.name == "Applicant's Country" {
                        employee.home_address_country = fd.value.trim().to_string();
                    }
                    if fd.name == "Start Date" {
                        let start_date = NaiveDate::parse_from_str(fd.value.trim(), "%m/%d/%Y")?;
                        employee.start_date = start_date;
                    }
                }
            }

            // Update the employee.
            employee.update(db).await?;
        }

        for fd in form_data {
            // TODO: we could somehow use the manager data here or above. The manager data is in
            // the docusign data.
            // Only set the start date if we haven't set it already.
            if fd.name == "Start Date" && self.start_date.is_none() {
                let start_date = NaiveDate::parse_from_str(fd.value.trim(), "%m/%d/%Y")?;
                self.start_date = Some(start_date);
            }
        }

        self.update(db).await?;

        Ok(())
    }

    pub async fn do_docusign_piia(
        &mut self,
        db: &Database,
        ds: &DocuSign,
        new_envelope: docusign::Envelope,
    ) -> Result<()> {
        // Keep the fields from Airtable we need just in case they changed.
        self.keep_fields_from_airtable(db).await;

        // We look for "Onboarding" here as well since we want to make sure we can actually update
        // the data for the user.
        if self.status != crate::applicant_status::Status::GivingOffer.to_string()
            && self.status != crate::applicant_status::Status::Onboarding.to_string()
            && self.status != crate::applicant_status::Status::Hired.to_string()
        {
            // We can return early.
            return Ok(());
        }

        if self.docusign_piia_envelope_id.is_empty()
            && self.status == crate::applicant_status::Status::GivingOffer.to_string()
        {
            info!(
                "applicant {} has status giving offer: generating employee agreements in docusign for them!",
                self.id
            );

            // Let's create the envelope.
            let envelope = ds.create_envelope(new_envelope).await?;

            // Set the id of the envelope.
            self.docusign_piia_envelope_id = envelope.envelope_id.to_string();
            // Set the status of the envelope.
            self.docusign_piia_envelope_status = envelope.status.to_string();

            // Update the applicant in the database.
            self.update(db).await?;
        } else if !self.docusign_piia_envelope_id.is_empty() {
            // We have sent their employee agreements.
            // Let's get the status of the envelope in Docusign.
            let envelope = ds.get_envelope(&self.docusign_piia_envelope_id).await?;

            self.update_applicant_from_docusign_piia_envelope(db, ds, envelope)
                .await?;
        }

        Ok(())
    }

    pub async fn send_new_piia_for_accepted_applicant(
        &mut self,
        db: &Database,
        ds: &DocuSign,
        new_envelope: docusign::Envelope,
    ) -> Result<()> {
        // Keep the fields from Airtable we need just in case they changed.
        self.keep_fields_from_airtable(db).await;

        // Only allow documents to be re-generated if we are in the process of or have already
        // hired this applicant
        if self.status == crate::applicant_status::Status::GivingOffer.to_string()
            && self.status == crate::applicant_status::Status::Onboarding.to_string()
            && self.status == crate::applicant_status::Status::Hired.to_string()
        {
            info!(
                "generating new employee agreements for applicant {} in docusign",
                self.id
            );

            // Reset the current piia fields
            self.docusign_piia_envelope_id = String::new();
            self.docusign_piia_envelope_status = String::new();
            self.piia_envelope_created = None;
            self.piia_envelope_completed = None;

            // Let's create the envelope.
            let envelope = ds.create_envelope(new_envelope).await?;

            // Set the id of the envelope.
            self.docusign_piia_envelope_id = envelope.envelope_id.to_string();

            // Set the status of the envelope.
            self.docusign_piia_envelope_status = envelope.status.to_string();

            // Update the applicant in the database.
            self.update(db).await?;

            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "Unable to regenerate PIIA documents for non-hiring employees"
            ))
        }
    }

    pub async fn keep_fields_from_airtable(&mut self, db: &Database) {
        // Let's get the existing record from Airtable, so we can use it as the source
        // of truth for various things.
        if let Some(ex) = self.get_existing_airtable_record(db).await {
            let existing = ex.fields;
            // We keep the scorers from Airtable in case someone assigned someone from the UI.
            self.scorers = existing.scorers.clone();
            // Keep the interviewers from Airtable since they are updated out of bound by Airtable.
            self.interviews = existing.interviews.clone();
            // Keep the reviews, since these are updated out of band by Airtable.
            self.link_to_reviews = existing.link_to_reviews;

            // We want to keep the status and status raw since we might have modified
            // it to move a candidate along in the process.
            self.status = existing.status.to_string();
            self.raw_status = existing.raw_status.to_string();

            // Mostly the start date will populate from docusign, but just in case they
            // are someone who worked remotely, we might have to manually set it.
            // If docusign is incorrect, make sure Airtable always has the source of truth.
            self.start_date = existing.start_date;
        } else {
            log::warn!(
                "Could not find existing Airtable record for email -> {}, id -> {}",
                self.email,
                self.id
            );
        }
    }

    pub async fn update_applicant_from_docusign_piia_envelope(
        &mut self,
        db: &Database,
        ds: &DocuSign,
        envelope: docusign::Envelope,
    ) -> Result<()> {
        // Keep the fields from Airtable we need just in case they changed.
        self.keep_fields_from_airtable(db).await;

        let company = self.company(db).await?;

        // Set the status in the database and airtable.
        self.docusign_piia_envelope_status = envelope.status.to_string();
        self.piia_envelope_created = envelope.created_date_time;

        // If the document is completed, let's save it to Google Drive.
        if envelope.status != "completed" {
            // We will skip to the end and return early, only updating the status.
            self.update(db).await?;
            return Ok(());
        }

        // Set the completed time.
        self.piia_envelope_completed = envelope.completed_date_time;
        // We do not change the applicant's status or anything since they don't need
        // to complete these docs until their start date.
        // However, other than manually, we should have a gate to make sure they _do_
        // complete these documents before their start date.

        // Let's update the database here since nothing else has to do with that.
        self.update(db).await?;

        // Initialize the Google Drive client.
        let drive_client = company.authenticate_google_drive(db).await?;
        // Figure out where our directory is.
        // It should be in the shared drive : "Offer Letters"
        let shared_drive = drive_client.drives().get_by_name("Offer Letters").await?;
        let drive_id = shared_drive.id.to_string();

        // TODO: only save the documents if we don't already have them.
        for document in &envelope.documents {
            let mut bytes = base64::decode(&document.pdf_bytes).unwrap_or_default();
            // Check if we already have bytes to the data.
            if document.pdf_bytes.is_empty() {
                // Get the document from docusign.
                // In order to not "over excessively poll the API here, we need to sleep for 15
                // min before getting each of the documents.
                // https://developers.docusign.com/docs/esign-rest-api/esign101/rules-and-limits/
                //thread::sleep(std::time::Duration::from_secs(15));
                bytes = ds.get_document(&envelope.envelope_id, &document.id).await?.to_vec();
            }

            // Create the folder for our applicant with their name.
            let name_folder_id = drive_client
                .files()
                .create_folder(&shared_drive.id, "", &self.name)
                .await?;

            let mut filename = format!("{} - {}.pdf", self.name, document.name);
            if document.name.contains("Employee Mediation") || document.name.contains("Employee_Mediation") {
                filename = format!("{} - Mediation Agreement.pdf", self.name);
            } else if document.name.contains("Employee Proprietary") || document.name.contains("Employee_Proprietary") {
                filename = format!("{} - PIIA.pdf", self.name);
            } else if document.name.contains("Summary") {
                filename = format!("{} - Employee Agreements - DocuSign Summary.pdf", self.name);
            } else if document.name.contains("Offer Letter") {
                filename = format!("{} - Offer.pdf", self.name);
            }

            // Create or update the file in the google_drive.
            drive_client
                .files()
                .create_or_update(&drive_id, &name_folder_id, &filename, "application/pdf", &bytes)
                .await?;
            info!("uploaded completed file `{}` to drive", filename);
        }

        Ok(())
    }
}

pub async fn refresh_new_applicants_and_reviews(
    db: &Database,
    company: &Company,
    app_config: &AppConfig,
) -> Result<()> {
    if company.airtable_base_id_hiring.is_empty() {
        // Return early.
        return Ok(());
    }

    let github = company.authenticate_github()?;

    // Get all the hiring issues on the configs repository.
    let configs_issues = github
        .issues()
        .list_all_for_repo(
            &company.github_org,
            "configs",
            // milestone
            "",
            octorust::types::IssuesListState::All,
            // assignee
            "",
            // creator
            "",
            // mentioned
            "",
            // labels
            "hiring",
            // sort
            Default::default(),
            // direction
            Default::default(),
            // since
            None,
        )
        .await?;

    // We want all the applicants without a sheet id, since this is the list of applicants we care
    // about. Everything else came from Google Sheets and therefore uses the old system.
    let applicants_id_range: (Option<i32>, Option<i32>) = applicants::dsl::applicants
        .filter(applicants::dsl::sheet_id.eq("".to_string()))
        .select((diesel::dsl::min(applicants::id), diesel::dsl::max(applicants::id)))
        .first_async(db.pool())
        .await?;

    if let (Some(min), Some(max)) = applicants_id_range {
        log::info!("Preparing to process applicants {} through {}", min, max);

        let applicant_ids = (min..=max).collect::<Vec<i32>>();
        let applicant_id_chunks = applicant_ids.chunks(100);

        for chunk in applicant_id_chunks {
            log::info!("Fetching applicants {:?} through {:?}", chunk.first(), chunk.last());

            let applicants = applicants::dsl::applicants
                .filter(applicants::dsl::sheet_id.eq("".to_string()))
                .filter(applicants::dsl::id.eq_any(chunk.to_vec()))
                .order_by(applicants::dsl::id.asc())
                .load_async::<Applicant>(db.pool())
                .await?;

            let fetched_applicant_ids = applicants.iter().map(|a| a.id).collect::<Vec<_>>();
            log::info!("Preparing to sync applicants {:?}", fetched_applicant_ids);

            // Iterate over the applicants and update them.
            // We should do these concurrently, but limit it to maybe 3 at a time.
            let applicant_chunks = applicants.chunks(3).map(|c| c.to_vec()).collect::<Vec<_>>();

            for applicant_chunk in applicant_chunks {
                let ids = applicant_chunk.iter().map(|a| a.id).collect::<Vec<_>>();
                log::info!("Sync applicants {:?}", ids);

                // Disable sync while verifying logic
                let tasks: Vec<_> = applicant_chunk
                    .into_iter()
                    .map(|mut applicant| {
                        tokio::spawn(
                            enclose! { (db, company, github, configs_issues, app_config) async move {
                                applicant.refresh(&db, &company, &github, &configs_issues, app_config).await
                            }},
                        )
                    })
                    .collect();

                let mut results: Vec<Result<()>> = Default::default();
                for task in tasks {
                    results.push(task.await?);
                }

                for result in results {
                    result?;
                }
            }
        }
    }

    // Update Airtable.
    // TODO: this might cause some racy problems, maybe only run at night (?)
    // Or maybe always get the latest from the database and update airtable with that (?)
    // Applicants::get_from_db(db, company.id)?.update_airtable(db).await?;

    Ok(())
}

#[cfg(test)]
pub mod tests {
    use async_bb8_diesel::AsyncRunQueryDsl;
    use chrono::{NaiveDate, Utc};
    use diesel::prelude::*;
    use serde_json::json;

    use crate::{
        app_config::NewHireIssue,
        applicants::{Applicant, Applicants},
        db::Database,
        schema::applicants,
    };

    pub fn mock_applicant() -> Applicant {
        Applicant {
            id: 0,
            name: "Test User".to_string(),
            role: "Engineering".to_string(),
            sheet_id: String::default(),
            status: String::default(),
            raw_status: String::default(),
            submitted_time: Utc::now(),
            email: "random-test@testemaildomain.com".to_string(),
            phone: String::default(),
            country_code: String::default(),
            location: String::default(),
            latitude: 0.0,
            longitude: 0.0,
            github: String::default(),
            gitlab: String::default(),
            linkedin: String::default(),
            portfolio: String::default(),
            portfolio_pdf: String::default(),
            website: String::default(),
            resume: String::default(),
            materials: String::default(),
            sent_email_received: false,
            sent_email_follow_up: false,
            rejection_sent_date_time: None,
            value_reflected: String::default(),
            value_violated: String::default(),
            values_in_tension: vec![],
            resume_contents: String::default(),
            materials_contents: String::default(),
            work_samples: String::default(),
            writing_samples: String::default(),
            analysis_samples: String::default(),
            presentation_samples: String::default(),
            exploratory_samples: String::default(),
            question_technically_challenging: String::default(),
            question_proud_of: String::default(),
            question_happiest: String::default(),
            question_unhappiest: String::default(),
            question_value_reflected: String::default(),
            question_value_violated: String::default(),
            question_values_in_tension: String::default(),
            question_why_oxide: String::default(),
            interview_packet: String::default(),
            interviews: vec![],
            interviews_started: None,
            interviews_completed: None,
            scorers: vec![],
            scorers_completed: vec![],
            scoring_form_id: String::default(),
            scoring_form_url: String::default(),
            scoring_form_responses_url: String::default(),
            scoring_evaluations_count: 0,
            scoring_enthusiastic_yes_count: 0,
            scoring_yes_count: 0,
            scoring_pass_count: 0,
            scoring_no_count: 0,
            scoring_not_applicable_count: 0,
            scoring_insufficient_experience_count: 0,
            scoring_inapplicable_experience_count: 0,
            scoring_job_function_yet_needed_count: 0,
            scoring_underwhelming_materials_count: 0,
            criminal_background_check_status: String::default(),
            motor_vehicle_background_check_status: String::default(),
            start_date: Some(NaiveDate::from_ymd(2092, 01, 01)),
            interested_in: vec![],
            geocode_cache: String::default(),
            docusign_envelope_id: String::default(),
            docusign_envelope_status: String::default(),
            offer_created: None,
            offer_completed: None,
            docusign_piia_envelope_id: String::default(),
            docusign_piia_envelope_status: String::default(),
            piia_envelope_created: None,
            piia_envelope_completed: None,
            link_to_reviews: vec![],
            cio_company_id: 0,
            airtable_record_id: String::default(),
        }
    }

    #[ignore]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_serialize_deserialize_applicants() {
        crate::utils::setup_logger();

        let db = Database::new().await;
        // Make sure we even have applicants.
        let apps = Applicants::get_from_db(&db, 1).await.unwrap();
        if apps.into_iter().len() > 0 {
            let applicant = applicants::dsl::applicants
                .filter(applicants::dsl::id.eq(318))
                .first_async::<Applicant>(db.pool())
                .await
                .unwrap();

            // Let's test that serializing this is going to give us an array of Airtable users.
            let scorers = json!(applicant).to_string();
            // Let's assert in the string are the scorers formatted as Airtable users.
            assert!(scorers.contains("\"scorers\":[{\"email\":\""));

            // Let's test that deserializing a string will give us the same applicant we had
            // originally.
            let a: Applicant = serde_json::from_str(&scorers).unwrap();
            assert_eq!(applicant, a);
        }
    }

    fn mock_new_hire_issue() -> NewHireIssue {
        NewHireIssue {
            assignees: vec!["assign1".to_string(), "assign2".to_string()],
            alerts: vec!["alert1".to_string(), "alert2".to_string()],
            default_groups: vec!["group1".to_string(), "group2".to_string()],
            aws_roles: vec!["role1".to_string(), "role2".to_string()],
        }
    }

    #[test]
    fn test_creates_new_hire_issue_body() {
        let applicant = mock_applicant();
        let config = mock_new_hire_issue();

        let body = applicant.create_new_hire_issue_body("test-username", &config);

        assert_eq!(
            r#"- [ ] Add to users.toml
- [ ] Provision user in Airtable
- [ ] Add to matrix chat

Start Date: Tuesday, January 1, 2092
Personal Email: random-test@testemaildomain.com
Twitter: [TWITTER HANDLE]
GitHub: 
Phone: 
Location: 
cc @alert1
cc @alert2

```
[users.test-username]
first_name = 'Test'
last_name = 'User'
username = 'test-username'
aliases = []
groups = [
    'group1',
    'group2'
]
recovery_email = 'random-test@testemaildomain.com'
recovery_phone = ''
gender = ''
github = ''
chat = ''
aws_role = 'role1,role2'
department = ''
manager = ''
```"#,
            body
        );
    }
}
