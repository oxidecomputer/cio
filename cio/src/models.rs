use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{stderr, stdout, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::from_utf8;
use std::str::FromStr;

use airtable_api::{api_key_from_env, Airtable, Record};
use async_trait::async_trait;
use chrono::offset::Utc;
use chrono::{DateTime, NaiveDate};
use chrono_humanize::HumanTime;
use diesel::deserialize::{self, FromSql};
use diesel::pg::Pg;
use diesel::serialize::{self, Output, ToSql};
use diesel::sql_types::Jsonb;
use google_drive::GoogleDrive;
use hubcaps::comments::CommentOptions;
use hubcaps::issues::{Issue, IssueOptions};
use hubcaps::repositories::{Repo, Repository};
use hubcaps::Github;
use macros::db_struct;
use regex::Regex;
use schemars::JsonSchema;
use sendgrid_api::SendGrid;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sheets::Sheets;
use slack_chat_api::{FormattedMessage, MessageBlock, MessageBlockText, MessageBlockType, MessageType};
use tracing::instrument;

use crate::airtable::{
    AIRTABLE_APPLICATIONS_TABLE, AIRTABLE_AUTH_USERS_TABLE, AIRTABLE_AUTH_USER_LOGINS_TABLE, AIRTABLE_BASE_ID_CUSTOMER_LEADS, AIRTABLE_BASE_ID_MISC, AIRTABLE_BASE_ID_RACK_ROADMAP,
    AIRTABLE_BASE_ID_RECURITING_APPLICATIONS, AIRTABLE_JOURNAL_CLUB_MEETINGS_TABLE, AIRTABLE_JOURNAL_CLUB_PAPERS_TABLE, AIRTABLE_MAILING_LIST_SIGNUPS_TABLE, AIRTABLE_RFD_TABLE,
};
use crate::applicants::{get_file_contents, get_role_from_sheet_id, ApplicantSheetColumns};
use crate::core::UpdateAirtableRecord;
use crate::rfds::{clean_rfd_html_links, get_images_in_branch, get_rfd_contents_from_repo, parse_markdown, update_discussion_link, update_state};
use crate::schema::{applicants, auth_user_logins, auth_users, github_repos, journal_club_meetings, journal_club_papers, mailing_list_subscribers, rfds as r_f_ds, rfds};
use crate::utils::{check_if_github_issue_exists, create_or_update_file_in_github_repo, github_org, write_file, DOMAIN};

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
#[db_struct {
    new_name = "Applicant",
    base_id = "AIRTABLE_BASE_ID_RECURITING_APPLICATIONS",
    table = "AIRTABLE_APPLICATIONS_TABLE",
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[table_name = "applicants"]
pub struct NewApplicant {
    pub name: String,
    pub role: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sheet_id: String,
    pub status: String,
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
            status: Default::default(),
            sent_email_received: false,
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
                "Thank you for submitting your application materials! We really appreciate all
the time and thought everyone puts into their application. We will be in touch
within the next couple weeks with more information.
Sincerely,
  The Oxide Team"
                    .to_string(),
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
    pub fn parse_from_row_with_columns(sheet_name: &str, sheet_id: &str, columns: &ApplicantSheetColumns, row: &[String]) -> Self {
        // If the length of the row is greater than the status column
        // then we have a status.
        let status = if row.len() > columns.status {
            crate::applicant_status::Status::from_str(&row[columns.status]).unwrap_or_default()
        } else {
            crate::applicant_status::Status::NeedsToBeTriaged
        };

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
        let value_reflected = if row.len() > columns.value_reflected && columns.value_reflected != 0 {
            row[columns.value_reflected].trim().to_lowercase()
        } else {
            "".to_lowercase()
        };

        // If the length of the row is greater than the value_violated column
        // then we have a value_violated.
        let value_violated = if row.len() > columns.value_violated && columns.value_violated != 0 {
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

        let email = row[columns.email].trim().to_string();
        let location = row[columns.location].trim().to_string();
        let phone = row[columns.phone].trim().to_string();
        let resume = row[columns.resume].to_string();
        let materials = row[columns.materials].to_string();

        NewApplicant {
            submitted_time: NewApplicant::parse_timestamp(&row[columns.timestamp]),
            name: row[columns.name].to_string(),
            email,
            location,
            phone,
            country_code: Default::default(),
            github,
            gitlab,
            linkedin,
            portfolio,
            website,
            resume,
            materials,
            status: status.to_string(),
            sent_email_received,
            role: sheet_name.to_string(),
            sheet_id: sheet_id.to_string(),
            value_reflected,
            value_violated,
            values_in_tension,
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
                    .trim_start_matches('@')
                    .trim_end_matches('/')
            );
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
    pub async fn expand(&mut self, drive_client: &GoogleDrive, sheets_client: &Sheets, sent_email_received_column_index: usize, row_index: usize) {
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

    #[instrument]
    #[inline]
    pub async fn create_github_next_steps_issue(&self, github: &Github, meta_issues: &[Issue]) {
        // Check if we already have an issue for this user.
        let issue = check_if_github_issue_exists(&meta_issues, &self.name);

        // Check if their status is next steps, we only care about folks in the next steps.
        if !self.status.contains("Next steps") {
            // Make sure we don't already have an issue for them.
            if let Some(i) = issue {
                if i.state == "open" {
                    // We only care if the issue is still opened.
                    return;
                }

                // Delete the "next steps" issue from the "meta" repository.
                // This is because they are no longer in "next steps".
                let repo = github.repo(github_org(), "meta");

                // Comment on the issue that this person is now set to be onboarded.
                repo.issue(i.number)
                    .comments()
                    .create(&CommentOptions {
                        body: format!("Closing issue automatically since the applicant is now status: `{}`", self.status,),
                    })
                    .await
                    .unwrap_or_else(|e| panic!("could comment on issue {}: {}", i.number, e));

                // Close the issue.
                repo.issue(i.id)
                    .edit(&IssueOptions {
                        title: i.title.to_string(),
                        body: Default::default(),
                        assignee: Default::default(),
                        labels: Default::default(),
                        milestone: Default::default(),
                        state: Some("closed".to_string()),
                    })
                    .await
                    .unwrap_or_else(|e| panic!("could not close issue {}: {}", i.id, e));
            }
            // Return early.
            return;
        }

        if issue.is_some() {
            // Return early we don't want to update the issue because it will overwrite
            // any changes we made.
            return;
        }

        // Create an issue for the applicant.
        let title = format!("Hiring: {}", self.name);
        let labels = vec!["hiring".to_string()];
        let body = format!(
            "- [ ] Schedule follow up meetings
- [ ] Schedule sync to discuss

## Candidate Information

Submitted Date: {}
Email: {}
Phone: {}
Location: {}
GitHub: {}
Resume: {}
Oxide Candidate Materials: {}

## Reminder

To view the all the candidates refer to the Airtable workspace: https://airtable-applicants.corp.oxide.computer

cc @jessfraz @sdtuck @bcantrill",
            self.submitted_time, self.email, self.phone, self.location, self.github, self.resume, self.materials
        );

        // Create the issue.
        github
            .repo(github_org(), "meta")
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

        println!("[applicant]: created hiring issue for {}", self.email);
    }

    #[instrument]
    #[inline]
    pub async fn create_github_onboarding_issue(&self, github: &Github, configs_issues: &[Issue], meta_issues: &[Issue]) {
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
cc @jessfraz @sdtuck @bcantrill",
            self.email, self.github, self.phone,
        );

        // Create the issue.
        let new_issue = github
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

        // Delete the "next steps" issue from the "meta" repository.
        if let Some(mi) = check_if_github_issue_exists(&meta_issues, &self.name) {
            if mi.state == "open" {
                // We only care if the issue is still opened.
                return;
            }

            let repo = github.repo(github_org(), "meta");

            // Comment on the issue that this person is now set to be onboarded.
            repo.issue(mi.id)
                .comments()
                .create(&CommentOptions {
                    body: format!(
                        "Closing issue automatically since the applicant is set to be onboarded.
The onboarding issue is: {}/configs#{}",
                        github_org(),
                        new_issue.number
                    ),
                })
                .await
                .unwrap();

            // Close the issue.
            repo.issue(mi.id)
                .edit(&IssueOptions {
                    title: mi.title.to_string(),
                    body: Default::default(),
                    assignee: Default::default(),
                    labels: Default::default(),
                    milestone: Default::default(),
                    state: Some("closed".to_string()),
                })
                .await
                .unwrap();
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
    async fn update_airtable_record(&mut self, _record: Applicant) {}
}

/// The data type for an NewAuthUser.
#[db_struct {
    new_name = "AuthUser",
    base_id = "AIRTABLE_BASE_ID_CUSTOMER_LEADS",
    table = "AIRTABLE_AUTH_USERS_TABLE",
    custom_partial_eq = true,
    airtable_fields = [
        "id",
        "link_to_people",
        "logins_count",
        "updated_at",
        "created_at",
        "user_id",
        "last_login",
        "email_verified",
        "link_to_auth_user_logins",
        "last_application_accessed",
        "company",
    ],
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[table_name = "auth_users"]
pub struct NewAuthUser {
    pub user_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub nickname: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub username: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email: String,
    #[serde(default)]
    pub email_verified: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub picture: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub company: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub blog: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub phone: String,
    #[serde(default)]
    pub phone_verified: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub locale: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub login_provider: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_login: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub last_application_accessed: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub last_ip: String,
    pub logins_count: i32,
    /// link to another table in Airtable
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub link_to_people: Vec<String>,
    /// link to another table in Airtable
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub link_to_auth_user_logins: Vec<String>,
}

/// Implement updating the Airtable record for a AuthUser.
#[async_trait]
impl UpdateAirtableRecord<AuthUser> for AuthUser {
    #[instrument]
    #[inline]
    async fn update_airtable_record(&mut self, record: AuthUser) {
        // Set the link_to_people and link_to_auth_user_logins from the original so it stays intact.
        self.link_to_people = record.link_to_people.clone();
        self.link_to_auth_user_logins = record.link_to_auth_user_logins;
    }
}

impl PartialEq for AuthUser {
    // We implement our own here because Airtable has a different data type for the picture.
    #[instrument]
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.user_id == other.user_id
            && self.last_login == other.last_login
            && self.logins_count == other.logins_count
            && self.last_application_accessed == other.last_application_accessed
            && self.company == other.company
    }
}

/// The data type for a NewAuthUserLogin.
#[db_struct {
    new_name = "AuthUserLogin",
    base_id = "AIRTABLE_BASE_ID_CUSTOMER_LEADS",
    table = "AIRTABLE_AUTH_USER_LOGINS_TABLE",
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, Deserialize, Serialize)]
#[table_name = "auth_user_logins"]
pub struct NewAuthUserLogin {
    pub date: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "type")]
    pub typev: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub connection: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub connection_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub client_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub client_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ip: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub hostname: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub user_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub user_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub audience: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub scope: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub strategy: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub strategy_type: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub log_id: String,
    #[serde(default, alias = "isMobile")]
    pub is_mobile: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub user_agent: String,
    /// link to another table in Airtable
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub link_to_auth_user: Vec<String>,
}

