use std::collections::BTreeMap;
use std::env;
use std::rc::Rc;

use clap::ArgMatches;
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
pub async fn cmd_gsuite_run(cli_matches: &ArgMatches<'_>) {
    // Get the config.
    let config = read_config_from_files(cli_matches);

    // Get the domain.
    let domain = cli_matches.value_of("domain").unwrap();

    // Initialize the clients for the config.
    let mut client = GSuiteClient::new(config, domain).await;

    // Run sync.
    client.sync().await;
}

/// The Client for performing operations in GSuite.
struct GSuiteClient {
    config: Config,
    domain: String,

    gsuite: Rc<GSuite>,
    google_groups: BTreeMap<String, Group>,
    google_users: Vec<User>,
    google_resources: Vec<CalendarResource>,
    google_buildings: Vec<Building>,
}

impl GSuiteClient {
    /// Initialize the various clients for groups, users, resources, and buildings.
    pub async fn new(config: Config, domain: &str) -> Self {
        let gsuite_customer = env::var("GADMIN_ACCOUNT_ID").unwrap();
        // Get the GSuite token.
        let token = get_gsuite_token().await;

        // Initialize thje GSuite gsuite client.
        let gsuite = GSuite::new(&gsuite_customer, domain, token);

        // Get the existing google groups.
        info!("[google] getting current groups...");
        let mut google_groups: BTreeMap<String, Group> = BTreeMap::new();
        let groups = gsuite.list_groups().await;
        for g in groups {
            // Add the team to our hash map.
            google_groups.insert(g.name.clone().unwrap(), g);
        }

        // Get the existing google users.
        info!("[google] getting current users...");
        let google_users = gsuite.list_users().await;

        // Get the existing google resources.
        info!("[google] getting current calendar resources...");
        let google_resources = gsuite.list_calendar_resources().await;

        // Get the existing google buildings.
        info!("[google] getting current buildings...");
        let google_buildings = gsuite.list_buildings().await;

        Self {
            config,
            domain: domain.to_string(),

            gsuite: Rc::new(gsuite),
            google_groups,
            google_users,
            google_resources,
            google_buildings,
        }
    }

    /// Sync GSuite with our configuration files.
    pub async fn sync(&mut self) {
        // Update the GSuite groups.
        self.update_google_groups().await;

        // Update the GSuite users.
        self.update_google_users().await;

        // Update the GSuite resources.
        self.update_google_resources().await;

        // Update the GSuite buildings.
        self.update_google_buildings().await;
    }

    /// Update the buildings in GSuite to match our configuration files.
    pub async fn update_google_buildings(&mut self) {
        // TODO(cbiffle): fix clone here once update doesn't consume self
        for mut b in self.google_buildings.clone() {
            // TODO(cbiffle): we're cloning id here because b is going to
            // consume itself on update below. I intend to fix this shortly.
            let id = b.id.clone();

            // Check if we have that building already in our settings.
            let building: BuildingConfig;
            match self.config.buildings.get(&id) {
                Some(val) => building = val.clone(),
                // Continue through the loop and we will add the building later.
                None => continue,
            }

            // Update the building with the settings from the config for the building.
            b = b.update(&building, &id);

            // Update the building with the given settings.
            self.gsuite.update_building(&b).await;

            // Remove the building from the config map and continue.
            // This allows us to add all the remaining new building after.
            self.config.buildings.remove(&id);

            info!("[google] updated building: {}", building.name);
        }

        // Create any remaining buildings from the config that we do not have in GSuite.
        for (id, building) in &self.config.buildings {
            // Create the building.
            let mut b: Building = Default::default();

            b = b.update(&building, id);

            self.gsuite.create_building(&b).await;

            info!("[google] created building: {}", id);
        }
    }

    /// Update the resources in GSuite to match our configuration files.
    pub async fn update_google_resources(&mut self) {
        // TODO(cbiffle): fix clone here once update doesn't consume self.
        for mut r in self.google_resources.clone() {
            // Create a shorthand id for the resource which is the name of the
            // resource with the spaces removed so it works with toml.
            // Caution: this is not r.id.
            let id = r.clone().name.replace(" ", "");

            // Check if we have that resource already in our settings.
            let resource: ResourceConfig;
            match self.config.resources.get(&id) {
                Some(val) => resource = val.clone(),
                // Continue through the loop and we will add the resource later.
                None => continue,
            }

            // Update the resource with the settings from the config for the resource.
            // TODO(cbiffle): cloning r.id because r consumes itself; fix later
            let rid = r.id.clone();
            r = r.update(&resource, &rid);

            // Update the resource with the given settings.
            self.gsuite.update_calendar_resource(&r).await;

            // Remove the resource from the config map and continue.
            // This allows us to add all the remaining new resource after.
            self.config.resources.remove(&id);

            info!("[google] updated resource: {}", id);
        }

        // Create any remaining resources from the config that we do not have in GSuite.
        for (id, resource) in &self.config.resources {
            // Create the resource.
            let mut r: CalendarResource = Default::default();

            r = r.update(&resource, id);

            self.gsuite.create_calendar_resource(&r).await;

            info!("[google] created resource: {}", id);
        }
    }

