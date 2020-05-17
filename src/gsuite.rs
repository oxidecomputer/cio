use std::collections::BTreeMap;
use std::env;
use std::rc::Rc;

use clap::{value_t, ArgMatches};
use log::info;

use crate::utils::{get_gsuite_token, read_config_from_files};

use cio::{BuildingConfig, Config, GroupConfig, ResourceConfig, UserConfig};
use gsuite::{Building, CalendarResource, GSuite, Group, User};
use sendgrid::SendGrid;

/**
 * Sync the configuration files with GSuite.
 *
 * This will create or update user's accounts, Google groups, GSuite
 * buildings, and resources.
 */
pub fn cmd_gsuite_run(cli_matches: &ArgMatches) {
    // Get the config.
    let config = read_config_from_files(cli_matches);

    // Get the domain.
    let domain = value_t!(cli_matches, "domain", String).unwrap();

    // Initialize the clients for the config.
    let mut client = GSuiteClient::new(config, domain);

    // Run sync.
    client.sync();
}

/// The Client for performing operations in GSuite.
struct GSuiteClient {
    config: Config,
    domain: String,

    sendgrid: Rc<SendGrid>,

    gsuite: Rc<GSuite>,
    google_groups: BTreeMap<String, Group>,
    google_users: Vec<User>,
    google_resources: Vec<CalendarResource>,
    google_buildings: Vec<Building>,
}

impl GSuiteClient {
    /// Initialize the various clients for groups, users, resources, and buildings.
    pub fn new(config: Config, domain: String) -> Self {
        let gsuite_customer = env::var("GADMIN_ACCOUNT_ID").unwrap();
        // Get the GSuite token.
        let token = get_gsuite_token();

        // Initialize thje GSuite gsuite client.
        let gsuite =
            GSuite::new(gsuite_customer, domain.to_string(), token.clone());

        // Get the existing google groups.
        info!("[google] getting current groups...");
        let mut google_groups: BTreeMap<String, Group> = BTreeMap::new();
        let groups = gsuite.list_groups();
        for g in groups {
            // Add the team to our hash map.
            google_groups.insert(g.clone().name.unwrap().to_string(), g);
        }

        // Get the existing google users.
        info!("[google] getting current users...");
        let google_users = gsuite.list_users();

        // Get the existing google resources.
        info!("[google] getting current calendar resources...");
        let google_resources = gsuite.list_calendar_resources();

        // Get the existing google buildings.
        info!("[google] getting current buildings...");
        let google_buildings = gsuite.list_buildings();

        // Initialize the SendGrid client.
        let sendgrid = SendGrid::new_from_env();

        return Self {
            config: config,
            domain: domain,

            sendgrid: Rc::new(sendgrid),

            gsuite: Rc::new(gsuite),
            google_groups: google_groups,
            google_users: google_users,
            google_resources: google_resources,
            google_buildings: google_buildings,
        };
    }

    /// Sync GSuite with our configuration files.
    pub fn sync(&mut self) {
        // Update the GSuite groups.
        self.update_google_groups();

        // Update the GSuite users.
        self.update_google_users();

        // Update the GSuite resources.
        self.update_google_resources();

        // Update the GSuite buildings.
        self.update_google_buildings();
    }

    /// Update the buildings in GSuite to match our configuration files.
    pub fn update_google_buildings(&mut self) {
        for mut b in self.google_buildings.clone() {
            let id = b.clone().id.to_string();

            // Check if we have that building already in our settings.
            let building: BuildingConfig;
            match self.config.buildings.get(&id) {
                Some(val) => building = val.clone(),
                // Continue through the loop and we will add the building later.
                None => continue,
            }

            // Update the building with the settings from the config for the building.
            b = b.clone().update(building.clone(), id.to_string());

            // Update the building with the given settings.
            self.gsuite.update_building(b.clone());

            // Remove the building from the config map and continue.
            // This allows us to add all the remaining new building after.
            self.config.buildings.remove(&id);

            info!("[google] updated building: {}", building.name);
        }

        // Create any remaining buildings from the config that we do not have in GSuite.
        for (id, building) in &self.config.buildings {
            // Create the building.
            let mut b: Building = Default::default();

            b = b.clone().update(building.clone(), id.to_string());

            self.gsuite.create_building(b);

            info!("[google] created building: {}", id);
        }
    }