/// Implement updating the Airtable record for a AuthUserLogin.
#[async_trait]
impl UpdateAirtableRecord<AuthUserLogin> for AuthUserLogin {
    #[instrument]
    #[inline]
    async fn update_airtable_record(&mut self, _record: AuthUserLogin) {
        // Get the current auth users in Airtable so we can link to it.
        // TODO: make this more dry so we do not call it every single damn time.
        let auth_users = AuthUsers::get_from_airtable().await;

        // Iterate over the auth_users and see if we find a match.
        for (_id, auth_user_record) in auth_users {
            if auth_user_record.fields.user_id == self.user_id {
                // Set the link_to_auth_user to the right user.
                self.link_to_auth_user = vec![auth_user_record.id];
                // Break the loop and return early.
                break;
            }
        }
    }
}

// TODO: figure out the meeting null date bullshit
/// The data type for a NewJournalClubMeeting.
#[db_struct {
    new_name = "JournalClubMeeting",
    base_id = "AIRTABLE_BASE_ID_MISC",
    table = "AIRTABLE_JOURNAL_CLUB_MEETINGS_TABLE",
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, Deserialize, Serialize)]
#[table_name = "journal_club_meetings"]
pub struct NewJournalClubMeeting {
    pub title: String,
    pub issue: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub papers: Vec<String>,
    #[serde(
        default = "crate::utils::default_date",
        deserialize_with = "crate::journal_clubs::meeting_date_format::deserialize",
        serialize_with = "crate::journal_clubs::meeting_date_format::serialize"
    )]
    pub issue_date: NaiveDate,
    #[serde(
        default = "crate::utils::default_date",
        deserialize_with = "crate::journal_clubs::meeting_date_format::deserialize",
        serialize_with = "crate::journal_clubs::meeting_date_format::serialize"
    )]
    pub meeting_date: NaiveDate,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub coordinator: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub state: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub recording: String,
}

