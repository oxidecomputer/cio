#![allow(clippy::from_over_into)]
use std::{
    collections::{BTreeMap, HashMap},
    fmt,
    str::from_utf8,
};

use anyhow::{bail, Result};
use async_bb8_diesel::AsyncRunQueryDsl;
use async_trait::async_trait;
use chrono::naive::NaiveDate;
use diesel::{
    deserialize::{self, FromSql},
    pg::{Pg, PgValue},
    serialize::{self, Output, ToSql},
    sql_types::VarChar,
    expression::AsExpression,
    deserialize::FromSqlRow
};
use google_calendar::types::{Event, EventAttendee, EventDateTime};
use google_geocode::Geocode;
use gsuite_api::types::{
    Building as GSuiteBuilding, CalendarResource as GSuiteCalendarResource, Group as GSuiteGroup, User as GSuiteUser,
};
use gusto_api::Client as Gusto;
use log::{info, warn};
use macros::db;
use schemars::JsonSchema;
use sendgrid_api::{traits::MailOps, Client as SendGrid};
use serde::{Deserialize, Serialize};
use zoom_api::Client as Zoom;

use crate::{
    airtable::{
        AIRTABLE_BUILDINGS_TABLE, AIRTABLE_EMPLOYEES_TABLE, AIRTABLE_GROUPS_TABLE, AIRTABLE_LINKS_TABLE,
        AIRTABLE_RESOURCES_TABLE,
    },
    applicants::Applicant,
    certs::{Certificate, Certificates, NewCertificate},
    companies::Company,
    core::UpdateAirtableRecord,
    db::Database,
    gsuite::{update_gsuite_building, update_gsuite_calendar_resource},
    providers::{ProviderReadOps, ProviderWriteOps},
    schema::{applicants, buildings, groups, links, resources, users},
    shipments::NewOutboundShipment,
    utils::{get_file_content_from_repo, get_github_user_public_ssh_keys},
};

/// The data type for our configuration files.
#[derive(Debug, Default, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    pub users: BTreeMap<String, UserConfig>,
    #[serde(default)]
    pub groups: BTreeMap<String, GroupConfig>,

    #[serde(default)]
    pub buildings: BTreeMap<String, BuildingConfig>,

    #[serde(default)]
    pub resources: BTreeMap<String, NewResourceConfig>,

    #[serde(default)]
    pub links: BTreeMap<String, LinkConfig>,

    #[serde(default, alias = "github-outside-collaborators")]
    pub github_outside_collaborators: BTreeMap<String, GitHubOutsideCollaboratorsConfig>,

    #[serde(default)]
    pub huddles: BTreeMap<String, HuddleConfig>,

    #[serde(default)]
    pub certificates: BTreeMap<String, NewCertificate>,
}

#[derive(Debug, Deserialize, Clone, JsonSchema, Serialize, PartialEq, FromSqlRow, AsExpression)]
#[serde(rename_all = "lowercase")]
#[diesel(sql_type = VarChar)]
pub enum ExternalServices {
    Airtable,
    GitHub,
    Google,
    Okta,
    Ramp,
    Zoom,
}

impl ExternalServices {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExternalServices::Airtable => "airtable",
            ExternalServices::GitHub => "github",
            ExternalServices::Google => "google",
            ExternalServices::Okta => "okta",
            ExternalServices::Ramp => "ramp",
            ExternalServices::Zoom => "zoom",
        }
    }

    pub async fn get_provider_writer(
        &self,
        db: &Database,
        company: &Company,
    ) -> Result<Box<dyn ProviderWriteOps + Send + Sync>> {
        Ok(match self {
            // We don't need a base id here since we are only using the enterprise api features.
            ExternalServices::Airtable => Box::new(company.authenticate_airtable("")),
            ExternalServices::GitHub => Box::new(company.authenticate_github()?),
            ExternalServices::Google => Box::new(company.authenticate_google_admin(db).await?),
            ExternalServices::Okta => Box::new(
                company
                    .authenticate_okta()
                    .ok_or_else(|| anyhow::anyhow!("Failed to instantiate Okta client"))?,
            ),
            ExternalServices::Ramp => Box::new(company.authenticate_ramp(db).await?),
            ExternalServices::Zoom => Box::new(company.authenticate_zoom(db).await?),
        })
    }
}

impl fmt::Display for ExternalServices {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExternalServices::Airtable => write!(f, "Airtable"),
            ExternalServices::GitHub => write!(f, "GitHub"),
            ExternalServices::Google => write!(f, "Google"),
            ExternalServices::Okta => write!(f, "Okta"),
            ExternalServices::Ramp => write!(f, "Ramp"),
            ExternalServices::Zoom => write!(f, "Zoom"),
        }
    }
}

impl ToSql<VarChar, Pg> for ExternalServices {
    fn to_sql<'a>(&'a self, out: &mut Output<'a, '_, Pg>) -> serialize::Result {
        <str as ToSql<VarChar, Pg>>::to_sql(self.as_str(), out)
    }
}

impl FromSql<VarChar, Pg> for ExternalServices {
    fn from_sql(bytes: PgValue<'_>) -> deserialize::Result<Self> {
        match bytes.as_bytes() {
            b"airtable" => Ok(ExternalServices::Airtable),
            b"github" => Ok(ExternalServices::GitHub),
            b"google" => Ok(ExternalServices::Google),
            b"okta" => Ok(ExternalServices::Okta),
            b"ramp" => Ok(ExternalServices::Ramp),
            b"zoom" => Ok(ExternalServices::Zoom),
            unknown_service => Err(format!(
                "Encountered unknown external service value {:?} in database. Unable to deserialize.",
                from_utf8(unknown_service)
            )
            .into()),
        }
    }
}

/// The data type for a user.
#[db {
    new_struct_name = "User",
    airtable_base = "directory",
    airtable_table = "AIRTABLE_EMPLOYEES_TABLE",
    match_on = {
        "cio_company_id" = "i32",
        "username" = "String",
    },
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[diesel(table_name = users)]
pub struct UserConfig {
    #[serde(alias = "first_name")]
    pub first_name: String,
    #[serde(alias = "last_name")]
    pub last_name: String,
    pub username: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    #[serde(default, alias = "recovery_email", skip_serializing_if = "String::is_empty")]
    pub recovery_email: String,
    #[serde(default, alias = "recovery_phone", skip_serializing_if = "String::is_empty")]
    pub recovery_phone: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub gender: String,

    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub chat: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub github: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub twitter: String,

    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub department: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub manager: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub link_to_manager: Vec<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<String>,

    #[serde(default, alias = "is_group_admin")]
    pub is_group_admin: bool,

    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub building: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub link_to_building: Vec<String>,

    #[serde(default, alias = "aws_role", skip_serializing_if = "String::is_empty")]
    pub aws_role: String,

    /// Defines a list of services that the user should not be provisioned in or
    /// granted access to
    #[serde(default)]
    pub denied_services: Vec<ExternalServices>,

    /// The following fields do not exist in the config files but are populated
    /// by the Gusto API before the record gets saved in the database.
    /// Home address (automatically populated by Gusto)
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub home_address_street_1: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub home_address_street_2: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub home_address_city: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub home_address_state: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub home_address_zipcode: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub home_address_country: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub home_address_country_code: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub home_address_formatted: String,
    #[serde(default)]
    pub home_address_latitude: f32,
    #[serde(default)]
    pub home_address_longitude: f32,

    /// The following fields do not exist in the config files but are populated
    /// automatically based on the user's location.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub work_address_street_1: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub work_address_street_2: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub work_address_city: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub work_address_state: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub work_address_zipcode: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub work_address_country: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub work_address_country_code: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub work_address_formatted: String,

    /// Start date (automatically populated by Gusto)
    #[serde(
        default = "crate::utils::default_date",
        alias = "start_date",
        serialize_with = "null_date_format::serialize"
    )]
    pub start_date: NaiveDate,
    /// Birthday (automatically populated by Gusto)
    #[serde(
        default = "crate::utils::default_date",
        serialize_with = "null_date_format::serialize"
    )]
    pub birthday: NaiveDate,

    /// The following field does not exist in the config files but is populated by
    /// the GitHub API before the record gets saved in the database.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub public_ssh_keys: Vec<String>,

    #[serde(default, rename = "type", skip_serializing_if = "String::is_empty")]
    pub typev: String,

    /// This field is automatically populated by airtable based on the user's start date.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub google_anniversary_event_id: String,

    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub gusto_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub okta_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub google_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub airtable_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ramp_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub zoom_id: String,

    /// This field is used by Airtable for mapping the location data.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub geocode_cache: String,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub working_on: Vec<String>,

    /// The CIO company ID.
    #[serde(default)]
    pub cio_company_id: i32,
}

pub mod null_date_format {
    use chrono::{naive::NaiveDate, DateTime, TimeZone, Utc};
    use serde::{self, Deserialize, Deserializer, Serializer};

    const FORMAT: &str = "%Y-%m-%d";

    // The signature of a serialize_with function must follow the pattern:
    //
    //    fn serialize<S>(&T, S) -> Result<S::Ok, S::Error>
    //    where
    //        S: Serializer
    //
    // although it may also be generic over the input types T.
    pub fn serialize<S>(date: &NaiveDate, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut s = format!("{}", date.format(FORMAT));
        if *date == crate::utils::default_date() {
            s = "".to_string();
        }
        serializer.serialize_str(&s)
    }

    // The signature of a deserialize_with function must follow the pattern:
    //
    //    fn deserialize<'de, D>(D) -> Result<T, D::Error>
    //    where
    //        D: Deserializer<'de>
    //
    // although it may also be generic over the output types T.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
    where
        D: Deserializer<'de>,
    {
        // TODO: actually get the Unix timestamp.
        let s = String::deserialize(deserializer).unwrap_or_else(|_| "2020-12-03T15:49:27Z".to_string());

        // Try to convert from the string to an int, in case we have a numerical
        // time stamp.
        match s.parse::<i64>() {
            Ok(_int) => {
                // Return the parsed time since epoch.
                return Ok(Utc.datetime_from_str(&s, "%s").unwrap());
            }
            Err(_e) => (),
        }

        Ok(Utc.datetime_from_str(&s, "%+").unwrap())
    }
}

