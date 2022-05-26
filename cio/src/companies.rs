use std::{convert::TryInto, env};

use airtable_api::Airtable;
use anyhow::{anyhow, bail, Result};
use async_bb8_diesel::AsyncRunQueryDsl;
use async_trait::async_trait;
use checkr::Checkr;
use chrono::Utc;
use cloudflare::framework::{
    async_api::Client as CloudflareClient, auth::Credentials as CloudflareCredentials, Environment, HttpApiClientConfig,
};
use docusign::DocuSign;
use google_calendar::Client as GoogleCalendar;
use google_drive::Client as GoogleDrive;
use google_groups_settings::Client as GoogleGroupsSettings;
use gsuite_api::Client as GoogleAdmin;
use gusto_api::Client as Gusto;
use log::{info, warn};
use macros::db;
use mailchimp_api::MailChimp;
use octorust::{
    auth::{Credentials, InstallationTokenGenerator, JWTCredentials},
    http_cache::FileBasedCache,
};
use okta::Client as Okta;
use quickbooks::QuickBooks;
use ramp_api::Client as Ramp;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sheets::Client as GoogleSheets;
use shipbob::Client as ShipBob;
use slack_chat_api::Slack;
use tailscale_api::Tailscale;
use tripactions::Client as TripActions;
use zoom_api::Client as Zoom;

use crate::{
    airtable::{AIRTABLE_COMPANIES_TABLE, AIRTABLE_GRID_VIEW},
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
#[diesel(table_name = companys)]
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
    pub shipbob_pat: String,
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
    pub airtable_workspace_read_only_id: String,
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

    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub slack_channel_applicants: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub slack_channel_swag: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub slack_channel_shipments: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub slack_channel_mailing_lists: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub slack_channel_finance: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub slack_channel_debug: String,

    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub google_service_account: String,

    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub nginx_ip: String,

    /// The CIO company ID.
    #[serde(default)]
    pub cio_company_id: i32,
}

/// Implement updating the Airtable record for a Company.
#[async_trait]
impl UpdateAirtableRecord<Company> for Company {
    async fn update_airtable_record(&mut self, _record: Company) -> Result<()> {
        Ok(())
    }
}

impl Company {
    /// Returns the shippo data structure for the address at the office
    /// for the company.
    pub async fn hq_shipping_address(&self, db: &Database) -> Result<shippo::Address> {
        // Get the buildings from the company.
        let buildings: Vec<Building> = Buildings::get_from_db(db, self.cio_company_id).await?.into();
        // Get the first one.
        // TODO: when there is more than one building, figure this out.
        let building = buildings.get(0).unwrap();

        Ok(shippo::Address {
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
        })
    }

    pub async fn post_to_slack_channel(&self, db: &Database, msg: &slack_chat_api::FormattedMessage) -> Result<()> {
        // Create the Slack client.
        let r = self.authenticate_slack(db).await;
        if let Err(e) = r {
            if e.to_string().contains("no token") {
                // Return early, this company does not use Slack.
                return Ok(());
            }

            bail!("authenticating slack failed: {}", e);
        }

        let slack = r?;

        // Post the message;
        if let Err(e) = slack.post_message(msg).await {
            // Give useful information with the error.
            return Err(anyhow!(
                "posting `{}` as a slack message failed: {}",
                json!(msg).to_string(),
                e
            ));
        }

        Ok(())
    }

    pub async fn get_from_slack_team_id(db: &Database, team_id: &str) -> Result<Self> {
        // We need to get the token first with the matching team id.
        let token = api_tokens::dsl::api_tokens
            .filter(
                api_tokens::dsl::company_id
                    .eq(team_id.to_string())
                    .and(api_tokens::dsl::product.eq("slack".to_lowercase())),
            )
            .first_async::<APIToken>(db.pool())
            .await?;

        // Now we can get the company.
        Company::get_by_id(db, token.auth_company_id).await
    }