    /// Update the resources in GSuite to match our configuration files.
    pub fn update_google_resources(&mut self) {
        for mut r in self.google_resources.clone() {
            // Create a shorthand id for the resource which is the name of the
            // resource with the spaces removed so it works with toml.
            let id = r.clone().name.replace(" ", "");

            // Check if we have that resource already in our settings.
            let resource: ResourceConfig;
            match self.config.resources.get(&id) {
                Some(val) => resource = val.clone(),
                // Continue through the loop and we will add the resource later.
                None => continue,
            }

            // Update the resource with the settings from the config for the resource.
            r = r.clone().update(resource.clone(), r.id.to_string());

            // Update the resource with the given settings.
            self.gsuite.update_calendar_resource(r.clone());

            // Remove the resource from the config map and continue.
            // This allows us to add all the remaining new resource after.
            self.config.resources.remove(&id);

            info!("[google] updated resource: {}", id);
        }

        // Create any remaining resources from the config that we do not have in GSuite.
        for (id, resource) in &self.config.resources {
            // Create the resource.
            let mut r: CalendarResource = Default::default();

            r = r.clone().update(resource.clone(), id.to_string());

            self.gsuite.create_calendar_resource(r);

            info!("[google] created resource: {}", id);
        }
    }

    /// Update the users in GSuite to match our configuration files.
    pub fn update_google_users(&mut self) {
        for mut u in self.google_users.clone() {
            // Get the shorthand username and match it against our existing users.
            let username = u
                .primary_email
                .clone()
                .unwrap()
                .trim_end_matches(&format!("@{}", self.domain))
                .replace(".", "-");

            // Check if we have that user already in our settings.
            let user: UserConfig;
            match self.config.users.get(&username) {
                Some(val) => user = val.clone(),
                // Continue through the loop and we will add the user later.
                None => continue,
            }

            // Update the user with the settings from the config for the user.
            u = u
                .clone()
                .update(user.clone(), self.domain.to_string(), false);

            self.gsuite.update_user(u.clone());

            self.update_user_aliases(u.clone());

            // Add the user to their teams and groups.
            self.update_user(user.clone());

            // Remove the user from the config map and continue.
            // This allows us to add all the remaining new user after.
            self.config.users.remove(&username);

            info!("updated user: {}", username);
        }

        // Create any remaining users from the config that we do not have in GSuite.
        for (username, user) in self.config.users.clone() {
            // Create the user.
            let mut u: User = Default::default();

            u = u
                .clone()
                .update(user.clone(), self.domain.to_string(), true);

            self.gsuite.create_user(u.clone());

            self.update_user_aliases(u.clone());

            // Add the user to their teams and groups.
            self.update_user(user.clone());

            let mut github: String = "".to_string();
            match user.clone().github {
                Some(val) => {
                    github = val.clone().to_string();
                }
                None => (),
            }

            let mut password: String = "".to_string();
            match u.clone().password {
                Some(val) => {
                    password = val.clone().to_string();
                }
                None => (),
            }

            // Send an email to the new user.
            let message = user_email_message(
                u.clone(),
                password.to_string(),
                github.to_string(),
                self.domain.to_string(),
            );
            self.sendgrid.send_new_user(
                u.clone().primary_email.unwrap().to_string(),
                u.clone().recovery_email.unwrap().to_string(),
                message,
            );

            info!("created new user: {}", username);
        }
    }