impl UserConfig {
    /// Sync a user from the config file with the services.
    #[allow(clippy::too_many_arguments)]
    pub async fn sync(
        &mut self,
        db: &Database,
        company: &Company,
        github: &octorust::Client,
        gsuite_users_map: &BTreeMap<String, GSuiteUser>,
        okta_users: &HashMap<String, okta::types::User>,
        ramp_users: &HashMap<String, ramp_api::types::User>,
        zoom_users: &HashMap<String, zoom_api::types::UsersResponse>,
        zoom_users_pending: &HashMap<String, zoom_api::types::UsersResponse>,
        gusto_users: &HashMap<String, gusto_api::types::Employee>,
        gusto_users_by_id: &HashMap<String, gusto_api::types::Employee>,
    ) -> Result<()> {
        // Get everything we need to authenticate with GSuite.
        // Initialize the GSuite client.
        let gsuite = company.authenticate_google_admin(db).await?;

        // We don't need a base id here since we are only using the enterprise api features.
        let airtable_auth = company.authenticate_airtable("");

        // Initialize the Gusto client.
        let gusto_auth = company.authenticate_gusto(db).await;

        // Initialize the Okta client.
        let okta_auth = company.authenticate_okta();

        // Initialize the Ramp client.
        let ramp_auth = company.authenticate_ramp(db).await;

        // Initialize the Zoom client.
        let zoom_auth = company.authenticate_zoom(db).await;

        // Set the user's email.
        self.email = format!("{}@{}", self.username, company.gsuite_domain);

        // Check if we already have the new user in the database.
        let existing = User::get_from_db(db, company.id, self.username.to_string()).await;

        // Update or create the user in the database.
        if let Some(e) = existing.clone() {
            self.google_anniversary_event_id = e.google_anniversary_event_id;
        }

        // See if we have a gsuite user for the user.
        if let Some(gsuite_user) = gsuite_users_map.get(&self.email) {
            self.google_id = gsuite_user.id.to_string();
        }

        // See if we have a okta user for the user.
        if let Some(okta_user) = okta_users.get(&self.email) {
            self.okta_id = okta_user.id.to_string();
        }

        // Check if we have a Ramp user for the user.
        if let Some(ramp_user) = ramp_users.get(&self.email) {
            self.ramp_id = ramp_user.id.to_string();
        }

        // See if we have a zoom user for the user.
        if let Some(zoom_user) = zoom_users.get(&self.email) {
            self.zoom_id = zoom_user.id.to_string();
        } else {
            // See if we have a pending zoom user for the user.
            if let Some(zoom_user) = zoom_users_pending.get(&self.email) {
                if !self.zoom_id.is_empty() {
                    self.zoom_id = zoom_user.id.to_string();
                } else if let Some(ref e) = existing.clone() {
                    // Get it from the database.
                    self.zoom_id = e.zoom_id.to_string();
                }
            }
        }

        // See if we have a gusto user for the user.
        // The user's email can either be their personal email or their oxide email.
        if let Some(gusto_user) = gusto_users.get(&self.email) {
            self.update_from_gusto(gusto_user);
        } else if let Some(gusto_user) = gusto_users.get(&self.recovery_email) {
            self.update_from_gusto(gusto_user);
        } else {
            // Grab their date of birth, start date, and address from Airtable.
            if let Some(e) = existing.clone() {
                if let Some(airtable_record) = e.get_existing_airtable_record(db).await {
                    self.home_address_street_1 = airtable_record.fields.home_address_street_1.to_string();
                    self.home_address_street_2 = airtable_record.fields.home_address_street_2.to_string();
                    self.home_address_city = airtable_record.fields.home_address_city.to_string();
                    self.home_address_state = airtable_record.fields.home_address_state.to_string();
                    self.home_address_zipcode = airtable_record.fields.home_address_zipcode.to_string();
                    self.home_address_country = airtable_record.fields.home_address_country.to_string();
                    self.birthday = airtable_record.fields.birthday;
                    // Keep the start date in airtable if we already have one.
                    if self.start_date == crate::utils::default_date()
                        && airtable_record.fields.start_date != crate::utils::default_date()
                    {
                        self.start_date = airtable_record.fields.start_date;
                    }
                    self.gusto_id = airtable_record.fields.gusto_id;
                }

                if !e.gusto_id.is_empty() {
                    if let Some(gusto_user) = gusto_users_by_id.get(&e.gusto_id) {
                        self.update_from_gusto(gusto_user);
                    }
                } else if let Ok((ref gusto, ref gusto_company_id)) = gusto_auth {
                    self.populate_home_address().await?;
                    // Create the user in Gusto if necessary.
                    self.create_in_gusto_if_needed(gusto, gusto_company_id).await?;
                }
            }
        }

        // Expand the user.
        self.expand(db, company).await?;

        let mut new_user = self.upsert(db).await?;

        // Attempt to provision this user with our known external services

        if let Some(ref okta) = okta_auth {
            // ONLY DO THIS IF WE USE OKTA FOR CONFIGURATION,
            // OTHERWISE THE GSUITE CODE WILL SEND ITS OWN EMAIL.
            // Ensure the okta user.
            let okta_id = okta.ensure_user(db, company, &new_user).await?;
            // Set the GSuite ID for the user.
            new_user.okta_id = okta_id.to_string();
            // Update the user in the database.
            new_user = new_user.update(db).await?;
        } else {
            // Update the user in GSuite.
            // ONLY DO THIS IF THE COMPANY DOES NOT USE OKTA.
            let gsuite_id = gsuite.ensure_user(db, company, &new_user).await?;
            // Set the GSuite ID for the user.
            new_user.google_id = gsuite_id.to_string();
            // Update the user in the database.
            new_user = new_user.update(db).await?;

            // Create a zoom account for the user, if we have zoom credentials and
            // we cannot find the zoom user.
            // Otherwise update the zoom user.
            // We only do this if not managed by Okta.
            if let Ok(ref zoom) = zoom_auth {
                match zoom.ensure_user(db, company, &new_user).await {
                    Ok(zoom_id) => {
                        // Set the Zoom ID for the user.
                        new_user.zoom_id = zoom_id.to_string();
                        // Update the user in the database.
                        new_user = new_user.update(db).await?;
                    }
                    Err(e) => {
                        warn!("Failed to ensure zoom user `{}`: {}", new_user.id, e);
                    }
                }
            }
        }

        // Add the user to their GitHub teams and the org.
        if !new_user.github.is_empty() {
            // Add them to the org and any teams they need to be added to.
            // We don't return an id here.
            match github.ensure_user(db, company, &new_user).await {
                Ok(id) => Ok(id),
                Err(err) => {
                    warn!("Failed to ensure GitHub user `{}`: {}", new_user.id, err);
                    Err(err)
                }
            }?;
        }

        if let Ok(ref ramp) = ramp_auth {
            match ramp.ensure_user(db, company, &new_user).await {
                Ok(ramp_id) => {
                    // Set the Ramp ID for the user.
                    new_user.ramp_id = ramp_id.to_string();
                    // Update the user in the database.
                    new_user = new_user.update(db).await?;
                }
                Err(e) => {
                    warn!("Failed to ensure ramp user `{}`: {}", new_user.id, e);
                }
            }
        }

        // Get the Airtable information for the user.
        match airtable_auth.ensure_user(db, company, &new_user).await {
            Ok(airtable_id) => {
                new_user.airtable_id = airtable_id;

                // Update the user in the database.
                new_user = new_user.update(db).await?;
            }
            Err(e) => {
                warn!("Failed to ensure airtable user `{}`: {}", new_user.id, e);
            }
        }

        // Deprovision this user explicitly from any service they should not have access to
        for denied_service in &new_user.denied_services {
            match denied_service.get_provider_writer(db, company).await {
                Ok(denied_service_provider) => {
                    info!(
                        "Removing user {} from {} as they are denied access in their config",
                        new_user.id, denied_service
                    );

                    match denied_service_provider.delete_user(db, company, &new_user).await {
                        Ok(_) => info!("Removed user {} from {}", new_user.id, denied_service),
                        Err(err) => warn!(
                            "Failed to remove user {} from {}. err: {:?}",
                            new_user.id, denied_service, err
                        ),
                    }
                }
                Err(err) => warn!(
                    "Failed to create provider client for {} when handling denied services for user {}. err: {}",
                    denied_service, new_user.id, err
                ),
            }
        }

        // Update with any other changes we made to the user.
        new_user.update(db).await?;

        Ok(())
    }

    pub async fn create_in_gusto_if_needed(&mut self, gusto: &Gusto, gusto_company_id: &str) -> Result<()> {
        // Only do this if we have a start date.
        if self.start_date == crate::utils::default_date() {
            // Return early.
            return Ok(());
        }

        // If we don't know their address yet, return early.
        if self.home_address_street_1.is_empty() || self.home_address_country.is_empty() {
            // Return early.
            return Ok(());
        }

        // If they are not in the US skip them.
        if self.home_address_country != "US"
            && self.home_address_country != "United States"
            && self.home_address_country != "USA"
        {
            // Return early.
            return Ok(());
        }

        // If they are not full-time, return early.
        if !self.is_full_time() {
            // Return early.
            return Ok(());
        }

        if !self.gusto_id.is_empty() {
            // Return early, they already exist in Gusto.
            return Ok(());
        }

        // Create the applicant in Gusto.
        let employee = gusto
            .employees()
            .post(
                gusto_company_id,
                &gusto_api::types::PostEmployeesRequest {
                    first_name: self.first_name.to_string(),
                    middle_initial: "".to_string(),
                    last_name: self.last_name.to_string(),
                    email: self.recovery_email.to_string(),
                    date_of_birth: None,
                    ssn: "".to_string(),
                },
            )
            .await?;
        // Set the gusto id.
        self.gusto_id = employee.id.to_string();

        // Update the address for the employee in gusto.
        // The state needs to be the abbreviation.
        let state = crate::states::StatesMap::shorthand(&self.home_address_state);
        gusto
            .employees()
            .put_home_address(
                &self.gusto_id,
                &gusto_api::types::PutEmployeeHomeAddressRequest {
                    version: "".to_string(),
                    street_1: self.home_address_street_1.to_string(),
                    street_2: self.home_address_street_2.to_string(),
                    city: self.home_address_city.to_string(),
                    state,
                    zip: self.home_address_zipcode.to_string(),
                },
            )
            .await?;

        Ok(())
    }

    fn update_from_gusto(&mut self, gusto_user: &gusto_api::types::Employee) {
        self.gusto_id = gusto_user.id.to_string();

        if gusto_user.jobs.is_empty() {
            // Return early.
            return;
        }

        // Update the user's start date.
        if let Some(start_date) = gusto_user.jobs[0].hire_date {
            self.start_date = start_date;
        }

        // Update the user's birthday.
        if let Some(birthday) = gusto_user.date_of_birth {
            self.birthday = birthday;
        }

        // Update the user's home address.
        // Gusto now becomes the source of truth for people's addresses.
        if let Some(home_address) = &gusto_user.home_address {
            self.home_address_street_1 = home_address.street_1.to_string();
            self.home_address_street_2 = home_address.street_2.to_string();
            self.home_address_city = home_address.city.to_string();
            self.home_address_state = home_address.state.to_string();
            self.home_address_zipcode = home_address.zip.to_string();
            self.home_address_country = home_address.country.to_string();
        }

        if self.home_address_country == "US"
            || self.home_address_country == "USA"
            || self.home_address_country.is_empty()
        {
            self.home_address_country = "United States".to_string();
        }
    }

    async fn populate_ssh_keys(&mut self) -> Result<()> {
        if self.github.is_empty() {
            // Return early if we don't know their github handle.
            return Ok(());
        }

        self.public_ssh_keys = get_github_user_public_ssh_keys(&self.github).await?;

        Ok(())
    }

    async fn populate_home_address(&mut self) -> Result<()> {
        let mut street_address = self.home_address_street_1.to_string();
        if !self.home_address_street_2.is_empty() {
            street_address = format!("{}\n{}", self.home_address_street_1, self.home_address_street_2,);
        }
        // Make sure the state is not an abreev.
        self.home_address_state = crate::states::StatesMap::match_abreev_or_return_existing(&self.home_address_state);

        // Set the formatted address.
        self.home_address_formatted = format!(
            "{}\n{}, {} {} {}",
            street_address,
            self.home_address_city,
            self.home_address_state,
            self.home_address_zipcode,
            self.home_address_country
        )
        .trim()
        .trim_matches(',')
        .trim()
        .to_string();

        // Populate the country code.
        if self.home_address_country.is_empty() || self.home_address_country == "United States" {
            self.home_address_country = "United States".to_string();
            self.home_address_country_code = "US".to_string();
        }

        if !self.home_address_formatted.is_empty() {
            // Create the geocode client.
            let geocode = Geocode::new_from_env();
            // Get the latitude and longitude.
            if let Ok(result) = geocode.get(&self.home_address_formatted).await {
                let location = result.geometry.location;
                self.home_address_latitude = location.lat as f32;
                self.home_address_longitude = location.lng as f32;
            }
        }

        Ok(())
    }

