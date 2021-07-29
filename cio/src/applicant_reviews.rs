use async_trait::async_trait;
use macros::db;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    airtable::AIRTABLE_REVIEWS_TABLE, applicants::ApplicantReviewer, companies::Company,
    core::UpdateAirtableRecord, db::Database, schema::applicant_reviews,
};

#[db {
    new_struct_name = "ApplicantReview",
    airtable_base = "hiring",
    airtable_table = "AIRTABLE_REVIEWS_TABLE",
    match_on = {
        "name" = "String",
    },
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[table_name = "applicant_reviews"]
pub struct NewApplicantReview {
    // TODO: We don't have to do this crazy rename after we update to not use the
    // Airtable form.
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "Name",
        alias = "name"
    )]
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
        rename = "Values in Tension (from Questionnaire)"
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
        rename = "If \"Pass\" or \"No\", rationale if applicable (check all that apply)"
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
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "Applicant")]
    pub applicant: Vec<String>,

    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        rename = "Link to Leaderboard"
        alias = "link_to_leaderboard"
    )]
    pub link_to_leaderboard: Vec<String>,

    /// The CIO company ID.
    #[serde(default)]
    pub cio_company_id: i32,
}

/// Implement updating the Airtable record for a ApplicantReview.
#[async_trait]
impl UpdateAirtableRecord<ApplicantReview> for ApplicantReview {
    async fn update_airtable_record(&mut self, _record: ApplicantReview) {
        // Set name to empty since it is a function we cannot update it.
        self.name = "".to_string();
    }
}

impl ApplicantReview {
    pub fn expand(&mut self, db: &Database) {
        // We need to get the person from the leaderboard that matches this reviewer.
        let reviewer = ApplicantReviewer::get_from_db(db, self.reviewer.to_string()).unwrap();
        // Set this to the link to leaderboard.
        self.link_to_leaderboard = vec![reviewer.airtable_record_id];
    }
}

pub async fn refresh_reviews(db: &Database, company: &Company) {
    let is: Vec<airtable_api::Record<ApplicantReview>> = company
        .authenticate_airtable(&company.airtable_base_id_hiring)
        .list_records(&ApplicantReview::airtable_table(), "Grid view", vec![])
        .await
        .unwrap();

    for record in is {
        if record.fields.name.is_empty() || record.fields.applicant.is_empty() {
            // Ignore it, it's a blank record.
            continue;
        }

        let new_review: NewApplicantReview = record.fields.into();

        let mut review = new_review.upsert_in_db(db);
        if review.airtable_record_id.is_empty() {
            review.airtable_record_id = record.id;
        }
        review.cio_company_id = company.id;

        review.expand(db);

        review.update(db).await;
    }

    // Update them all from the database.
    ApplicantReviews::get_from_db(db, company.id)
        .update_airtable(db)
        .await;
}

#[cfg(test)]
mod tests {
    use crate::{applicant_reviews::refresh_reviews, companies::Company, db::Database};

    #[ignore]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_applicant_reviews() {
        let db = Database::new();

        let oxide = Company::get_from_db(&db, "Oxide".to_string()).unwrap();

        refresh_reviews(&db, &oxide).await;
    }
}
