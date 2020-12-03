use std::collections::BTreeMap;
use std::fs;
use std::str::from_utf8;

use async_trait::async_trait;
use chrono::naive::NaiveDate;
use clap::ArgMatches;
use futures_util::stream::TryStreamExt;
use hubcaps::Github;
use macros::db_struct;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::airtable::{
    AIRTABLE_BASE_ID_DIRECTORY, AIRTABLE_BUILDINGS_TABLE,
    AIRTABLE_CONFERENCE_ROOMS_TABLE, AIRTABLE_EMPLOYEES_TABLE,
    AIRTABLE_GROUPS_TABLE,
};
use crate::certs::NewCertificate;
use crate::core::UpdateAirtableRecord;
use crate::db::Database;
use crate::schema::{
    buildings, conference_rooms, github_labels, groups, links, users,
};
use crate::utils::{get_github_user_public_ssh_keys, github_org};

/// The data type for our configuration files.
#[derive(
    Debug, Default, PartialEq, Clone, JsonSchema, Deserialize, Serialize,
)]
pub struct Config {
    pub users: BTreeMap<String, UserConfig>,
    pub groups: BTreeMap<String, GroupConfig>,

    pub buildings: BTreeMap<String, BuildingConfig>,
    pub resources: BTreeMap<String, ResourceConfig>,

    pub links: BTreeMap<String, LinkConfig>,

    pub labels: Vec<LabelConfig>,

    #[serde(alias = "github-outside-collaborators")]
    pub github_outside_collaborators:
        BTreeMap<String, GitHubOutsideCollaboratorsConfig>,

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
            let body =
                fs::read_to_string(file).expect("reading the file failed");

            // Append the body of the file to the rest of the contents.
            contents.push_str(&body);
        }

        // Decode the contents.
        let config: Config = toml::from_str(&contents).unwrap();

        config
    }
}

/// The data type for a user.
#[db_struct {
    new_name = "User",
    base_id = "AIRTABLE_BASE_ID_DIRECTORY",
    table = "AIRTABLE_EMPLOYEES_TABLE",
}]
#[derive(
    Debug,
    Insertable,
    AsChangeset,
    PartialEq,
    Clone,
    JsonSchema,
    Deserialize,
    Serialize,
)]
#[table_name = "users"]
pub struct UserConfig {
    #[serde(alias = "first_name")]
    pub first_name: String,
    #[serde(alias = "last_name")]
    pub last_name: String,
    pub username: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    #[serde(
        default,
        alias = "recovery_email",
        skip_serializing_if = "String::is_empty"
    )]
    pub recovery_email: String,
    #[serde(
        default,
        alias = "recovery_phone",
        skip_serializing_if = "String::is_empty"
    )]
    pub recovery_phone: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub gender: String,

    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub chat: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub github: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub twitter: String,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<String>,

    #[serde(default, alias = "is_group_admin")]
    pub is_group_admin: bool,
    #[serde(default, alias = "is_system_account")]
    pub is_system_account: bool,

    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub building: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub link_to_building: Vec<String>,

    #[serde(
        default,
        alias = "aws_role",
        skip_serializing_if = "String::is_empty"
    )]
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
    pub home_address_formatted: String,
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
    pub fn serialize<S>(
        date: &NaiveDate,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
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
    pub fn deserialize<'de, D>(
        deserializer: D,
    ) -> Result<DateTime<Utc>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;

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
    async fn populate_ssh_keys(&mut self) {
        if self.github.is_empty() {
            // Return early if we don't know their github handle.
            return;
        }

        self.public_ssh_keys =
            get_github_user_public_ssh_keys(&self.github).await;
    }

    async fn populate_from_gusto(&mut self) {
        // TODO: actually get the data from Guso once we have credentials.
        let mut street_address = self.home_address_street_1.to_string();
        if !self.home_address_street_2.is_empty() {
            street_address = format!(
                "{}\n{}",
                self.home_address_street_1, self.home_address_street_2,
            );
        }
        self.home_address_formatted = format!(
            "{}\n{}, {} {}, {}",
            street_address,
            self.home_address_city,
            self.home_address_state,
            self.home_address_zipcode,
            self.home_address_country
        );
    }

    pub async fn expand(&mut self) {
        self.populate_ssh_keys().await;

        self.populate_from_gusto().await;
    }
}