    async fn populate_work_address(&mut self, db: &Database) {
        // Populate the address based on the user's location.
        if !self.building.is_empty() {
            // The user has an actual building for their work address.
            // Let's get it.
            let building = Building::get_from_db(db, self.cio_company_id, self.building.to_string())
                .await
                .unwrap();
            // Now let's set their address to the building's address.
            self.work_address_street_1 = building.street_address.to_string();
            self.work_address_street_2 = "".to_string();
            self.work_address_city = building.city.to_string();
            self.work_address_state = crate::states::StatesMap::match_abreev_or_return_existing(&building.state);
            self.work_address_zipcode = building.zipcode.to_string();
            self.work_address_country = building.country.to_string();
            if self.work_address_country == "US"
                || self.work_address_country == "USA"
                || self.work_address_country.is_empty()
            {
                self.work_address_country = "United States".to_string();
            }
            self.work_address_formatted = building.address_formatted.to_string();

            let city_group = building.city.to_lowercase().replace(' ', "-");

            // Ensure we have added the group for that city.
            if !self.groups.contains(&city_group) {
                self.groups.push(city_group);
            }
        } else {
            // They are remote so we should use their home address.
            self.work_address_street_1 = self.home_address_street_1.to_string();
            self.work_address_street_2 = self.home_address_street_2.to_string();
            self.work_address_city = self.home_address_city.to_string();
            self.work_address_state =
                crate::states::StatesMap::match_abreev_or_return_existing(&self.home_address_state);
            self.work_address_zipcode = self.home_address_zipcode.to_string();
            self.work_address_country = self.home_address_country.to_string();
            self.work_address_country_code = self.home_address_country_code.to_string();
            self.work_address_formatted = self.home_address_formatted.to_string();

            if self.typev != "system account" && self.typev != "consultant" {
                let group = "remote".to_string();
                // Ensure we have added the remote group.
                if !self.groups.contains(&group) {
                    self.groups.push(group);
                }
            }
        }

        // Populate the country code.
        if self.work_address_country.is_empty() || self.work_address_country == "United States" {
            self.work_address_country = "United States".to_string();
            self.work_address_country_code = "US".to_string();
        }

        // Replace new lines.
        self.work_address_formatted = self.work_address_formatted.replace('\n', "\\n");
    }

    pub async fn populate_start_date(&mut self, db: &Database) {
        // Only populate the start date, if we could not update it from Gusto.
        if self.start_date == crate::utils::default_date() {
            if let Ok(a) = applicants::dsl::applicants
                .filter(applicants::dsl::email.eq(self.recovery_email.to_string()))
                .first_async::<Applicant>(db.pool())
                .await
            {
                // Get their start date.
                if a.start_date.is_some() {
                    self.start_date = a.start_date.unwrap();
                }
            }
        }
    }

    pub fn populate_type(&mut self) {
        // TODO: make this an enum.
        self.typev = "full-time".to_string();
        if self.groups.contains(&"consultants".to_string()) {
            self.typev = "consultant".to_string();
        } else if self.groups.contains(&"system-accounts".to_string()) {
            self.typev = "system account".to_string();
        }
    }

    pub fn is_full_time(&mut self) -> bool {
        if self.typev.is_empty() {
            self.populate_type();
        }

        self.typev == "full-time"
    }

    pub fn ensure_all_aliases(&mut self) {
        if !self.github.is_empty() && !self.aliases.contains(&self.github) {
            self.aliases.push(self.github.to_string());
        }

        if !self.twitter.is_empty() && !self.aliases.contains(&self.twitter) {
            self.aliases.push(self.twitter.to_string());
        }

        let name_alias = format!(
            "{}.{}",
            self.first_name.to_lowercase().replace(' ', "-"),
            self.last_name.to_lowercase().replace(' ', "-").replace('á', "a")
        );
        if !self.aliases.contains(&name_alias) && self.username != name_alias {
            self.aliases.push(name_alias);
        }
    }

    pub fn ensure_all_groups(&mut self) {
        let mut department_group = self.department.to_lowercase().trim().to_string();
        if department_group == "engineering" {
            department_group = "eng".to_string();
        }
        if !department_group.is_empty() && !self.groups.contains(&department_group) {
            self.groups.push(department_group);
        }
    }

    pub async fn expand(&mut self, db: &Database, company: &Company) -> Result<()> {
        self.cio_company_id = company.id;

        self.email = format!("{}@{}", self.username, company.gsuite_domain);

        // Do this first.
        self.populate_type();

        self.ensure_all_aliases();
        self.ensure_all_groups();

        self.populate_ssh_keys().await?;

        self.populate_home_address().await?;
        self.populate_work_address(db).await;

        self.populate_start_date(db).await;

        // Create the link to the manager.
        if !self.manager.is_empty() {
            self.link_to_manager = vec![self.manager.to_string()];
        }

        // Title case the department.
        self.department = titlecase::titlecase(&self.department);

        Ok(())
    }
}

impl User {
    /// Get the user's manager, if they have one, otherwise return Jess.
    pub async fn manager(&self, db: &Database) -> User {
        let mut manager = self.manager.to_string();
        if manager.is_empty() {
            manager = "jess".to_string();
        }

        User::get_from_db(db, self.cio_company_id, manager).await.unwrap()
    }

    /// Generate and return the full name for the user.
    pub fn full_name(&self) -> String {
        format!("{} {}", self.first_name, self.last_name)
    }

    pub fn is_system_account(&self) -> bool {
        self.typev == "system account"
    }

    pub fn is_consultant(&self) -> bool {
        self.typev == "consultant"
    }

    pub fn is_full_time(&self) -> bool {
        self.typev == "full-time"
    }

    /// Create an internal swag shipment to an employee's home address.
    /// This will:
    /// - Check if the user has a home address.
    /// - Create a record in outgoing shipments.
    /// - Generate the shippo label.
    /// - Print said shippo label.
    pub async fn create_shipment_to_home_address(&self, db: &Database) -> Result<()> {
        // First let's check if the user even has an address.
        // If not we can return early.
        if self.home_address_formatted.is_empty() {
            warn!(
                "cannot create shipping label for user {} since we don't know their home address",
                self.username
            );
            return Ok(());
        }

        // Let's create the shipment.
        let new_shipment = NewOutboundShipment::from(self.clone());
        // Let's add it to our database.
        let mut shipment = new_shipment.upsert_in_db(db).await?;
        // Create the shipment in shippo.
        shipment.create_or_get_shippo_shipment(db).await?;
        // Update airtable and the database again.
        shipment.update(db).await?;

        Ok(())
    }

    /// Send an email to the new consultant about their account.
    pub async fn send_email_new_consultant(&self, db: &Database) -> Result<()> {
        let company = self.company(db).await?;

        // Initialize the SendGrid client.
        let sendgrid = SendGrid::new_from_env();

        // Get the user's aliases if they have one.
        let aliases = self.aliases.join(", ");

        // Send the message.
        sendgrid
            .mail_send()
            .send_plain_text(
                &format!("Your New Email Account: {}", self.email),
                &format!(
                    "Yoyoyo {},

You should have an email from Okta about setting up your account with them.
We use Okta to authenticate to a number of different apps -- including
Google Workspace. This includes email, calendar, drive, etc.

After setting up your Okta account your email account with Google will be
provisioned. You can then login to your email from: mail.corp.{}.
Details for accessing are below.

Website for Okta login: https://oxidecomputerlogin.okta.com
Website for email login: https://mail.corp.{}
Email: {}
Aliases: {}

Make sure you set up two-factor authentication for your account, or in one week
you will be locked out.

If you have any questions or your email does not work please email your
administrator, who is cc-ed on this email. Spoiler alert it's Jess...
jess@{}.

xoxo,
  The Onboarding Bot",
                    self.first_name, company.domain, company.domain, self.email, aliases, company.gsuite_domain,
                ),
                &[self.recovery_email.to_string()],
                &[self.email.to_string(), format!("jess@{}", company.gsuite_domain)],
                &[],
                &format!("admin@{}", company.gsuite_domain),
            )
            .await?;

        Ok(())
    }