impl JournalClubMeeting {
    /// Convert the journal club meeting into JSON as Slack message.
    #[instrument]
    #[inline]
    pub fn as_slack_msg(&self) -> Value {
        let mut objects: Vec<MessageBlock> = Default::default();

        objects.push(MessageBlock {
            block_type: MessageBlockType::Section,
            text: Some(MessageBlockText {
                text_type: MessageType::Markdown,
                text: format!("<{}|*{}*>", self.issue, self.title),
            }),
            elements: Default::default(),
            accessory: Default::default(),
            block_id: Default::default(),
            fields: Default::default(),
        });

        let mut text = format!(
            "<https://github.com/{}|@{}> | issue date: {} | status: *{}*",
            self.coordinator,
            self.coordinator,
            self.issue_date.format("%m/%d/%Y"),
            self.state
        );
        let meeting_date = self.meeting_date.format("%m/%d/%Y").to_string();
        if meeting_date != *"01/01/1969" {
            text += &format!(" | meeting date: {}", meeting_date);
        }
        objects.push(MessageBlock {
            block_type: MessageBlockType::Context,
            elements: vec![MessageBlockText {
                text_type: MessageType::Markdown,
                text,
            }],
            text: Default::default(),
            accessory: Default::default(),
            block_id: Default::default(),
            fields: Default::default(),
        });

        if !self.recording.is_empty() {
            objects.push(MessageBlock {
                block_type: MessageBlockType::Context,
                elements: vec![MessageBlockText {
                    text_type: MessageType::Markdown,
                    text: format!("<{}|Meeting recording>", self.recording),
                }],
                text: Default::default(),
                accessory: Default::default(),
                block_id: Default::default(),
                fields: Default::default(),
            });
        }

        for paper in self.papers.clone() {
            let p: NewJournalClubPaper = serde_json::from_str(&paper).unwrap();

            let mut title = p.title.to_string();
            if p.title == self.title {
                title = "Paper".to_string();
            }
            objects.push(MessageBlock {
                block_type: MessageBlockType::Context,
                elements: vec![MessageBlockText {
                    text_type: MessageType::Markdown,
                    text: format!("<{}|{}>", p.link, title),
                }],
                text: Default::default(),
                accessory: Default::default(),
                block_id: Default::default(),
                fields: Default::default(),
            });
        }

        json!(FormattedMessage {
            channel: Default::default(),
            attachments: Default::default(),
            blocks: objects,
        })
    }
}

/// Implement updating the Airtable record for a JournalClubMeeting.
#[async_trait]
impl UpdateAirtableRecord<JournalClubMeeting> for JournalClubMeeting {
    #[instrument]
    #[inline]
    async fn update_airtable_record(&mut self, record: JournalClubMeeting) {
        // Set the papers field, since it is pre-populated as table links.
        self.papers = record.papers;
    }
}

/// The data type for a NewJournalClubPaper.
#[db_struct {
    new_name = "JournalClubPaper",
    base_id = "AIRTABLE_BASE_ID_MISC",
    table = "AIRTABLE_JOURNAL_CLUB_PAPERS_TABLE",
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, Deserialize, Serialize)]
#[table_name = "journal_club_papers"]
pub struct NewJournalClubPaper {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub link: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub meeting: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub link_to_meeting: Vec<String>,
}

/// Implement updating the Airtable record for a JournalClubPaper.
#[async_trait]
impl UpdateAirtableRecord<JournalClubPaper> for JournalClubPaper {
    #[instrument]
    #[inline]
    async fn update_airtable_record(&mut self, _record: JournalClubPaper) {
        // Get the current journal club meetings in Airtable so we can link to it.
        // TODO: make this more dry so we do not call it every single damn time.
        let journal_club_meetings = JournalClubMeetings::get_from_airtable().await;

        // Iterate over the journal_club_meetings and see if we find a match.
        for (_id, meeting_record) in journal_club_meetings {
            if meeting_record.fields.issue == self.meeting {
                // Set the link_to_meeting to the right meeting.
                self.link_to_meeting = vec![meeting_record.id];
                // Break the loop and return early.
                break;
            }
        }
    }
}

