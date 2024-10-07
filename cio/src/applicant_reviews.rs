use anyhow::Result;
use async_bb8_diesel::AsyncRunQueryDsl;
use async_trait::async_trait;
use macros::db;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    airtable::AIRTABLE_REVIEWS_TABLE,
    applicants::{ApplicantReviewer, NewApplicantReviewer},
    companies::Company,
    configs::User,
    core::UpdateAirtableRecord,
    db::Database,
    schema::applicant_reviews,
};

#[db {
    new_struct_name = "ApplicantReview",
    match_on = {
        "name" = "String",
        "applicant" = "Vec<String>",
    },
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[diesel(table_name = applicant_reviews)]
pub struct NewApplicantReview {
    // TODO: We don't have to do this crazy rename after we update to not use the
    // Airtable form.
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "Name", alias = "name")]
    pub name: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "Value Reflected (from Questionnaire)",
        alias = "value_reflected"
    )]
    pub value_reflected: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "Value Violated (from Questionnaire)",
        alias = "value_violated"
    )]
    pub value_violated: String,
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        rename = "Values in Tension (from Questionnaire)",
        alias = "values_in_tension"
    )]
    pub values_in_tension: Vec<String>,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "Evaluation",
        alias = "evaluation"
    )]
    pub evaluation: String,

    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        rename = "If \"Pass\" or \"No\", rationale if applicable (check all that apply)",
        alias = "rationale"
    )]
    pub rationale: Vec<String>,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "Any additional evaluation (not to be shared with applicant)",
        alias = "notes"
    )]
    pub notes: String,

    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        serialize_with = "airtable_api::user_format_as_string::serialize",
        deserialize_with = "airtable_api::user_format_as_string::deserialize",
        rename = "Reviewer",
        alias = "reviewer"
    )]
    pub reviewer: String,
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        rename = "Applicant",
        alias = "applicant"
    )]
    pub applicant: Vec<String>,

    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        rename = "Link to Leaderboard",
        alias = "link_to_leaderboard"
    )]
    pub link_to_leaderboard: Vec<String>,

    /// The CIO company ID.
    #[serde(default)]
    pub cio_company_id: i32,
}

impl ApplicantReview {
    pub async fn expand(&mut self, db: &Database) -> Result<()> {
        let company = self.company(db).await?;

        // We need to get the person from the leaderboard that matches this reviewer.
        if let Some(reviewer) = ApplicantReviewer::get_from_db(db, self.reviewer.to_string()).await {
            // Set this to the link to leaderboard.
            self.link_to_leaderboard = vec![reviewer.airtable_record_id];
        } else if let Some(user) = User::get_from_db(
            db,
            company.id,
            self.reviewer
                .trim_end_matches(&company.gsuite_domain)
                .trim_end_matches('@')
                .to_string(),
        )
        .await
        {
            // We need to addd them to the leaderboard.
            let reviewer = NewApplicantReviewer {
                name: user.full_name(),
                email: self.reviewer.to_string(),
                evaluations: 0,
                emphatic_yes: 0,
                yes: 0,
                pass: 0,
                no: 0,
                not_applicable: 0,
                cio_company_id: self.cio_company_id,
            };

            // Upsert the applicant reviewer in the database.
            reviewer.upsert(db).await?;
        }

        Ok(())
    }
}