    pub async fn get_from_github_org(db: &Database, org: &str) -> Result<Self> {
        Ok(companys::dsl::companys
            .filter(
                companys::dsl::github_org
                    .eq(org.to_string())
                    .or(companys::dsl::github_org.eq(org.to_lowercase())),
            )
            .first_async::<Company>(db.pool())
            .await?)
    }

    pub async fn get_from_shipbob_channel_id(db: &Database, channel_id: &str) -> Result<Self> {
        let token = api_tokens::dsl::api_tokens
            .filter(
                api_tokens::dsl::company_id
                    .eq(channel_id.to_string())
                    .and(api_tokens::dsl::product.eq("shipbob".to_string())),
            )
            .first_async::<APIToken>(db.pool())
            .await?;

        Company::get_by_id(db, token.auth_company_id).await
    }

    pub async fn get_from_mailchimp_list_id(db: &Database, list_id: &str) -> Result<Self> {
        Ok(companys::dsl::companys
            .filter(companys::dsl::mailchimp_list_id.eq(list_id.to_string()))
            .first_async::<Company>(db.pool())
            .await?)
    }

    pub async fn get_from_domain(db: &Database, domain: &str) -> Result<Self> {
        let result = companys::dsl::companys
            .filter(
                companys::dsl::domain
                    .eq(domain.to_string())
                    .or(companys::dsl::gsuite_domain.eq(domain.to_string())),
            )
            .first_async::<Company>(db.pool())
            .await;
        if let Ok(company) = result {
            return Ok(company);
        }

        // We could not find the company by domain.
        // Check if we only have one company in the database, if so just return that,
        // otherwise return an error.
        let count = Companys::get_from_db(db, 1).await?.into_iter().len();
        if count == 1 {
            return Ok(companys::dsl::companys.first_async::<Company>(db.pool()).await?);
        }

        bail!("could not find company with domain `{}`", domain);
    }

    /// Authenticate with Cloudflare.
    pub fn authenticate_cloudflare(&self) -> Result<CloudflareClient> {
        if self.cloudflare_api_key.is_empty() || self.gsuite_subject.is_empty() {
            // Return early.
            bail!("no token");
        }

        // Create the Cloudflare client.
        let cf_creds = CloudflareCredentials::UserAuthKey {
            email: self.gsuite_subject.to_string(),
            key: self.cloudflare_api_key.to_string(),
        };
        let api_client = CloudflareClient::new(cf_creds, HttpApiClientConfig::default(), Environment::Production)?;
        Ok(api_client)
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
        Some(Okta::new(&self.okta_api_key).with_host(self.okta_endpoint()))
    }

    fn okta_endpoint(&self) -> String {
        format!(
            "https://{}.okta.com",
            self.okta_domain
                .trim_start_matches("https://")
                .trim_start_matches("https://")
                .trim_end_matches('/')
                .trim_end_matches(".okta.com")
                .trim_end_matches('/')
        )
    }

    /// Authenticate with Airtable.
    pub fn authenticate_airtable(&self, base_id: &str) -> Airtable {
        Airtable::new(&self.airtable_api_key, base_id, &self.airtable_enterprise_account_id)
    }

    /// Authenticate with ShipBob.
    pub async fn authenticate_shipbob(&self) -> Result<ShipBob> {
        if self.shipbob_pat.is_empty() {
            bail!("no shipbob personal access token");
        }

        Ok(ShipBob::new(&self.shipbob_pat))
    }