/// The data type for a MailingListSubscriber.
#[db_struct {
    new_name = "MailingListSubscriber",
    base_id = "AIRTABLE_BASE_ID_CUSTOMER_LEADS",
    table = "AIRTABLE_MAILING_LIST_SIGNUPS_TABLE",
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[table_name = "mailing_list_subscribers"]
pub struct NewMailingListSubscriber {
    pub email: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub first_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub last_name: String,
    /// (generated) name is a combination of first_name and last_name.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub company: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub interest: String,
    #[serde(default)]
    pub wants_podcast_updates: bool,
    #[serde(default)]
    pub wants_newsletter: bool,
    #[serde(default)]
    pub wants_product_updates: bool,
    pub date_added: DateTime<Utc>,
    pub date_optin: DateTime<Utc>,
    pub date_last_changed: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// link to another table in Airtable
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub link_to_people: Vec<String>,
}

impl NewMailingListSubscriber {
    /// Push the mailing list signup to our Airtable workspace.
    #[instrument]
    #[inline]
    pub async fn push_to_airtable(&self) {
        // Initialize the Airtable client.
        let airtable = Airtable::new(api_key_from_env(), AIRTABLE_BASE_ID_CUSTOMER_LEADS, "");

        // Create the record.
        let record = Record {
            id: "".to_string(),
            created_time: Default::default(),
            fields: self.clone(),
        };

        // Send the new record to the Airtable client.
        // Batch can only handle 10 at a time.
        airtable.create_records(AIRTABLE_MAILING_LIST_SIGNUPS_TABLE, vec![record]).await.unwrap();

        println!("created mailing list record in Airtable: {:?}", self);
    }

    /// Get the human duration of time since the signup was fired.
    #[instrument]
    #[inline]
    pub fn human_duration(&self) -> HumanTime {
        let mut dur = self.date_added - Utc::now();
        if dur.num_seconds() > 0 {
            dur = -dur;
        }

        HumanTime::from(dur)
    }

    /// Convert the mailing list signup into JSON as Slack message.
    #[instrument]
    #[inline]
    pub fn as_slack_msg(&self) -> Value {
        let time = self.human_duration();

        let msg = format!("*{}* <mailto:{}|{}>", self.name, self.email, self.email);

        let mut interest: MessageBlock = Default::default();
        if !self.interest.is_empty() {
            interest = MessageBlock {
                block_type: MessageBlockType::Section,
                text: Some(MessageBlockText {
                    text_type: MessageType::Markdown,
                    text: format!("\n>{}", self.interest),
                }),
                elements: Default::default(),
                accessory: Default::default(),
                block_id: Default::default(),
                fields: Default::default(),
            };
        }

        let updates = format!(
            "podcast updates: _{}_ | newsletter: _{}_ | product updates: _{}_",
            self.wants_podcast_updates, self.wants_newsletter, self.wants_product_updates,
        );

        let mut context = "".to_string();
        if !self.company.is_empty() {
            context += &format!("works at {} | ", self.company);
        }
        context += &format!("subscribed to mailing list {}", time);

        json!(FormattedMessage {
            channel: Default::default(),
            attachments: Default::default(),
            blocks: vec![
                MessageBlock {
                    block_type: MessageBlockType::Section,
                    text: Some(MessageBlockText {
                        text_type: MessageType::Markdown,
                        text: msg,
                    }),
                    elements: Default::default(),
                    accessory: Default::default(),
                    block_id: Default::default(),
                    fields: Default::default(),
                },
                interest,
                MessageBlock {
                    block_type: MessageBlockType::Context,
                    elements: vec![MessageBlockText {
                        text_type: MessageType::Markdown,
                        text: updates,
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
                        text: context,
                    }],
                    text: Default::default(),
                    accessory: Default::default(),
                    block_id: Default::default(),
                    fields: Default::default(),
                }
            ],
        })
    }
}

impl Default for NewMailingListSubscriber {
    #[instrument]
    #[inline]
    fn default() -> Self {
        NewMailingListSubscriber {
            email: String::new(),
            first_name: String::new(),
            last_name: String::new(),
            name: String::new(),
            company: String::new(),
            interest: String::new(),
            wants_podcast_updates: false,
            wants_newsletter: false,
            wants_product_updates: false,
            date_added: Utc::now(),
            date_optin: Utc::now(),
            date_last_changed: Utc::now(),
            notes: String::new(),
            tags: Default::default(),
            link_to_people: Default::default(),
        }
    }
}

/// Implement updating the Airtable record for a MailingListSubscriber.
#[async_trait]
impl UpdateAirtableRecord<MailingListSubscriber> for MailingListSubscriber {
    #[instrument]
    #[inline]
    async fn update_airtable_record(&mut self, record: MailingListSubscriber) {
        // Set the link_to_people from the original so it stays intact.
        self.link_to_people = record.link_to_people;
    }
}

/// The data type for a GitHub user.
#[derive(Debug, Default, PartialEq, Clone, JsonSchema, FromSqlRow, AsExpression, Serialize, Deserialize)]
#[sql_type = "Jsonb"]
pub struct GitHubUser {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub login: String,
    #[serde(default)]
    pub id: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub username: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub email: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub avatar_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub gravatar_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub html_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub followers_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub following_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub gists_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub starred_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub subscriptions_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub organizations_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub repos_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub events_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub received_events_url: String,
    #[serde(default)]
    pub site_admin: bool,
}

