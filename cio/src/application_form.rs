use chrono::Utc;
use google_drive::GoogleDrive;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{applicants::NewApplicant, companies::Company, db::Database};

#[derive(Debug, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
pub struct ApplicationForm {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub phone: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub role: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub interested_in: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub location: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub github: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub linkedin: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub portfolio: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub website: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub resume: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub materials: String,
    #[serde(default)]
    pub cio_company_id: i32,
}

impl ApplicationForm {
    pub async fn do_form(&self, db: &Database) {
        // If their email is empty return early.
        if self.email.is_empty()
            || self.name.is_empty()
            || self.role.is_empty()
            || self.materials.is_empty()
            || self.resume.is_empty()
            || self.phone.is_empty()
        {
            // This should not happen since we verify on the client side we have these
            // things.
            return;
        }

        // Convert the application form to an applicant.
        let new_applicant: NewApplicant = self.clone().into();

        // Add the applicant to the database.
        let mut applicant = new_applicant.upsert(db).await;

        let company = Company::get_by_id(db, self.cio_company_id);

        // Get the GSuite token.
        let token = company.authenticate_google(db).await;

        // Initialize the GSuite sheets client.
        let drive_client = GoogleDrive::new(token.clone());

        let github = company.authenticate_github();

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
            .await
            .unwrap();

        // Expand the application.
        applicant
            .expand(db, &drive_client, &github, &configs_issues)
            .await;

        // Update airtable and the database again.
        applicant.update(db).await;
    }
}

impl From<ApplicationForm> for NewApplicant {
    fn from(form: ApplicationForm) -> Self {
        NewApplicant {
            submitted_time: Utc::now(),
            role: form.role.to_string(),
            interested_in: form.interested_in,
            sheet_id: "".to_string(),
            name: form.name.to_string(),
            email: form.email.to_string(),
            location: form.location.to_string(),
            latitude: Default::default(),
            longitude: Default::default(),
            phone: form.phone.to_string(),
            country_code: Default::default(),
            github: form.github.to_string(),
            gitlab: "".to_string(),
            linkedin: form.linkedin.to_string(),
            portfolio: form.portfolio.to_string(),
            website: form.website.to_string(),
            resume: form.resume.to_string(),
            materials: form.materials.to_string(),
            status: crate::applicant_status::Status::NeedsToBeTriaged.to_string(),
            raw_status: Default::default(),
            sent_email_received: false,
            sent_email_follow_up: false,
            rejection_sent_date_time: None,
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
            interviews_started: Default::default(),
            interviews_completed: Default::default(),
            scorers: Default::default(),
            scorers_completed: Default::default(),
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
            criminal_background_check_status: Default::default(),
            motor_vehicle_background_check_status: Default::default(),
            start_date: None,
            geocode_cache: Default::default(),
            docusign_envelope_id: Default::default(),
            docusign_envelope_status: Default::default(),
            offer_created: Default::default(),
            offer_completed: Default::default(),
            docusign_piia_envelope_id: Default::default(),
            docusign_piia_envelope_status: Default::default(),
            piia_envelope_created: Default::default(),
            piia_envelope_completed: Default::default(),
            link_to_reviews: Default::default(),
            cio_company_id: form.cio_company_id,
        }
    }
}
