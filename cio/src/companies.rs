use std::convert::TryInto;
use std::env;
use std::fs;
use std::io::Write;

use async_trait::async_trait;
use chrono::Utc;
use docusign::DocuSign;
use gusto_api::Gusto;
use hubcaps::http_cache::FileBasedCache;
use hubcaps::{Credentials, Github, InstallationTokenGenerator, JWTCredentials};
use macros::db;
use quickbooks::QuickBooks;
use ramp_api::Ramp;
use reqwest::{header, Client};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use yup_oauth2::{read_service_account_key, AccessToken, ServiceAccountAuthenticator};

use crate::airtable::{AIRTABLE_BASE_ID_CIO, AIRTABLE_COMPANIES_TABLE};
use crate::api_tokens::{APIToken, NewAPIToken};
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
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub okta_domain: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub mailchimp_list_id: String,
    #[serde(default)]
    pub github_app_installation_id: i32,
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

    pub fn get_from_mailchimp_list_id(db: &Database, list_id: &str) -> Self {
        companys::dsl::companys
            .filter(companys::dsl::mailchimp_list_id.eq(list_id.to_string()))
            .first::<Company>(&db.conn())
            .unwrap()
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
        // Get the APIToken from the database.
        /* if let Some(mut t) = APIToken::get_from_db(db, self.id, "google".to_string()) {
            let nt = refresh_google_access_token(db, t).await.unwrap();
            return nt.access_token.to_string();
        }

        "".to_string()*/

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
        let token = auth.token(&get_google_scopes()).await.expect("failed to get token");

        if token.as_str().is_empty() {
            panic!("empty token is not valid");
        }

        token
    }

    /// Authenticate GitHub with JSON web token credentials, for an application installation.
    pub fn authenticate_github(&self) -> Github {
        // Parse our env variables.
        let app_id_str = env::var("GH_APP_ID").unwrap();
        let app_id = app_id_str.parse::<u64>().unwrap();

        let encoded_private_key = env::var("GH_PRIVATE_KEY").unwrap();
        let private_key = base64::decode(encoded_private_key).unwrap();

        // Decode the key.
        let key = nom_pem::decode_block(&private_key).unwrap();

        // Get the JWT credentials.
        let jwt = JWTCredentials::new(app_id, key.data).unwrap();

        // Create the HTTP cache.
        let http_cache = Box::new(FileBasedCache::new(format!("{}/.cache/github", env::var("HOME").unwrap())));

        let token_generator = InstallationTokenGenerator::new(self.github_app_installation_id.try_into().unwrap(), jwt);

        Github::custom(
            "https://api.github.com",
            concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")),
            Credentials::InstallationToken(token_generator),
            Client::builder().build().unwrap(),
            http_cache,
        )
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

pub async fn get_google_consent_url() -> String {
    let secret = get_google_credentials().await;
    format!(
        "https://accounts.google.com/o/oauth2/v2/auth?response_type=code&client_id={}&redirect_uri={}&scope={}",
        secret.client_id,
        secret.redirect_uris[0],
        get_google_scopes().join(" ")
    )
}

pub async fn get_google_credentials() -> yup_oauth2::ApplicationSecret {
    let google_key = env::var("GOOGLE_CIO_KEY_ENCODED").unwrap_or_default();
    let b = base64::decode(google_key).unwrap();
    // Save the google key to a tmp file.
    let mut file_path = env::temp_dir();
    file_path.push("google_key.json");
    // Create the file and write to it.
    let mut file = fs::File::create(file_path.clone()).unwrap();
    file.write_all(&b).unwrap();
    // Set the Google credential file to the temp path.
    let google_credential_file = file_path.to_str().unwrap().to_string();

    yup_oauth2::read_application_secret(google_credential_file).await.expect("failed to read google credential file")
}

pub fn get_google_scopes() -> Vec<String> {
    vec![
        "https://www.googleapis.com/auth/admin.directory.group".to_string(),
        "https://www.googleapis.com/auth/admin.directory.resource.calendar".to_string(),
        "https://www.googleapis.com/auth/admin.directory.user".to_string(),
        "https://www.googleapis.com/auth/calendar".to_string(),
        "https://www.googleapis.com/auth/apps.groups.settings".to_string(),
        "https://www.googleapis.com/auth/spreadsheets".to_string(),
        "https://www.googleapis.com/auth/drive".to_string(),
    ]
}

pub async fn get_google_access_token(db: &Database, code: &str) {
    let secret = get_google_credentials().await;

    let mut headers = header::HeaderMap::new();
    headers.append(header::ACCEPT, header::HeaderValue::from_static("application/json"));

    let params = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", &secret.redirect_uris[0]),
        ("client_id", &secret.client_id),
        ("client_secret", &secret.client_secret),
    ];
    let client = Client::new();
    let resp = client.post("https://oauth2.googleapis.com/token").headers(headers).form(&params).send().await.unwrap();

    // Unwrap the response.
    let t: ramp_api::AccessToken = resp.json().await.unwrap();

    // Save the token to the database.
    let mut token = NewAPIToken {
        product: "google".to_string(),
        token_type: t.token_type.to_string(),
        access_token: t.access_token.to_string(),
        expires_in: t.expires_in as i32,
        refresh_token: t.refresh_token.to_string(),
        refresh_token_expires_in: t.refresh_token_expires_in as i32,
        company_id: "".to_string(),
        item_id: "".to_string(),
        user_email: "".to_string(),
        last_updated_at: Utc::now(),
        expires_date: None,
        refresh_token_expires_date: None,
        endpoint: "".to_string(),
        // TODO: fill this in.
        cio_company_id: 1,
    };
    token.expand();

    // Update it in the database.
    token.upsert(db).await;
}

pub async fn refresh_google_access_token(db: &Database, mut t: APIToken) -> APIToken {
    let secret = get_google_credentials().await;

    let mut headers = header::HeaderMap::new();
    headers.append(header::ACCEPT, header::HeaderValue::from_static("application/json"));

    let params = [
        ("grant_type", "refresh_token"),
        ("refresh_token", &t.refresh_token),
        ("client_id", &secret.client_id),
        ("client_secret", &secret.client_secret),
    ];
    let client = Client::new();
    let resp = client.post("https://oauth2.googleapis.com/token").headers(headers).form(&params).send().await.unwrap();

    // Unwrap the response.
    let nt: ramp_api::AccessToken = resp.json().await.unwrap();

    t.access_token = nt.access_token.to_string();
    t.expires_in = nt.expires_in as i32;
    t.refresh_token = nt.refresh_token.to_string();
    t.refresh_token_expires_in = nt.refresh_token_expires_in as i32;
    t.last_updated_at = Utc::now();
    t.expand();

    // Update the token in the database.
    t.update(&db).await;

    t
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