    /// Update the users in GSuite to match our configuration files.
    pub async fn update_google_users(&mut self) {
        // TODO(cbiffle): fix clone here once update doesn't consume self.
        for mut u in self.google_users.clone() {
            // Get the shorthand username and match it against our existing users.
            let username = u
                .primary_email
                .as_deref()
                .unwrap()
                .trim_end_matches(&format!("@{}", self.domain))
                .replace(".", "-");

            // Check if we have that user already in our settings.
            let user: UserConfig;
            match self.config.users.get(&username) {
                // TODO(cbiffle): fix this, too, once update doesn't consume
                // self.
                Some(val) => user = val.clone(),
                // Continue through the loop and we will add the user later.
                None => continue,
            }

            // Update the user with the settings from the config for the user.
            u = u.update(&user, &self.domain, false).await;

            self.gsuite.update_user(&u).await;

            self.update_user_aliases(&u).await;

            // Add the user to their teams and groups.
            self.update_user(&user).await;

            // Remove the user from the config map and continue.
            // This allows us to add all the remaining new user after.
            self.config.users.remove(&username);

            info!("updated user: {}", username);
        }

        // Create any remaining users from the config that we do not have in GSuite.
        // TODO(cbiffle): same same
        for (username, user) in self.config.users.clone() {
            // Create the user.
            let mut u: User = Default::default();

            u = u.update(&user, &self.domain, true).await;

            self.gsuite.create_user(&u).await;

            self.update_user_aliases(&u).await;

            // Add the user to their teams and groups.
            self.update_user(&user).await;

            let github = user.github.as_deref().unwrap_or_else(|| "");
            let password = u.password.as_deref().unwrap_or_else(|| "");

            // Send an email to the new user.
            email_send_new_user(&u, password, github, &self.domain).await;

            info!("created new user: {}", username);
        }
    }

    /// Update a user's aliases in GSuite to match our configuration files.
    pub async fn update_user_aliases(&mut self, u: &User) {
        if let Some(val) = &u.aliases {
            // Update the user's aliases.
            let email = u.primary_email.as_ref().unwrap();
            self.gsuite.update_user_aliases(email, val).await;
            info!("[google] updated user aliases: {}", email);
        }
    }

    /// Update a user in GSuite to match our configuration files.
    pub async fn update_user(&mut self, user: &UserConfig) {
        self.update_user_google_groups(user).await;
    }