    /// Update a user's aliases in GSuite to match our configuration files.
    pub fn update_user_aliases(&mut self, u: User) {
        match u.aliases {
            Some(val) => {
                // Update the user's aliases.
                let email = u.primary_email.unwrap();
                self.gsuite.update_user_aliases(email.clone(), val);
                info!("[google] updated user aliases: {}", email);
            }
            None => (),
        }
    }

    /// Update a user in GSuite to match our configuration files.
    pub fn update_user(&mut self, user: UserConfig) {
        self.update_user_google_groups(user.clone());
    }

    /// Update a user's groups in GSuite to match our configuration files.
    pub fn update_user_google_groups(&self, user: UserConfig) {
        let email = format!("{}@{}", user.username, self.domain);
        let groups: Vec<String>;
        match user.groups {
            Some(val) => groups = val,
            None => {
                // Return early because they have no groups.
                return;
            }
        }

        // Iterate over the groups and add the user as a member to it.
        for g in groups.clone() {
            // Make sure the group exists.
            let group: Group;
            match self.google_groups.get(&g) {
                Some(val) => group = val.clone(),
                // Continue through the loop and we will add the user later.
                None => panic!(
                    "google group {} does not exist so cannot add user {}",
                    g, email
                ),
            }

            let mut role = "MEMBER";
            match user.is_super_admin {
                None => (),
                Some(is_super_admin) => {
                    if is_super_admin {
                        role = "OWNER";
                    }
                }
            }

            // Check if the user is already a member of the group.
            let is_member = self
                .gsuite
                .group_has_member(group.clone().id.unwrap(), email.to_string());
            if is_member {
                // They are a member so we can just update their member status.
                self.gsuite.group_update_member(
                    group.clone().id.unwrap(),
                    email.to_string(),
                    role.to_string(),
                );

                // Continue through the other groups.
                continue;
            }

            // Add the user to the group.
            self.gsuite.group_insert_member(
                group.id.unwrap(),
                email.to_string(),
                role.to_string(),
            );

            info!(
                "[groups]: added {} to {} as {}",
                email,
                group.name.unwrap(),
                role
            );
        }

        // Iterate over all the groups and if the user is a member and should not
        // be, remove them from the group.
        for (slug, group) in &self.google_groups {
            if groups.contains(&slug) {
                continue;
            }

            // Now we have a google group. The user should not be a member of it,
            // but we need to make sure they are not a member.
            let is_member = self
                .gsuite
                .group_has_member(group.clone().id.unwrap(), email.to_string());

            if !is_member {
                // They are not a member so we can continue early.
                continue;
            }

            // They are a member of the group.
            // We need to remove them.
            self.gsuite.group_remove_member(
                group.clone().id.unwrap(),
                email.to_string(),
            );

            info!(
                "[groups]: removed {} from {}",
                email,
                group.clone().name.unwrap()
            );
        }
    }

    /// Update the groups in GSuite to match our configuration files.
    pub fn update_google_groups(&mut self) {
        for (slug, g) in &self.google_groups {
            // Check if we already have this group in our config.
            let group: GroupConfig;
            match self.config.groups.get(slug) {
                Some(val) => group = val.clone(),
                // Continue through the loop and we will add the group later.
                None => continue,
            }

            // Update the group with the settings from the config for the group.
            let mut updated_group: Group = g.clone();
            updated_group.description = Some(group.description.to_string());

            // Write the group aliases.
            let mut aliases: Vec<String> = Default::default();
            match group.clone().aliases {
                Some(val) => {
                    for alias in val {
                        aliases.push(format!("{}@{}", alias, self.domain));
                    }
                }
                None => (),
            }
            updated_group.aliases = Some(aliases);

            self.gsuite.update_group(updated_group.clone());

            self.update_group_aliases(updated_group);

            // Update the groups settings.
            self.update_google_group_settings(group);

            // Remove the group from the config map and continue.
            // This allows us to add all the remaining new groups after.
            self.config.groups.remove(slug);

            info!("[groups]: updated group {}", slug);
        }

        // Create any remaining groups from the config that we do not have in GSuite.
        for (slug, group) in &self.config.groups {
            // Create the group.
            let mut g: Group = Default::default();

            g.name = Some(group.name.to_string());
            g.email = Some(format!("{}@{}", group.name, self.domain));
            g.description = Some(group.description.to_string());

            // Write the group aliases.
            let mut aliases: Vec<String> = Default::default();
            match group.clone().aliases {
                Some(val) => {
                    for alias in val {
                        aliases.push(format!("{}@{}", alias, self.domain));
                    }
                }
                None => (),
            }
            g.aliases = Some(aliases);

            let new_group: Group = self.gsuite.create_group(g.clone());

            self.update_group_aliases(g);

            // Update the groups settings.
            self.update_google_group_settings(group.clone());

            // Add the group to our list of GSuite groups so when we add users to
            // the groups, later in the script, it is there.
            self.google_groups.insert(group.name.to_string(), new_group);

            info!("[groups]: created group {}", slug);
        }
    }

