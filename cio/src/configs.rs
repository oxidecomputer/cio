use std::collections::BTreeMap;
use std::fs;
use std::str::from_utf8;

use clap::ArgMatches;
use futures_util::stream::TryStreamExt;
use hubcaps::Github;
use macros::db_struct;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::db::Database;
use crate::schema::{
    buildings, conference_rooms, github_labels, groups, links, users,
};
use crate::utils::github_org;

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

    pub github_outside_collaborators:
        BTreeMap<String, GitHubOutsideCollaboratorsConfig>,
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
}]
#[serde(rename_all = "camelCase")]
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

    #[serde(default, alias = "is_super_admin")]
    pub is_super_admin: bool,

    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub building: String,

    #[serde(
        default,
        alias = "aws_role",
        skip_serializing_if = "String::is_empty"
    )]
    pub aws_role: String,
}

/// The data type for a group. This applies to Google Groups.
#[db_struct {
    new_name = "Group",
}]
#[serde(rename_all = "camelCase")]
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
    pub description: String,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,

    /// allow_external_members: Identifies whether members external to your
    /// organization can join the group. Possible values are:
    /// - true: G Suite users external to your organization can become
    /// members of this group.
    /// - false: Users not belonging to the organization are not allowed to
    /// become members of this group.
    #[serde(alias = "allow_external_members")]
    pub allow_external_members: bool,

    /// allow_web_posting: Allows posting from web. Possible values are:
    /// - true: Allows any member to post to the group forum.
    /// - false: Members only use Gmail to communicate with the group.
    #[serde(alias = "allow_web_posting")]
    pub allow_web_posting: bool,

    /// is_archived: Allows the Group contents to be archived. Possible values
    /// are:
    /// - true: Archive messages sent to the group.
    /// - false: Do not keep an archive of messages sent to this group. If
    /// false, previously archived messages remain in the archive.
    #[serde(alias = "is_archived")]
    pub is_archived: bool,

    /// who_can_discover_group: Specifies the set of users for whom this group
    /// is discoverable. Possible values are:
    /// - ANYONE_CAN_DISCOVER
    /// - ALL_IN_DOMAIN_CAN_DISCOVER
    /// - ALL_MEMBERS_CAN_DISCOVER
    #[serde(alias = "who_can_discover_group")]
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
    #[serde(alias = "who_can_join")]
    pub who_can_join: String,

    /// who_can_moderate_members: Specifies who can manage members. Possible
    /// values are:
    /// - ALL_MEMBERS
    /// - OWNERS_AND_MANAGERS
    /// - OWNERS_ONLY
    /// - NONE
    #[serde(alias = "who_can_moderate_members")]
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
    #[serde(alias = "who_can_post_message")]
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
    #[serde(alias = "who_can_view_group")]
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
    #[serde(alias = "who_can_view_membership")]
    pub who_can_view_membership: String,
}

/// The data type for a building.
#[db_struct {
    new_name = "Building",
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
    pub description: String,
    pub address: String,
    pub city: String,
    pub state: String,
    pub zipcode: String,
    pub country: String,
    pub floors: Vec<String>,
}

/// The data type for a resource. These are conference rooms that people can book
/// through GSuite or Zoom.
#[db_struct {
    new_name = "ConferenceRoom",
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
    pub description: String,
    #[serde(rename = "type")]
    pub typev: String,
    pub building: String,
    pub capacity: i32,
    pub floor: String,
    pub section: String,
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
    for (_, building) in configs.buildings {
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
    for (_, group) in configs.groups {
        db.upsert_group(&group);
    }

    // Sync links.
    for (name, mut link) in configs.links {
        link.name = name;
        db.upsert_link(&link);
    }

    // Sync users.
    for (_, user) in configs.users {
        db.upsert_user(&user);
    }
}

#[cfg(test)]
mod tests {
    use crate::configs::refresh_db_configs;
    use crate::utils::authenticate_github;

    #[tokio::test(threaded_scheduler)]
    async fn test_configs() {
        let github = authenticate_github();
        refresh_db_configs(&github).await;
    }
}