impl FromSql<Jsonb, Pg> for GitHubUser {
    #[instrument]
    #[inline]
    fn from_sql(bytes: Option<&[u8]>) -> deserialize::Result<Self> {
        let value = <serde_json::Value as FromSql<Jsonb, Pg>>::from_sql(bytes)?;
        Ok(serde_json::from_value(value).unwrap())
    }
}

impl ToSql<Jsonb, Pg> for GitHubUser {
    fn to_sql<W: Write>(&self, out: &mut Output<W, Pg>) -> serialize::Result {
        let value = serde_json::to_value(self).unwrap();
        <serde_json::Value as ToSql<Jsonb, Pg>>::to_sql(&value, out)
    }
}

/// The data type for a GitHub repository.
#[db_struct {
    new_name = "GithubRepo",
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[table_name = "github_repos"]
pub struct NewRepo {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub github_id: String,
    pub owner: GitHubUser,
    pub name: String,
    pub full_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub private: bool,
    pub fork: bool,
    pub url: String,
    pub html_url: String,
    pub archive_url: String,
    pub assignees_url: String,
    pub blobs_url: String,
    pub branches_url: String,
    pub clone_url: String,
    pub collaborators_url: String,
    pub comments_url: String,
    pub commits_url: String,
    pub compare_url: String,
    pub contents_url: String,
    pub contributors_url: String,
    pub deployments_url: String,
    pub downloads_url: String,
    pub events_url: String,
    pub forks_url: String,
    pub git_commits_url: String,
    pub git_refs_url: String,
    pub git_tags_url: String,
    pub git_url: String,
    pub hooks_url: String,
    pub issue_comment_url: String,
    pub issue_events_url: String,
    pub issues_url: String,
    pub keys_url: String,
    pub labels_url: String,
    pub languages_url: String,
    pub merges_url: String,
    pub milestones_url: String,
    #[serde(default, deserialize_with = "deserialize_null_string::deserialize", skip_serializing_if = "String::is_empty")]
    pub mirror_url: String,
    pub notifications_url: String,
    pub pulls_url: String,
    pub releases_url: String,
    pub ssh_url: String,
    pub stargazers_url: String,
    pub statuses_url: String,
    pub subscribers_url: String,
    pub subscription_url: String,
    pub svn_url: String,
    pub tags_url: String,
    pub teams_url: String,
    pub trees_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub homepage: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub language: String,
    pub forks_count: i32,
    pub stargazers_count: i32,
    pub watchers_count: i32,
    pub size: i32,
    pub default_branch: String,
    pub open_issues_count: i32,
    pub has_issues: bool,
    pub has_wiki: bool,
    pub has_pages: bool,
    pub has_downloads: bool,
    pub archived: bool,
    #[serde(deserialize_with = "crate::configs::null_date_format::deserialize")]
    pub pushed_at: DateTime<Utc>,
    #[serde(deserialize_with = "crate::configs::null_date_format::deserialize")]
    pub created_at: DateTime<Utc>,
    #[serde(deserialize_with = "crate::configs::null_date_format::deserialize")]
    pub updated_at: DateTime<Utc>,
}

pub mod deserialize_null_string {
    use serde::{self, Deserialize, Deserializer};

    // The signature of a deserialize_with function must follow the pattern:
    //
    //    fn deserialize<'de, D>(D) -> Result<T, D::Error>
    //    where
    //        D: Deserializer<'de>
    //
    // although it may also be generic over the output types T.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<String, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer).unwrap_or_default();

        Ok(s)
    }
}