    /// Ensure the company has ShipBob webhooks setup.
    pub async fn ensure_shipbob_webhooks(&self) -> Result<()> {
        let shipbob_auth = self.authenticate_shipbob().await;
        if let Err(e) = shipbob_auth {
            if e.to_string().contains("no shipbob personal access token") {
                // Return early, they don't use ShipBob.
                return Ok(());
            }

            // Otherwise bail!
            bail!(e);
        }

        let shipbob = shipbob_auth?;
        let shipbob_webhooks_url =
            env::var("SHIPBOB_WEBHOOKS_URL").map_err(|e| anyhow!("expected SHIPBOB_WEBHOOKS_URL to be set: {}", e))?;

        let topics = vec![
            shipbob::types::WebhooksTopics::OrderShipped,
            shipbob::types::WebhooksTopics::ShipmentDelivered,
            shipbob::types::WebhooksTopics::ShipmentException,
            shipbob::types::WebhooksTopics::ShipmentOnhold,
        ];

        for topic in topics {
            // Check if the webhook already exists.
            let mut exists = false;
            match shipbob.webhooks().get_all(topic.clone()).await {
                Ok(webhooks) => {
                    for webhook in webhooks {
                        // Check if we already have the webhooks.
                        if webhook.subscription_url == shipbob_webhooks_url {
                            exists = true;
                            info!(
                                "shipbob webhook for topic `{}` to url `{}` already exists",
                                topic, shipbob_webhooks_url
                            );
                            break;
                        }
                    }

                    if exists {
                        continue;
                    }

                    // Create it if not.
                    match shipbob
                        .webhooks()
                        .post(&shipbob::types::WebhooksCreateWebhookSubscriptionModel {
                            subscription_url: shipbob_webhooks_url.to_string(),
                            topic: topic.clone(),
                        })
                        .await
                    {
                        Ok(_) => info!(
                            "created shipbob webhook for topic `{}` to url `{}`",
                            topic, shipbob_webhooks_url
                        ),
                        Err(e) => warn!(
                            "failed to create shipbob webhook for topic `{}` to url `{}`: {}",
                            topic, shipbob_webhooks_url, e
                        ),
                    }
                }
                Err(e) => {
                    warn!("getting shipbob webhooks for topic `{}` failed: {}", topic, e);
                }
            }
        }

        Ok(())
    }

    /// Authenticate with MailChimp.
    pub async fn authenticate_mailchimp(&self, db: &Database) -> Result<MailChimp> {
        // Get the APIToken from the database.
        if let Some(mut t) = APIToken::get_from_db(db, self.id, "mailchimp".to_string()).await {
            // Initialize the MailChimp client.
            let mut mailchimp = MailChimp::new_from_env(
                t.access_token.to_string(),
                t.refresh_token.to_string(),
                t.endpoint.to_string(),
            );

            // MailChimp does not give you a refresh token so we should never refresh.
            // But just in case in the future they do, we will leave this here.
            // https://mailchimp.com/developer/marketing/guides/access-user-data-oauth-2/
            if !t.refresh_token.is_empty() && t.is_expired() {
                let nt = mailchimp.refresh_access_token().await?;
                if !nt.access_token.is_empty() {
                    t.access_token = nt.access_token.to_string();
                }
                if nt.expires_in > 0 {
                    t.expires_in = nt.expires_in as i32;
                }
                t.last_updated_at = Utc::now();
                if !nt.refresh_token.is_empty() {
                    t.refresh_token = nt.refresh_token.to_string();
                }
                if nt.x_refresh_token_expires_in > 0 {
                    t.refresh_token_expires_in = nt.x_refresh_token_expires_in as i32;
                }
                t.expand();
                // Update the token in the database.
                t.update(db).await?;
            }

            return Ok(mailchimp);
        }

        bail!("no token");
    }

    /// Authenticate with Slack.
    pub async fn authenticate_slack(&self, db: &Database) -> Result<Slack> {
        // Get the bot token and user token from the database.
        if let Ok(bot_token) = api_tokens::dsl::api_tokens
            .filter(
                api_tokens::dsl::cio_company_id
                    .eq(self.id)
                    .and(api_tokens::dsl::product.eq("slack".to_string()))
                    .and(api_tokens::dsl::token_type.eq("bot".to_string())),
            )
            .first_async::<APIToken>(db.pool())
            .await
        {
            if let Ok(user_token) = api_tokens::dsl::api_tokens
                .filter(
                    api_tokens::dsl::cio_company_id
                        .eq(self.id)
                        .and(api_tokens::dsl::product.eq("slack".to_string()))
                        .and(api_tokens::dsl::token_type.eq("user".to_string())),
                )
                .first_async::<APIToken>(db.pool())
                .await
            {
                // Initialize the Slack client.
                let slack = Slack::new_from_env(
                    bot_token.company_id.to_string(),
                    bot_token.access_token,
                    user_token.access_token,
                );
                // Slack does not give you refresh tokens.
                // So we don't need to do any song and dance to refresh.

                return Ok(slack);
            }
        }

        bail!("no token");
    }

