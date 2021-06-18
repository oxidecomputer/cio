use std::env;
use std::fs;
use std::io::Write;

use async_trait::async_trait;
use chrono::Utc;
use docusign::DocuSign;
use gusto_api::Gusto;
use macros::db;
use quickbooks::QuickBooks;
use ramp_api::Ramp;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use yup_oauth2::{read_service_account_key, AccessToken, ServiceAccountAuthenticator};

use crate::airtable::{AIRTABLE_BASE_ID_CIO, AIRTABLE_COMPANIES_TABLE};
use crate::api_tokens::APIToken;
use crate::core::UpdateAirtableRecord;
use crate::db::Database;
use crate::schema::companys;

#[db {
    new_struct_name = "Company",
    airtable_base_id = "AIRTABLE_BASE_ID_CIO",
    airtable_table = "AIRTABLE_COMPANIES_TABLE",
    match_on = {
        "name" = "String",
    },
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[table_name = "companys"]
pub struct NewCompany {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub gsuite_domain: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub github_org: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub website: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub domain: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub gsuite_account_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub gsuite_subject: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub phone: String,
}

/// Implement updating the Airtable record for a Company.
#[async_trait]
impl UpdateAirtableRecord<Company> for Company {
    async fn update_airtable_record(&mut self, _record: Company) {}
}

impl Company {
    pub fn get_from_github_org(db: &Database, org: &str) -> Self {
        companys::dsl::companys.filter(companys::dsl::github_org.eq(org.to_string())).first::<Company>(&db.conn()).unwrap()
    }

    /// Authenticate with Ramp.
    pub async fn authenticate_ramp(&self, db: &Database) -> Ramp {
        // Get the APIToken from the database.
        if let Some(mut t) = APIToken::get_from_db(db, self.id, "ramp".to_string()) {
            // Initialize the Ramp client.
            let mut ramp = Ramp::new_from_env(t.access_token, t.refresh_token.to_string());
            let nt = ramp.refresh_access_token().await.unwrap();
            t.access_token = nt.access_token.to_string();
            t.expires_in = nt.expires_in as i32;
            t.last_updated_at = Utc::now();
            if !nt.refresh_token.is_empty() {
                t.refresh_token = nt.refresh_token.to_string();
            }
            if nt.refresh_token_expires_in > 0 {
                t.refresh_token_expires_in = nt.refresh_token_expires_in as i32;
            }
            t.expand();
            // Update the token in the database.
            t.update(&db).await;

            return ramp;
        }

        Ramp::new_from_env("", "")
    }

    /// Authenticate with DocuSign.
    pub async fn authenticate_docusign(&self, db: &Database) -> DocuSign {
        // Get the APIToken from the database.
        if let Some(mut t) = APIToken::get_from_db(db, self.id, "docusign".to_string()) {
            // Initialize the DocuSign client.
            let mut ds = DocuSign::new_from_env(t.access_token, t.refresh_token, t.company_id.to_string(), t.endpoint.to_string());
            let nt = ds.refresh_access_token().await.unwrap();
            t.access_token = nt.access_token.to_string();
            t.expires_in = nt.expires_in as i32;
            t.refresh_token = nt.refresh_token.to_string();
            t.refresh_token_expires_in = nt.x_refresh_token_expires_in as i32;
            t.last_updated_at = Utc::now();
            t.expand();
            // Update the token in the database.
            t.update(&db).await;

            return ds;
        }

        DocuSign::new_from_env("", "", "", "")
    }

    /// Authenticate with Gusto.
    pub async fn authenticate_gusto(&self, db: &Database) -> Gusto {
        // Get the APIToken from the database.
        if let Some(mut t) = APIToken::get_from_db(db, self.id, "gusto".to_string()) {
            // Initialize the Gusto client.
            let mut gusto = Gusto::new_from_env(t.access_token, t.refresh_token, t.company_id.to_string());
            let nt = gusto.refresh_access_token().await.unwrap();
            t.access_token = nt.access_token.to_string();
            t.expires_in = nt.expires_in as i32;
            t.refresh_token = nt.refresh_token.to_string();
            t.refresh_token_expires_in = nt.x_refresh_token_expires_in as i32;
            t.last_updated_at = Utc::now();
            t.expand();
            // Update the token in the database.
            t.update(&db).await;

            return gusto;
        }

        Gusto::new_from_env("", "", "")
    }

