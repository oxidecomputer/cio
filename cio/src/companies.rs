use std::{convert::TryInto, env, fs, io::Write};

use airtable_api::Airtable;
use async_trait::async_trait;
use checkr::Checkr;
use chrono::Utc;
use cloudflare::framework::{
    async_api::Client as CloudflareClient, auth::Credentials as CloudflareCredentials, Environment,
    HttpApiClientConfig,
};
use docusign::DocuSign;
use gusto_api::Gusto;
use macros::db;
use mailchimp_api::MailChimp;
use octorust::{
    auth::{Credentials, InstallationTokenGenerator, JWTCredentials},
    http_cache::FileBasedCache,
};
use okta::Okta;
use quickbooks::QuickBooks;
use ramp_api::Ramp;
use reqwest::{header, Client};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use slack_chat_api::Slack;
use tailscale_api::Tailscale;
use tripactions::TripActions;

use crate::{
    airtable::AIRTABLE_COMPANIES_TABLE,
    api_tokens::{APIToken, NewAPIToken},
    configs::{Building, Buildings},
    core::UpdateAirtableRecord,
    db::Database,
    schema::{api_tokens, companys},
};

#[db {
    new_struct_name = "Company",
    airtable_base = "cio",
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
    pub okta_api_key: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub mailchimp_list_id: String,
    #[serde(default)]
    pub github_app_installation_id: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub cloudflare_api_key: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub checkr_api_key: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub printer_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tailscale_api_key: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tripactions_client_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tripactions_client_secret: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub airtable_api_key: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub airtable_enterprise_account_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub airtable_workspace_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub airtable_base_id_customer_leads: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub airtable_base_id_directory: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub airtable_base_id_misc: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub airtable_base_id_roadmap: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub airtable_base_id_hiring: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub airtable_base_id_shipments: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub airtable_base_id_finance: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub airtable_base_id_swag: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub airtable_base_id_assets: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub airtable_base_id_travel: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub airtable_base_id_cio: String,
    /// The CIO company ID.
    #[serde(default)]
    pub cio_company_id: i32,
}

/// Implement updating the Airtable record for a Company.
#[async_trait]
impl UpdateAirtableRecord<Company> for Company {
    async fn update_airtable_record(&mut self, _record: Company) {}
}

impl Company {
    /// Returns the shippo data structure for the address at the office
    /// for the company.
    pub fn hq_shipping_address(&self, db: &Database) -> shippo::Address {
        // Get the buildings from the company.
        let buildings: Vec<Building> = Buildings::get_from_db(db, self.cio_company_id).into();
        // Get the first one.
        // TODO: when there is more than one building, figure this out.
        let building = buildings.get(0).unwrap();

        shippo::Address {
            company: self.name.to_string(),
            name: "The Shipping Bot".to_string(),
            street1: building.street_address.to_string(),
            city: building.city.to_string(),
            state: building.state.to_string(),
            zip: building.zipcode.to_string(),
            country: building.country.to_string(),
            phone: building.phone.to_string(),
            email: format!("packages@{}", &self.gsuite_domain),
            is_complete: Default::default(),
            object_id: Default::default(),
            test: Default::default(),
            street2: Default::default(),
            validation_results: None,
        }
    }

    pub async fn post_to_slack_channel(&self, db: &Database, value: serde_json::Value) {
        // We need to get the url from the api tokens.
        // Only do this if we have a token and the token is not empty.
        if let Ok(token) = api_tokens::dsl::api_tokens
            .filter(
                api_tokens::dsl::auth_company_id
                    .eq(self.id)
                    .and(api_tokens::dsl::product.eq("slack".to_lowercase())),
            )
            .first::<APIToken>(&db.conn())
        {
            if !token.endpoint.is_empty() {
                Slack::post_to_channel(token.endpoint, value).await.unwrap();
            }
        }
    }