    /// Authenticate with Ramp.
    pub async fn authenticate_ramp(&self, db: &Database) -> Result<Ramp> {
        // Get the APIToken from the database.
        if let Some(mut t) = APIToken::get_from_db(db, self.id, "ramp".to_string()).await {
            // Initialize the Ramp client.
            let mut ramp = Ramp::new_from_env(t.access_token.to_string(), t.refresh_token.to_string());

            if t.is_expired() {
                // Only refresh the token if it is expired.
                let nt = ramp.refresh_access_token().await?;
                if !nt.access_token.is_empty() {
                    t.access_token = nt.access_token.to_string();
                }
                if nt.expires_in > 0 {
                    t.expires_in = nt.expires_in as i32;
                }
                t.last_updated_at = Utc::now();
                if !nt.refresh_token.is_empty() {
                    t.refresh_token = nt.refresh_token.to_string();
                }
                if nt.refresh_token_expires_in > 0 {
                    t.refresh_token_expires_in = nt.refresh_token_expires_in as i32;
                }
                t.expand();
                // Update the token in the database.
                t.update(db).await?;
            }

            return Ok(ramp);
        }

        bail!("no token");
    }

    /// Authenticate with Zoom.
    pub async fn authenticate_zoom(&self, db: &Database) -> Result<Zoom> {
        // Get the APIToken from the database.
        if let Some(mut t) = APIToken::get_from_db(db, self.id, "zoom".to_string()).await {
            // Initialize the Zoom client.
            let mut zoom = Zoom::new_from_env(t.access_token.to_string(), t.refresh_token.to_string());

            if t.is_expired() {
                // Update the token if it is expired.
                let nt = zoom.refresh_access_token().await?;
                if !nt.access_token.is_empty() {
                    t.access_token = nt.access_token.to_string();
                }
                if nt.expires_in > 0 {
                    t.expires_in = nt.expires_in as i32;
                }
                t.last_updated_at = Utc::now();
                if !nt.refresh_token.is_empty() {
                    t.refresh_token = nt.refresh_token.to_string();
                }
                if nt.refresh_token_expires_in > 0 {
                    t.refresh_token_expires_in = nt.refresh_token_expires_in as i32;
                }
                t.expand();
                // Update the token in the database.
                t.update(db).await?;
            }

            return Ok(zoom);
        }

        bail!("no token");
    }

    /// Authenticate with DocuSign.
    pub async fn authenticate_docusign(&self, db: &Database) -> Result<DocuSign> {
        // Get the APIToken from the database.
        if let Some(mut t) = APIToken::get_from_db(db, self.id, "docusign".to_string()).await {
            // Initialize the DocuSign client.
            let mut ds = DocuSign::new_from_env(
                t.access_token.to_string(),
                t.refresh_token.to_string(),
                t.company_id.to_string(),
                t.endpoint.to_string(),
            );

            if t.is_expired() {
                // Only refresh the token if it is expired.
                let nt = ds.refresh_access_token().await?;
                if !nt.access_token.is_empty() {
                    t.access_token = nt.access_token.to_string();
                }
                if nt.expires_in > 0 {
                    t.expires_in = nt.expires_in as i32;
                }
                if !nt.refresh_token.is_empty() {
                    t.refresh_token = nt.refresh_token.to_string();
                }
                if nt.x_refresh_token_expires_in > 0 {
                    t.refresh_token_expires_in = nt.x_refresh_token_expires_in as i32;
                }
                t.last_updated_at = Utc::now();
                t.expand();
                // Update the token in the database.
                t.update(db).await?;
            }

            return Ok(ds);
        }

        bail!("no token");
    }