    /// Authenticate with QuickBooks.
    pub async fn authenticate_quickbooks(&self, db: &Database) -> QuickBooks {
        // Get the APIToken from the database.
        if let Some(mut t) = APIToken::get_from_db(db, self.id, "quickbooks".to_string()) {
            // Initialize the QuickBooks client.
            let mut qb = QuickBooks::new_from_env(t.company_id.to_string(), t.access_token, t.refresh_token);
            let nt = qb.refresh_access_token().await.unwrap();
            t.access_token = nt.access_token.to_string();
            t.expires_in = nt.expires_in as i32;
            t.refresh_token = nt.refresh_token.to_string();
            t.refresh_token_expires_in = nt.x_refresh_token_expires_in as i32;
            t.last_updated_at = Utc::now();
            t.expand();
            // Update the token in the database.
            t.update(&db).await;

            return qb;
        }

        QuickBooks::new_from_env("", "", "")
    }

    /// Get a Google token.
    pub async fn get_google_token(&self, subject: &str) -> AccessToken {
        let gsuite_key = env::var("GSUITE_KEY_ENCODED").unwrap_or_default();
        // Get the GSuite credentials file.
        let mut gsuite_credential_file = env::var("GADMIN_CREDENTIAL_FILE").unwrap_or_default();

        if gsuite_credential_file.is_empty() && !gsuite_key.is_empty() {
            let b = base64::decode(gsuite_key).unwrap();

            // Save the gsuite key to a tmp file.
            let mut file_path = env::temp_dir();
            file_path.push("gsuite_key.json");

            // Create the file and write to it.
            let mut file = fs::File::create(file_path.clone()).unwrap();
            file.write_all(&b).unwrap();

            // Set the GSuite credential file to the temp path.
            gsuite_credential_file = file_path.to_str().unwrap().to_string();
        }

        let mut gsuite_subject = self.gsuite_subject.to_string();
        if !subject.is_empty() {
            gsuite_subject = subject.to_string();
        }
        let gsuite_secret = read_service_account_key(gsuite_credential_file).await.expect("failed to read gsuite credential file");
        let auth = ServiceAccountAuthenticator::builder(gsuite_secret)
            .subject(gsuite_subject.to_string())
            .build()
            .await
            .expect("failed to create authenticator");

        // Add the scopes to the secret and get the token.
        let token = auth
            .token(&[
                "https://www.googleapis.com/auth/admin.directory.group",
                "https://www.googleapis.com/auth/admin.directory.resource.calendar",
                "https://www.googleapis.com/auth/admin.directory.user",
                "https://www.googleapis.com/auth/calendar",
                "https://www.googleapis.com/auth/apps.groups.settings",
                "https://www.googleapis.com/auth/spreadsheets",
                "https://www.googleapis.com/auth/drive",
                "https://www.googleapis.com/auth/devstorage.full_control",
            ])
            .await
            .expect("failed to get token");

        if token.as_str().is_empty() {
            panic!("empty token is not valid");
        }

        token
    }
}

pub async fn refresh_companies() {
    let db = Database::new();

    let is: Vec<airtable_api::Record<Company>> = Company::airtable().list_records(&Company::airtable_table(), "Grid view", vec![]).await.unwrap();

    for record in is {
        if record.fields.name.is_empty() || record.fields.website.is_empty() {
            // Ignore it, it's a blank record.
            continue;
        }

        let new_company: NewCompany = record.fields.into();

        let mut company = new_company.upsert_in_db(&db);
        if company.airtable_record_id.is_empty() {
            company.airtable_record_id = record.id;
        }
        company.update(&db).await;
    }
    Companys::get_from_db(&db).update_airtable().await;
}

#[cfg(test)]
mod tests {
    use crate::companies::refresh_companies;

    #[ignore]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_cron_companies() {
        refresh_companies().await;
    }
}