    /// Update a group's aliases in GSuite to match our configuration files.
    pub fn update_group_aliases(&self, g: Group) {
        match g.aliases {
            Some(val) => {
                // Update the user's aliases.
                let email = g.email.unwrap();
                self.gsuite.update_group_aliases(email.clone(), val);
                info!("[google] updated group aliases: {}", email);
            }
            None => (),
        }
    }

    /// Update a group's settings in GSuite to match our configuration files.
    pub fn update_google_group_settings(&self, group: GroupConfig) {
        // Get the current group settings.
        let email = format!("{}@{}", group.name, self.domain);
        let mut settings = self.gsuite.get_group_settings(email.to_string());

        // Update the groups settings.
        settings.email = Some(email.to_string());
        settings.name = Some(group.name.to_string());
        settings.description = Some(group.description);
        settings.allow_external_members =
            Some(group.allow_external_members.to_string());
        settings.allow_web_posting = Some(group.allow_web_posting.to_string());
        settings.is_archived = Some(group.is_archived.to_string());
        settings.who_can_discover_group = Some(group.who_can_discover_group);
        settings.who_can_join = Some(group.who_can_join);
        settings.who_can_moderate_members =
            Some(group.who_can_moderate_members);
        settings.who_can_post_message = Some(group.who_can_post_message);
        settings.who_can_view_group = Some(group.who_can_view_group);
        settings.who_can_view_membership = Some(group.who_can_view_membership);
        settings.who_can_contact_owner =
            Some("ALL_IN_DOMAIN_CAN_CONTACT".to_string());

        // Update the group with the given settings.
        self.gsuite.update_group_settings(settings);

        info!("[groups]: updated groups settings {}", group.name);
    }
}

fn user_email_message(
    user: User,
    password: String,
    github: String,
    domain: String,
) -> String {
    // Get the user's aliases if they have one.
    let mut aliases: Vec<String> = Default::default();
    match user.clone().aliases {
        None => (),
        Some(val) => aliases = val.clone(),
    }

    return format!(
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

You will be invited to create a Zoom account from an email sent to {}. Once
completed, your personal URL for Zoom calls will be https://oxide.zoom.us/my/{}.

If you have any questions or your email does not work please email your
administrator, who is cc-ed on this email. Spoiler alert it's Jess...
jess@{}. If you want other email aliases, let Jess know as well.

Once you login to your email, a great place to start would be taking a look at
our on-boarding doc:
https://docs.google.com/document/d/18Nymnd3rU1Nz4woxPfcohFeyouw7FvbYq5fGfQ6ZSGY/edit?usp=sharing.

xoxo,
  The GSuite/GitHub/Zoom Bot",
                        user.clone().name.unwrap().given_name.unwrap(),
                        domain,
                        domain,
                        user.clone().primary_email.unwrap(),
                        password,
                        aliases.join(", "),
                        github.clone(),
                        "oxidecomputer",
                        "oxidecomputer",
                        user.clone().primary_email.unwrap(),
                        github,
                        domain
                    );
}