    /// Authenticate with Gusto.
    pub async fn authenticate_gusto(&self, db: &Database) -> Result<(Gusto, String)> {
        // Get the APIToken from the database.
        if let Some(mut t) = APIToken::get_from_db(db, self.id, "gusto".to_string()).await {
            // Initialize the Gusto client.
            let mut gusto = Gusto::new_from_env(t.access_token.to_string(), t.refresh_token.to_string());

            if t.is_expired() {
                // Only refresh the token if it is expired.
                let nt = gusto.refresh_access_token().await?;
                if !nt.access_token.is_empty() {
                    t.access_token = nt.access_token.to_string();
                }
                if nt.expires_in > 0 {
                    t.expires_in = nt.expires_in as i32;
                }
                if !nt.refresh_token.is_empty() {
                    t.refresh_token = nt.refresh_token.to_string();
                }
                if nt.refresh_token_expires_in > 0 {
                    t.refresh_token_expires_in = nt.refresh_token_expires_in as i32;
                }
                t.last_updated_at = Utc::now();
                t.expand();
                // Update the token in the database.
                t.update(db).await?;
            }

            return Ok((gusto, t.company_id.to_string()));
        }

        bail!("no token");
    }

    /// Authenticate with Tailscale.
    pub fn authenticate_tailscale(&self) -> Tailscale {
        Tailscale::new(&self.tailscale_api_key, &self.gsuite_domain)
    }

    /// Authenticate with TripActions.
    pub async fn authenticate_tripactions(&self, db: &Database) -> Result<TripActions> {
        if self.tripactions_client_id.is_empty() || self.tripactions_client_secret.is_empty() {
            // bail early we don't have a token.
            bail!("no token");
        }

        // Get the APIToken from the database.
        if let Some(mut t) = APIToken::get_from_db(db, self.id, "tripactions".to_string()).await {
            // Initialize the TripActions client.
            let mut ta = TripActions::new(
                self.tripactions_client_id.to_string(),
                self.tripactions_client_secret.to_string(),
                t.access_token.to_string(),
            );

            if t.is_expired() {
                // Only refresh the token if it is expired.
                let nt = ta.get_access_token().await?;
                if !nt.access_token.is_empty() {
                    t.access_token = nt.access_token.to_string();
                }
                if nt.expires_in > 0 {
                    t.expires_in = nt.expires_in as i32;
                }
                if !nt.refresh_token.is_empty() {
                    t.refresh_token = nt.refresh_token.to_string();
                }
                if nt.refresh_token_expires_in > 0 {
                    t.refresh_token_expires_in = nt.refresh_token_expires_in as i32;
                }
                t.last_updated_at = Utc::now();
                t.expand();
                // Update the token in the database.
                t.update(db).await?;
            }

            return Ok(ta);
        }

        let mut ta = TripActions::new(
            self.tripactions_client_id.to_string(),
            self.tripactions_client_secret.to_string(),
            "",
        );
        let t = ta.get_access_token().await?;

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
        token.upsert(db).await?;

        Ok(ta)
    }

    /// Authenticate with QuickBooks.
    pub async fn authenticate_quickbooks(&self, db: &Database) -> Result<QuickBooks> {
        // Get the APIToken from the database.
        if let Some(mut t) = APIToken::get_from_db(db, self.id, "quickbooks".to_string()).await {
            // Initialize the QuickBooks client.
            let mut qb = QuickBooks::new_from_env(
                t.company_id.to_string(),
                t.access_token.to_string(),
                t.refresh_token.to_string(),
            );

            if t.is_expired() {
                // Only refresh the token if it is expired.
                let nt = qb.refresh_access_token().await?;
                if !nt.access_token.is_empty() {
                    t.access_token = nt.access_token.to_string();
                }
                if nt.expires_in > 0 {
                    t.expires_in = nt.expires_in as i32;
                }
                if !nt.refresh_token.is_empty() {
                    t.refresh_token = nt.refresh_token.to_string();
                }
                if nt.x_refresh_token_expires_in > 0 {
                    t.refresh_token_expires_in = nt.x_refresh_token_expires_in as i32;
                }
                t.last_updated_at = Utc::now();
                t.expand();
                // Update the token in the database.
                t.update(db).await?;
            }

            return Ok(qb);
        }

        bail!("no token");
    }