    /// Send an email to the GSuite user about their account.
    pub async fn send_email_new_gsuite_user(&self, db: &Database, password: &str) -> Result<()> {
        let company = self.company(db).await?;

        // Initialize the SendGrid client.
        let sendgrid = SendGrid::new_from_env();

        // Get the user's aliases if they have one.
        let aliases = self.aliases.join(", ");

        // Send the message.
        sendgrid
            .mail_send()
            .send_plain_text(
                &format!("Your New Email Account: {}", self.email),
                &format!(
                    "Yoyoyo {},

We have set up your account on mail.corp.{}. Details for accessing
are below. You will be required to reset your password the next time you login.

Website for Login: https://mail.corp.{}
Email: {}
Password: {}
Aliases: {}

Make sure you set up two-factor authentication for your account, or in one week
you will be locked out.

Your GitHub @{} has been added to our organization (https://github.com/{}) and
various teams within it. GitHub should have sent an email with instructions on
accepting the invitation to our organization to the email you used
when you signed up for GitHub. Or you can alternatively accept our invitation
by going to https://github.com/{}.

If you have any questions or your email does not work please email your
administrator, who is cc-ed on this email. Spoiler alert it's Jess...
jess@{}. If you want other email aliases, let Jess know as well.

xoxo,
  The Onboarding Bot",
                    self.first_name,
                    company.domain,
                    company.domain,
                    self.email,
                    password,
                    aliases,
                    self.github,
                    company.github_org,
                    company.github_org,
                    company.gsuite_domain,
                ),
                &[self.recovery_email.to_string()],
                &[self.email.to_string(), format!("jess@{}", company.gsuite_domain)],
                &[],
                &format!("admin@{}", company.gsuite_domain),
            )
            .await?;

        Ok(())
    }

    /// Send an email to the new user about their account.
    pub async fn send_email_new_user(&self, db: &Database) -> Result<()> {
        let company = self.company(db).await?;
        // Initialize the SendGrid client.
        let sendgrid = SendGrid::new_from_env();

        // Get the user's aliases if they have one.
        let aliases = self.aliases.join(", ");

        let mut github_copy = format!(
            "Your GitHub @{} has been added to our organization (https://github.com/{})
and various \
             teams within it. GitHub should have sent an email with instructions on
accepting the invitation \
             to our organization to the email you used
when you signed up for GitHub. Or you can alternatively \
             accept our invitation
by going to https://github.com/{}.",
            self.github, company.github_org, company.github_org
        );
        if self.github.is_empty() {
            // Let the new hire know they need to create a GitHub account.
            github_copy = format!(
                "We do not have a github account for you. You will need to create one at https://github.com
OR let jess@{} know your handle, if you already have one. Either way, be sure to
let jess@{} know what your GitHub handle is.",
                company.gsuite_domain, company.gsuite_domain
            );
        }

        // Send the message.
        sendgrid
            .mail_send()
            .send_plain_text(
                &format!("Your New Email Account: {}", self.email),
                &format!(
                    "Yoyoyo {},

You should have an email from Okta about setting up your account with them.
We use Okta to authenticate to a number of different apps -- including
Google Workspace and GitHub. This includes email, calendar, drive, etc.

After setting up your Okta account your email account with Google will be
provisioned. You can then login to your email from: mail.corp.{}.

Details for accessing are below.

Website for Okta login: https://oxidecomputerlogin.okta.com
Website for email login: https://mail.corp.{}
Email: {}
Aliases: {}

Make sure you set up two-factor authentication for your account, or in one week
you will be locked out.

{}

If you have any questions or your email does not work please email your
administrator, who is cc-ed on this email. Spoiler alert it's Jess...
jess@{}. If you want other email aliases, let Jess know as well.

You can find more onboarding information in GitHub:
https://github.com/{}/meta/blob/master/general/onboarding.md

You can find information about internal processes and applications at:
https://github.com/{}/meta/blob/master/general/README.md

As a first contribution to one of our repos, add a book
to our internal library: https://github.com/{}/library

We use Airtable for storing just about everything. You can login with single
sign-on (SSO) after setting up your email at:
https://airtable.com/sso/login.

You will automatically be added to the workspace after you are finished setting up
your email.

We have both a Riot server and a Slack for chat. Josh (josh@oxidecomputer.com) can get

you set up with an account on the Riot server. You can use SSO to login to the Slack
at https://oxidecomputer.slack.com. Once you have a matrix chat account, you can
update your chat handle in the configs repo:
https://github.com/oxidecomputer/configs/blob/master/configs/users.toml.
It's pretty self explanatory if you look at the other users and then your user
where `chat = ''`. If you need help you can ask Josh or Jess.

Lastly, be sure to order yourself some swag: https://swag.oxide.computer


xoxo,
  The Onboarding Bot",
                    self.first_name,
                    company.domain,
                    company.domain,
                    self.email,
                    aliases,
                    github_copy,
                    company.gsuite_domain,
                    company.github_org,
                    company.github_org,
                    company.github_org,
                ),
                &[self.recovery_email.to_string()],
                &[self.email.to_string(), format!("jess@{}", company.gsuite_domain)],
                &[],
                &format!("admin@{}", company.gsuite_domain),
            )
            .await?;

        Ok(())
    }

    pub async fn update_zoom_vanity_name(
        &self,
        db: &Database,
        zoom: &Zoom,
        zoom_user_id: &str,
        zu: &zoom_api::types::UserResponseAllOf,
        vanity_name: &str,
    ) -> Result<()> {
        let update_user = zoom_api::types::UserUpdate {
            // Set values from Zoom.
            cms_user_id: zu.user_response.cms_user_id.to_string(),
            company: zu.user_response.company.to_string(),
            // Since this is a PATCH call, we can pass None, here just fine.
            custom_attributes: None,
            host_key: zu.user_response.host_key.to_string(),
            job_title: zu.user_response.job_title.to_string(),
            language: zu.user_response.language.to_string(),
            location: zu.user_response.location.to_string(),

            // Set more values from Zoom.
            pmi: zu.user.pmi,
            type_: zu.user.type_,
            timezone: zu.user.timezone.to_string(),

            // Get the groups information.
            group_id: zu.groups.id.to_string(),

            // This is depreciated, user phone_numbers instead.
            phone_country: "US".to_string(),
            phone_number: self.recovery_phone.trim_start_matches("+1").to_string(),

            // Set our values.
            vanity_name: vanity_name.to_string(),
            use_pmi: true,
            dept: self.department.to_string(),
            first_name: self.first_name.to_string(),
            last_name: self.last_name.to_string(),
            manager: self.manager(db).await.email,
            /*
             * This is broken and should be an array the spec is wrong.
             * FIX THIS WHEN THE SPEC IS FIXED.
            * */
            /*phone_numbers: Some(zoom_api::types::PhoneNumbers {
                // TODO: Make this work for people outside the US as well.
                code: "+1".to_string(),
                number: self.recovery_phone.trim_start_matches("+1").to_string(),
                label: Some(zoom_api::types::Label::Mobile),
                // TODO: Make this work for people outside the US as well.
                country: "US".to_string(),
            }),*/
            phone_numbers: None,
        };

        zoom.users()
            .update(
                zoom_user_id,
                zoom_api::types::LoginType::Noop, // We don't know their login type...
                &update_user,
            )
            .await
    }
}

/// Implement updating the Airtable record for a User.
#[async_trait]
impl UpdateAirtableRecord<User> for User {
    async fn update_airtable_record(&mut self, record: User) -> Result<()> {
        // Get the current groups in Airtable so we can link to them.
        // TODO: make this more dry so we do not call it every single damn time.
        let db = Database::new().await;
        let groups = Groups::get_from_airtable(&db, self.cio_company_id).await?;

        let mut links: Vec<String> = Default::default();
        // Iterate over the group names in our record and match it against the
        // group ids and see if we find a match.
        for group in &self.groups {
            // Iterate over the groups to get the ID.
            for g in groups.values() {
                if *group == g.fields.name {
                    // Append the ID to our links.
                    links.push(g.id.to_string());
                    // Break the loop and return early.
                    break;
                }
            }
        }

        self.groups = links;

        self.geocode_cache = record.geocode_cache.to_string();

        if self.start_date == crate::utils::default_date() && record.start_date != crate::utils::default_date() {
            self.start_date = record.start_date;
        }

        if !record.google_anniversary_event_id.is_empty() {
            self.google_anniversary_event_id = record.google_anniversary_event_id.to_string();
        }

        // Set the building to right building link.
        // Get the current buildings in Airtable so we can link to it.
        // TODO: make this more dry so we do not call it every single damn time.
        let buildings = Buildings::get_from_airtable(&db, self.cio_company_id).await?;
        // Iterate over the buildings to get the ID.
        for building in buildings.values() {
            if self.building == building.fields.name {
                // Set the ID.
                self.link_to_building = vec![building.id.to_string()];
                // Break the loop and return early.
                break;
            }
        }

        self.work_address_formatted = self.work_address_formatted.replace("\\n", "\n");

        Ok(())
    }
}

/// The data type for a group. This applies to Google Groups.
#[db {
    new_struct_name = "Group",
    airtable_base = "directory",
    airtable_table = "AIRTABLE_GROUPS_TABLE",
    match_on = {
        "cio_company_id" = "i32",
        "name" = "String",
    },
}]
#[derive(Debug, Default, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[diesel(table_name = groups)]
pub struct GroupConfig {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub link: String,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub members: Vec<String>,

    /// Specific repos this group should have access to.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repos: Vec<String>,

    /// allow_external_members: Identifies whether members external to your
    /// organization can join the group. Possible values are:
    /// - true: G Suite users external to your organization can become
    /// members of this group.
    /// - false: Users not belonging to the organization are not allowed to
    /// become members of this group.
    #[serde(default, alias = "allow_external_members")]
    pub allow_external_members: bool,

    /// allow_web_posting: Allows posting from web. Possible values are:
    /// - true: Allows any member to post to the group forum.
    /// - false: Members only use Gmail to communicate with the group.
    #[serde(default, alias = "allow_web_posting")]
    pub allow_web_posting: bool,

    /// is_archived: Allows the Group contents to be archived. Possible values
    /// are:
    /// - true: Archive messages sent to the group.
    /// - false: Do not keep an archive of messages sent to this group. If
    /// false, previously archived messages remain in the archive.
    #[serde(default, alias = "is_archived")]
    pub is_archived: bool,

    /// who_can_discover_group: Specifies the set of users for whom this group
    /// is discoverable. Possible values are:
    /// - ANYONE_CAN_DISCOVER
    /// - ALL_IN_DOMAIN_CAN_DISCOVER
    /// - ALL_MEMBERS_CAN_DISCOVER
    #[serde(alias = "who_can_discover_group", skip_serializing_if = "String::is_empty", default)]
    pub who_can_discover_group: String,

    /// who_can_join: Permission to join group. Possible values are:
    /// - ANYONE_CAN_JOIN: Anyone in the account domain can join. This
    /// includes accounts with multiple domains.
    /// - ALL_IN_DOMAIN_CAN_JOIN: Any Internet user who is outside your
    /// domain can access your Google Groups service and view the list of
    /// groups in your Groups directory. Warning: Group owners can add
    /// external addresses, outside of the domain to their groups. They can
    /// also allow people outside your domain to join their groups. If you
    /// later disable this option, any external addresses already added to
    /// users' groups remain in those groups.
    /// - INVITED_CAN_JOIN: Candidates for membership can be invited to join.
    ///
    /// - CAN_REQUEST_TO_JOIN: Non members can request an invitation to join.
    #[serde(alias = "who_can_join", skip_serializing_if = "String::is_empty", default)]
    pub who_can_join: String,

    /// who_can_moderate_members: Specifies who can manage members. Possible
    /// values are:
    /// - ALL_MEMBERS
    /// - OWNERS_AND_MANAGERS
    /// - OWNERS_ONLY
    /// - NONE
    #[serde(
        alias = "who_can_moderate_members",
        skip_serializing_if = "String::is_empty",
        default
    )]
    pub who_can_moderate_members: String,

    /// who_can_post_message: Permissions to post messages. Possible values are:
    ///
    /// - NONE_CAN_POST: The group is disabled and archived. No one can post
    /// a message to this group.
    /// - When archiveOnly is false, updating who_can_post_message to
    /// NONE_CAN_POST, results in an error.
    /// - If archiveOnly is reverted from true to false, who_can_post_messages
    /// is set to ALL_MANAGERS_CAN_POST.
    /// - ALL_MANAGERS_CAN_POST: Managers, including group owners, can post
    /// messages.
    /// - ALL_MEMBERS_CAN_POST: Any group member can post a message.
    /// - ALL_OWNERS_CAN_POST: Only group owners can post a message.
    /// - ALL_IN_DOMAIN_CAN_POST: Anyone in the account can post a message.
    ///
    /// - ANYONE_CAN_POST: Any Internet user who outside your account can
    /// access your Google Groups service and post a message. Note: When
    /// who_can_post_message is set to ANYONE_CAN_POST, we recommend the
    /// messageModerationLevel be set to MODERATE_NON_MEMBERS to protect the
    /// group from possible spam.
    #[serde(alias = "who_can_post_message", skip_serializing_if = "String::is_empty", default)]
    pub who_can_post_message: String,

    /// who_can_view_group: Permissions to view group messages. Possible values
    /// are:
    /// - ANYONE_CAN_VIEW: Any Internet user can view the group's messages.
    ///
    /// - ALL_IN_DOMAIN_CAN_VIEW: Anyone in your account can view this
    /// group's messages.
    /// - ALL_MEMBERS_CAN_VIEW: All group members can view the group's
    /// messages.
    /// - ALL_MANAGERS_CAN_VIEW: Any group manager can view this group's
    /// messages.
    #[serde(alias = "who_can_view_group", skip_serializing_if = "String::is_empty", default)]
    pub who_can_view_group: String,

    /// who_can_view_membership: Permissions to view membership. Possible values
    /// are:
    /// - ALL_IN_DOMAIN_CAN_VIEW: Anyone in the account can view the group
    /// members list.
    /// If a group already has external members, those members can still send
    /// email to this group.
    ///
    /// - ALL_MEMBERS_CAN_VIEW: The group members can view the group members
    /// list.
    /// - ALL_MANAGERS_CAN_VIEW: The group managers can view group members
    /// list.
    #[serde(alias = "who_can_view_membership", skip_serializing_if = "String::is_empty", default)]
    pub who_can_view_membership: String,

    /// Specifies whether a collaborative inbox will remain turned on for the group.
    #[serde(default)]
    pub enable_collaborative_inbox: bool,
    /// The CIO company ID.
    #[serde(default)]
    pub cio_company_id: i32,
}