/// Implement updating the Airtable record for a User.
#[async_trait]
impl UpdateAirtableRecord<User> for User {
    async fn update_airtable_record(&mut self, _record: User) {
        // Get the current groups in Airtable so we can link to them.
        // TODO: make this more dry so we do not call it every single damn time.
        let groups = Groups::get_from_airtable().await;

        let mut links: Vec<String> = Default::default();
        // Iterate over the group names in our record and match it against the
        // group ids and see if we find a match.
        for group in &self.groups {
            // Iterate over the groups to get the ID.
            for (_id, g) in &groups {
                if group.to_string() == g.fields.name {
                    // Append the ID to our links.
                    links.push(g.id.to_string());
                    // Break the loop and return early.
                    break;
                }
            }
        }

        self.groups = links;

        // Set the building to right building link.
        // Get the current buildings in Airtable so we can link to it.
        // TODO: make this more dry so we do not call it every single damn time.
        let buildings = Buildings::get_from_airtable().await;
        // Iterate over the buildings to get the ID.
        for (_id, building) in &buildings {
            if self.building == building.fields.name {
                // Set the ID.
                self.link_to_building = vec![building.id.to_string()];
                // Break the loop and return early.
                break;
            }
        }
    }
}

/// The data type for a group. This applies to Google Groups.
#[db_struct {
    new_name = "Group",
    base_id = "AIRTABLE_BASE_ID_DIRECTORY",
    table = "AIRTABLE_GROUPS_TABLE",
}]
#[derive(
    Debug,
    Default,
    Insertable,
    AsChangeset,
    PartialEq,
    Clone,
    JsonSchema,
    Deserialize,
    Serialize,
)]
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
    #[serde(
        alias = "who_can_discover_group",
        skip_serializing_if = "String::is_empty",
        default
    )]
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
    #[serde(
        alias = "who_can_join",
        skip_serializing_if = "String::is_empty",
        default
    )]
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
    #[serde(
        alias = "who_can_post_message",
        skip_serializing_if = "String::is_empty",
        default
    )]
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
    #[serde(
        alias = "who_can_view_group",
        skip_serializing_if = "String::is_empty",
        default
    )]
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
    #[serde(
        alias = "who_can_view_membership",
        skip_serializing_if = "String::is_empty",
        default
    )]
    pub who_can_view_membership: String,
}

impl GroupConfig {
    pub fn get_link(&self) -> String {
        format!(
            "https://groups.google.com/a/oxidecomputer.com/forum/#!forum/{}",
            self.name
        )
    }

    pub fn expand(&mut self) {
        self.link = self.get_link();
    }
}

/// Implement updating the Airtable record for a Group.
#[async_trait]
impl UpdateAirtableRecord<Group> for Group {
    async fn update_airtable_record(&mut self, record: Group) {
        // Make sure we don't mess with the members since that is populated by the Users table.
        self.members = record.members.clone();
    }
}

/// The data type for a building.
#[db_struct {
    new_name = "Building",
    base_id = "AIRTABLE_BASE_ID_DIRECTORY",
    table = "AIRTABLE_BUILDINGS_TABLE",
}]
#[derive(
    Debug,
    Insertable,
    AsChangeset,
    Default,
    PartialEq,
    Clone,
    JsonSchema,
    Deserialize,
    Serialize,
)]
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
}

impl BuildingConfig {
    fn expand(&mut self) {
        self.address_formatted = format!(
            "{}\n{}, {} {}, {}",
            self.street_address,
            self.city,
            self.state,
            self.zipcode,
            self.country
        );
    }
}

/// Implement updating the Airtable record for a Building.
#[async_trait]
impl UpdateAirtableRecord<Building> for Building {
    async fn update_airtable_record(&mut self, record: Building) {
        // Make sure we don't mess with the employees since that is populated by the Users table.
        self.employees = record.employees.clone();
        // Make sure we don't mess with the conference_rooms since that is populated by the Conference Rooms table.
        self.conference_rooms = record.conference_rooms.clone();
    }
}

/// The data type for a resource. These are conference rooms that people can book
/// through GSuite or Zoom.
#[db_struct {
    new_name = "ConferenceRoom",
    base_id = "AIRTABLE_BASE_ID_DIRECTORY",
    table = "AIRTABLE_CONFERENCE_ROOMS_TABLE",
}]
#[derive(
    Debug,
    Insertable,
    AsChangeset,
    Default,
    PartialEq,
    Clone,
    JsonSchema,
    Deserialize,
    Serialize,
)]
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
}