    /// Authenticate Google Admin.
    pub async fn authenticate_google_admin(&self, db: &Database) -> Result<GoogleAdmin> {
        // Get the APIToken from the database.
        if let Some(mut t) = APIToken::get_from_db(db, self.id, "google".to_string()).await {
            // Initialize the client.
            let mut g = GoogleAdmin::new_from_env(t.access_token.to_string(), t.refresh_token.to_string()).await;
            g.set_auto_access_token_refresh(true);

            if t.is_expired() {
                // Only refresh the token if it is expired.
                let nt = g.refresh_access_token().await?;
                if !nt.access_token.is_empty() {
                    t.access_token = nt.access_token.to_string();
                }
                if nt.expires_in > 0 {
                    t.expires_in = nt.expires_in as i32;
                }
                if !nt.refresh_token.is_empty() {
                    t.refresh_token = nt.refresh_token.to_string();
                }
                if nt.refresh_token_expires_in > 0 {
                    t.refresh_token_expires_in = nt.refresh_token_expires_in as i32;
                }
                t.last_updated_at = Utc::now();
                t.expand();
                // Update the token in the database.
                t.update(db).await?;
            }

            return Ok(g);
        }

        bail!("no token");
    }

    /// Authenticate Google Calendar.
    pub async fn authenticate_google_calendar(&self, db: &Database) -> Result<GoogleCalendar> {
        // Get the APIToken from the database.
        if let Some(mut t) = APIToken::get_from_db(db, self.id, "google".to_string()).await {
            // Initialize the client.
            let mut g = GoogleCalendar::new_from_env(t.access_token.to_string(), t.refresh_token.to_string()).await;
            g.set_auto_access_token_refresh(true);

            if t.is_expired() {
                // Only refresh the token if it is expired.
                let nt = g.refresh_access_token().await?;
                if !nt.access_token.is_empty() {
                    t.access_token = nt.access_token.to_string();
                }
                if nt.expires_in > 0 {
                    t.expires_in = nt.expires_in as i32;
                }
                if !nt.refresh_token.is_empty() {
                    t.refresh_token = nt.refresh_token.to_string();
                }
                if nt.refresh_token_expires_in > 0 {
                    t.refresh_token_expires_in = nt.refresh_token_expires_in as i32;
                }
                t.last_updated_at = Utc::now();
                t.expand();
                // Update the token in the database.
                t.update(db).await?;
            }

            return Ok(g);
        }

        bail!("no token");
    }

    /// Authenticate Google Calendar with Service Account.
    /// This allows mocking as another user.
    /// TODO: figure out why we can't mock with the standard token.
    pub async fn authenticate_google_calendar_with_service_account(&self, as_user: &str) -> Result<GoogleCalendar> {
        let token = self.get_google_service_account_token(as_user).await?;

        // Initialize the client.
        Ok(GoogleCalendar::new_from_env(&token, "").await)
    }