impl GroupConfig {
    pub fn get_link(&self, company: &Company) -> String {
        format!(
            "https://groups.google.com/a/{}/forum/#!forum/{}",
            company.gsuite_domain, self.name
        )
    }

    pub fn expand(&mut self, company: &Company) {
        self.link = self.get_link(company);

        self.cio_company_id = company.id;
    }
}

/// Implement updating the Airtable record for a Group.
#[async_trait]
impl UpdateAirtableRecord<Group> for Group {
    async fn update_airtable_record(&mut self, record: Group) -> Result<()> {
        // Make sure we don't mess with the members since that is populated by the Users table.
        self.members = record.members;
        Ok(())
    }
}

/// The data type for a building.
#[db {
    new_struct_name = "Building",
    airtable_base = "directory",
    airtable_table = "AIRTABLE_BUILDINGS_TABLE",
    match_on = {
        "cio_company_id" = "i32",
        "name" = "String",
    },
}]
#[derive(Debug, Insertable, AsChangeset, Default, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[diesel(table_name = buildings)]
pub struct BuildingConfig {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub street_address: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub city: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub state: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub zipcode: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub country: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub address_formatted: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub floors: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub phone: String,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub employees: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conference_rooms: Vec<String>,

    /// This field is used by Airtable for mapping the location data.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub geocode_cache: String,
    /// The CIO company ID.
    #[serde(default)]
    pub cio_company_id: i32,
}

impl BuildingConfig {
    pub fn expand(&mut self, company: &Company) {
        self.address_formatted = format!(
            "{}\n{}, {} {}, {}",
            self.street_address, self.city, self.state, self.zipcode, self.country
        );

        self.cio_company_id = company.id;
    }
}

/// Implement updating the Airtable record for a Building.
#[async_trait]
impl UpdateAirtableRecord<Building> for Building {
    async fn update_airtable_record(&mut self, record: Building) -> Result<()> {
        // Make sure we don't mess with the employees since that is populated by the Users table.
        self.employees = record.employees.clone();
        // Make sure we don't mess with the conference_rooms since that is populated by the Conference Rooms table.
        self.conference_rooms = record.conference_rooms;

        self.geocode_cache = record.geocode_cache;

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, JsonSchema, Serialize, Deserialize, FromSqlRow, AsExpression)]
#[diesel(sql_type = VarChar)]
pub enum ResourceCategory {
    ConferenceRoom,
    Other,
}

impl Default for ResourceCategory {
    fn default() -> Self {
        ResourceCategory::ConferenceRoom
    }
}

impl ResourceCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            ResourceCategory::ConferenceRoom => "ConferenceRoom",
            ResourceCategory::Other => "Other",
        }
    }

    pub fn to_api_value(&self) -> String {
        match self {
            ResourceCategory::ConferenceRoom => "CONFERENCE_ROOM".to_string(),
            ResourceCategory::Other => "OTHER".to_string(),
        }
    }
}

impl ToSql<VarChar, Pg> for ResourceCategory {
    fn to_sql<'a>(&'a self, out: &mut Output<'a, '_, Pg>) -> serialize::Result {
        <str as ToSql<VarChar, Pg>>::to_sql(self.as_str(), out)
    }
}

impl FromSql<VarChar, Pg> for ResourceCategory {
    fn from_sql(bytes: PgValue<'_>) -> deserialize::Result<Self> {
        match bytes.as_bytes() {
            b"ConferenceRoom" => Ok(ResourceCategory::ConferenceRoom),
            b"Other" => Ok(ResourceCategory::Other),
            _ => Err("Unrecognized enum variant".into()),
        }
    }
}

fn default_resource_category() -> ResourceCategory {
    ResourceCategory::ConferenceRoom
}

/// The data type for a resource. These are conference rooms, machines, or other resources with fixed
/// availability that people can book through GSuite.
#[db {
    new_struct_name = "Resource",
    airtable_base = "directory",
    airtable_table = "AIRTABLE_RESOURCES_TABLE",
    match_on = {
        "cio_company_id" = "i32",
        "name" = "String",
    },
}]
#[derive(Debug, Insertable, AsChangeset, Default, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[diesel(table_name = resources)]
pub struct NewResourceConfig {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(rename = "type")]
    pub typev: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub building: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub link_to_building: Vec<String>,
    pub capacity: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub floor: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub section: String,
    #[serde(default = "default_resource_category")]
    pub category: ResourceCategory,
    /// The CIO company ID.
    #[serde(default)]
    pub cio_company_id: i32,
}

/// Implement updating the Airtable record for a Resource.
#[async_trait]
impl UpdateAirtableRecord<Resource> for Resource {
    async fn update_airtable_record(&mut self, _record: Resource) -> Result<()> {
        // Set the building to right building link.
        // Get the current buildings in Airtable so we can link to it.
        // TODO: make this more dry so we do not call it every single damn time.
        let db = Database::new().await;
        let buildings = Buildings::get_from_airtable(&db, self.cio_company_id).await?;
        // Iterate over the buildings to get the ID.
        for building in buildings.values() {
            if self.building == building.fields.name {
                // Set the ID.
                self.link_to_building = vec![building.id.to_string()];
                // Break the loop and return early.
                break;
            }
        }

        Ok(())
    }
}

/// The data type for a link. These get turned into short links like
/// `{name}.corp.oxide.compuer` by the `shorturls` subcommand.
#[db {
    new_struct_name = "Link",
    airtable_base = "directory",
    airtable_table = "AIRTABLE_LINKS_TABLE",
    match_on = {
        "cio_company_id" = "i32",
        "name" = "String",
    },
}]
#[derive(Debug, Insertable, AsChangeset, Default, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[diesel(table_name = links)]
pub struct LinkConfig {
    /// name will not be used in config files.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    pub description: String,
    pub link: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub short_link: String,
    /// The CIO company ID.
    #[serde(default)]
    pub cio_company_id: i32,
}

/// Implement updating the Airtable record for a Link.
#[async_trait]
impl UpdateAirtableRecord<Link> for Link {
    async fn update_airtable_record(&mut self, _record: Link) -> Result<()> {
        Ok(())
    }
}

/// The data type for GitHub outside collaborators to repositories.
#[derive(Debug, Default, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
pub struct GitHubOutsideCollaboratorsConfig {
    pub description: String,
    pub users: Vec<String>,
    pub repos: Vec<String>,
    pub perm: String,
}

/// The data type for a huddle meeting that syncs with Airtable and notes in GitHub.
#[derive(Debug, Default, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
pub struct HuddleConfig {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    pub description: String,
    pub airtable_base_id: String,
    pub email: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub link_to_airtable_form: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub link_to_airtable_workspace: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub calendar_event_fuzzy_search: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub link_to_notes: String,
    #[serde(default)]
    pub time_to_cancel: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub calendar_owner: String,
}

impl HuddleConfig {
    // Return the full domain id for the calendar.
    pub fn calendar_id(&self, company: &Company) -> String {
        format!("{}@{}", self.calendar_owner, company.gsuite_domain)
    }
}
/// Get the configs from the GitHub repository and parse them.
pub async fn get_configs_from_repo(github: &octorust::Client, company: &Company) -> Result<Config> {
    let owner = &company.github_org;
    let repo = "configs";

    log::info!("Getting configs from GitHub");
    let files = github
        .repos()
        .get_content_vec_entries(
            owner,
            repo,
            "/configs/",
            "", // leaving the branch blank gives us the default branch
        )
        .await?;

    let mut file_contents = String::new();
    for file in files {
        info!("decoding {}", file.name);
        // Get the contents of the file.
        let (contents, _) = get_file_content_from_repo(
            github, owner, repo, "", // leaving the branch blank gives us the default branch
            &file.path,
        )
        .await?;

        let decoded = from_utf8(&contents)?.trim().to_string();

        // Append the body of the file to the rest of the contents.
        file_contents.push('\n');
        file_contents.push_str(&decoded);
    }

    let config: Config = toml::from_str(&file_contents)?;

    Ok(config)
}

/// Sync GitHub outside collaborators with our configs.
pub async fn sync_github_outside_collaborators(
    db: &Database,
    github: &octorust::Client,
    outside_collaborators: BTreeMap<String, GitHubOutsideCollaboratorsConfig>,
    company: &Company,
) -> Result<()> {
    // We create a map of the collaborators per repo.
    // This way we can delete any collaborators that should no longer have access.
    let mut collaborators_map: BTreeMap<String, Vec<String>> = BTreeMap::new();

    // Add the outside contributors to the specified repos.
    for (name, outside_collaborators_config) in outside_collaborators {
        info!("running configuration for outside collaborators: {}", name);
        for repo in &outside_collaborators_config.repos {
            // Push the collaborators to our map, so we can use it later.
            match collaborators_map.get(repo) {
                Some(val) => {
                    let mut users = val.clone();
                    // Update the users to include this user.
                    let mut collaborators = outside_collaborators_config.users.clone();
                    users.append(&mut collaborators);
                    collaborators_map.insert(repo.to_string(), users.to_vec());
                }
                None => {
                    collaborators_map.insert(repo.to_string(), outside_collaborators_config.users.clone());
                }
            }

            let mut perm = octorust::types::TeamsAddUpdateRepoPermissionsInOrgRequestPermission::Pull;
            if outside_collaborators_config.perm == "push" {
                perm = octorust::types::TeamsAddUpdateRepoPermissionsInOrgRequestPermission::Push;
            }

            // Iterate over the users.
            for user in &outside_collaborators_config.users {
                if github
                    .repos()
                    .check_collaborator(&company.github_org, repo, user)
                    .await
                    .is_err()
                {
                    // The user is not already a collaborator
                    // Add the collaborator.
                    match github
                        .repos()
                        .add_collaborator(
                            &company.github_org,
                            repo,
                            user,
                            &octorust::types::ReposAddCollaboratorRequest {
                                permission: Some(perm.clone()),
                                permissions: Default::default(),
                            },
                        )
                        .await
                    {
                        Ok(_) => {
                            info!(
                                "[{}] added user {} as a collaborator ({}) on repo {}",
                                name, user, perm, repo
                            );
                        }
                        Err(e) => info!(
                            "[{}] adding user {} as a collaborator ({}) on repo {} FAILED: {}",
                            name, user, perm, repo, e
                        ),
                    }
                } else {
                    info!(
                        "[{}] user {} is already a collaborator ({}) on repo {}",
                        name, user, perm, repo
                    );
                }
            }
        }

        info!("successfully ran configuration for outside collaborators: {}", name);
    }

    // Get all the internal to the company collaborators.
    let mut internal_github_users: Vec<String> = Vec::new();
    let internal_users = Users::get_from_db(db, company.id).await?;
    for i in internal_users {
        if !i.github.is_empty() {
            internal_github_users.push(i.github.to_string());
        }
    }

    for (repo, mut collaborators) in collaborators_map {
        // Remove any duplicates.
        collaborators.sort_unstable();
        collaborators.dedup();

        // Get the collaborators on the repo.
        let github_collaborators = github
            .repos()
            .list_all_collaborators(&company.github_org, &repo, octorust::types::Affiliation::All)
            .await?;

        // Iterate over the users added to the repo, and make sure they exist in our
        // vector.
        for existing_collaborator in github_collaborators {
            // Check if they are an internal user.
            if internal_github_users
                .iter()
                .any(|internal| internal.to_lowercase() == existing_collaborator.login.to_lowercase())
            {
                // They are an internal user so continue;
                continue;
            }

            // Check if they should have access.
            if collaborators
                .iter()
                .any(|external| external.to_lowercase() == existing_collaborator.login.to_lowercase())
            {
                // They are supposed to be an external collaborator so continue;
                continue;
            }

            // Remove the user.
            info!(
                "REPO: {} USER: {} should not have access! Removing!",
                repo, existing_collaborator.login
            );
            github
                .repos()
                .remove_collaborator(&company.github_org, &repo, &existing_collaborator.login)
                .await?;
        }
    }

    Ok(())
}

