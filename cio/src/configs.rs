#![allow(clippy::from_over_into)]
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::fs;
use std::str::from_utf8;
use std::{thread, time};

use async_trait::async_trait;
use chrono::naive::NaiveDate;
use clap::ArgMatches;
use futures_util::stream::TryStreamExt;
use google_geocode::Geocode;
use gsuite_api::{Attendee, Building as GSuiteBuilding, CalendarEvent, CalendarResource as GSuiteCalendarResource, Date, GSuite, Group as GSuiteGroup, User as GSuiteUser};
use hubcaps::collaborators::Permissions;
use hubcaps::Github;
use macros::db;
use schemars::JsonSchema;
use sendgrid_api::SendGrid;
use serde::{Deserialize, Serialize};

use crate::airtable::{AIRTABLE_BUILDINGS_TABLE, AIRTABLE_CONFERENCE_ROOMS_TABLE, AIRTABLE_EMPLOYEES_TABLE, AIRTABLE_GROUPS_TABLE, AIRTABLE_LINKS_TABLE};
use crate::applicants::Applicant;
use crate::certs::{Certificate, Certificates, NewCertificate};
use crate::companies::Company;
use crate::core::UpdateAirtableRecord;
use crate::db::Database;
use crate::gsuite::{update_google_group_settings, update_group_aliases, update_gsuite_building, update_gsuite_calendar_resource, update_gsuite_user, update_user_aliases, update_user_google_groups};
use crate::schema::{applicants, buildings, conference_rooms, groups, links, users};
use crate::shipments::NewOutboundShipment;
use crate::templates::{generate_terraform_files_for_aws_and_github, generate_terraform_files_for_okta};
use crate::utils::get_github_user_public_ssh_keys;

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
    pub resources: BTreeMap<String, ResourceConfig>,

    #[serde(default)]
    pub links: BTreeMap<String, LinkConfig>,

    #[serde(default, alias = "github-outside-collaborators")]
    pub github_outside_collaborators: BTreeMap<String, GitHubOutsideCollaboratorsConfig>,

    #[serde(default)]
    pub huddles: BTreeMap<String, HuddleConfig>,

    #[serde(default)]
    pub certificates: BTreeMap<String, NewCertificate>,
}

impl Config {
    /// Read and decode the config from the files that are passed on the command line.
    pub fn read(cli_matches: &ArgMatches) -> Self {
        let files: Vec<String>;
        match cli_matches.values_of("file") {
            None => panic!("no configuration files specified"),
            Some(val) => {
                files = val.map(|s| s.to_string()).collect();
            }
        };

        let mut contents = String::new();
        for file in files.iter() {
            println!("decoding {}", file);

            // Read the file.
            let body = fs::read_to_string(file).expect("reading the file failed");

            // Append the body of the file to the rest of the contents.
            contents.push_str(&body);
        }

        // Decode the contents.
        let config: Config = toml::from_str(&contents).unwrap();

        config
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
#[table_name = "users"]
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
    #[serde(default = "crate::utils::default_date", alias = "start_date", serialize_with = "null_date_format::serialize")]
    pub start_date: NaiveDate,
    /// Birthday (automatically populated by Gusto)
    #[serde(default = "crate::utils::default_date", serialize_with = "null_date_format::serialize")]
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

    /// This field is used by Airtable for mapping the location data.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub geocode_cache: String,
    /// The CIO company ID.
    #[serde(default)]
    pub cio_company_id: i32,
}

pub mod null_date_format {
    use chrono::naive::NaiveDate;
    use chrono::{DateTime, TimeZone, Utc};
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
    fn update_from_gusto(&mut self, gusto_user: &gusto_api::Employee) {
        self.gusto_id = gusto_user.id.to_string();

        // Update the user's start date.
        self.start_date = gusto_user.jobs[0].hire_date;

        // Update the user's birthday.
        self.birthday = gusto_user.date_of_birth;

        // Update the user's home address.
        // Gusto now becomes the source of truth for people's addresses.
        self.home_address_street_1 = gusto_user.home_address.street_1.to_string();
        self.home_address_street_2 = gusto_user.home_address.street_2.to_string();
        self.home_address_city = gusto_user.home_address.city.to_string();
        self.home_address_state = gusto_user.home_address.state.to_string();
        self.home_address_zipcode = gusto_user.home_address.zip.to_string();
        self.home_address_country = gusto_user.home_address.country.to_string();
    }

    async fn populate_ssh_keys(&mut self) {
        if self.github.is_empty() {
            // Return early if we don't know their github handle.
            return;
        }

        self.public_ssh_keys = get_github_user_public_ssh_keys(&self.github).await;
    }

    async fn populate_home_address(&mut self) {
        let mut street_address = self.home_address_street_1.to_string();
        if !self.home_address_street_2.is_empty() {
            street_address = format!("{}\n{}", self.home_address_street_1, self.home_address_street_2,);
        }
        // Make sure the state is not an abreev.
        self.home_address_state = crate::states::StatesMap::match_abreev_or_return_existing(&self.home_address_state);

        // Set the formatted address.
        self.home_address_formatted = format!(
            "{}\n{}, {} {} {}",
            street_address, self.home_address_city, self.home_address_state, self.home_address_zipcode, self.home_address_country
        )
        .trim()
        .trim_matches(',')
        .trim()
        .to_string();

        // Populate the country code.
        if self.home_address_country.is_empty() || self.home_address_country == "United States" {
            self.work_address_country = "United States".to_string();
            self.home_address_country_code = "US".to_string();
        }

        if !self.home_address_formatted.is_empty() {
            // Create the geocode client.
            let geocode = Geocode::new_from_env();
            // Get the latitude and longitude.
            let result = geocode.get(&self.home_address_formatted).await.unwrap();
            let location = result.geometry.location;
            self.home_address_latitude = location.lat as f32;
            self.home_address_longitude = location.lng as f32;
        }
    }