    /// Update a user's groups in GSuite to match our configuration files.
    pub async fn update_user_google_groups(&self, user: &UserConfig) {
        let email = format!("{}@{}", user.username, self.domain);
        let groups = if let Some(val) = &user.groups {
            val
        } else {
            // Return early because they have no groups.
            return;
        };

        // Iterate over the groups and add the user as a member to it.
        for g in groups {
            // Make sure the group exists.
            let group: &Group;
            match self.google_groups.get(g) {
                Some(val) => group = val,
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
                .group_has_member(group.id.as_ref().unwrap(), &email)
                .await;
            if is_member {
                // They are a member so we can just update their member status.
                self.gsuite
                    .group_update_member(
                        group.id.as_ref().unwrap(),
                        &email,
                        &role,
                    )
                    .await;

                // Continue through the other groups.
                continue;
            }

            // Add the user to the group.
            self.gsuite
                .group_insert_member(group.id.as_ref().unwrap(), &email, &role)
                .await;

            info!(
                "[groups]: added {} to {} as {}",
                email,
                group.name.as_ref().unwrap(),
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
                .group_has_member(group.id.as_ref().unwrap(), &email)
                .await;

            if !is_member {
                // They are not a member so we can continue early.
                continue;
            }

            // They are a member of the group.
            // We need to remove them.
            self.gsuite
                .group_remove_member(group.id.as_ref().unwrap(), &email)
                .await;

            info!(
                "[groups]: removed {} from {}",
                email,
                group.name.as_ref().unwrap()
            );
        }
    }

    /// Update the groups in GSuite to match our configuration files.
    pub async fn update_google_groups(&mut self) {
        for (slug, g) in &self.google_groups {
            // Check if we already have this group in our config.
            let group = if let Some(val) = self.config.groups.get(slug) {
                val
            } else {
                // Continue through the loop and we will add the group later.
                continue;
            };

            // Update the group with the settings from the config for the group.
            let mut updated_group: Group = g.clone();
            updated_group.description = Some(group.description.to_string());

            // Write the group aliases.
            let mut aliases: Vec<String> = Default::default();
            if let Some(val) = &group.aliases {
                for alias in val {
                    aliases.push(format!("{}@{}", alias, self.domain));
                }
            }
            updated_group.aliases = Some(aliases);

            self.gsuite.update_group(&updated_group).await;

            self.update_group_aliases(&updated_group).await;

            // Update the groups settings.
            // TODO(cbiffle): uhhh should this be updated_group?
            self.update_google_group_settings(&group).await;

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
            if let Some(val) = &group.aliases {
                for alias in val {
                    aliases.push(format!("{}@{}", alias, self.domain));
                }
            }
            g.aliases = Some(aliases);

            let new_group: Group = self.gsuite.create_group(&g).await;

            self.update_group_aliases(&g).await;

            // Update the groups settings.
            self.update_google_group_settings(&group).await;

            // Add the group to our list of GSuite groups so when we add users to
            // the groups, later in the script, it is there.
            self.google_groups.insert(group.name.to_string(), new_group);

            info!("[groups]: created group {}", slug);
        }
    }

    /// Update a group's aliases in GSuite to match our configuration files.
    pub async fn update_group_aliases(&self, g: &Group) {
        if let Some(val) = &g.aliases {
            // Update the user's aliases.
            let email = g.email.as_ref().unwrap();
            self.gsuite.update_group_aliases(email, val).await;
            info!("[google] updated group aliases: {}", email);
        }
    }

    /// Update a group's settings in GSuite to match our configuration files.
    pub async fn update_google_group_settings(&self, group: &GroupConfig) {
        // Get the current group settings.
        let email = format!("{}@{}", group.name, self.domain);
        let mut settings = self.gsuite.get_group_settings(&email).await;

        // Update the groups settings.
        settings.email = Some(email);
        settings.name = Some(group.name.clone());
        settings.description = Some(group.description.clone());
        settings.allow_external_members =
            Some(group.allow_external_members.to_string());
        settings.allow_web_posting = Some(group.allow_web_posting.to_string());
        settings.is_archived = Some(group.is_archived.to_string());
        settings.who_can_discover_group =
            Some(group.who_can_discover_group.clone());
        settings.who_can_join = Some(group.who_can_join.clone());
        settings.who_can_moderate_members =
            Some(group.who_can_moderate_members.clone());
        settings.who_can_post_message =
            Some(group.who_can_post_message.clone());
        settings.who_can_view_group = Some(group.who_can_view_group.clone());
        settings.who_can_view_membership =
            Some(group.who_can_view_membership.clone());
        settings.who_can_contact_owner =
            Some("ALL_IN_DOMAIN_CAN_CONTACT".to_string());

        // Update the group with the given settings.
        // TODO(cbiffle): we did all that cloning and we're going to use this
        // structure once and toss it? Revisit.
        self.gsuite.update_group_settings(&settings).await;

        info!("[groups]: updated groups settings {}", group.name);
    }
}

async fn email_send_new_user(
    user: &User,
    password: &str,
    github: &str,
    domain: &str,
) {
    // Initialize the SendGrid client.
    let sendgrid = SendGrid::new_from_env();

    // Create the message.
    let message = user_email_message(user, password, github, domain);

    // Send the message.
    sendgrid
        .send_mail(
            format!(
                "Your New Email Account: {}",
                user.primary_email.as_ref().unwrap()
            ),
            message,
            vec![user.recovery_email.clone().unwrap()],
            vec![
                user.primary_email.clone().unwrap(),
                format!("jess@{}", domain),
            ],
            vec![],
            format!("admin@{}", domain),
        )
        .await;
}

fn user_email_message(
    user: &User,
    password: &str,
    github: &str,
    domain: &str,
) -> String {
    // Get the user's aliases if they have one.
    let aliases = user.aliases.as_deref().unwrap_or(&[]).join(", ");

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
                        user.name.as_ref().unwrap().given_name.as_ref().unwrap(),
                        domain,
                        domain,
                        user.primary_email.as_ref().unwrap(),
                        password,
                        aliases,
                        github,
                        "oxidecomputer",
                        "oxidecomputer",
                        user.primary_email.as_ref().unwrap(),
                        github,
                        domain
                    );
}