/// Sync our users with our database and then update Airtable from the database.
pub async fn sync_users(
    db: &Database,
    github: &octorust::Client,
    users: BTreeMap<String, UserConfig>,
    company: &Company,
) -> Result<()> {
    // Get everything we need to authenticate with GSuite.
    // Initialize the GSuite client.
    let gsuite = company.authenticate_google_admin(db).await?;
    let gcal = company.authenticate_google_calendar(db).await?;

    // We don't need a base id here since we are only using the enterprise api features.
    let airtable_auth = company.authenticate_airtable("");

    // Initialize the Gusto client.
    let mut gusto_users: HashMap<String, gusto_api::types::Employee> = HashMap::new();
    let mut gusto_users_by_id: HashMap<String, gusto_api::types::Employee> = HashMap::new();
    let gusto_auth = company.authenticate_gusto(db).await;
    if let Ok((ref gusto, ref gusto_company_id)) = gusto_auth {
        let gu = gusto.employees().get_all_company(gusto_company_id, false, &[]).await?;
        for g in gu {
            gusto_users.insert(g.email.to_string(), g.clone());
            gusto_users_by_id.insert(g.id.to_string(), g);
        }
    }

    // Initialize the Okta client.
    let mut okta_users: HashMap<String, okta::types::User> = HashMap::new();
    let okta_auth = company.authenticate_okta();
    if let Some(ref okta) = okta_auth {
        let gu = okta.list_provider_users(company).await?;
        for g in gu {
            okta_users.insert(g.profile.as_ref().unwrap().email.to_string(), g);
        }
    }

    // Initialize the Ramp client.
    let mut ramp_users: HashMap<String, ramp_api::types::User> = HashMap::new();
    let mut ramp_departments: HashMap<String, ramp_api::types::Department> = HashMap::new();
    let ramp_auth = company.authenticate_ramp(db).await;
    if let Ok(ref ramp) = ramp_auth {
        let ru = ramp.list_provider_users(company).await?;
        for r in ru {
            ramp_users.insert(r.email.to_string(), r);
        }
        let rd = ramp.departments().get_all().await?;
        for r in rd {
            ramp_departments.insert(r.name.to_string(), r);
        }
    }

    // Initialize the Zoom client.
    let mut zoom_users: HashMap<String, zoom_api::types::UsersResponse> = HashMap::new();
    let mut zoom_users_pending: HashMap<String, zoom_api::types::UsersResponse> = HashMap::new();
    let zoom_auth = company.authenticate_zoom(db).await;
    if let Ok(ref zoom) = zoom_auth {
        match zoom.list_provider_users(company).await {
            Ok(active_users) => {
                for r in active_users {
                    zoom_users.insert(r.email.to_string(), r);
                }
            }
            Err(e) => {
                warn!("getting zoom active users for company {} failed: {}", company.name, e);
            }
        }

        // Get the pending Zoom users.
        match zoom
            .users()
            .get_all(
                zoom_api::types::UsersStatus::Pending,
                "", // role id
                zoom_api::types::UsersIncludeFields::Noop,
            )
            .await
        {
            Ok(pending_users) => {
                for r in pending_users {
                    zoom_users_pending.insert(r.email.to_string(), r);
                }
            }
            Err(e) => {
                warn!("getting zoom pending users for company {} failed: {}", company.name, e);
            }
        }
    }

    // Get the existing GSuite users.
    let gsuite_users = gsuite.list_provider_users(company).await?;
    let mut gsuite_users_map: BTreeMap<String, GSuiteUser> = BTreeMap::new();
    for g in gsuite_users.clone() {
        // Add the group to our map.
        gsuite_users_map.insert(g.primary_email.to_string(), g);
    }

    // Get the GSuite groups.
    let mut gsuite_groups: BTreeMap<String, GSuiteGroup> = BTreeMap::new();
    let groups = gsuite.list_provider_groups(company).await?;
    for g in groups {
        // Add the group to our map.
        gsuite_groups.insert(g.name.to_string(), g);
    }

    // Get the list of our calendars.
    let calendars = gcal
        .calendar_list()
        .list_all(google_calendar::types::MinAccessRole::Noop, false, false)
        .await?;

    let mut anniversary_cal_id = "".to_string();

    // Find the anniversary calendar.
    // Iterate over the calendars.
    for calendar in calendars {
        if calendar.summary.contains("Anniversaries") {
            // We are on the anniversaries calendar.
            anniversary_cal_id = calendar.id;
            break;
        }
    }

    // Get all the users.
    let db_users = Users::get_from_db(db, company.id).await?;
    // Create a BTreeMap
    let mut user_map: BTreeMap<String, User> = Default::default();
    for u in db_users {
        user_map.insert(u.username.to_string(), u);
    }

    // Sync users.
    // Iterate over the users and update.
    // We should do these concurrently, but limit it to maybe 3 at a time.
    let mut i = 0;
    let take = 3;
    let mut skip = 0;
    while i < users.clone().len() {
        let tasks: Vec<_> = users
            .clone()
            .into_iter()
            .skip(skip)
            .take(take)
            .map(|(_, mut user)| {
                tokio::spawn(crate::enclose! { (db, company, github, gsuite_users_map, okta_users, ramp_users, zoom_users, zoom_users_pending, gusto_users, gusto_users_by_id) async move {
                user.sync(
                    &db,
                    &company,
                    &github,
                    &gsuite_users_map,
                    &okta_users,
                    &ramp_users,
                    &zoom_users,
                    &zoom_users_pending,
                    &gusto_users,
                    &gusto_users_by_id,
                )
                .await
                }})
            })
            .collect();

        let mut results: Vec<Result<()>> = Default::default();
        for task in tasks {
            match task.await {
                Ok(task) => results.push(task),
                Err(err) => warn!("Task syncing user panicked. err: {:?}", err),
            }
        }

        for result in results {
            if let Err(err) = result {
                warn!("Syncing user failed. err: {:?}", err);
            }
        }

        i += take;
        skip += take;
    }

    for (_, user) in users {
        // Remove the user from the BTreeMap.
        user_map.remove(&user.username);
    }

    info!(
        "Remaining users that would be removed during sync: {:?}",
        user_map.keys()
    );

    // Remove any users that should no longer be in the database.
    // This is found by the remaining users that are in the map since we removed
    // the existing repos from the map above.
    for (username, user) in user_map {
        info!("deleting user `{}` from the database and other services", username);

        let mut has_failures = false;

        if !user.google_anniversary_event_id.is_empty() {
            // First delete the recurring event for their anniversary.
            let cal_delete = gcal
                .events()
                .delete(
                    &anniversary_cal_id,
                    &user.google_anniversary_event_id,
                    true, // send_notifications
                    google_calendar::types::SendUpdates::All,
                )
                .await;

            match cal_delete {
                Ok(_) => {
                    info!(
                        "deleted user {} event {} from google",
                        username, user.google_anniversary_event_id
                    );
                }
                Err(err) => {
                    let msg = format!("{}", err);

                    // An anniversary calender event may not exist if the user was partially
                    // provisioned or deprovisioned. In the case of deprovisioning, Google will
                    // return a 410 Gone error if the calendar event has already been removed.
                    // This should not be considered a failure.

                    // Errors from the Google Calendar client are stringy and do not return
                    // structured data. As a result this check is extremely brittle. We can not
                    // use its failure to authorize anything destructive.
                    if !msg.starts_with("code: 410 Gone") {
                        warn!(
                            "Failed to delete anniversary calendar {} / {}. err: {}",
                            username, user.google_anniversary_event_id, msg
                        );

                        has_failures = true;
                    } else {
                        info!(
                            "Ignoring error for anniversary calendar {} / {} delete",
                            username, user.google_anniversary_event_id
                        );
                    }
                }
            }
        }

        // Supend the user from okta.
        if let Some(ref okta) = okta_auth {
            match okta.delete_user(db, company, &user).await {
                Ok(_) => {
                    info!("Deleted user {} from okta", username);
                }
                Err(err) => {
                    warn!("Failed to delete user {} from okta. err: {:?}", username, err);

                    has_failures = true;
                }
            }
        }

        if company.okta_domain.is_empty() {
            // Delete the user from GSuite and other apps.
            // ONLY DO THIS IF THE COMPANY DOES NOT USE OKTA.
            // Suspend the user from GSuite so we can transfer their data.
            match gsuite.delete_user(db, company, &user).await {
                Ok(_) => {
                    info!("Deactivated user {} in GSuite", username);
                }
                Err(err) => {
                    warn!("Failed to deactivate user {} in GSuite. err: {:?}", username, err);

                    has_failures = true;
                }
            }
        }

        // Remove the user from the github org.
        match github.delete_user(db, company, &user).await {
            Ok(_) => {
                info!("Deleted user {} from GitHub", username);
            }
            Err(err) => {
                warn!("Failed to delete user {} from GitHub. err: {:?}", username, err);
            }
        }

        // TODO: Deactivate the user from Ramp.
        // We only want to lock the cards from more purchases. Removing GSuite/Okta
        // will disallow them from logging in. And we want their purchase history so
        // we don't want to delete them.

        // TODO: Delete the user from Slack.
        // Removing SSO (GSuite/Okta) will disallow them from logging in.

        // Delete the user from Zoom.
        if let Ok(ref zoom) = zoom_auth {
            match zoom.delete_user(db, company, &user).await {
                Ok(_) => {
                    info!("Deleted user {} from Zoom", username);
                }
                Err(err) => {
                    warn!("Failed to delete user {} from Zoom. err: {:?}", username, err);

                    has_failures = true;
                }
            }
        }

        // Delete the user from Airtable.
        // Okta should take care of this if we are using Okta.
        // But let's do it anyway.
        match airtable_auth.delete_user(db, company, &user).await {
            Ok(_) => {
                info!("Deleted user {} from Airtable", username);
            }
            Err(err) => {
                warn!("Failed to delete user {} from Airtable. err: {:?}", username, err);
            }
        }

        // User deletes are currently disabled. We no longer want to allow the behavior of removing
        // user records from our system. Instead they should be only marked as deleted so that we
        // can restore them in the future if needed.
        let enable_user_deletes = false;

        // Only delete the user from the database and Airtable if all previous deletes
        // have actually succeeded and user deletes are enabled.
        if !has_failures {
            if enable_user_deletes {
                match user.delete(db).await {
                    Ok(_) => {
                        info!("Successfully deleted user {} from database", username);
                    }
                    Err(err) => {
                        warn!("Failed to delete user {} from database. err: {:?}", username, err);
                    }
                }
            } else {
                info!(
                    "Would delete user {} from database, but user deletes have been disabled",
                    username
                );
            }
        } else {
            info!("Skipping final user deletion due to previous delete steps failing");
        }
    }

    info!("updated configs users in the database");

    // Update users in airtable.
    Users::get_from_db(db, company.id).await?.update_airtable(db).await?;

    Ok(())
}