    /// Authenticate Google Drive.
    pub async fn authenticate_google_drive(&self, db: &Database) -> Result<GoogleDrive> {
        // Get the APIToken from the database.
        if let Some(mut t) = APIToken::get_from_db(db, self.id, "google".to_string()).await {
            // Initialize the client.
            let mut g = GoogleDrive::new_from_env(t.access_token.to_string(), t.refresh_token.to_string()).await;
            g.set_auto_access_token_refresh(true);

            if t.is_expired() {
                // Only refresh the token if it is expired.
                let nt = g.refresh_access_token().await?;
                if !nt.access_token.is_empty() {
                    t.access_token = nt.access_token.to_string();
                }
                if nt.expires_in > 0 {
                    t.expires_in = nt.expires_in as i32;
                }
                if !nt.refresh_token.is_empty() {
                    t.refresh_token = nt.refresh_token.to_string();
                }
                if nt.refresh_token_expires_in > 0 {
                    t.refresh_token_expires_in = nt.refresh_token_expires_in as i32;
                }
                t.last_updated_at = Utc::now();
                t.expand();
                // Update the token in the database.
                t.update(db).await?;
            }

            return Ok(g);
        }

        bail!("no token");
    }

    /// Authenticate Google Drive with Service Account.
    /// This allows mocking as another user.
    /// TODO: figure out why we can't mock with the standard token.
    pub async fn authenticate_google_drive_with_service_account(&self, as_user: &str) -> Result<GoogleDrive> {
        let token = self.get_google_service_account_token(as_user).await?;

        // Initialize the client.
        Ok(GoogleDrive::new_from_env(&token, "").await)
    }

    async fn get_google_service_account_token(&self, as_user: &str) -> Result<String> {
        if self.google_service_account.is_empty() {
            bail!("no service account");
        }

        let subject = if as_user.is_empty() {
            self.gsuite_subject.to_string()
        } else {
            as_user.to_string()
        };

        let client_secret = yup_oauth2::parse_service_account_key(&self.google_service_account)?;
        let auth = yup_oauth2::ServiceAccountAuthenticator::builder(client_secret)
            .subject(subject)
            .build()
            .await?;

        let token = auth
            .token(&[
                "https://www.googleapis.com/auth/admin.directory.group",
                "https://www.googleapis.com/auth/admin.directory.resource.calendar",
                "https://www.googleapis.com/auth/admin.directory.user",
                "https://www.googleapis.com/auth/calendar",
                "https://www.googleapis.com/auth/apps.groups.settings",
                "https://www.googleapis.com/auth/spreadsheets",
                "https://www.googleapis.com/auth/drive",
            ])
            .await?;

        let token_string = token.as_str().to_string();
        if token_string.is_empty() {
            bail!("empty token returned from authenticator");
        }

        Ok(token_string)
    }

    /// Authenticate Google Sheets.
    pub async fn authenticate_google_sheets(&self, db: &Database) -> Result<GoogleSheets> {
        // Get the APIToken from the database.
        if let Some(mut t) = APIToken::get_from_db(db, self.id, "google".to_string()).await {
            // Initialize the client.
            let mut g = GoogleSheets::new_from_env(t.access_token.to_string(), t.refresh_token.to_string()).await;
            g.set_auto_access_token_refresh(true);

            if t.is_expired() {
                // Only refresh the token if it is expired.
                let nt = g.refresh_access_token().await?;
                if !nt.access_token.is_empty() {
                    t.access_token = nt.access_token.to_string();
                }
                if nt.expires_in > 0 {
                    t.expires_in = nt.expires_in as i32;
                }
                if !nt.refresh_token.is_empty() {
                    t.refresh_token = nt.refresh_token.to_string();
                }
                if nt.refresh_token_expires_in > 0 {
                    t.refresh_token_expires_in = nt.refresh_token_expires_in as i32;
                }
                t.last_updated_at = Utc::now();
                t.expand();
                // Update the token in the database.
                t.update(db).await?;
            }

            return Ok(g);
        }

        bail!("no token");
    }