impl NewRepo {
    #[instrument]
    #[inline]
    pub async fn new(r: Repo) -> Self {
        // TODO: get the languages as well
        // https://docs.rs/hubcaps/0.6.1/hubcaps/repositories/struct.Repo.html

        let mut homepage = String::new();
        if r.homepage.is_some() {
            homepage = r.homepage.unwrap();
        }

        let mut description = String::new();
        if r.description.is_some() {
            description = r.description.unwrap();
        }

        let mut language = String::new();
        if r.language.is_some() {
            language = r.language.unwrap();
        }

        let mut mirror_url = String::new();
        if r.mirror_url.is_some() {
            mirror_url = r.mirror_url.unwrap();
        }

        NewRepo {
            github_id: r.id.to_string(),
            owner: GitHubUser {
                login: r.owner.login.to_string(),
                id: r.owner.id,
                name: r.owner.login.to_string(),
                username: r.owner.login,
                email: "".to_string(),
                avatar_url: r.owner.avatar_url,
                gravatar_id: r.owner.gravatar_id,
                url: r.owner.url,
                html_url: r.owner.html_url,
                followers_url: r.owner.followers_url,
                following_url: r.owner.following_url,
                gists_url: r.owner.gists_url,
                starred_url: r.owner.starred_url,
                subscriptions_url: r.owner.subscriptions_url,
                organizations_url: r.owner.organizations_url,
                repos_url: r.owner.repos_url,
                events_url: r.owner.events_url,
                received_events_url: r.owner.received_events_url,
                site_admin: r.owner.site_admin,
            },
            name: r.name,
            full_name: r.full_name,
            description,
            private: r.private,
            fork: r.fork,
            url: r.url,
            html_url: r.html_url,
            archive_url: r.archive_url,
            assignees_url: r.assignees_url,
            blobs_url: r.blobs_url,
            branches_url: r.branches_url,
            clone_url: r.clone_url,
            collaborators_url: r.collaborators_url,
            comments_url: r.comments_url,
            commits_url: r.commits_url,
            compare_url: r.compare_url,
            contents_url: r.contents_url,
            contributors_url: r.contributors_url,
            deployments_url: r.deployments_url,
            downloads_url: r.downloads_url,
            events_url: r.events_url,
            forks_url: r.forks_url,
            git_commits_url: r.git_commits_url,
            git_refs_url: r.git_refs_url,
            git_tags_url: r.git_tags_url,
            git_url: r.git_url,
            hooks_url: r.hooks_url,
            issue_comment_url: r.issue_comment_url,
            issue_events_url: r.issue_events_url,
            issues_url: r.issues_url,
            keys_url: r.keys_url,
            labels_url: r.labels_url,
            languages_url: r.languages_url,
            merges_url: r.merges_url,
            milestones_url: r.milestones_url,
            mirror_url,
            notifications_url: r.notifications_url,
            pulls_url: r.pulls_url,
            releases_url: r.releases_url,
            ssh_url: r.ssh_url,
            stargazers_url: r.stargazers_url,
            statuses_url: r.statuses_url,
            subscribers_url: r.subscribers_url,
            subscription_url: r.subscription_url,
            svn_url: r.svn_url,
            tags_url: r.tags_url,
            teams_url: r.teams_url,
            trees_url: r.trees_url,
            homepage,
            language,
            forks_count: r.forks_count.to_string().parse::<i32>().unwrap(),
            stargazers_count: r.stargazers_count.to_string().parse::<i32>().unwrap(),
            watchers_count: r.watchers_count.to_string().parse::<i32>().unwrap(),
            size: r.size.to_string().parse::<i32>().unwrap(),
            default_branch: r.default_branch,
            open_issues_count: r.open_issues_count.to_string().parse::<i32>().unwrap(),
            has_issues: r.has_issues,
            has_wiki: r.has_wiki,
            has_pages: r.has_pages,
            has_downloads: r.has_downloads,
            archived: r.archived,
            pushed_at: DateTime::parse_from_rfc3339(&r.pushed_at).unwrap().with_timezone(&Utc),
            created_at: DateTime::parse_from_rfc3339(&r.created_at).unwrap().with_timezone(&Utc),
            updated_at: DateTime::parse_from_rfc3339(&r.updated_at).unwrap().with_timezone(&Utc),
        }
    }
}

/// The data type for an RFD.
#[db_struct {
    new_name = "RFD",
    base_id = "AIRTABLE_BASE_ID_RACK_ROADMAP",
    table = "AIRTABLE_RFD_TABLE",
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[table_name = "rfds"]
pub struct NewRFD {
    // TODO: remove this alias when we update https://github.com/oxidecomputer/rfd/blob/master/.helpers/rfd.csv
    // When you do this you need to update src/components/images.js in the rfd repo as well.
    // those are the only two things remaining that parse the CSV directly.
    #[serde(alias = "num")]
    pub number: i32,
    /// (generated) number_string is the long version of the number with leading zeros
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub number_string: String,
    pub title: String,
    /// (generated) name is a combination of number and title.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    pub state: String,
    /// link is the canonical link to the source.
    pub link: String,
    /// (generated) short_link is the generated link in the form of https://{number}.rfd.oxide.computer
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub short_link: String,
    /// (generated) rendered_link is the link to the rfd in the rendered html website in the form of
    /// https://rfd.shared.oxide.computer/rfd/{{number_string}}
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub rendered_link: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub discussion: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub authors: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub html: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub content: String,
    /// sha is the SHA of the last commit that modified the file
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sha: String,
    /// commit_date is the date of the last commit that modified the file
    #[serde(default = "Utc::now")]
    pub commit_date: DateTime<Utc>,
    /// milestones only exist in Airtable
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub milestones: Vec<String>,
    /// relevant_components only exist in Airtable
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relevant_components: Vec<String>,
}

impl NewRFD {
    /// Return a NewRFD from a parsed file on a specific GitHub branch.
    #[instrument(skip(repo))]
    #[inline]
    pub async fn new_from_github(repo: &Repository, branch: &str, file_path: &str, commit_date: DateTime<Utc>) -> Self {
        // Get the file from GitHub.
        let file = repo.content().file(file_path, branch).await.unwrap();
        let content = from_utf8(&file.content).unwrap().trim().to_string();

        // Parse the RFD directory as an int.
        let (dir, _) = file_path.trim_start_matches("rfd/").split_once('/').unwrap();
        let number = dir.trim_start_matches('0').parse::<i32>().unwrap();

        let number_string = NewRFD::generate_number_string(number);

        // Parse the RFD title from the contents.
        let title = NewRFD::get_title(&content);

        // Parse the state from the contents.
        let state = NewRFD::get_state(&content);

        // Parse the discussion from the contents.
        let discussion = NewRFD::get_discussion(&content);

        NewRFD {
            number,
            number_string,
            title,
            name: Default::default(),
            state,
            link: file.html_url,
            short_link: Default::default(),
            rendered_link: Default::default(),
            discussion,
            authors: Default::default(),
            // We parse this below.
            html: Default::default(),
            content,
            sha: file.sha,
            commit_date,
            // Only exists in Airtable,
            milestones: Default::default(),
            // Only exists in Airtable,
            relevant_components: Default::default(),
        }
    }

    #[instrument]
    #[inline]
    pub fn get_title(content: &str) -> String {
        let mut re = Regex::new(r"(?m)(RFD .*$)").unwrap();
        match re.find(&content) {
            Some(v) => {
                // TODO: find less horrible way to do this.
                let trimmed = v.as_str().replace("RFD", "").replace("# ", "").replace("= ", " ").trim().to_string();

                let (_, s) = trimmed.split_once(' ').unwrap();

                // If the string is empty, it means there is no RFD in our
                // title.
                if s.is_empty() {}

                s.to_string()
            }
            None => {
                // There is no "RFD" in our title. This is the case for RFD 31.
                re = Regex::new(r"(?m)(^= .*$)").unwrap();
                let results = re.find(&content).unwrap();
                results.as_str().replace("RFD", "").replace("# ", "").replace("= ", " ").trim().to_string()
            }
        }
    }