/// Sync our buildings with our database and then update Airtable from the database.
pub async fn sync_buildings(
    db: &Database,
    buildings: BTreeMap<String, BuildingConfig>,
    company: &Company,
) -> Result<()> {
    // Get everything we need to authenticate with GSuite.
    // Initialize the GSuite client.
    let gsuite = company.authenticate_google_admin(db).await?;

    // Get the existing google buildings.
    let gsuite_buildings = gsuite
        .resources()
        .buildings_list_all(&company.gsuite_account_id)
        .await?;

    // Get all the buildings.
    let db_buildings = Buildings::get_from_db(db, company.id).await?;
    // Create a BTreeMap
    let mut building_map: BTreeMap<String, Building> = Default::default();
    for u in db_buildings {
        building_map.insert(u.name.to_string(), u);
    }
    // Sync buildings.
    for (_, mut building) in buildings {
        building.expand(company);

        building.upsert(db).await?;

        // Remove the building from the BTreeMap.
        building_map.remove(&building.name);
    }
    // Remove any buildings that should no longer be in the database.
    // This is found by the remaining buildings that are in the map since we removed
    // the existing repos from the map above.
    for (name, building) in building_map {
        info!("deleting building {} from the database, gsuite, etc", name);

        building.delete(db).await?;

        // Delete the building from GSuite.
        gsuite
            .resources()
            .buildings_delete(&company.gsuite_account_id, &name)
            .await?;
        info!("deleted building from gsuite: {}", name);
    }
    info!("updated configs buildings in the database");

    // Update the buildings in GSuite.
    // Get all the buildings.
    let db_buildings = Buildings::get_from_db(db, company.id).await?;
    // Create a BTreeMap
    let mut building_map: BTreeMap<String, Building> = Default::default();
    for u in db_buildings {
        building_map.insert(u.name.to_string(), u);
    }
    for b in gsuite_buildings {
        let id = b.building_id.to_string();

        // Check if we have that building already in our database.
        let building: Building = match building_map.get(&id) {
            Some(val) => val.clone(),
            None => {
                // If the building does not exist in our map we need to delete
                // them from GSuite.
                info!("deleting building {} from gsuite", id);
                gsuite
                    .resources()
                    .buildings_delete(&company.gsuite_account_id, &id)
                    .await?;

                info!("deleted building from gsuite: {}", id);
                continue;
            }
        };

        // Update the building with the settings from the database for the building.
        let new_b = update_gsuite_building(&b, &building, &id);

        // Update the building with the given settings.
        gsuite
            .resources()
            .buildings_update(
                &company.gsuite_account_id,
                &new_b.building_id,
                gsuite_api::types::CoordinatesSource::SourceUnspecified,
                &new_b,
            )
            .await?;

        // Remove the building from the database map and continue.
        // This allows us to add all the remaining new building after.
        building_map.remove(&id);

        info!("updated building from gsuite: {}", id);
    }

    // Create any remaining buildings from the database that we do not have in GSuite.
    for (id, building) in building_map {
        // Create the building.
        let b: GSuiteBuilding = Default::default();

        let new_b = update_gsuite_building(&b, &building, &id);

        gsuite
            .resources()
            .buildings_insert(
                &company.gsuite_account_id,
                gsuite_api::types::CoordinatesSource::SourceUnspecified,
                &new_b,
            )
            .await?;

        info!("created building from gsuite: {}", id);
    }

    // Update buildings in airtable.
    Buildings::get_from_db(db, company.id)
        .await?
        .update_airtable(db)
        .await?;

    Ok(())
}

/// Sync our resources with our database and then update Airtable from the database.
pub async fn sync_resources(
    db: &Database,
    resources: BTreeMap<String, NewResourceConfig>,
    company: &Company,
) -> Result<()> {
    // Get everything we need to authenticate with GSuite.
    // Initialize the GSuite client.
    let gsuite = company.authenticate_google_admin(db).await?;

    // Get the existing GSuite calendar resources.
    let g_suite_calendar_resources = gsuite
        .resources()
        .calendars_list_all(
            &company.gsuite_account_id,
            "", // order by
            "", // query
        )
        .await?;

    // Get all the resources.
    let db_resources = Resources::get_from_db(db, company.id).await?;
    // Create a BTreeMap
    let mut resource_map: BTreeMap<String, Resource> = Default::default();
    for u in db_resources {
        resource_map.insert(u.name.to_string(), u);
    }
    // Sync resources.
    for (_, mut resource) in resources {
        resource.cio_company_id = company.id;
        resource.upsert(db).await.map_err(|err| {
            log::warn!("Failed to upsert resource {:?}. err: {:?}", resource, err);
            err
        })?;

        // Remove the resource from the BTreeMap.
        resource_map.remove(&resource.name);
    }
    // Remove any resources that should no longer be in the database.
    // This is found by the remaining resources that are in the map since we removed
    // the existing repos from the map above.
    for (name, room) in resource_map {
        info!("deleting conference room {} from the database", name);
        room.delete(db).await?;
    }
    info!("updated configs resources in the database");

    // Update the resources in GSuite.
    // Get all the resources.
    let db_resources = Resources::get_from_db(db, company.id).await?;
    // Create a BTreeMap
    let mut resource_map: BTreeMap<String, Resource> = Default::default();
    for u in db_resources {
        resource_map.insert(u.name.to_string(), u);
    }
    for r in g_suite_calendar_resources {
        let id = r.resource_name.to_string();

        // Check if we have that resource already in our database.
        let resource: Resource = match resource_map.get(&id) {
            Some(val) => val.clone(),
            None => {
                // If the conference room does not exist in our map we need to delete
                // it from GSuite.
                info!("deleting conference room {} from gsuite", id);

                // Do not delete externally provisioned resources as this can be destructive
                // gsuite
                //     .resources()
                //     .calendars_delete(&company.gsuite_account_id, &r.resource_id)
                //     .await?;

                info!("deleted conference room from gsuite: {}", id);
                continue;
            }
        };

        // Update the resource with the settings from the database for the resource.
        let new_r = update_gsuite_calendar_resource(&r, &resource, &r.resource_id);

        // Update the resource with the given settings.
        gsuite
            .resources()
            .calendars_update(&company.gsuite_account_id, &new_r.resource_id, &new_r)
            .await?;

        // Remove the resource from the database map and continue.
        // This allows us to add all the remaining new resource after.
        resource_map.remove(&id);

        info!("updated conference room in gsuite: {}", id);
    }

    // Create any remaining resources from the database that we do not have in GSuite.
    for (id, resource) in resource_map {
        // Create the resource.
        let r: GSuiteCalendarResource = Default::default();

        let new_r = update_gsuite_calendar_resource(&r, &resource, &id);

        gsuite
            .resources()
            .calendars_insert(&company.gsuite_account_id, &new_r)
            .await?;

        info!("created conference room in gsuite: {}", id);
    }

    // Update resources in airtable.
    Resources::get_from_db(db, company.id)
        .await?
        .update_airtable(db)
        .await?;

    Ok(())
}

/// Sync our groups with our database and then update Airtable from the database.
pub async fn sync_groups(db: &Database, groups: BTreeMap<String, GroupConfig>, company: &Company) -> Result<()> {
    // Get everything we need to authenticate with GSuite.
    // Initialize the GSuite client.
    let gsuite = company.authenticate_google_admin(db).await?;

    let github = company.authenticate_github()?;

    let okta_auth = company.authenticate_okta();

    // Get all the groups.
    let db_groups = Groups::get_from_db(db, company.id).await?;
    // Create a BTreeMap
    let mut group_map: BTreeMap<String, Group> = Default::default();
    for u in db_groups {
        group_map.insert(u.name.to_string(), u);
    }

    // Sync groups.
    for (_, mut group) in groups {
        group.expand(company);

        group.upsert(db).await?;

        // Remove the group from the BTreeMap.
        group_map.remove(&group.name);
    }

    // Remove any groups that should no longer be in the database.
    // This is found by the remaining groups that are in the map since we removed
    // the existing repos from the map above.
    for (name, group) in group_map {
        info!("deleting group `{}` from the database, gsuite, github, okta, etc", name);

        // Delete the group from the database and Airtable.
        group.delete(db).await?;

        gsuite.delete_group(company, &group).await?;

        github.delete_group(company, &group).await?;

        if let Some(ref okta) = okta_auth {
            okta.delete_group(company, &group).await?;
        }
    }

    info!("updated configs groups in the database");

    // Update the groups in GitHub and GSuite.
    // Get all the groups.
    let db_groups = Groups::get_from_db(db, company.id).await?;
    // Iterate over all the groups in our database.
    // TODO: delete any groups that are not in the database for each vendor.
    for g in db_groups {
        github.ensure_group(db, company, &g).await?;

        gsuite.ensure_group(db, company, &g).await?;

        if let Some(ref okta) = okta_auth {
            okta.ensure_group(db, company, &g).await?;
        }
    }

    // Update groups in airtable.
    Groups::get_from_db(db, company.id).await?.update_airtable(db).await?;

    Ok(())
}

/// Sync our links with our database and then update Airtable from the database.
pub async fn sync_links(
    db: &Database,
    links: BTreeMap<String, LinkConfig>,
    huddles: BTreeMap<String, HuddleConfig>,
    company: &Company,
) -> Result<()> {
    // Get all the links.
    let db_links = Links::get_from_db(db, company.id).await?;
    // Create a BTreeMap
    let mut link_map: BTreeMap<String, Link> = Default::default();
    for u in db_links {
        link_map.insert(u.name.to_string(), u);
    }
    // Sync links.
    for (name, mut link) in links {
        link.name = name.to_string();
        link.short_link = format!("https://{}.corp.{}", name, company.domain);
        link.cio_company_id = company.id;

        link.upsert(db).await?;

        // Remove the link from the BTreeMap.
        link_map.remove(&link.name);
    }
    for (slug, huddle) in huddles {
        // Create the link for the workspace.
        let mut link = LinkConfig {
            name: format!("{}-huddle", slug),
            description: huddle.description.to_string(),
            link: huddle.link_to_airtable_workspace.to_string(),
            aliases: vec![format!("airtable-{}-huddle", slug)],
            short_link: format!("https://{}-huddle.corp.{}", slug, company.domain),
            cio_company_id: company.id,
        };

        link.upsert(db).await?;

        // Remove the link from the BTreeMap.
        link_map.remove(&link.name);

        // Update the link for the form.
        link.name = format!("{}-huddle-form", slug);
        link.link = huddle.link_to_airtable_form.to_string();
        link.aliases = vec![format!("airtable-{}-huddle-form", slug)];
        link.short_link = format!("https://{}-huddle-form.corp.{}", slug, company.domain);
        link.description = format!(
            "Form for submitting topics to the {}",
            huddle.description.to_lowercase()
        );

        link.upsert(db).await?;

        // Remove the link from the BTreeMap.
        link_map.remove(&link.name);
    }
    // Remove any links that should no longer be in the database.
    // This is found by the remaining links that are in the map since we removed
    // the existing repos from the map above.
    for (_, link) in link_map {
        link.delete(db).await?;
    }
    info!("updated configs links in the database");

    // Update links in airtable.
    Links::get_from_db(db, company.id).await?.update_airtable(db).await?;

    Ok(())
}

