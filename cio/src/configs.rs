use std::collections::BTreeMap;
use std::fs;

use clap::ArgMatches;
use serde::{Deserialize, Serialize};

/// The data type for our configuration files.
#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub users: BTreeMap<String, UserConfig>,
    pub groups: BTreeMap<String, GroupConfig>,

    pub buildings: BTreeMap<String, BuildingConfig>,
    pub resources: BTreeMap<String, ResourceConfig>,

    pub links: BTreeMap<String, LinkConfig>,

    pub labels: Vec<LabelConfig>,
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

        let mut contents = String::from("");
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
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UserConfig {
    pub first_name: String,
    pub last_name: String,
    pub username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aliases: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovery_email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovery_phone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gender: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub chat: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub github: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub twitter: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub groups: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_super_admin: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub building: Option<String>,
}

/// The data type for a group. This applies to Google Groups.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GroupConfig {
    pub name: String,
    pub description: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub aliases: Option<Vec<String>>,

    /// allow_external_members: Identifies whether members external to your
    /// organization can join the group. Possible values are:
    /// - true: G Suite users external to your organization can become
    /// members of this group.
    /// - false: Users not belonging to the organization are not allowed to
    /// become members of this group.
    pub allow_external_members: bool,

    /// allow_web_posting: Allows posting from web. Possible values are:
    /// - true: Allows any member to post to the group forum.
    /// - false: Members only use Gmail to communicate with the group.
    pub allow_web_posting: bool,

    /// is_archived: Allows the Group contents to be archived. Possible values
    /// are:
    /// - true: Archive messages sent to the group.
    /// - false: Do not keep an archive of messages sent to this group. If
    /// false, previously archived messages remain in the archive.
    pub is_archived: bool,

    /// who_can_discover_group: Specifies the set of users for whom this group
    /// is discoverable. Possible values are:
    /// - ANYONE_CAN_DISCOVER
    /// - ALL_IN_DOMAIN_CAN_DISCOVER
    /// - ALL_MEMBERS_CAN_DISCOVER
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
    pub who_can_join: String,

    /// who_can_moderate_members: Specifies who can manage members. Possible
    /// values are:
    /// - ALL_MEMBERS
    /// - OWNERS_AND_MANAGERS
    /// - OWNERS_ONLY
    /// - NONE
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
    pub who_can_view_membership: String,
}

/// The data type for a building.
#[derive(Debug, Clone, Deserialize, Serialize)]
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
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ResourceConfig {
    pub name: String,
    pub description: String,
    #[serde(rename = "type")]
    pub typev: String,
    pub building: String,
    pub capacity: i32,
    pub floor: String,
    pub section: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_zoom_room: Option<bool>,
}

/// The data type for a link. These get turned into short links like
/// `{name}.corp.oxide.compuer` by the `shorturls` subcommand.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LinkConfig {
    /// name will not be used in config files.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub description: String,
    pub link: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aliases: Option<Vec<String>>,
    /// subdomain will not be used in config files.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subdomain: Option<String>,
    /// discussion will not be used in config files.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discussion: Option<String>,
}

/// The data type for a label. These become GitHub labels for all the repositories
/// in our organization.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LabelConfig {
    pub name: String,
    pub description: String,
    pub color: String,
}