    pub fn get_from_slack_team_id(db: &Database, team_id: &str) -> Self {
        // We need to get the token first with the matching team id.
        let token = api_tokens::dsl::api_tokens
            .filter(
                api_tokens::dsl::company_id
                    .eq(team_id.to_string())
                    .and(api_tokens::dsl::product.eq("slack".to_lowercase())),
            )
            .first::<APIToken>(&db.conn())
            .unwrap_or_else(|e| {
                panic!(
                    "could not find slack api token matching team id {}: {}",
                    team_id, e
                )
            });

        // Now we can get the company.
        Company::get_by_id(db, token.auth_company_id)
    }

    pub fn get_from_github_org(db: &Database, org: &str) -> Self {
        companys::dsl::companys
            .filter(
                companys::dsl::github_org
                    .eq(org.to_string())
                    .or(companys::dsl::github_org.eq(org.to_lowercase())),
            )
            .first::<Company>(&db.conn())
            .unwrap_or_else(|e| panic!("could not find company matching github org {}: {}", org, e))
    }

    pub fn get_from_mailchimp_list_id(db: &Database, list_id: &str) -> Self {
        companys::dsl::companys
            .filter(companys::dsl::mailchimp_list_id.eq(list_id.to_string()))
            .first::<Company>(&db.conn())
            .unwrap()
    }

    pub fn get_from_domain(db: &Database, domain: &str) -> Self {
        companys::dsl::companys
            .filter(
                companys::dsl::domain
                    .eq(domain.to_string())
                    .or(companys::dsl::gsuite_domain.eq(domain.to_string())),
            )
            .first::<Company>(&db.conn())
            .unwrap()
    }

    /// Authenticate with Cloudflare.
    pub fn authenticate_cloudflare(&self) -> Option<CloudflareClient> {
        if self.cloudflare_api_key.is_empty() || self.gsuite_subject.is_empty() {
            // Return early.
            return None;
        }

        // Create the Cloudflare client.
        let cf_creds = CloudflareCredentials::UserAuthKey {
            email: self.gsuite_subject.to_string(),
            key: self.cloudflare_api_key.to_string(),
        };
        let api_client = CloudflareClient::new(
            cf_creds,
            HttpApiClientConfig::default(),
            Environment::Production,
        )
        .unwrap();
        Some(api_client)
    }

    /// Authenticate with Checkr.
    pub fn authenticate_checkr(&self) -> Option<Checkr> {
        if self.checkr_api_key.is_empty() {
            // Return early.
            return None;
        }
        Some(Checkr::new(&self.checkr_api_key))
    }

    /// Authenticate with Okta.
    pub fn authenticate_okta(&self) -> Option<Okta> {
        if self.okta_api_key.is_empty() || self.okta_domain.is_empty() {
            // Return early.
            return None;
        }
        Some(Okta::new(&self.okta_api_key, &self.okta_domain))
    }

    /// Authenticate with Airtable.
    pub fn authenticate_airtable(&self, base_id: &str) -> Airtable {
        Airtable::new(
            &self.airtable_api_key,
            base_id,
            &self.airtable_enterprise_account_id,
        )
    }

    /// Authenticate with MailChimp.
    pub async fn authenticate_mailchimp(&self, db: &Database) -> Option<MailChimp> {
        // Get the APIToken from the database.
        if let Some(mut t) = APIToken::get_from_db(db, self.id, "mailchimp".to_string()) {
            // Initialize the MailChimp client.
            let mut mailchimp = MailChimp::new_from_env(
                t.access_token,
                t.refresh_token.to_string(),
                t.endpoint.to_string(),
            );
            // MailChimp does not give you a refresh token so we should never refresh.
            // https://mailchimp.com/developer/marketing/guides/access-user-data-oauth-2/
            if !t.refresh_token.is_empty() {
                let nt = mailchimp.refresh_access_token().await.unwrap();
                t.access_token = nt.access_token.to_string();
                t.expires_in = nt.expires_in as i32;
                t.last_updated_at = Utc::now();
                if !nt.refresh_token.is_empty() {
                    t.refresh_token = nt.refresh_token.to_string();
                }
                if nt.x_refresh_token_expires_in > 0 {
                    t.refresh_token_expires_in = nt.x_refresh_token_expires_in as i32;
                }
                t.expand();
                // Update the token in the database.
                t.update(db).await;
            }

            return Some(mailchimp);
        }

        None
    }