/// Sync our certificates with our database and then update Airtable from the database.
pub async fn sync_certificates(
    db: &Database,
    github: &octorust::Client,
    certificates: BTreeMap<String, NewCertificate>,
    company: &Company,
) -> Result<()> {
    // Get all the certificates.
    let db_certificates = Certificates::get_from_db(db, company.id).await?;
    // Create a BTreeMap
    let mut certificate_map: BTreeMap<String, Certificate> = Default::default();
    for u in db_certificates {
        certificate_map.insert(u.domain.to_string(), u);
    }
    // Sync certificates.
    for (_, mut certificate) in certificates {
        certificate.cio_company_id = company.id;

        certificate.populate_from_github(github, company).await?;

        let mut send_notification = false;

        // If the cert is going to expire in less than 12 days, renew it.
        // Otherwise, return early.
        if certificate.valid_days_left > 12 {
            info!(
                "cert {} is valid for {} more days, skipping",
                certificate.domain, certificate.valid_days_left
            );
        } else {
            // Populate the certificate.
            certificate.populate(company).await?;

            // Save the certificate to disk.
            certificate.save_to_github_repo(github, company).await?;

            // Update the Github Action secrets, with the new certificates if there are some.
            certificate.update_github_action_secrets(github, company).await?;

            send_notification = true;
        }

        if certificate.certificate.is_empty() {
            // Continue early.
            continue;
        }

        // Update the database and Airtable.
        certificate.upsert(db).await?;

        // Send the notification, do this after updating the database, just in case there was
        // an error.
        if send_notification {
            certificate.send_slack_notification(db, company).await?;
        }

        // Remove the certificate from the BTreeMap.
        certificate_map.remove(&certificate.domain);
    }

    // Remove any certificates that should no longer be in the database.
    // This is found by the remaining certificates that are in the map since we removed
    // the existing repos from the map above.
    for (_, cert) in certificate_map {
        cert.delete(db).await?;
    }
    info!("updated configs certificates in the database");

    // Update certificates in airtable.
    Certificates::get_from_db(db, company.id)
        .await?
        .update_airtable(db)
        .await?;

    Ok(())
}

pub async fn refresh_db_configs_and_airtable(db: &Database, company: &Company) -> Result<()> {
    let github = company.authenticate_github()?;

    let configs = get_configs_from_repo(&github, company).await?;

    // Sync buildings.
    // Syncing buildings must happen before we sync resource.
    sync_buildings(db, configs.buildings, company).await?;

    // Sync resources.
    sync_resources(db, configs.resources, company).await?;

    // Sync groups.
    // Syncing groups must happen before we sync the users.
    sync_groups(db, configs.groups, company).await?;

    // Sync users.
    sync_users(db, &github, configs.users, company).await?;

    // Sync links.
    let (links, certs, ghout, ann) = tokio::join!(
        sync_links(db, configs.links, configs.huddles, company),
        // Sync certificates.
        sync_certificates(db, &github, configs.certificates, company),
        // Sync github outside collaborators.
        sync_github_outside_collaborators(db, &github, configs.github_outside_collaborators, company),
        refresh_anniversary_events(db, company),
    );

    if let Err(e) = links {
        warn!("error syncing links: {}", e);
    }
    if let Err(e) = certs {
        warn!("error syncing certificates: {}", e);
    }
    if let Err(e) = ghout {
        warn!("error syncing github outside collaborators: {}", e);
    }
    if let Err(e) = ann {
        warn!("error refreshing anniversary events: {}", e);
    }

    Ok(())
}

pub async fn refresh_anniversary_events(db: &Database, company: &Company) -> Result<()> {
    let gcal = company.authenticate_google_calendar(db).await?;

    // Get the list of our calendars.
    let calendars = gcal
        .calendar_list()
        .list_all(google_calendar::types::MinAccessRole::Noop, false, false)
        .await?;

    let mut anniversary_cal_id = "".to_string();

    // Find the anniversary calendar.
    // Iterate over the calendars.
    for calendar in calendars {
        if calendar.summary.contains("Anniversaries") {
            // We are on the anniversaries calendar.
            anniversary_cal_id = calendar.id;
            break;
        }
    }

    if anniversary_cal_id.is_empty() {
        // Return early we couldn't find the calendar.
        bail!("Couldn't find calendar named 'Anniversaries'!");
    }

    // Get our list of users from our database.
    let users = Users::get_from_db(db, company.id).await?;
    // For each user, create an anniversary for their start date.
    for mut user in users {
        // We only care if the user has a start date.
        if user.start_date == crate::utils::default_date() {
            continue;
        }

        // Create a new event.
        let mut new_event: Event = Default::default();

        new_event.start = Some(EventDateTime {
            time_zone: "America/Los_Angeles".to_string(),
            date: Some(user.start_date),
            date_time: None,
        });
        new_event.end = Some(EventDateTime {
            time_zone: "America/Los_Angeles".to_string(),
            date: Some(user.start_date),
            date_time: None,
        });
        new_event.summary = format!("{} {}'s Anniversary", user.first_name, user.last_name);
        new_event.description = format!(
            "On {}, {} {} joined the company!",
            user.start_date.format("%A, %B %-d, %C%y"),
            user.first_name,
            user.last_name
        );
        new_event.recurrence = vec!["RRULE:FREQ=YEARLY;".to_string()];
        new_event.transparency = "transparent".to_string();
        new_event.attendees = vec![EventAttendee {
            id: Default::default(),
            email: user.email.to_string(),
            display_name: Default::default(),
            organizer: false,
            resource: false,
            optional: false,
            response_status: Default::default(),
            comment: Default::default(),
            additional_guests: 0,
            self_: false,
        }];

        if user.google_anniversary_event_id.is_empty() {
            // Create the event.
            let event = gcal
                .events()
                .insert(
                    &anniversary_cal_id,
                    0,                                        // conference data version, leave blank
                    0,                                        // max attendees
                    true,                                     // send notifications
                    google_calendar::types::SendUpdates::All, // send updates
                    true,                                     // supports_attachments
                    &new_event,
                )
                .await?;
            info!("created event for user {} anniversary: {:?}", user.username, event);

            user.google_anniversary_event_id = event.id.to_string();
        } else {
            // Get the existing event.
            let old_event = gcal
                .events()
                .get(
                    &anniversary_cal_id,
                    &user.google_anniversary_event_id,
                    0,  // max_attendees set to 0 to ignore
                    "", // time_zone
                )
                .await?;

            if old_event.description != new_event.description
                || old_event.summary != new_event.summary
                || old_event.start.as_ref().unwrap().date != new_event.start.as_ref().unwrap().date
            {
                // Only update it if it has changed.

                // Set the correct sequence so we don't error out.
                new_event.sequence = old_event.sequence;
                // Update the event.
                let event = gcal
                    .events()
                    .update(
                        &anniversary_cal_id,
                        &user.google_anniversary_event_id,
                        0,                                        // conference data version, set to 0 to ignore
                        0,                                        // max_attendees set to 0 to ignore
                        true,                                     // send_notifications
                        google_calendar::types::SendUpdates::All, // send updates
                        true,                                     // supports_attachments
                        &new_event,
                    )
                    .await?;
                info!("updated event for user {} anniversary: {:?}", user.username, event);
            }
        }

        // Update the user in the database.
        user.update(db).await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};
    use serde_json;

    use super::{ExternalServices, UserConfig};

    #[derive(Debug, PartialEq, Deserialize, Serialize)]
    struct ServiceWrapper {
        service: ExternalServices,
    }

    #[test]
    fn test_handles_lowercase_services() {
        assert_eq!(
            ServiceWrapper {
                service: ExternalServices::Airtable
            },
            serde_json::from_str::<ServiceWrapper>("{\"service\": \"airtable\"}").unwrap()
        );
        assert_eq!(
            ServiceWrapper {
                service: ExternalServices::GitHub
            },
            serde_json::from_str::<ServiceWrapper>("{\"service\": \"github\"}").unwrap()
        );
        assert_eq!(
            ServiceWrapper {
                service: ExternalServices::Google
            },
            serde_json::from_str::<ServiceWrapper>("{\"service\": \"google\"}").unwrap()
        );
        assert_eq!(
            ServiceWrapper {
                service: ExternalServices::Okta
            },
            serde_json::from_str::<ServiceWrapper>("{\"service\": \"okta\"}").unwrap()
        );
        assert_eq!(
            ServiceWrapper {
                service: ExternalServices::Ramp
            },
            serde_json::from_str::<ServiceWrapper>("{\"service\": \"ramp\"}").unwrap()
        );
        assert_eq!(
            ServiceWrapper {
                service: ExternalServices::Zoom
            },
            serde_json::from_str::<ServiceWrapper>("{\"service\": \"zoom\"}").unwrap()
        );

        assert_eq!(
            "{\"service\":\"airtable\"}",
            serde_json::to_string(&ServiceWrapper {
                service: ExternalServices::Airtable
            })
            .unwrap()
            .as_str()
        );
        assert_eq!(
            "{\"service\":\"github\"}",
            serde_json::to_string(&ServiceWrapper {
                service: ExternalServices::GitHub
            })
            .unwrap()
            .as_str()
        );
        assert_eq!(
            "{\"service\":\"google\"}",
            serde_json::to_string(&ServiceWrapper {
                service: ExternalServices::Google
            })
            .unwrap()
            .as_str()
        );
        assert_eq!(
            "{\"service\":\"okta\"}",
            serde_json::to_string(&ServiceWrapper {
                service: ExternalServices::Okta
            })
            .unwrap()
            .as_str()
        );
        assert_eq!(
            "{\"service\":\"ramp\"}",
            serde_json::to_string(&ServiceWrapper {
                service: ExternalServices::Ramp
            })
            .unwrap()
            .as_str()
        );
        assert_eq!(
            "{\"service\":\"zoom\"}",
            serde_json::to_string(&ServiceWrapper {
                service: ExternalServices::Zoom
            })
            .unwrap()
            .as_str()
        );
    }

    #[test]
    fn test_deserializes_user_config() {
        let user: UserConfig = toml::from_str(
            r#"
first_name = 'Test'
last_name = 'User'
username = 'test'
is_group_admin = true
aliases = [
    "parse_test",
]
groups = [
    'alpha',
    'beta',
    'gamma',
]
denied_services = [
    'airtable',
    'github',
    'google',
    'okta',
    'ramp',
    'zoom'
]
recovery_email = 'testuser@localhost'
recovery_phone = '+15555555555'
gender = ''
github = 'github_username'
chat = ''
aws_role = 'arn:aws:iam::5555555:role/AnArbitraryAWSRole,arn:aws:iam::5555555:role/AnotherArbitraryAWSRole'
department = 'aerospace'
manager = 'orb'
        "#,
        )
        .expect("Failed to parse user config");

        assert_eq!(user.first_name, "Test");
        assert_eq!(user.last_name, "User");
        assert_eq!(
            user.denied_services,
            vec![
                ExternalServices::Airtable,
                ExternalServices::GitHub,
                ExternalServices::Google,
                ExternalServices::Okta,
                ExternalServices::Ramp,
                ExternalServices::Zoom
            ]
        );
    }

    #[test]
    fn test_deserializes_user_config_without_denies() {
        let user: UserConfig = toml::from_str(
            r#"
first_name = 'Test'
last_name = 'User'
username = 'test'
is_group_admin = true
aliases = [
    "parse_test",
]
groups = [
    'alpha',
    'beta',
    'gamma',
]
recovery_email = 'testuser@localhost'
recovery_phone = '+15555555555'
gender = ''
github = 'github_username'
chat = ''
aws_role = 'arn:aws:iam::5555555:role/AnArbitraryAWSRole,arn:aws:iam::5555555:role/AnotherArbitraryAWSRole'
department = 'aerospace'
manager = 'orb'
        "#,
        )
        .expect("Failed to parse user config");

        assert_eq!(user.first_name, "Test");
        assert_eq!(user.last_name, "User");
        assert_eq!(user.denied_services, vec![]);
    }
}