    #[instrument]
    #[inline]
    pub fn get_state(content: &str) -> String {
        let re = Regex::new(r"(?m)(state:.*$)").unwrap();
        match re.find(&content) {
            Some(v) => return v.as_str().replace("state:", "").trim().to_string(),
            None => Default::default(),
        }
    }

    #[instrument]
    #[inline]
    pub fn get_discussion(content: &str) -> String {
        let re = Regex::new(r"(?m)(discussion:.*$)").unwrap();
        match re.find(&content) {
            Some(v) => return v.as_str().replace("discussion:", "").trim().to_string(),
            None => Default::default(),
        }
    }

    #[instrument]
    #[inline]
    pub fn generate_number_string(number: i32) -> String {
        // Add leading zeros to the number for the number_string.
        let mut number_string = number.to_string();
        while number_string.len() < 4 {
            number_string = format!("0{}", number_string);
        }

        number_string
    }

    #[instrument]
    #[inline]
    pub fn generate_name(number: i32, title: &str) -> String {
        format!("RFD {} {}", number, title)
    }

    #[instrument]
    #[inline]
    pub fn generate_short_link(number: i32) -> String {
        format!("https://{}.rfd.oxide.computer", number)
    }

    #[instrument]
    #[inline]
    pub fn generate_rendered_link(number_string: &str) -> String {
        format!("https://rfd.shared.oxide.computer/rfd/{}", number_string)
    }

    #[instrument]
    #[inline]
    pub fn get_authors(content: &str, is_markdown: bool) -> String {
        if is_markdown {
            // TODO: make work w asciidoc.
            let re = Regex::new(r"(?m)(^authors.*$)").unwrap();
            match re.find(&content) {
                Some(v) => return v.as_str().replace("authors:", "").trim().to_string(),
                None => Default::default(),
            }
        }

        // We must have asciidoc content.
        // We want to find the line under the first "=" line (which is the title), authors is under
        // that.
        let re = Regex::new(r"(?m:^=.*$)[\n\r](?m)(.*$)").unwrap();
        match re.find(&content) {
            Some(v) => {
                let val = v.as_str().trim().to_string();
                let parts: Vec<&str> = val.split('\n').collect();
                if parts.len() < 2 {
                    Default::default()
                } else {
                    parts[1].to_string()
                }
            }
            None => Default::default(),
        }
    }
}

impl RFD {
    #[instrument(skip(repo))]
    #[inline]
    pub async fn get_html(&self, repo: &Repository, branch: &str, is_markdown: bool) -> String {
        let html: String;
        if is_markdown {
            // Parse the markdown.
            html = parse_markdown(&self.content);
        } else {
            // Parse the acsiidoc.
            html = self.parse_asciidoc(repo, branch).await;
        }

        clean_rfd_html_links(&html, &self.number_string)
    }

    #[instrument(skip(repo))]
    #[inline]
    pub async fn parse_asciidoc(&self, repo: &Repository, branch: &str) -> String {
        let dir = format!("rfd/{}", self.number_string);

        // Create the temporary directory.
        let mut path = env::temp_dir();
        path.push("asciidoc-temp/");
        let pparent = path.clone();
        let parent = pparent.as_path().to_str().unwrap().trim_end_matches('/');
        path.push("contents.adoc");

        // Write the contents to a temporary file.
        write_file(&path, &self.content);

        // If the file contains inline images, we need to save those images locally.
        // TODO: we don't need to save all the images, only the inline ones, clean this up
        // eventually.
        if self.content.contains("[opts=inline]") {
            let images = get_images_in_branch(repo, &dir, branch).await;
            for image in images {
                // Save the image to our temporary directory.
                let image_path = format!("{}/{}", parent, image.path.replace(&dir, "").trim_start_matches('/'));

                write_file(&PathBuf::from(image_path), from_utf8(&image.content).unwrap_or_default());
            }
        }

        let cmd_output = Command::new("asciidoctor").args(&["-o", "-", "--no-header-footer", path.to_str().unwrap()]).output().unwrap();

        let result = if cmd_output.status.success() {
            from_utf8(&cmd_output.stdout).unwrap()
        } else {
            println!("[rfds] running asciidoctor failed:");
            stdout().write_all(&cmd_output.stdout).unwrap();
            stderr().write_all(&cmd_output.stderr).unwrap();

            Default::default()
        };

        // Delete the parent directory.
        let pdir = Path::new(parent);
        if pdir.exists() && pdir.is_dir() {
            fs::remove_dir_all(pdir).unwrap();
        }

        result.to_string()
    }

    /// Convert an RFD into JSON as Slack message.
    // TODO: make this include more fields
    #[instrument]
    #[inline]
    pub fn as_slack_msg(&self) -> String {
        let mut msg = format!("{} (_*{}*_) <{}|github> <{}|rendered>", self.name, self.state, self.short_link, self.rendered_link);

        if !self.discussion.is_empty() {
            msg += &format!(" <{}|discussion>", self.discussion);
        }

        msg
    }

    /// Get the filename for the PDF of the RFD.
    #[instrument]
    #[inline]
    pub fn get_pdf_filename(&self) -> String {
        format!("RFD {} {}.pdf", self.number_string, self.title.replace("/", "-").replace("'", "").replace(":", "").trim())
    }