    /// Authenticate with Slack.
    pub fn authenticate_slack(&self, db: &Database) -> Option<Slack> {
        // Get the bot token and user token from the database.
        if let Ok(bot_token) = api_tokens::dsl::api_tokens
            .filter(
                api_tokens::dsl::cio_company_id
                    .eq(self.id)
                    .and(api_tokens::dsl::product.eq("slack".to_string()))
                    .and(api_tokens::dsl::token_type.eq("bot".to_string())),
            )
            .first::<APIToken>(&db.conn())
        {
            if let Ok(user_token) = api_tokens::dsl::api_tokens
                .filter(
                    api_tokens::dsl::cio_company_id
                        .eq(self.id)
                        .and(api_tokens::dsl::product.eq("slack".to_string()))
                        .and(api_tokens::dsl::token_type.eq("user".to_string())),
                )
                .first::<APIToken>(&db.conn())
            {
                // Initialize the Slack client.
                let slack = Slack::new_from_env(
                    bot_token.company_id.to_string(),
                    bot_token.access_token,
                    user_token.access_token,
                );
                // Slack does not give you refresh tokens.
                // So we don't need to do any song and dance to refresh.

                return Some(slack);
            }
        }

        None
    }

    /// Authenticate with Ramp.
    pub async fn authenticate_ramp(&self, db: &Database) -> Option<Ramp> {
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
            t.update(db).await;

            return Some(ramp);
        }