/// Implement updating the Airtable record for a ConferenceRoom.
#[async_trait]
impl UpdateAirtableRecord<ConferenceRoom> for ConferenceRoom {
    async fn update_airtable_record(&mut self, _record: ConferenceRoom) {
        // Set the building to right building link.
        // Get the current buildings in Airtable so we can link to it.
        // TODO: make this more dry so we do not call it every single damn time.
        let buildings = Buildings::get_from_airtable().await;
        // Iterate over the buildings to get the ID.
        for (_id, building) in &buildings {
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
#[db_struct {
    new_name = "Link",
}]
#[derive(
    Debug,
    Insertable,
    AsChangeset,
    Default,
    PartialEq,
    Clone,
    JsonSchema,
    Deserialize,
    Serialize,
)]
#[table_name = "links"]
pub struct LinkConfig {
    /// name will not be used in config files.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    pub description: String,
    pub link: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
}

/// The data type for a label. These become GitHub labels for all the repositories
/// in our organization.
#[db_struct {
    new_name = "GithubLabel",
}]
#[derive(
    Debug,
    Insertable,
    AsChangeset,
    Default,
    PartialEq,
    Clone,
    JsonSchema,
    Deserialize,
    Serialize,
)]
#[table_name = "github_labels"]
pub struct LabelConfig {
    pub name: String,
    pub description: String,
    pub color: String,
}

/// The data type for GitHub outside collaborators to repositories.
#[derive(
    Debug, Default, PartialEq, Clone, JsonSchema, Deserialize, Serialize,
)]
pub struct GitHubOutsideCollaboratorsConfig {
    pub description: String,
    pub users: Vec<String>,
    pub repos: Vec<String>,
    pub perm: String,
}

/// The data type for a huddle meeting that syncs with Airtable and notes in GitHub.
#[derive(
    Debug, Default, PartialEq, Clone, JsonSchema, Deserialize, Serialize,
)]
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
    pub google_calendar_event_name: String,
}

/// Get the configs from the GitHub repository and parse them.
pub async fn get_configs_from_repo(github: &Github) -> Config {
    let repo_contents = github.repo(github_org(), "configs").content();

    let files = repo_contents
        .iter("/configs/", "master")
        .try_collect::<Vec<hubcaps::content::DirectoryItem>>()
        .await
        .unwrap();

    let mut file_contents = String::new();
    for file in files {
        println!("decoding {}", file.name);
        // Get the contents of the file.
        let contents = repo_contents
            .file(&format!("/{}", file.path), "master")
            .await
            .unwrap();

        let decoded = from_utf8(&contents.content).unwrap().trim().to_string();

        // Append the body of the file to the rest of the contents.
        file_contents.push_str(&"\n");
        file_contents.push_str(&decoded);
    }

    let config: Config = toml::from_str(&file_contents).unwrap();

    config
}

pub async fn refresh_db_configs(github: &Github) {
    let configs = get_configs_from_repo(&github).await;

    // Initialize our database.
    let db = Database::new();

    // Sync buildings.
    for (_, mut building) in configs.buildings {
        building.expand();

        db.upsert_building(&building);
    }

    // Sync conference rooms.
    for (_, room) in configs.resources {
        db.upsert_conference_room(&room);
    }

    // Sync GitHub labels.
    for label in configs.labels {
        db.upsert_github_label(&label);
    }

    // Sync groups.
    for (_, mut group) in configs.groups {
        group.expand();

        db.upsert_group(&group);
    }

    // Sync links.
    for (name, mut link) in configs.links {
        link.name = name;
        db.upsert_link(&link);

        let conference_rooms = db.get_conference_rooms();
        // Update conference rooms in Airtable.
        ConferenceRooms(conference_rooms).update_airtable().await;
    }

    // Sync users.
    for (_, mut user) in configs.users {
        user.expand().await;

        db.upsert_user(&user);
    }

    // Sync certificates.
    for (_, mut cert) in configs.certificates {
        cert.populate_from_github(github).await;

        db.upsert_certificate(&cert);
    }
}

#[cfg(test)]
mod tests {
    use crate::certs::Certificates;
    use crate::configs::{
        refresh_db_configs, Buildings, ConferenceRooms, Groups, Users,
    };
    use crate::db::Database;
    use crate::utils::authenticate_github;

    #[tokio::test(threaded_scheduler)]
    async fn test_configs() {
        let github = authenticate_github();
        refresh_db_configs(&github).await;

        // Initialize our database.
        let db = Database::new();

        let users = db.get_users();
        // Update users in airtable.
        Users(users).update_airtable().await;

        let groups = db.get_groups();
        // Update groups in Airtable.
        Groups(groups).update_airtable().await;

        let buildings = db.get_buildings();
        // Update buildings in Airtable.
        Buildings(buildings).update_airtable().await;

        let conference_rooms = db.get_conference_rooms();
        // Update conference rooms in Airtable.
        ConferenceRooms(conference_rooms).update_airtable().await;

        let certificates = db.get_certificates();
        // Update certificates in Airtable.
        Certificates(certificates).update_airtable().await;
    }
}