    async fn populate_work_address(&mut self, db: &Database) {
        // Populate the address based on the user's location.
        if !self.building.is_empty() {
            // The user has an actual building for their work address.
            // Let's get it.
            let building = Building::get_from_db(db, self.cio_company_id, self.building.to_string()).unwrap();
            // Now let's set their address to the building's address.
            self.work_address_street_1 = building.street_address.to_string();
            self.work_address_street_2 = "".to_string();
            self.work_address_city = building.city.to_string();
            self.work_address_state = crate::states::StatesMap::match_abreev_or_return_existing(&building.state);
            self.work_address_zipcode = building.zipcode.to_string();
            self.work_address_country = building.country.to_string();
            if self.work_address_country == "US" || self.work_address_country.is_empty() {
                self.work_address_country = "United States".to_string();
            }
            self.work_address_formatted = building.address_formatted.to_string();

            let city_group = building.city.to_lowercase().replace(" ", "-");

            // Ensure we have added the group for that city.
            if !self.groups.contains(&city_group) {
                self.groups.push(city_group);
            }
        } else {
            // They are remote so we should use their home address.
            self.work_address_street_1 = self.home_address_street_1.to_string();
            self.work_address_street_2 = self.home_address_street_2.to_string();
            self.work_address_city = self.home_address_city.to_string();
            self.work_address_state = crate::states::StatesMap::match_abreev_or_return_existing(&self.home_address_state);
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

    pub fn populate_start_date(&mut self, db: &Database) {
        // Only populate the start date, if we could not update it from Gusto.
        if self.start_date == crate::utils::default_date() {
            if let Ok(a) = applicants::dsl::applicants
                .filter(applicants::dsl::email.eq(self.recovery_email.to_string()))
                .first::<Applicant>(&db.conn())
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

    pub fn ensure_all_aliases(&mut self) {
        if !self.github.is_empty() && !self.aliases.contains(&self.github) {
            self.aliases.push(self.github.to_string());
        }

        if !self.twitter.is_empty() && !self.aliases.contains(&self.twitter) {
            self.aliases.push(self.twitter.to_string());
        }

        let name_alias = format!("{}.{}", self.first_name.to_lowercase().replace(' ', "-"), self.last_name.to_lowercase().replace(' ', "-"));
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

    pub async fn expand(&mut self, db: &Database, company: &Company) {
        self.cio_company_id = company.id;

        self.email = format!("{}@{}", self.username, company.gsuite_domain);

        // Do this first.
        self.populate_type();

        self.ensure_all_aliases();
        self.ensure_all_groups();

        self.populate_ssh_keys().await;

        self.populate_home_address().await;
        self.populate_work_address(db).await;

        self.populate_start_date(db);

        // Create the link to the manager.
        if !self.manager.is_empty() {
            self.link_to_manager = vec![self.manager.to_string()];
        }

        // Title case the department.
        self.department = titlecase::titlecase(&self.department);
    }
}

impl User {
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

    /// Create an internal swag shipment to an employee's home address.
    /// This will:
    /// - Check if the user has a home address.
    /// - Create a record in outgoing shipments.
    /// - Generate the shippo label.
    /// - Print said shippo label.
    pub async fn create_shipment_to_home_address(&self, db: &Database) {
        // First let's check if the user even has an address.
        // If not we can return early.
        if self.home_address_formatted.is_empty() {
            println!("cannot create shipping label for user {} since we don't know their home address", self.username);
            return;
        }

        // Let's create the shipment.
        let new_shipment = NewOutboundShipment::from(self.clone());
        // Let's add it to our database.
        let mut shipment = new_shipment.upsert(db).await;
        // Create the shipment in shippo.
        shipment.create_or_get_shippo_shipment(db).await;
        // Update airtable and the database again.
        shipment.update(db).await;
    }

    /// Send an email to the new consultant about their account.
    async fn send_email_new_consultant(&self, db: &Database) {
        let company = self.company(db);

        // Initialize the SendGrid client.
        let sendgrid = SendGrid::new_from_env();

        // Get the user's aliases if they have one.
        let aliases = self.aliases.join(", ");

        // Send the message.
        sendgrid
            .send_mail(
                format!("Your New Email Account: {}", self.email),
                format!(
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
                vec![self.recovery_email.to_string()],
                vec![self.email.to_string(), format!("jess@{}", company.gsuite_domain)],
                vec![],
                format!("admin@{}", company.gsuite_domain),
            )
            .await;
    }

    /// Send an email to the GSuite user about their account.
    async fn send_email_new_gsuite_user(&self, db: &Database, password: &str) {
        let company = self.company(db);

        // Initialize the SendGrid client.
        let sendgrid = SendGrid::new_from_env();

        // Get the user's aliases if they have one.
        let aliases = self.aliases.join(", ");

        // Send the message.
        sendgrid
            .send_mail(
                format!("Your New Email Account: {}", self.email),
                format!(
                    "Yoyoyo {},

We have set up your account on mail.corp.{}. Details for accessing
are below. You will be required to reset your password the next time you login.

Website for Login: https://mail.corp.{}
Email: {}
Password: {}
Aliases: {}

Make sure you set up two-factor authentication for your account, or in one week
you will be locked out.

Your GitHub @{} has been added to our organization (https://github.com/{})
and various teams within it. GitHub should have sent an email with instructions on
accepting the invitation to our organization to the email you used
when you signed up for GitHub. Or you can alternatively accept our invitation
by going to https://github.com/{}.

If you have any questions or your email does not work please email your
administrator, who is cc-ed on this email. Spoiler alert it's Jess...
jess@{}. If you want other email aliases, let Jess know as well.

xoxo,
  The Onboarding Bot",
                    self.first_name, company.domain, company.domain, self.email, password, aliases, self.github, company.github_org, company.github_org, company.gsuite_domain,
                ),
                vec![self.recovery_email.to_string()],
                vec![self.email.to_string(), format!("jess@{}", company.gsuite_domain)],
                vec![],
                format!("admin@{}", company.gsuite_domain),
            )
            .await;
    }

    /// Send an email to the new user about their account.
    async fn send_email_new_user(&self, db: &Database) {
        let company = self.company(db);
        // Initialize the SendGrid client.
        let sendgrid = SendGrid::new_from_env();

        // Get the user's aliases if they have one.
        let aliases = self.aliases.join(", ");

        let mut github_copy = format!(
            "Your GitHub @{} has been added to our organization (https://github.com/{})
and various teams within it. GitHub should have sent an email with instructions on
accepting the invitation to our organization to the email you used
when you signed up for GitHub. Or you can alternatively accept our invitation
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
            .send_mail(
                format!("Your New Email Account: {}", self.email),
                format!(
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
To join our Airtable workspace you need to click this link:
https://airtable-join.corp.oxide.computer.
Poke around once you've joined :)

We have both a Riot server and a Slack for chat. Josh (josh@oxidecomputer.com) can get
you set up with an account on the Riot server. You can use SSO to login to the Slack
at https://oxidecomputer.slack.com.

Lastly, be sure to order yourself some swag: https://swag.oxide.computer

xoxo,
  The Onboarding Bot",
                    self.first_name, company.domain, company.domain, self.email, aliases, github_copy, company.gsuite_domain, company.github_org, company.github_org, company.github_org,
                ),
                vec![self.recovery_email.to_string()],
                vec![self.email.to_string(), format!("jess@{}", company.gsuite_domain)],
                vec![],
                format!("admin@{}", company.gsuite_domain),
            )
            .await;
    }
}

/// Implement updating the Airtable record for a User.
#[async_trait]
impl UpdateAirtableRecord<User> for User {
    async fn update_airtable_record(&mut self, record: User) {
        // Get the current groups in Airtable so we can link to them.
        // TODO: make this more dry so we do not call it every single damn time.
        let db = Database::new();
        let groups = Groups::get_from_airtable(&db, self.cio_company_id).await;

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
        let buildings = Buildings::get_from_airtable(&db, self.cio_company_id).await;
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
#[table_name = "groups"]
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
    #[serde(alias = "who_can_moderate_members", skip_serializing_if = "String::is_empty", default)]
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
        format!("https://groups.google.com/a/{}/forum/#!forum/{}", company.gsuite_domain, self.name)
    }

    pub fn expand(&mut self, company: &Company) {
        self.link = self.get_link(company);

        self.cio_company_id = company.id;
    }
}

/// Implement updating the Airtable record for a Group.
#[async_trait]
impl UpdateAirtableRecord<Group> for Group {
    async fn update_airtable_record(&mut self, record: Group) {
        // Make sure we don't mess with the members since that is populated by the Users table.
        self.members = record.members;
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
#[table_name = "buildings"]
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
        self.address_formatted = format!("{}\n{}, {} {}, {}", self.street_address, self.city, self.state, self.zipcode, self.country);

        self.cio_company_id = company.id;
    }
}

/// Implement updating the Airtable record for a Building.
#[async_trait]
impl UpdateAirtableRecord<Building> for Building {
    async fn update_airtable_record(&mut self, record: Building) {
        // Make sure we don't mess with the employees since that is populated by the Users table.
        self.employees = record.employees.clone();
        // Make sure we don't mess with the conference_rooms since that is populated by the Conference Rooms table.
        self.conference_rooms = record.conference_rooms;

        self.geocode_cache = record.geocode_cache;
    }
}

/// The data type for a resource. These are conference rooms that people can book
/// through GSuite or Zoom.
#[db {
    new_struct_name = "ConferenceRoom",
    airtable_base = "directory",
    airtable_table = "AIRTABLE_CONFERENCE_ROOMS_TABLE",
    match_on = {
        "cio_company_id" = "i32",
        "name" = "String",
    },
}]
#[derive(Debug, Insertable, AsChangeset, Default, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[table_name = "conference_rooms"]
pub struct ResourceConfig {
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
    /// The CIO company ID.
    #[serde(default)]
    pub cio_company_id: i32,
}

/// Implement updating the Airtable record for a ConferenceRoom.
#[async_trait]
impl UpdateAirtableRecord<ConferenceRoom> for ConferenceRoom {
    async fn update_airtable_record(&mut self, _record: ConferenceRoom) {
        // Set the building to right building link.
        // Get the current buildings in Airtable so we can link to it.
        // TODO: make this more dry so we do not call it every single damn time.
        let db = Database::new();
        let buildings = Buildings::get_from_airtable(&db, self.cio_company_id).await;
        // Iterate over the buildings to get the ID.
        for building in buildings.values() {
            if self.building == building.fields.name {
                // Set the ID.
                self.link_to_building = vec![building.id.to_string()];
                // Break the loop and return early.
                break;
            }
        }
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
#[table_name = "links"]
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
    async fn update_airtable_record(&mut self, _record: Link) {}
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
    #[serde(default)]
    pub time_to_cancel: i32,
}

/// Get the configs from the GitHub repository and parse them.
pub async fn get_configs_from_repo(github: &Github, company: &Company) -> Config {
    let repo = github.repo(&company.github_org, "configs");
    let r = repo.get().await.unwrap();
    let repo_contents = repo.content();

    let files = repo_contents.iter("/configs/", &r.default_branch).try_collect::<Vec<hubcaps::content::DirectoryItem>>().await.unwrap();

    let mut file_contents = String::new();
    for file in files {
        println!("decoding {}", file.name);
        // Get the contents of the file.
        let contents = repo_contents.file(&format!("/{}", file.path), &r.default_branch).await.unwrap();

        let decoded = from_utf8(&contents.content).unwrap().trim().to_string();

        // Append the body of the file to the rest of the contents.
        file_contents.push_str(&"\n");
        file_contents.push_str(&decoded);
    }

    let config: Config = toml::from_str(&file_contents).unwrap();

    config
}

/// Sync GitHub outside collaborators with our configs.
pub async fn sync_github_outside_collaborators(github: &Github, outside_collaborators: BTreeMap<String, GitHubOutsideCollaboratorsConfig>, company: &Company) {
    // Add the outside contributors to the specified repos.
    for (name, outside_collaborators_config) in outside_collaborators {
        println!("Running configuration for outside collaborators: {}", name);
        for repo in &outside_collaborators_config.repos {
            // Get the repository collaborators interface from hubcaps.
            let repo_collaborators = github.repo(&company.github_org, repo.to_string()).collaborators();

            let mut perm = Permissions::Pull;
            if outside_collaborators_config.perm == "push" {
                perm = Permissions::Push;
            }

            // Iterate over the users.
            for user in &outside_collaborators_config.users {
                if !repo_collaborators.is_collaborator(&user).await.unwrap_or(false) {
                    // Add the collaborator.
                    match repo_collaborators.add(&user, &perm).await {
                        Ok(_) => {
                            println!("[{}] added user {} as a collaborator ({}) on repo {}", name, user, perm, repo);
                        }
                        Err(e) => println!("[{}] adding user {} as a collaborator ({}) on repo {} FAILED: {}", name, user, perm, repo, e),
                    }
                } else {
                    println!("[{}] user {} is already a collaborator ({}) on repo {}", name, user, perm, repo);
                }
            }
        }

        println!("Successfully ran configuration for outside collaborators: {}", name);
    }
}

/// Sync our users with our database and then update Airtable from the database.
pub async fn sync_users(db: &Database, github: &Github, users: BTreeMap<String, UserConfig>, company: &Company) {
    // Get everything we need to authenticate with GSuite.
    // Initialize the GSuite client.
    let token = company.authenticate_google(&db).await;
    let gsuite = GSuite::new(&company.gsuite_account_id, &company.gsuite_domain, token);

    // Initialize the Gusto client.
    let mut gusto_users: HashMap<String, gusto_api::Employee> = HashMap::new();
    let mut gusto_users_by_id: HashMap<String, gusto_api::Employee> = HashMap::new();
    let gusto_auth = company.authenticate_gusto(db).await;
    if let Some(gusto) = gusto_auth {
        let gu = gusto.list_employees().await.unwrap();
        for g in gu {
            gusto_users.insert(g.email.to_string(), g.clone());
            gusto_users_by_id.insert(g.id.to_string(), g);
        }
    }

    // Initialize the Okta client.
    let mut okta_users: HashMap<String, okta::User> = HashMap::new();
    let okta_auth = company.authenticate_okta();
    if let Some(okta) = okta_auth {
        let gu = okta.list_users().await.unwrap();
        for g in gu {
            okta_users.insert(g.profile.email.to_string(), g);
        }
    }

    // Initialize the Ramp client.
    let mut ramp_users: HashMap<String, ramp_api::User> = HashMap::new();
    let mut ramp_departments: HashMap<String, ramp_api::Department> = HashMap::new();
    let ramp_auth = company.authenticate_ramp(db).await;
    if let Some(ref ramp) = ramp_auth {
        let ru = ramp.list_users().await.unwrap();
        for r in ru {
            ramp_users.insert(r.email.to_string(), r);
        }
        let rd = ramp.list_departments().await.unwrap();
        for r in rd {
            ramp_departments.insert(r.name.to_string(), r);
        }
    }

    // Get the existing GSuite users.
    let gsuite_users = gsuite.list_users().await.unwrap();
    let mut gsuite_users_map: BTreeMap<String, GSuiteUser> = BTreeMap::new();
    for g in gsuite_users.clone() {
        // Add the group to our map.
        gsuite_users_map.insert(g.primary_email.to_string(), g);
    }

    // Get the GSuite groups.
    let mut gsuite_groups: BTreeMap<String, GSuiteGroup> = BTreeMap::new();
    let groups = gsuite.list_groups().await.unwrap();
    for g in groups {
        // Add the group to our map.
        gsuite_groups.insert(g.name.to_string(), g);
    }

    // Find the anniversary calendar.
    // Get the list of our calendars.
    let calendars = gsuite.list_calendars().await.unwrap();

    let mut anniversary_cal_id = "".to_string();

    // Iterate over the calendars.
    for calendar in calendars {
        if calendar.summary.contains("Anniversaries") {
            // We are on the anniversaries calendar.
            anniversary_cal_id = calendar.id;
            break;
        }
    }

    // Get all the users.
    let db_users = Users::get_from_db(db, company.id);
    // Create a BTreeMap
    let mut user_map: BTreeMap<String, User> = Default::default();
    for u in db_users {
        user_map.insert(u.username.to_string(), u);
    }
    // Sync users.
    for (_, mut user) in users {
        // Set the user's email.
        user.email = format!("{}@{}", user.username, company.gsuite_domain);

        // Check if we already have the new user in the database.
        let existing = User::get_from_db(db, company.id, user.username.to_string());

        // Update or create the user in the database.
        if let Some(e) = existing.clone() {
            user.google_anniversary_event_id = e.google_anniversary_event_id.to_string();
        }

        // Get the Airtable information for the user.
        if !company.airtable_enterprise_account_id.is_empty() {
            // We don't need a base id here since we are only using the enterprise api features.
            let airtable_auth = company.authenticate_airtable("");
            match airtable_auth.get_enterprise_user(&user.email).await {
                Ok(airtable_user) => {
                    println!("airtable_user: {:?}", airtable_user);
                    user.airtable_id = airtable_user.id.to_string();

                    // If we don't have the airtable user added to our workspace,
                    // we need to add them.
                    let mut has_access_to_workspace = false;
                    for collabs in airtable_user.collaborations.workspace_collaborations {
                        if collabs.workspace_id == company.airtable_workspace_id {
                            // We already have given the user the permissions.
                            has_access_to_workspace = true;
                            break;
                        }
                    }

                    // Add the user, if we found out they did not already have permissions
                    // to the workspace.
                    if !has_access_to_workspace {
                        println!("giving {} access to airtable workspace {}", user.email, company.airtable_workspace_id);
                        //airtable_auth.add_collaborator_to_workspace(&company.airtable_workspace_id, &user.airtable_id, "create").await.unwrap();
                    }
                }
                Err(e) => {
                    println!("getting airtable enterprise user for {} failed: {}", user.email, e);
                }
            }
        }

        // See if we have a gsuite user for the user.
        if let Some(gsuite_user) = gsuite_users_map.get(&user.email) {
            user.google_id = gsuite_user.id.to_string();
        }

        // See if we have a okta user for the user.
        if let Some(okta_user) = okta_users.get(&user.email) {
            user.okta_id = okta_user.id.to_string();
        }

        // See if we have a gusto user for the user.
        // The user's email can either be their personal email or their oxide email.
        if let Some(gusto_user) = gusto_users.get(&user.email) {
            user.update_from_gusto(&gusto_user);
        } else if let Some(gusto_user) = gusto_users.get(&user.recovery_email) {
            user.update_from_gusto(&gusto_user);
        } else {
            // Grab their date of birth, start date, and address from Airtable.
            if let Some(e) = existing.clone() {
                if let Some(airtable_record) = e.get_existing_airtable_record(db).await {
                    user.home_address_street_1 = airtable_record.fields.home_address_street_1.to_string();
                    user.home_address_street_2 = airtable_record.fields.home_address_street_2.to_string();
                    user.home_address_city = airtable_record.fields.home_address_city.to_string();
                    user.home_address_state = airtable_record.fields.home_address_state.to_string();
                    user.home_address_zipcode = airtable_record.fields.home_address_zipcode.to_string();
                    user.home_address_country = airtable_record.fields.home_address_country.to_string();
                    user.birthday = airtable_record.fields.birthday;
                    // Keep the start date in airtable if we already have one.
                    if user.start_date == crate::utils::default_date() && airtable_record.fields.start_date != crate::utils::default_date() {
                        user.start_date = airtable_record.fields.start_date;
                    }
                }

                if !e.gusto_id.is_empty() {
                    if let Some(gusto_user) = gusto_users_by_id.get(&e.gusto_id) {
                        user.update_from_gusto(&gusto_user);
                    }
                }
            }
        }

        // Expand the user.
        user.expand(db, company).await;

        let mut new_user = user.upsert(db).await;

        if existing.is_none() && !company.okta_domain.is_empty() {
            // ONLY DO THIS IF WE USE OKTA FOR CONFIGURATION,
            // OTHERWISE THE GSUITE CODE WILL SEND ITS OWN EMAIL.
            // Now we need to update Okta to include the new user.
            // We do this so that when we send emails from ramp and for the new user,
            // they should have a Google account by then.
            // Sync okta users and group from the database.
            // Do this after we update the users and groups in the database.
            generate_terraform_files_for_okta(github, db, company).await;
            // TODO: this is horrible, but we will sleep here to allow the terraform
            // job to run.
            // We also need a better way to ensure the terraform job passed...
            thread::sleep(time::Duration::from_secs(120));

            // The user did not already exist in the database.
            // We should send them an email about setting up their account.
            println!("sending email to new user: {}", new_user.username);
            if new_user.is_consultant() {
                new_user.send_email_new_consultant(db).await;
            } else {
                new_user.send_email_new_user(db).await;
            }
        }

        if let Some(ref ramp) = ramp_auth {
            if !new_user.is_consultant() && !new_user.is_system_account() {
                // Check if we have a Ramp user for the user.
                match ramp_users.get(&new_user.email) {
                    // We have the user, we don't need to do anything.
                    Some(ramp_user) => {
                        new_user.ramp_id = ramp_user.id.to_string();
                    }
                    None => {
                        println!("inviting new ramp user {}", new_user.username);
                        // Invite the new ramp user.
                        let mut ramp_user: ramp_api::User = Default::default();
                        ramp_user.email = new_user.email.to_string();
                        ramp_user.first_name = new_user.first_name.to_string();
                        ramp_user.last_name = new_user.last_name.to_string();
                        ramp_user.phone = new_user.recovery_phone.to_string();
                        ramp_user.role = "BUSINESS_USER".to_string();
                        if let Some(department) = ramp_departments.get(&new_user.department) {
                            ramp_user.department_id = department.id.to_string();
                        }
                        let r = ramp.invite_new_user(&ramp_user).await.unwrap();
                        new_user.ramp_id = r.id.to_string();

                        // TODO: Create them a card.
                    }
                }
            }
        }

        // Update with any other changes we made to the user.
        new_user.update(db).await;

        // Remove the user from the BTreeMap.
        user_map.remove(&user.username);
    }
    // Remove any users that should no longer be in the database.
    // This is found by the remaining users that are in the map since we removed
    // the existing repos from the map above.
    for (username, user) in user_map {
        println!("deleting user {} from the database", username);

        if !user.google_anniversary_event_id.is_empty() {
            // First delete the recurring event for their anniversary.
            gsuite.delete_calendar_event(&anniversary_cal_id, &user.google_anniversary_event_id).await.unwrap();
            println!("deleted user {} event {} from google", username, user.google_anniversary_event_id);
        }

        if company.okta_domain.is_empty() {
            // Delete the user from GSuite.
            // ONLY DO THIS IF THE COMPANY DOES NOT USE OKTA.
            gsuite.delete_user(&user.email).await.unwrap_or_else(|e| panic!("deleting user {} from gsuite failed: {}", username, e));
            println!("deleted user from gsuite: {}", username);
        }

        // Delete the user from the database and Airtable.
        user.delete(db).await;
    }
    println!("updated configs users in the database");

    if company.okta_domain.is_empty() {
        // Update the users in GSuite.
        // ONLY DO THIS IF THE COMPANY DOES NOT USE OKTA.
        // Get all the users.
        let db_users = Users::get_from_db(db, company.id);
        // Create a BTreeMap
        let mut user_map: BTreeMap<String, User> = Default::default();
        for u in db_users {
            user_map.insert(u.username.to_string(), u);
        }
        // Iterate over the users already in GSuite.
        for u in gsuite_users {
            // Get the shorthand username and match it against our existing users.
            let username = u.primary_email.trim_end_matches(&format!("@{}", company.gsuite_domain)).to_string();

            // Check if we have that user already in our settings.
            let user: User;
            match user_map.get(&username) {
                Some(val) => user = val.clone(),
                None => {
                    // If the user does not exist in our map we need to delete
                    // them from GSuite.
                    println!("deleting user {} from gsuite", username);
                    gsuite
                        .delete_user(&format!("{}@{}", username, company.gsuite_domain))
                        .await
                        .unwrap_or_else(|e| panic!("deleting user {} from gsuite failed: {}", username, e));

                    println!("deleted user from gsuite: {}", username);
                    continue;
                }
            }

            // Update the user with the settings from the config for the user.
            let gsuite_user = update_gsuite_user(&u, &user, false, company).await;

            gsuite.update_user(&gsuite_user).await.unwrap_or_else(|e| panic!("updating user {} in gsuite failed: {}", username, e));

            update_user_aliases(&gsuite, &gsuite_user, user.aliases.clone(), company).await;

            // Add the user to their teams and groups.
            update_user_google_groups(&gsuite, &user, gsuite_groups.clone()).await;

            // Remove the user from the user map and continue.
            // This allows us to add all the remaining new user after.
            user_map.remove(&username);

            println!("updated user in gsuite: {}", username);
        }

        // Create any remaining users from the database that we do not have in GSuite.
        for (username, mut user) in user_map {
            // Create the user.
            let u: GSuiteUser = Default::default();

            // The last argument here tell us to create a password!
            // Make sure it is set to true.
            let gsuite_user = update_gsuite_user(&u, &user, true, company).await;

            let new_gsuite_user = gsuite.create_user(&gsuite_user).await.unwrap_or_else(|e| panic!("creating user {} in gsuite failed: {}", username, e));
            user.google_id = new_gsuite_user.id.to_string();
            // Update with any other changes we made to the user.
            user.update(db).await;

            // Send an email to the new user.
            // Do this here in case another step fails.
            user.send_email_new_gsuite_user(db, &gsuite_user.password).await;
            println!("created new user in gsuite: {}", username);

            update_user_aliases(&gsuite, &gsuite_user, user.aliases.clone(), company).await;

            // Add the user to their teams and groups.
            update_user_google_groups(&gsuite, &user, gsuite_groups.clone()).await;
        }
    }

    // Update users in airtable.
    Users::get_from_db(db, company.id).update_airtable(db).await;
}

/// Sync our buildings with our database and then update Airtable from the database.
pub async fn sync_buildings(db: &Database, buildings: BTreeMap<String, BuildingConfig>, company: &Company) {
    // Get everything we need to authenticate with GSuite.
    // Initialize the GSuite client.
    let token = company.authenticate_google(&db).await;
    let gsuite = GSuite::new(&company.gsuite_account_id, &company.gsuite_domain, token);

    // Get the existing google buildings.
    let gsuite_buildings = gsuite.list_buildings().await.unwrap();

    // Get all the buildings.
    let db_buildings = Buildings::get_from_db(db, company.id);
    // Create a BTreeMap
    let mut building_map: BTreeMap<String, Building> = Default::default();
    for u in db_buildings {
        building_map.insert(u.name.to_string(), u);
    }
    // Sync buildings.
    for (_, mut building) in buildings {
        building.expand(company);

        building.upsert(db).await;

        // Remove the building from the BTreeMap.
        building_map.remove(&building.name);
    }
    // Remove any buildings that should no longer be in the database.
    // This is found by the remaining buildings that are in the map since we removed
    // the existing repos from the map above.
    for (name, building) in building_map {
        println!("deleting building {} from the database, gsuite, etc", name);

        building.delete(db).await;

        // Delete the building from GSuite.
        gsuite.delete_building(&name).await.unwrap_or_else(|e| panic!("deleting building {} from gsuite failed: {}", name, e));
        println!("deleted building from gsuite: {}", name);
    }
    println!("updated configs buildings in the database");

    // Update the buildings in GSuite.
    // Get all the buildings.
    let db_buildings = Buildings::get_from_db(db, company.id);
    // Create a BTreeMap
    let mut building_map: BTreeMap<String, Building> = Default::default();
    for u in db_buildings {
        building_map.insert(u.name.to_string(), u);
    }
    for b in gsuite_buildings {
        let id = b.id.to_string();

        // Check if we have that building already in our database.
        let building: Building;
        match building_map.get(&id) {
            Some(val) => building = val.clone(),
            None => {
                // If the building does not exist in our map we need to delete
                // them from GSuite.
                println!("deleting building {} from gsuite", id);
                gsuite.delete_building(&id).await.unwrap_or_else(|e| panic!("deleting building {} from gsuite failed: {}", id, e));

                println!("deleted building from gsuite: {}", id);
                continue;
            }
        }

        // Update the building with the settings from the database for the building.
        let new_b = update_gsuite_building(&b, &building, &id);

        // Update the building with the given settings.
        gsuite.update_building(&new_b).await.unwrap_or_else(|e| panic!("updating building {} in gsuite failed: {}", id, e));

        // Remove the building from the database map and continue.
        // This allows us to add all the remaining new building after.
        building_map.remove(&id);

        println!("updated building from gsuite: {}", id);
    }

    // Create any remaining buildings from the database that we do not have in GSuite.
    for (id, building) in building_map {
        // Create the building.
        let b: GSuiteBuilding = Default::default();

        let new_b = update_gsuite_building(&b, &building, &id);

        gsuite.create_building(&new_b).await.unwrap_or_else(|e| panic!("creating building {} in gsuite failed: {}", id, e));

        println!("created building from gsuite: {}", id);
    }

    // Update buildings in airtable.
    Buildings::get_from_db(db, company.id).update_airtable(db).await;
}

/// Sync our conference_rooms with our database and then update Airtable from the database.
pub async fn sync_conference_rooms(db: &Database, conference_rooms: BTreeMap<String, ResourceConfig>, company: &Company) {
    // Get everything we need to authenticate with GSuite.
    // Initialize the GSuite client.
    let token = company.authenticate_google(&db).await;
    let gsuite = GSuite::new(&company.gsuite_account_id, &company.gsuite_domain, token);

    // Get the existing GSuite calendar resources.
    let g_suite_calendar_resources = gsuite.list_calendar_resources().await.unwrap();

    // Get all the conference_rooms.
    let db_conference_rooms = ConferenceRooms::get_from_db(db, company.id);
    // Create a BTreeMap
    let mut conference_room_map: BTreeMap<String, ConferenceRoom> = Default::default();
    for u in db_conference_rooms {
        conference_room_map.insert(u.name.to_string(), u);
    }
    // Sync conference_rooms.
    for (_, mut conference_room) in conference_rooms {
        conference_room.cio_company_id = company.id;
        conference_room.upsert(db).await;

        // Remove the conference_room from the BTreeMap.
        conference_room_map.remove(&conference_room.name);
    }
    // Remove any conference_rooms that should no longer be in the database.
    // This is found by the remaining conference_rooms that are in the map since we removed
    // the existing repos from the map above.
    for (name, room) in conference_room_map {
        println!("deleting conference room {} from the database", name);
        room.delete(db).await;
    }
    println!("updated configs conference_rooms in the database");

    // Update the conference_rooms in GSuite.
    // Get all the conference_rooms.
    let db_conference_rooms = ConferenceRooms::get_from_db(db, company.id);
    // Create a BTreeMap
    let mut conference_room_map: BTreeMap<String, ConferenceRoom> = Default::default();
    for u in db_conference_rooms {
        conference_room_map.insert(u.name.to_string(), u);
    }
    for r in g_suite_calendar_resources {
        let id = r.name.to_string();

        // Check if we have that resource already in our database.
        let resource: ConferenceRoom;
        match conference_room_map.get(&id) {
            Some(val) => resource = val.clone(),
            None => {
                // If the conference room does not exist in our map we need to delete
                // it from GSuite.
                println!("deleting conference room {} from gsuite", id);
                gsuite
                    .delete_calendar_resource(&r.id)
                    .await
                    .unwrap_or_else(|e| panic!("deleting conference room {} with id {} from gsuite failed: {}", id, r.id, e));

                println!("deleted conference room from gsuite: {}", id);
                continue;
            }
        }

        // Update the resource with the settings from the database for the resource.
        let new_r = update_gsuite_calendar_resource(&r, &resource, &r.id);

        // Update the resource with the given settings.
        gsuite
            .update_calendar_resource(&new_r)
            .await
            .unwrap_or_else(|e| panic!("updating conference room {} in gsuite failed: {}", id, e));

        // Remove the resource from the database map and continue.
        // This allows us to add all the remaining new resource after.
        conference_room_map.remove(&id);

        println!("updated conference room in gsuite: {}", id);
    }

    // Create any remaining resources from the database that we do not have in GSuite.
    for (id, resource) in conference_room_map {
        // Create the resource.
        let r: GSuiteCalendarResource = Default::default();

        let new_r = update_gsuite_calendar_resource(&r, &resource, &id);

        gsuite
            .create_calendar_resource(&new_r)
            .await
            .unwrap_or_else(|e| panic!("creating conference room {} in gsuite failed: {}", id, e));

        println!("created conference room in gsuite: {}", id);
    }

    // Update conference_rooms in airtable.
    ConferenceRooms::get_from_db(db, company.id).update_airtable(db).await;
}

/// Sync our groups with our database and then update Airtable from the database.
pub async fn sync_groups(db: &Database, groups: BTreeMap<String, GroupConfig>, company: &Company) {
    // Get everything we need to authenticate with GSuite.
    // Initialize the GSuite client.
    let token = company.authenticate_google(&db).await;
    let gsuite = GSuite::new(&company.gsuite_account_id, &company.gsuite_domain, token);

    // Get the GSuite groups.
    let gsuite_groups = gsuite.list_groups().await.unwrap();

    // Get all the groups.
    let db_groups = Groups::get_from_db(db, company.id);
    // Create a BTreeMap
    let mut group_map: BTreeMap<String, Group> = Default::default();
    for u in db_groups {
        group_map.insert(u.name.to_string(), u);
    }
    // Sync groups.
    for (_, mut group) in groups {
        group.expand(company);

        group.upsert(db).await;

        // Remove the group from the BTreeMap.
        group_map.remove(&group.name);
    }
    // Remove any groups that should no longer be in the database.
    // This is found by the remaining groups that are in the map since we removed
    // the existing repos from the map above.
    for (name, group) in group_map {
        println!("deleting group {} from the database, gsuite, etc", name);

        // Delete the group from the database and Airtable.
        group.delete(db).await;

        // Remove the group from GSuite.
        gsuite
            .delete_group(&format!("{}@{}", name, &company.gsuite_domain))
            .await
            .unwrap_or_else(|e| panic!("deleting group {} from gsuite failed: {}", name, e));
        println!("deleted group from gsuite: {}", name);
    }
    println!("updated configs groups in the database");

    // Update the groups in GSuite.
    // Get all the groups.
    let db_groups = Groups::get_from_db(db, company.id);
    // Create a BTreeMap
    let mut group_map: BTreeMap<String, Group> = Default::default();
    for u in db_groups {
        group_map.insert(u.name.to_string(), u);
    }
    // Iterate over the groups already in GSuite.
    for g in gsuite_groups {
        let name = g.name.to_string();

        // Check if we already have this group in our database.
        let group = if let Some(val) = group_map.get(&name) {
            val
        } else {
            // If the group does not exist in our map we need to delete
            // group from GSuite.
            println!("deleting group {} from gsuite", name);
            gsuite
                .delete_group(&format!("{}@{}", name, &company.gsuite_domain))
                .await
                .unwrap_or_else(|e| panic!("deleting group {} from gsuite failed: {}", name, e));
            println!("deleted group from gsuite: {}", name);
            continue;
        };

        // Update the group with the settings from the database for the group.
        let mut updated_group: GSuiteGroup = g.clone();
        updated_group.description = group.description.to_string();

        // Write the group aliases.
        let mut aliases: Vec<String> = Default::default();
        for alias in &group.aliases {
            aliases.push(format!("{}@{}", alias, &company.gsuite_domain));
        }
        updated_group.aliases = aliases;

        gsuite.update_group(&updated_group).await.unwrap_or_else(|e| panic!("updating group {} in gsuite failed: {}", name, e));

        update_group_aliases(&gsuite, &updated_group).await;

        // Update the groups settings.
        update_google_group_settings(&gsuite, &group, company).await;

        // Remove the group from the database map and continue.
        // This allows us to add all the remaining new groups after.
        group_map.remove(&name);

        println!("updated group in gsuite: {}", name);
    }

    // Create any remaining groups from the database  that we do not have in GSuite.
    for (name, group) in group_map {
        // Create the group.
        let mut g: GSuiteGroup = Default::default();

        // TODO: Make this more DRY since it is repeated above as well.
        g.name = group.name.to_string();
        g.email = format!("{}@{}", group.name, company.gsuite_domain);
        g.description = group.description.to_string();

        // Write the group aliases.
        let mut aliases: Vec<String> = Default::default();
        for alias in &group.aliases {
            aliases.push(format!("{}@{}", alias, &company.gsuite_domain));
        }
        g.aliases = aliases;

        let new_group: GSuiteGroup = gsuite.create_group(&g).await.unwrap_or_else(|e| panic!("creating group {} in gsuite failed: {}", name, e));

        update_group_aliases(&gsuite, &new_group).await;

        // Update the groups settings.
        update_google_group_settings(&gsuite, &group, company).await;

        println!("created group in gsuite: {}", name);
    }

    // Update groups in airtable.
    Groups::get_from_db(db, company.id).update_airtable(db).await;
}

/// Sync our links with our database and then update Airtable from the database.
pub async fn sync_links(db: &Database, links: BTreeMap<String, LinkConfig>, huddles: BTreeMap<String, HuddleConfig>, company: &Company) {
    // Get all the links.
    let db_links = Links::get_from_db(db, company.id);
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

        link.upsert(db).await;

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

        link.upsert(db).await;

        // Remove the link from the BTreeMap.
        link_map.remove(&link.name);

        // Update the link for the form.
        link.name = format!("{}-huddle-form", slug);
        link.link = huddle.link_to_airtable_form.to_string();
        link.aliases = vec![format!("airtable-{}-huddle-form", slug)];
        link.short_link = format!("https://{}-huddle-form.corp.{}", slug, company.domain);
        link.description = format!("Form for submitting topics to the {}", huddle.description.to_lowercase());

        link.upsert(db).await;

        // Remove the link from the BTreeMap.
        link_map.remove(&link.name);
    }
    // Remove any links that should no longer be in the database.
    // This is found by the remaining links that are in the map since we removed
    // the existing repos from the map above.
    for (_, link) in link_map {
        link.delete(db).await;
    }
    println!("updated configs links in the database");

    // Update links in airtable.
    Links::get_from_db(db, company.id).update_airtable(db).await;
}

/// Sync our certificates with our database and then update Airtable from the database.
pub async fn sync_certificates(db: &Database, github: &Github, certificates: BTreeMap<String, NewCertificate>, company: &Company) {
    // Get all the certificates.
    let db_certificates = Certificates::get_from_db(db, company.id);
    // Create a BTreeMap
    let mut certificate_map: BTreeMap<String, Certificate> = Default::default();
    for u in db_certificates {
        certificate_map.insert(u.domain.to_string(), u);
    }
    // Sync certificates.
    for (_, mut certificate) in certificates {
        certificate.cio_company_id = company.id;

        certificate.populate_from_github(github, company).await;

        // If the cert is going to expire in less than 7 days, renew it.
        // Otherwise, return early.
        if certificate.valid_days_left > 7 {
            println!("cert {} is valid for {} more days, skipping", certificate.domain, certificate.valid_days_left);
        } else {
            // Populate the certificate.
            certificate.populate(company).await;

            // Save the certificate to disk.
            certificate.save_to_github_repo(github, company).await;
        }

        if certificate.certificate.is_empty() {
            // Continue early.
            continue;
        }

        // Update the database and Airtable.
        certificate.upsert(db).await;

        // Remove the certificate from the BTreeMap.
        certificate_map.remove(&certificate.domain);
    }

    // Remove any certificates that should no longer be in the database.
    // This is found by the remaining certificates that are in the map since we removed
    // the existing repos from the map above.
    for (_, cert) in certificate_map {
        cert.delete(db).await;
    }
    println!("updated configs certificates in the database");

    // Update certificates in airtable.
    Certificates::get_from_db(db, company.id).update_airtable(&db).await;
}

pub async fn refresh_db_configs_and_airtable(db: &Database, company: &Company) {
    let github = company.authenticate_github();

    let configs = get_configs_from_repo(&github, &company).await;

    // Sync buildings.
    // Syncing buildings must happen before we sync conference rooms.
    sync_buildings(&db, configs.buildings, &company).await;

    // Sync conference rooms.
    sync_conference_rooms(&db, configs.resources, &company).await;

    // Sync groups.
    // Syncing groups must happen before we sync the users.
    sync_groups(&db, configs.groups, &company).await;

    // Sync users.
    sync_users(&db, &github, configs.users, &company).await;

    // Sync okta users and group from the database.
    // Do this after we update the users and groups in the database.
    generate_terraform_files_for_okta(&github, &db, &company).await;
    // Generate the terraform files for teams.
    generate_terraform_files_for_aws_and_github(&github, &db, &company).await;

    // Sync links.
    sync_links(&db, configs.links, configs.huddles, &company).await;

    // Sync certificates.
    sync_certificates(&db, &github, configs.certificates, &company).await;

    // Sync github outside collaborators.
    sync_github_outside_collaborators(&github, configs.github_outside_collaborators, &company).await;

    refresh_anniversary_events(&db, &company).await;
}

pub async fn refresh_anniversary_events(db: &Database, company: &Company) {
    // Get everything we need to authenticate with GSuite.
    // Initialize the GSuite client.
    let token = company.authenticate_google(&db).await;
    let gsuite = GSuite::new(&company.gsuite_account_id, &company.gsuite_domain, token);

    // Find the anniversary calendar.
    // Get the list of our calendars.
    let calendars = gsuite.list_calendars().await.unwrap();

    let mut anniversary_cal_id = "".to_string();

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
        return;
    }

    // Get our list of users from our database.
    let users = Users::get_from_db(db, company.id);
    // For each user, create an anniversary for their start date.
    for mut user in users {
        // We only care if the user has a start date.
        if user.start_date == crate::utils::default_date() {
            continue;
        }

        // Create a new event.
        let mut new_event: CalendarEvent = Default::default();

        new_event.start = Date {
            time_zone: "America/Los_Angeles".to_string(),
            date: Some(user.start_date),
            date_time: None,
        };
        new_event.end = Date {
            time_zone: "America/Los_Angeles".to_string(),
            date: Some(user.start_date),
            date_time: None,
        };
        new_event.summary = format!("{} {}'s Anniversary", user.first_name, user.last_name);
        new_event.description = format!(
            "On {}, {} {} joined the company!",
            user.start_date.format("%A, %B %-d, %C%y").to_string(),
            user.first_name,
            user.last_name
        );
        new_event.recurrence = vec!["RRULE:FREQ=YEARLY;".to_string()];
        new_event.transparency = "transparent".to_string();
        new_event.attendees = vec![Attendee {
            id: Default::default(),
            email: user.email.to_string(),
            display_name: Default::default(),
            organizer: false,
            resource: false,
            optional: false,
            response_status: Default::default(),
            comment: Default::default(),
            additional_guests: 0,
        }];

        if user.google_anniversary_event_id.is_empty() {
            // Create the event.
            let event = gsuite.create_calendar_event(&anniversary_cal_id, &new_event).await.unwrap();
            println!("created event for user {} anniversary: {:?}", user.username, event);

            user.google_anniversary_event_id = event.id.to_string();
        } else {
            // Get the existing event.
            let old_event = gsuite.get_calendar_event(&anniversary_cal_id, &user.google_anniversary_event_id).await.unwrap();
            // Set the correct sequence so we don't error out.
            new_event.sequence = old_event.sequence;

            // Update the event.
            let event = gsuite.update_calendar_event(&anniversary_cal_id, &user.google_anniversary_event_id, &new_event).await.unwrap();
            println!("updated event for user {} anniversary: {:?}", user.username, event);
        }

        // Update the user in the database.
        user.update(db).await;
    }
}

#[cfg(test)]
mod tests {
    use crate::companies::Companys;
    use crate::configs::refresh_db_configs_and_airtable;
    use crate::db::Database;

    #[ignore]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_cron_configs() {
        // Initialize our database.
        let db = Database::new();
        let companies = Companys::get_from_db(&db, 1);
        // Iterate over the companies and update.
        for company in companies {
            refresh_db_configs_and_airtable(&db, &company).await;
        }
    }
}