    /// Update an RFDs state.
    #[instrument]
    #[inline]
    pub fn update_state(&mut self, state: &str, is_markdown: bool) {
        self.content = update_state(&self.content, state, is_markdown);
        self.state = state.to_string();
    }

    /// Update an RFDs discussion link.
    #[instrument]
    #[inline]
    pub fn update_discussion(&mut self, link: &str, is_markdown: bool) {
        self.content = update_discussion_link(&self.content, link, is_markdown);
        self.discussion = link.to_string();
    }

    /// Convert the RFD content to a PDF and upload the PDF to the /pdfs folder of the RFD
    /// repository.
    #[instrument(skip(drive_client))]
    #[inline]
    pub async fn convert_and_upload_pdf(&self, github: &Github, drive_client: &GoogleDrive, drive_id: &str, parent_id: &str) {
        // Get the rfd repo client.
        let rfd_repo = github.repo(github_org(), "rfd");
        let repo = rfd_repo.get().await.unwrap();

        let mut path = env::temp_dir();
        path.push(format!("pdfcontents{}.adoc", self.number_string));

        let mut workspace = env::var("GITHUB_WORKSPACE").unwrap_or_else(|_| "..".to_string());
        workspace = workspace.trim_end_matches('/').to_string();

        // Fix the path for images.
        // TODO: this only fixes asciidoc images, not markdown.
        let rfd_content = self.content.replace("image::", &format!("image::{}/rfd/src/public/static/images/{}/", workspace, self.number_string));

        // Write the contents to a temporary file.
        let mut file = fs::File::create(path.clone()).unwrap();
        file.write_all(rfd_content.as_bytes()).unwrap();

        let file_name = self.get_pdf_filename();
        let rfd_path = format!("/pdfs/{}", file_name);

        let cmd_output = Command::new("asciidoctor-pdf")
            .args(&["-o", "-", "-a", "source-highlighter=rouge", path.to_str().unwrap()])
            .output()
            .unwrap();

        if !cmd_output.status.success() {
            println!("[rfdpdf] running asciidoctor failed:");
            stdout().write_all(&cmd_output.stdout).unwrap();
            stderr().write_all(&cmd_output.stderr).unwrap();
            return;
        }

        // Create or update the file in the github repository.
        create_or_update_file_in_github_repo(&rfd_repo, &repo.default_branch, &rfd_path, cmd_output.stdout.clone()).await;

        // Create or update the file in the google_drive.
        drive_client
            .create_or_upload_file(drive_id, parent_id, &file_name, "application/pdf", &cmd_output.stdout)
            .await
            .unwrap();

        // Delete our temporary file.
        if path.exists() && !path.is_dir() {
            fs::remove_file(path).unwrap();
        }
    }

    /// Expand the fields in the RFD.
    /// This will get the content, html, sha, commit_date as well as fill in all generated fields.
    #[instrument]
    #[inline]
    pub async fn expand(&mut self, github: &Github) {
        let repo = github.repo(github_org(), "rfd");
        let r = repo.get().await.unwrap();

        // Trim the title.
        self.title = self.title.trim().to_string();

        // Add leading zeros to the number for the number_string.
        self.number_string = NewRFD::generate_number_string(self.number);

        // Set the full name.
        self.name = NewRFD::generate_name(self.number, &self.title);

        // Set the short_link.
        self.short_link = NewRFD::generate_short_link(self.number);
        // Set the rendered_link.
        self.rendered_link = NewRFD::generate_rendered_link(&self.number_string);

        let mut branch = self.number_string.to_string();
        if self.link.contains(&format!("/{}/", r.default_branch)) {
            branch = r.default_branch.to_string();
        }

        // Get the RFD contents from the branch.
        let rfd_dir = format!("/rfd/{}", self.number_string);
        let (rfd_content, is_markdown, sha) = get_rfd_contents_from_repo(github, &branch, &rfd_dir).await;
        self.content = rfd_content;
        self.sha = sha;

        if branch == r.default_branch {
            // Get the commit date.
            let commits = repo.commits().list(&rfd_dir).await.unwrap();
            let commit = commits.get(0).unwrap();
            self.commit_date = commit.commit.author.date;
        } else {
            // Get the branch.
            let commit = repo.commits().get(&branch).await.unwrap();
            // TODO: we should not have to duplicate this code below
            // but the references were mad...
            self.commit_date = commit.commit.author.date;
        }

        // Parse the HTML.
        self.html = self.get_html(&repo, &branch, is_markdown).await;

        self.authors = NewRFD::get_authors(&self.content, is_markdown);
    }
}

/// Implement updating the Airtable record for an RFD.
#[async_trait]
impl UpdateAirtableRecord<RFD> for RFD {
    #[instrument]
    #[inline]
    async fn update_airtable_record(&mut self, record: RFD) {
        // Set the Link to People from the original so it stays intact.
        self.milestones = record.milestones.clone();
        self.relevant_components = record.relevant_components;
        // Airtable can only hold 100,000 chars. IDK which one is that long but LOL
        // https://community.airtable.com/t/what-is-the-long-text-character-limit/1780
        self.content = truncate(&self.content, 100000);
        self.html = truncate(&self.html, 100000);
    }
}

#[instrument]
#[inline]
fn truncate(s: &str, max_chars: usize) -> String {
    match s.char_indices().nth(max_chars) {
        None => s.to_string(),
        Some((idx, _)) => s[..idx].to_string(),
    }
}

#[instrument]
#[inline]
pub fn get_value(map: &HashMap<String, Vec<String>>, key: &str) -> String {
    let empty: Vec<String> = Default::default();
    let a = map.get(key).unwrap_or(&empty);

    if a.is_empty() {
        return Default::default();
    }

    a.get(0).unwrap().to_string()
}