        None
    }

    /// Authenticate with DocuSign.
    pub async fn authenticate_docusign(&self, db: &Database) -> Option<DocuSign> {
        // Get the APIToken from the database.
        if let Some(mut t) = APIToken::get_from_db(db, self.id, "docusign".to_string()) {
            // Initialize the DocuSign client.
            let mut ds = DocuSign::new_from_env(
                t.access_token,
                t.refresh_token,
                t.company_id.to_string(),
                t.endpoint.to_string(),
            );
            let nt = ds.refresh_access_token().await.unwrap();
            t.access_token = nt.access_token.to_string();
            t.expires_in = nt.expires_in as i32;
            t.refresh_token = nt.refresh_token.to_string();
            t.refresh_token_expires_in = nt.x_refresh_token_expires_in as i32;
            t.last_updated_at = Utc::now();
            t.expand();
            // Update the token in the database.
            t.update(db).await;

            return Some(ds);
        }

        None
    }

    /// Authenticate with Gusto.
    pub async fn authenticate_gusto(&self, db: &Database) -> Option<Gusto> {
        // Get the APIToken from the database.
        if let Some(mut t) = APIToken::get_from_db(db, self.id, "gusto".to_string()) {
            // Initialize the Gusto client.
            let mut gusto =
                Gusto::new_from_env(t.access_token, t.refresh_token, t.company_id.to_string());
            let nt = gusto.refresh_access_token().await.unwrap();
            t.access_token = nt.access_token.to_string();
            t.expires_in = nt.expires_in as i32;
            t.refresh_token = nt.refresh_token.to_string();
            t.refresh_token_expires_in = nt.x_refresh_token_expires_in as i32;
            t.last_updated_at = Utc::now();
            t.expand();
            // Update the token in the database.
            t.update(db).await;

            return Some(gusto);
        }

        None
    }

    /// Authenticate with Tailscale.
    pub fn authenticate_tailscale(&self) -> Tailscale {
        Tailscale::new(&self.tailscale_api_key, &self.gsuite_domain)
    }

    /// Authenticate with TripActions.
    pub async fn authenticate_tripactions(&self, db: &Database) -> TripActions {
        // Get the APIToken from the database.
        if let Some(mut t) = APIToken::get_from_db(db, self.id, "tripactions".to_string()) {
            // Initialize the TripActions client.
            let mut ta = TripActions::new(
                self.tripactions_client_id.to_string(),
                self.tripactions_client_secret.to_string(),
                t.access_token,
            );
            let nt = ta.get_access_token().await.unwrap();
            t.access_token = nt.access_token.to_string();
            t.expires_in = nt.expires_in as i32;
            t.refresh_token = nt.refresh_token.to_string();
            t.refresh_token_expires_in = nt.refresh_token_expires_in as i32;
            t.last_updated_at = Utc::now();
            t.expand();
            // Update the token in the database.
            t.update(db).await;

            return ta;
        }

        let mut ta = TripActions::new(
            self.tripactions_client_id.to_string(),
            self.tripactions_client_secret.to_string(),
            "",
        );
        let t = ta.get_access_token().await.unwrap();

        let mut token = NewAPIToken {
            product: "tripactions".to_string(),
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
            auth_company_id: self.id,
            company: Default::default(),
            // THIS IS ALWAYS OXIDE, THEY OWN ALL THE CREDS.
            cio_company_id: 1,
        };

        token.expand();
        token.upsert(db).await;

        ta
    }

    /// Authenticate with QuickBooks.
    pub async fn authenticate_quickbooks(&self, db: &Database) -> Option<QuickBooks> {
        // Get the APIToken from the database.
        if let Some(mut t) = APIToken::get_from_db(db, self.id, "quickbooks".to_string()) {
            // Initialize the QuickBooks client.
            let mut qb =
                QuickBooks::new_from_env(t.company_id.to_string(), t.access_token, t.refresh_token);
            let nt = qb.refresh_access_token().await.unwrap();
            t.access_token = nt.access_token.to_string();
            t.expires_in = nt.expires_in as i32;
            t.refresh_token = nt.refresh_token.to_string();
            t.refresh_token_expires_in = nt.x_refresh_token_expires_in as i32;
            t.last_updated_at = Utc::now();
            t.expand();
            // Update the token in the database.
            t.update(db).await;

            return Some(qb);
        }

        None
    }

    /// Get a Google token.
    pub async fn authenticate_google(&self, db: &Database) -> String {
        // Get the APIToken from the database.
        if let Some(t) = APIToken::get_from_db(db, self.id, "google".to_string()) {
            let nt = refresh_google_access_token(db, t).await;
            return nt.access_token;
        }

        "".to_string()
    }

    /// Authenticate GitHub with JSON web token credentials, for an application installation.
    pub fn authenticate_github(&self) -> octorust::Client {
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
        let http_cache = Box::new(FileBasedCache::new(format!(
            "{}/.cache/github",
            env::var("HOME").unwrap()
        )));

        let token_generator = InstallationTokenGenerator::new(
            self.github_app_installation_id.try_into().unwrap(),
            jwt,
        );

        octorust::Client::custom(
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

    // This should forever only be oxide.
    let oxide = Company::get_from_db(&db, "Oxide".to_string()).unwrap();

    let is: Vec<airtable_api::Record<Company>> = oxide
        .authenticate_airtable(&oxide.airtable_base_id_cio)
        .list_records(&Company::airtable_table(), "Grid view", vec![])
        .await
        .unwrap();

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
        company.cio_company_id = oxide.id;
        company.update(&db).await;
    }
    // Companies are only stored with Oxide.
    Companys::get_from_db(&db, 1).update_airtable(&db).await;
}

pub async fn get_google_consent_url() -> String {
    let secret = get_google_credentials().await;
    format!(
        "https://accounts.google.com/o/oauth2/v2/auth?response_type=code&client_id={}&redirect_uri={}&scope={}&access_type=offline",
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

    yup_oauth2::read_application_secret(google_credential_file)
        .await
        .expect("failed to read google credential file")
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
        "https://www.googleapis.com/auth/userinfo.profile".to_string(),
        "https://www.googleapis.com/auth/userinfo.email".to_string(),
    ]
}