    /// Authenticate Google Groups Settings.
    pub async fn authenticate_google_groups_settings(&self, db: &Database) -> Result<GoogleGroupsSettings> {
        // Get the APIToken from the database.
        if let Some(mut t) = APIToken::get_from_db(db, self.id, "google".to_string()).await {
            // Initialize the client.
            let mut g =
                GoogleGroupsSettings::new_from_env(t.access_token.to_string(), t.refresh_token.to_string()).await;
            if t.is_expired() {
                // Only refresh the token if it is expired.
                let nt = g.refresh_access_token().await?;
                if !nt.access_token.is_empty() {
                    t.access_token = nt.access_token.to_string();
                }
                if nt.expires_in > 0 {
                    t.expires_in = nt.expires_in as i32;
                }
                if !nt.refresh_token.is_empty() {
                    t.refresh_token = nt.refresh_token.to_string();
                }
                if nt.refresh_token_expires_in > 0 {
                    t.refresh_token_expires_in = nt.refresh_token_expires_in as i32;
                }
                t.last_updated_at = Utc::now();
                t.expand();
                // Update the token in the database.
                t.update(db).await?;
            }

            return Ok(g);
        }

        bail!("no token");
    }

    /// Authenticate GitHub with JSON web token credentials, for an application installation.
    pub fn authenticate_github(&self) -> Result<octorust::Client> {
        // Parse our env variables.
        let app_id_str = env::var("GH_APP_ID")?;
        let app_id = app_id_str.parse::<u64>()?;

        let encoded_private_key = env::var("GH_PRIVATE_KEY")?;
        let private_key = base64::decode(encoded_private_key)?;

        // Decode the key.
        let key = match nom_pem::decode_block(&private_key) {
            Ok(k) => k,
            Err(e) => bail!("nom_pem decode_block failed: {:?}", e),
        };

        // Get the JWT credentials.
        let jwt = JWTCredentials::new(app_id, key.data)?;

        // Create the HTTP cache.
        let http_cache = Box::new(FileBasedCache::new(format!("{}/.cache/github", env::var("HOME")?)));

        let token_generator = InstallationTokenGenerator::new(self.github_app_installation_id.try_into()?, jwt);

        let http = reqwest::Client::builder().build()?;
        let retry_policy = reqwest_retry::policies::ExponentialBackoff::builder().build_with_max_retries(3);
        let client = reqwest_middleware::ClientBuilder::new(http)
            // Trace HTTP requests. See the tracing crate to make use of these traces.
            .with(reqwest_tracing::TracingMiddleware)
            // Retry failed requests.
            .with(reqwest_retry::RetryTransientMiddleware::new_with_policy(retry_policy))
            .build();

        Ok(octorust::Client::custom(
            "https://api.github.com",
            concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")),
            Credentials::InstallationToken(token_generator),
            client,
            http_cache,
        ))
    }
}

pub async fn refresh_companies() -> Result<()> {
    let db = Database::new().await;

    // This should forever only be Oxide.
    let oxide = Company::get_from_db(&db, "Oxide".to_string()).await.unwrap();

    let is: Vec<airtable_api::Record<Company>> = oxide
        .authenticate_airtable(&oxide.airtable_base_id_cio)
        .list_records(&Company::airtable_table(), AIRTABLE_GRID_VIEW, vec![])
        .await?;

    for record in is {
        if record.fields.name.is_empty() || record.fields.website.is_empty() {
            // Ignore it, it's a blank record.
            continue;
        }

        let new_company: NewCompany = record.fields.into();

        let mut company = new_company.upsert_in_db(&db).await?;
        if company.airtable_record_id.is_empty() {
            company.airtable_record_id = record.id;
        }
        company.cio_company_id = oxide.id;
        company.update(&db).await?;
    }
    // Companies are only stored with Oxide.
    Companys::get_from_db(&db, 1).await?.update_airtable(&db).await?;

    Ok(())
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

pub fn get_shipbob_scopes() -> Vec<String> {
    vec![
        "channels_read".to_string(),
        "orders_read".to_string(),
        "orders_write".to_string(),
        "products_read".to_string(),
        "products_write".to_string(),
        "receiving_read".to_string(),
        "receiving_write".to_string(),
        "returns_read".to_string(),
        "returns_write".to_string(),
        "inventory_read".to_string(),
        "webhooks_read".to_string(),
        "webhooks_write".to_string(),
        "locations_read".to_string(),
        "offline_access".to_string(),
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