/// The data type for Google user info.
#[derive(Default, Clone, Debug, JsonSchema, Serialize, Deserialize)]
pub struct UserInfo {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub family_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub given_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub picture: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub locale: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub hd: String,
    #[serde(default)]
    pub verified_email: bool,
}

pub async fn get_google_access_token(db: &Database, code: &str) {
    let secret = get_google_credentials().await;

    let mut headers = header::HeaderMap::new();
    headers.append(
        header::ACCEPT,
        header::HeaderValue::from_static("application/json"),
    );

    let params = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", &secret.redirect_uris[0]),
        ("client_id", &secret.client_id),
        ("client_secret", &secret.client_secret),
    ];
    let client = Client::new();
    let resp = client
        .post("https://oauth2.googleapis.com/token")
        .headers(headers)
        .form(&params)
        .send()
        .await
        .unwrap();

    // Unwrap the response.
    let t: ramp_api::AccessToken = resp.json().await.unwrap();

    // Let's get the company from information about the user.
    let mut headers = header::HeaderMap::new();
    headers.append(
        header::ACCEPT,
        header::HeaderValue::from_static("application/json"),
    );
    headers.append(
        header::AUTHORIZATION,
        header::HeaderValue::from_str(&format!("Bearer {}", t.access_token)).unwrap(),
    );

    let params = [("alt", "json")];
    let resp = client
        .get("https://www.googleapis.com/oauth2/v1/userinfo")
        .headers(headers)
        .query(&params)
        .send()
        .await
        .unwrap();

    // Unwrap the response.
    let metadata: UserInfo = resp.json().await.unwrap();

    let company = Company::get_from_domain(db, &metadata.hd);

    // Save the token to the database.
    let mut token = NewAPIToken {
        product: "google".to_string(),
        token_type: t.token_type.to_string(),
        access_token: t.access_token.to_string(),
        expires_in: t.expires_in as i32,
        refresh_token: t.refresh_token.to_string(),
        refresh_token_expires_in: t.refresh_token_expires_in as i32,
        company_id: metadata.hd.to_string(),
        item_id: "".to_string(),
        user_email: metadata.email.to_string(),
        last_updated_at: Utc::now(),
        expires_date: None,
        refresh_token_expires_date: None,
        endpoint: "".to_string(),
        auth_company_id: company.id,
        company: Default::default(),
        // THIS SHOULD ALWAYS BE OXIDE, NO 1.
        cio_company_id: 1,
    };
    token.expand();

    // Update it in the database.
    token.upsert(db).await;
}

pub async fn refresh_google_access_token(db: &Database, mut t: APIToken) -> APIToken {
    let secret = get_google_credentials().await;

    let mut headers = header::HeaderMap::new();
    headers.append(
        header::ACCEPT,
        header::HeaderValue::from_static("application/json"),
    );

    let params = [
        ("grant_type", "refresh_token"),
        ("refresh_token", &t.refresh_token),
        ("client_id", &secret.client_id),
        ("client_secret", &secret.client_secret),
    ];
    let client = Client::new();
    let resp = client
        .post("https://oauth2.googleapis.com/token")
        .headers(headers)
        .form(&params)
        .send()
        .await
        .unwrap();

    // Unwrap the response.
    let nt: ramp_api::AccessToken = resp.json().await.unwrap();

    t.access_token = nt.access_token.to_string();
    t.expires_in = nt.expires_in as i32;
    if !nt.refresh_token.is_empty() {
        t.refresh_token = nt.refresh_token.to_string();
        t.refresh_token_expires_in = nt.refresh_token_expires_in as i32;
    }
    t.last_updated_at = Utc::now();
    t.expand();

    // Update the token in the database.
    t.update(db).await;

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
