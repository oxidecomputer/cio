use anyhow::{bail, Result};
use async_trait::async_trait;
use log::{info, warn};

use crate::{
    companies::Company,
    configs::{ExternalServices, Group, User},
    db::Database,
};

/// This trait defines how to implement a provider for a vendor that manages users
/// and groups.
#[async_trait]
pub trait ProviderWriteOps {
    /// Ensure the user exists and has the correct information.
    async fn ensure_user(&self, db: &Database, company: &Company, user: &User) -> Result<String>;

    /// Ensure the group exists and has the correct information.
    async fn ensure_group(&self, db: &Database, company: &Company, group: &Group) -> Result<()>;

    async fn check_user_is_member_of_group(&self, company: &Company, user: &User, group: &str) -> Result<bool>;

    async fn add_user_to_group(&self, company: &Company, user: &User, group: &str) -> Result<()>;

    async fn remove_user_from_group(&self, company: &Company, user: &User, group: &str) -> Result<()>;

    async fn delete_user(&self, db: &Database, company: &Company, user: &User) -> Result<()>;

    async fn delete_group(&self, company: &Company, group: &Group) -> Result<()>;
}

#[async_trait]
pub trait ProviderReadOps {
    type ProviderUser;
    type ProviderGroup;

    async fn list_provider_users(&self, company: &Company) -> Result<Vec<Self::ProviderUser>>;
    async fn list_provider_groups(&self, company: &Company) -> Result<Vec<Self::ProviderGroup>>;
}

#[async_trait]
impl ProviderWriteOps for ramp_api::Client {
    async fn ensure_user(&self, db: &Database, _company: &Company, user: &User) -> Result<String> {
        if user.denied_services.contains(&ExternalServices::Ramp) {
            log::info!(
                "User {} is denied access to {}. Exiting provisioning.",
                user.id,
                ExternalServices::Ramp
            );

            return Ok(String::new());
        }

        // Only do this if the user is full time.
        if !user.is_full_time() {
            return Ok(String::new());
        }

        // Only do this if we have a phone number for the user.
        if user.recovery_phone.is_empty() {
            return Ok(String::new());
        }

        // TODO: this is wasteful find another way to do this.
        let departments = self.departments().get_all().await?;
        // TODO: we need to create the department if it doesn't exist.

        if !user.ramp_id.is_empty() {
            // Get the existing ramp user.
            let ramp_user = self.users().get(&user.ramp_id).await?;

            // Update the user with their department and manager if
            // it has changed.

            // Set the department.
            // TODO: this loop is wasteful.
            let mut department_id = "".to_string();
            for dept in departments {
                if dept.name == user.department {
                    department_id = dept.id;
                    break;
                }
            }

            let manager = user.manager(db).await;
            let manager_ramp_id = if manager.id == user.id {
                "".to_string()
            } else {
                manager.ramp_id.to_string()
            };

            let mut location_id = "".to_string();
            if !ramp_user.location_id.is_empty() {
                location_id = ramp_user.location_id.to_string();
            }

            let role = if ramp_user.role == ramp_api::types::Role::BusinessOwner {
                None
            } else {
                Some(ramp_user.role.clone())
            };

            // Admins and Owners should not have a manager.
            let manager_ramp_id = if ramp_user.role == ramp_api::types::Role::BusinessOwner
                || ramp_user.role == ramp_api::types::Role::BusinessAdmin
            {
                "".to_string()
            } else {
                manager_ramp_id
            };

            let updated_user = ramp_api::types::PatchUsersRequest {
                department_id,
                direct_manager_id: manager_ramp_id,
                role,
                location_id,
            };

            self.users().patch(&user.ramp_id, &updated_user).await?;

            info!("updated ramp user `{}`", user.email);

            // Return early.
            return Ok(user.ramp_id.to_string());
        }

        // Invite the new ramp user.
        let mut ramp_user = ramp_api::types::PostUsersDeferredRequest {
            email: user.email.to_string(),
            first_name: user.first_name.to_string(),
            last_name: user.last_name.to_string(),
            phone: user.recovery_phone.to_string(),
            role: ramp_api::types::Role::BusinessUser,
            // Add the manager.
            direct_manager_id: user.manager(db).await.ramp_id,
            department_id: "".to_string(),
            location_id: "".to_string(),
        };

        // Set the department.
        // TODO: this loop is wasteful.
        for dept in departments {
            if dept.name == user.department {
                ramp_user.department_id = dept.id;
                break;
            }
        }

        // TODO: If the department for the user is not empty but we don't
        // have a Ramp department, create it.

        // Add the manager.
        let r = self.users().post_deferred(&ramp_user).await?;

        info!("created new ramp user `{}`", user.email);

        // TODO(should we?): Create them a card.

        Ok(r.id)
    }

    // Ramp does not have groups so this is a no-op.
    async fn ensure_group(&self, _db: &Database, _company: &Company, _group: &Group) -> Result<()> {
        Ok(())
    }

    // Ramp does not have groups so this is a no-op.
    async fn check_user_is_member_of_group(&self, _company: &Company, _user: &User, _group: &str) -> Result<bool> {
        Ok(false)
    }

    // Ramp does not have groups so this is a no-op.
    async fn add_user_to_group(&self, _company: &Company, _user: &User, _group: &str) -> Result<()> {
        Ok(())
    }

    // Ramp does not have groups so this is a no-op.
    async fn remove_user_from_group(&self, _company: &Company, _user: &User, _group: &str) -> Result<()> {
        Ok(())
    }

    async fn delete_user(&self, _db: &Database, _company: &Company, _user: &User) -> Result<()> {
        log::info!("Skipping Ramp user deletion as access is controlled via GSuite account. Ramp account is left in tact for auditing.");
        Ok(())
    }

    // Ramp does not have groups so this is a no-op.
    async fn delete_group(&self, _company: &Company, _group: &Group) -> Result<()> {
        Ok(())
    }
}

#[async_trait]
impl ProviderReadOps for ramp_api::Client {
    type ProviderUser = ramp_api::types::User;
    type ProviderGroup = ();

    async fn list_provider_users(&self, _company: &Company) -> Result<Vec<ramp_api::types::User>> {
        self.users()
            .get_all(
                "", // department id
                "", // location id
            )
            .await
    }

    // Ramp does not have groups so this is a no-op.
    async fn list_provider_groups(&self, _company: &Company) -> Result<Vec<()>> {
        Ok(vec![])
    }
}

#[async_trait]
impl ProviderWriteOps for octorust::Client {
    async fn ensure_user(&self, _db: &Database, company: &Company, user: &User) -> Result<String> {
        if user.denied_services.contains(&ExternalServices::GitHub) {
            log::info!(
                "User {} is denied access to {}. Exiting provisioning.",
                user.id,
                ExternalServices::GitHub
            );

            return Ok(String::new());
        }

        if user.github.is_empty() {
            // Return early, this user doesn't have a github handle.
            return Ok(String::new());
        }

        let role = if user.is_group_admin {
            octorust::types::OrgsSetMembershipUserRequestRole::Admin
        } else {
            octorust::types::OrgsSetMembershipUserRequestRole::Member
        };

        // Check if the user is already a member of the org.
        let user_exists = match self
            .orgs()
            .get_membership_for_user(&company.github_org, &user.github)
            .await
        {
            Ok(membership) => {
                if membership.role.to_string() == role.to_string() {
                    info!(
                        "user `{}` is already a member of the github org `{}` with role `{}`",
                        user.id, company.github_org, role
                    );

                    true
                } else {
                    false
                }
            }
            Err(e) => {
                // If the error is Not Found we need to add them.
                if !e.to_string().contains("404") {
                    // Otherwise bail.
                    bail!(
                        "checking if user `{}` is a member of the github org `{}` failed: {}",
                        user.id,
                        company.github_org,
                        e
                    );
                }

                false
            }
        };

        if !user_exists {
            // We need to add the user to the org or update their role, do it now.
            if let Err(err) = self
                .orgs()
                .set_membership_for_user(
                    &company.github_org,
                    &user.github,
                    &octorust::types::OrgsSetMembershipUserRequest {
                        role: Some(role.clone()),
                    },
                )
                .await
            {
                warn!(
                    "Failed to add user / update role {} @ {} on {} : {}",
                    user.id, role, company.github_org, err
                );
                return Err(err);
            };

            info!(
                "updated user `{}` as a member of the github org `{}` with role `{}`",
                user.id, company.github_org, role
            );
        }

        // Now we need to ensure our user is a member of all the correct groups.
        for group in &user.groups {
            let is_member = self.check_user_is_member_of_group(company, user, group).await?;

            if !is_member {
                // We need to add the user to the team or update their role, do it now.
                self.add_user_to_group(company, user, group).await?;
            }
        }

        // Get all the GitHub teams.
        let gh_teams = self.list_provider_groups(company).await?;

        // Iterate over all the teams and if the user is a member and should not
        // be, remove them from the team.
        for team in &gh_teams {
            if user.groups.contains(&team.slug) {
                // They should be in the team, continue.
                continue;
            }

            // Now we have a github team. The user should not be a member of it,
            // but we need to make sure they are not a member.
            let is_member = self.check_user_is_member_of_group(company, user, &team.slug).await?;

            // They are a member of the team.
            // We need to remove them.
            if is_member {
                self.remove_user_from_group(company, user, &team.slug).await?;
            }
        }

        // We don't need to store the user id, so just return an empty string here.
        Ok(String::new())
    }

    async fn ensure_group(&self, _db: &Database, company: &Company, group: &Group) -> Result<()> {
        // Check if the team exists.
        match self.teams().get_by_name(&company.github_org, &group.name).await {
            Ok(team) => {
                let parent_team_id = if let Some(parent) = team.parent { parent.id } else { 0 };

                self.teams()
                    .update_in_org(
                        &company.github_org,
                        &group.name,
                        &octorust::types::TeamsUpdateInOrgRequest {
                            name: group.name.to_string(),
                            description: group.description.to_string(),
                            parent_team_id,
                            permission: None, // This is depreciated, so just pass none.
                            privacy: Some(octorust::types::Privacy::Closed),
                        },
                    )
                    .await?;

                info!("updated group `{}` in github org `{}`", group.name, company.github_org);

                // Return early here.
                return Ok(());
            }
            Err(e) => {
                // If the error is Not Found we need to add the team.
                if !e.to_string().contains("404") {
                    // Otherwise bail.
                    bail!(
                        "checking if team `{}` exists in github org `{}` failed: {}",
                        group.name,
                        company.github_org,
                        e
                    );
                }
            }
        }

        // Create the team.
        let team = octorust::types::TeamsCreateRequest {
            name: group.name.to_string(),
            description: group.description.to_string(),
            maintainers: Default::default(),
            privacy: Some(octorust::types::Privacy::Closed),
            permission: None, // This is depreciated, so just pass none.
            parent_team_id: 0,
            repo_names: group.repos.clone(),
        };

        self.teams().create(&company.github_org, &team).await?;

        info!("created group `{}` in github org `{}`", group.name, company.github_org);

        Ok(())
    }

    async fn check_user_is_member_of_group(&self, company: &Company, user: &User, group: &str) -> Result<bool> {
        if user.github.is_empty() {
            // Return early.
            return Ok(false);
        }

        let role = if user.is_group_admin {
            octorust::types::TeamMembershipRole::Maintainer
        } else {
            octorust::types::TeamMembershipRole::Member
        };

        match self
            .teams()
            .get_membership_for_user_in_org(&company.github_org, group, &user.github)
            .await
        {
            Ok(membership) => {
                if membership.role == role {
                    // We can return early, they have the right perms.
                    info!(
                        "user `{}` is already a member of the github team `{}` with role `{}`",
                        user.github, group, role
                    );
                    return Ok(true);
                }
            }
            Err(e) => {
                // If the error is Not Found we need to add them.
                if !e.to_string().contains("404") {
                    // Otherwise bail.
                    bail!(
                        "checking if user `{}` is a member of the github team `{}` failed: {}",
                        user.github,
                        group,
                        e
                    );
                }
            }
        }

        Ok(false)
    }

    async fn add_user_to_group(&self, company: &Company, user: &User, group: &str) -> Result<()> {
        if user.github.is_empty() {
            // User does not have a github handle, return early.
            return Ok(());
        }

        let role = if user.is_group_admin {
            octorust::types::TeamMembershipRole::Maintainer
        } else {
            octorust::types::TeamMembershipRole::Member
        };

        // We need to add the user to the team or update their role, do it now.
        self.teams()
            .add_or_update_membership_for_user_in_org(
                &company.github_org,
                group,
                &user.github,
                &octorust::types::TeamsAddUpdateMembershipUserInOrgRequest {
                    role: Some(role.clone()),
                },
            )
            .await?;

        info!(
            "updated user `{}` as a member of the github team `{}` with role `{}`",
            user.github, group, role
        );

        Ok(())
    }

    async fn remove_user_from_group(&self, company: &Company, user: &User, group: &str) -> Result<()> {
        if user.github.is_empty() {
            // User does not have a github handle, return early.
            return Ok(());
        }

        self.teams()
            .remove_membership_for_user_in_org(&company.github_org, group, &user.github)
            .await?;

        info!("removed `{}` from github team `{}`", user.github, group);

        Ok(())
    }

    async fn delete_user(&self, _db: &Database, company: &Company, user: &User) -> Result<()> {
        if user.github.is_empty() {
            // Return early.
            return Ok(());
        }

        // Delete the user from the GitHub org.
        // Removing a user from this list will remove them from all teams and
        // they will no longer have any access to the organization’s repositories.
        self.orgs()
            .remove_member(&company.github_org, &user.github)
            .await
            .map(|_| {
                info!(
                    "deleted user `{}` from github org `{}`",
                    user.github, company.github_org
                )
            })
            .or_else(|err| {

                // If the error from GitHub is a 404 NotFound then the user does not exist in our
                // organization. This may be an attempt to remove a partially provisioned or
                // deprovisioned user. This is not considered a failure.

                // Errors from the GitHub client are anyhow::Error and we do not know what the
                // underlying error actually is. As such the best we can do is to try and parse the
                // string representation of the error. This is extremely brittle, and requires rework
                // of the public API of octorust to resolve.
                let msg = format!("{}", err);

                if !msg.starts_with("code: 404 Not Found") {
                    warn!("Failed to delete user {} from GitHub. err: {}", user.id, msg);
                    Err(err)
                } else {
                    info!("Ignoring error for GitHub user {} delete", user.id);
                    Ok(())
                }
            })
    }

    async fn delete_group(&self, company: &Company, group: &Group) -> Result<()> {
        self.teams().delete_in_org(&company.github_org, &group.name).await?;

        info!("deleted group `{}` in github org `{}`", group.name, company.github_org);

        Ok(())
    }
}

#[async_trait]
impl ProviderReadOps for octorust::Client {
    type ProviderUser = octorust::types::SimpleUser;
    type ProviderGroup = octorust::types::Team;

    async fn list_provider_users(&self, company: &Company) -> Result<Vec<octorust::types::SimpleUser>> {
        // List all the users in the GitHub organization.
        self.orgs()
            .list_all_members(
                &company.github_org,
                octorust::types::OrgsListMembersFilter::All,
                octorust::types::OrgsListMembersRole::All,
            )
            .await
    }

    async fn list_provider_groups(&self, company: &Company) -> Result<Vec<octorust::types::Team>> {
        // List all the teams in the GitHub organization.
        self.teams().list_all(&company.github_org).await
    }
}

#[async_trait]
impl ProviderWriteOps for gsuite_api::Client {
    async fn ensure_user(&self, db: &Database, company: &Company, user: &User) -> Result<String> {
        if user.denied_services.contains(&ExternalServices::Google) {
            log::info!(
                "User {} is denied access to {}. Exiting provisioning.",
                user.id,
                ExternalServices::Google
            );

            return Ok(String::new());
        }

        // First get the user from gsuite.
        match self
            .users()
            .get(
                &user.email,
                gsuite_api::types::DirectoryUsersListProjection::Full,
                gsuite_api::types::ViewType::AdminView,
            )
            .await
        {
            Ok(u) => {
                // Update the user with the settings from the config for the user.
                let gsuite_user = crate::gsuite::update_gsuite_user(&u, user, false, company).await;

                match self.users().update(&gsuite_user.id, &gsuite_user).await {
                    Ok(_) => {}
                    Err(e) => {
                        bail!(
                            "failed to update user `{}` in gsuite: {}\n{:?}",
                            user.email,
                            e,
                            gsuite_user
                        );
                    }
                };

                crate::gsuite::update_user_aliases(self, &gsuite_user, user.aliases.clone(), company).await?;

                // Add the user to their teams and groups.
                crate::gsuite::update_user_google_groups(self, user, company).await?;

                info!("updated user `{}` in GSuite", user.email);

                // Return the ID.
                return Ok(gsuite_user.id);
            }
            Err(e) => {
                // If the error is Not Found we need to add them.
                if !e.to_string().contains("404") {
                    // Otherwise bail.
                    bail!("checking if user `{}` exists in GSuite failed: {}", user.email, e);
                }
            }
        }

        // Create the user.
        let u: gsuite_api::types::User = Default::default();

        // The last argument here tell us to create a password!
        // Make sure it is set to true.
        let gsuite_user = crate::gsuite::update_gsuite_user(&u, user, true, company).await;

        let new_gsuite_user = self.users().insert(&gsuite_user).await?;

        // Send an email to the new user.
        // Do this here in case another step fails.
        user.send_email_new_gsuite_user(db, &gsuite_user.password).await?;

        crate::gsuite::update_user_aliases(self, &gsuite_user, user.aliases.clone(), company).await?;

        crate::gsuite::update_user_google_groups(self, user, company).await?;

        info!("created user `{}` in GSuite", user.email);

        Ok(new_gsuite_user.id)
    }

    async fn ensure_group(&self, db: &Database, company: &Company, group: &Group) -> Result<()> {
        match self
            .groups()
            .get(&format!("{}@{}", &group.name, &company.gsuite_domain))
            .await
        {
            Ok(mut google_group) => {
                google_group.description = group.description.to_string();

                // Write the group aliases.
                let mut aliases: Vec<String> = Default::default();
                for alias in &group.aliases {
                    aliases.push(format!("{}@{}", alias, &company.gsuite_domain));
                }
                google_group.aliases = aliases;

                self.groups()
                    .update(&format!("{}@{}", group.name, company.gsuite_domain), &google_group)
                    .await?;

                crate::gsuite::update_group_aliases(self, &google_group).await?;

                // Update the groups settings.
                crate::gsuite::update_google_group_settings(db, group, company).await?;

                info!("updated group `{}` in GSuite", group.name);

                // Return early.
                return Ok(());
            }
            Err(e) => {
                // If the error is Not Found we need to add them.
                if !e.to_string().contains("404") {
                    // Otherwise bail.
                    bail!("checking if group `{}` exists in GSuite failed: {}", group.name, e);
                }
            }
        }

        // Create the group.
        let mut g: gsuite_api::types::Group = Default::default();

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

        let new_group = self.groups().insert(&g).await?;

        crate::gsuite::update_group_aliases(self, &new_group).await?;

        // Update the groups settings.
        crate::gsuite::update_google_group_settings(db, group, company).await?;

        info!("created group `{}` in GSuite", group.name);

        Ok(())
    }

    async fn check_user_is_member_of_group(&self, company: &Company, user: &User, group: &str) -> Result<bool> {
        let role = if user.is_group_admin {
            "OWNER".to_string()
        } else {
            "MEMBER".to_string()
        };

        match self
            .members()
            .get(&format!("{}@{}", group, company.gsuite_domain), &user.email)
            .await
        {
            Ok(member) => {
                if member.role == role {
                    // They have the right permissions.
                    info!(
                        "user `{}` is already a member of the GSuite group `{}` with role `{}`",
                        user.email, group, role
                    );
                    return Ok(true);
                }
            }
            Err(e) => {
                if !e.to_string().contains("404") {
                    // Otherwise bail.
                    bail!(
                        "checking if user `{}` is a member of the GSuite group `{}` failed: {}",
                        user.email,
                        group,
                        e
                    );
                }
            }
        }

        Ok(false)
    }

    async fn add_user_to_group(&self, company: &Company, user: &User, group: &str) -> Result<()> {
        let role = if user.is_group_admin {
            "OWNER".to_string()
        } else {
            "MEMBER".to_string()
        };

        let is_member = self.check_user_is_member_of_group(company, user, group).await?;
        if !is_member {
            // Create the member of the group.
            if let Err(e) = self
                .members()
                .insert(
                    &format!("{}@{}", group, company.gsuite_domain),
                    &gsuite_api::types::Member {
                        role: role.to_string(),
                        email: user.email.to_string(),
                        delivery_settings: "ALL_MAIL".to_string(),
                        etag: "".to_string(),
                        id: "".to_string(),
                        kind: "".to_string(),
                        status: "".to_string(),
                        type_: "".to_string(),
                    },
                )
                .await
            {
                if e.to_string().contains("Member already exists") {
                    // We can ignore this error.
                    // Update their role instead.
                    self.members()
                        .update(
                            &format!("{}@{}", group, company.gsuite_domain),
                            &user.email,
                            &gsuite_api::types::Member {
                                role: role.to_string(),
                                email: user.email.to_string(),
                                delivery_settings: "ALL_MAIL".to_string(),
                                etag: "".to_string(),
                                id: "".to_string(),
                                kind: "".to_string(),
                                status: "".to_string(),
                                type_: "".to_string(),
                            },
                        )
                        .await?;
                } else {
                    bail!(
                        "adding user `{}` to group `{}` with role `{}` failed: {}",
                        user.email,
                        group,
                        role,
                        e
                    );
                }
            }

            info!(
                "created user `{}` membership to GSuite group `{}` with role `{}`",
                user.email, group, role
            );
        }

        Ok(())
    }

    async fn remove_user_from_group(&self, company: &Company, user: &User, group: &str) -> Result<()> {
        self.members()
            .delete(&format!("{}@{}", group, company.gsuite_domain), &user.email)
            .await?;

        info!("removed user `{}` from GSuite group `{}`", user.email, group);
        Ok(())
    }

    async fn delete_user(&self, _db: &Database, _company: &Company, user: &User) -> Result<()> {
        // First get the user from gsuite.
        let mut gsuite_user = self
            .users()
            .get(
                &user.email,
                gsuite_api::types::DirectoryUsersListProjection::Full,
                gsuite_api::types::ViewType::AdminView,
            )
            .await?;

        // Set them to be suspended.
        gsuite_user.suspended = true;
        gsuite_user.suspension_reason = "No longer in config file.".to_string();

        // Update the user.
        self.users().update(&user.email, &gsuite_user).await?;

        info!("suspended user `{}` from gsuite", user.email);

        Ok(())
    }

    async fn delete_group(&self, company: &Company, group: &Group) -> Result<()> {
        self.groups()
            .delete(&format!("{}@{}", &group.name, &company.gsuite_domain))
            .await?;

        info!("deleted group `{}` from gsuite", group.name);

        Ok(())
    }
}

#[async_trait]
impl ProviderReadOps for gsuite_api::Client {
    type ProviderUser = gsuite_api::types::User;
    type ProviderGroup = gsuite_api::types::Group;

    async fn list_provider_users(&self, company: &Company) -> Result<Vec<gsuite_api::types::User>> {
        self.users()
            .list_all(
                &company.gsuite_account_id,
                &company.gsuite_domain,
                gsuite_api::types::Event::Noop,
                gsuite_api::types::DirectoryUsersListOrderBy::Email,
                gsuite_api::types::DirectoryUsersListProjection::Full,
                "", // query
                "", // show deleted
                gsuite_api::types::SortOrder::Ascending,
                gsuite_api::types::ViewType::AdminView,
            )
            .await
    }

    async fn list_provider_groups(&self, company: &Company) -> Result<Vec<gsuite_api::types::Group>> {
        self.groups()
            .list_all(
                &company.gsuite_account_id,
                &company.gsuite_domain,
                gsuite_api::types::DirectoryGroupsListOrderBy::Email,
                "", // query
                gsuite_api::types::SortOrder::Ascending,
                "", // user_key
            )
            .await
    }
}

#[async_trait]
impl ProviderWriteOps for okta::Client {
    async fn ensure_user(&self, db: &Database, company: &Company, user: &User) -> Result<String> {
        if user.denied_services.contains(&ExternalServices::Okta) {
            log::info!(
                "User {} is denied access to {}. Exiting provisioning.",
                user.id,
                ExternalServices::Google
            );

            return Ok(String::new());
        }

        let mut user = user.clone();

        let mut aliases: Vec<String> = Default::default();
        for alias in &user.aliases {
            aliases.push(format!("{}@{}", alias, company.gsuite_domain));
        }

        // Create the profile for the Okta user.
        let profile = okta::types::UserProfile {
            city: user.home_address_city.to_string(),
            cost_center: Default::default(),
            country_code: user.home_address_country_code.to_string(),
            department: user.department.to_string(),
            display_name: user.full_name(),
            division: Default::default(),
            email: user.email.to_string(),
            employee_number: Default::default(),
            first_name: user.first_name.to_string(),
            honorific_prefix: Default::default(),
            honorific_suffix: Default::default(),
            last_name: user.last_name.to_string(),
            locale: Default::default(),
            login: user.email.to_string(),
            manager: user.manager(db).await.email,
            manager_id: Default::default(),
            middle_name: Default::default(),
            mobile_phone: user.recovery_phone.to_string(),
            nick_name: Default::default(),
            organization: company.name.to_string(),
            postal_address: user.home_address_formatted.to_string(),
            preferred_language: Default::default(),
            primary_phone: user.recovery_phone.to_string(),
            profile_url: Default::default(),
            second_email: user.recovery_email.to_string(),
            state: user.home_address_state.to_string(),
            street_address: format!("{}\n{}", user.home_address_street_1, user.home_address_street_2)
                .trim()
                .to_string(),
            timezone: Default::default(),
            title: Default::default(),
            user_type: Default::default(),
            zip_code: user.home_address_zipcode.to_string(),
            github_username: user.github.to_string(),
            matrix_username: user.chat.to_string(),
            aws_role: user.aws_role.to_string(),
            start_date: Some(user.start_date),
            birthday: Some(user.birthday),
            email_aliases: aliases,
            work_postal_address: user.work_address_formatted.to_string(),
            work_street_address: format!("{}\n{}", user.work_address_street_1, user.work_address_street_2)
                .trim()
                .to_string(),
            work_city: user.work_address_city.to_string(),
            work_state: user.work_address_state.to_string(),
            work_zip_code: user.work_address_zipcode.to_string(),
            work_country_code: user.work_address_country_code.to_string(),
        };

        // Try to get the user.
        let mut user_id = match self.users().get(&user.email.replace('@', "%40")).await {
            Ok(mut okta_user) => {
                // Update the Okta user.
                okta_user.profile = Some(profile.clone());
                self.users()
                    .update(
                        &okta_user.id,
                        false, // strict
                        &okta_user,
                    )
                    .await?;

                okta_user.id
            }
            Err(e) => {
                if !e.to_string().contains("404") {
                    // Otherwise bail.
                    bail!("checking if user `{}` exists in Okta failed: {}", user.email, e);
                }

                String::new()
            }
        };

        if user_id.is_empty() {
            // Create the user.
            let okta_user = self
                .users()
                .create(
                    true,  // activate
                    false, // provider
                    "",    // next_login
                    &okta::types::CreateUserRequest {
                        credentials: None,
                        group_ids: Default::default(),
                        profile: Some(profile),
                        type_: None,
                    },
                )
                .await?;

            user_id = okta_user.id;

            // The user did not already exist in Okta.
            // We should send them an email about setting up their account.
            info!("sending email to new Okta user `{}`", user.username);
            if user.is_consultant() {
                user.send_email_new_consultant(db).await?;
            } else {
                user.send_email_new_user(db).await?;
            }
        }

        // Set the okta id so we can perform operations on groups more easily.
        user.okta_id = user_id.to_string();

        // Add the user to their groups.
        for group in &user.groups {
            // Check if the user is a member of the group.
            let is_member = self.check_user_is_member_of_group(company, &user, group).await?;

            if !is_member {
                // Add the user to the group.
                self.add_user_to_group(company, &user, group).await?;
            }
        }

        // Get all the Okta groups.
        let okta_groups = self.list_provider_groups(company).await?;

        // Iterate over all the groups and if the user is a member and should not
        // be, remove them from the group.
        for group in &okta_groups {
            let group_name = group.profile.as_ref().unwrap().name.to_string();
            if user.groups.contains(&group_name) {
                // They should be in the group, continue.
                continue;
            }

            // Now we have an Okta group. The user should not be a member of it,
            // but we need to make sure they are not a member.
            let is_member = self.check_user_is_member_of_group(company, &user, &group_name).await?;

            // They are a member of the team.
            // We need to remove them.
            if is_member {
                self.remove_user_from_group(company, &user, &group_name).await?;
            }
        }

        Ok(user_id)
    }

    async fn ensure_group(&self, _db: &Database, _company: &Company, group: &Group) -> Result<()> {
        if group.name == "Everyone" {
            // Return early we can't modify this group.
            return Ok(());
        }

        // Try to find the group with the name.
        let results = self
            .groups()
            .list_all(
                &group.name, // query
                "",          // search
                "",          // expand
            )
            .await?;

        for mut result in results {
            let mut profile = result.profile.unwrap();
            if profile.name == group.name {
                // We found the group let's update it if we should.
                if profile.description != group.description {
                    // Update the group.
                    profile.description = group.description.to_string();

                    result.profile = Some(profile);

                    self.groups().update(&result.id, &result).await?;

                    info!("updated group `{}` in Okta", group.name);
                } else {
                    info!("existing group `{}` in Okta is up to date", group.name);
                }

                return Ok(());
            }
        }

        // The group did not exist, let's create it.
        self.groups()
            .create(&okta::types::Group {
                embedded: None,
                links: None,
                created: None,
                id: String::new(),
                last_membership_updated: None,
                last_updated: None,
                object_class: Default::default(),
                type_: None,
                profile: Some(okta::types::GroupProfile {
                    name: group.name.to_string(),
                    description: group.description.to_string(),
                }),
            })
            .await?;

        info!("created group `{}` in Okta", group.name);

        Ok(())
    }

    async fn check_user_is_member_of_group(&self, _company: &Company, user: &User, group: &str) -> Result<bool> {
        if group == "Everyone" {
            // Return early we can't modify this group.
            return Ok(true);
        }

        // Try to find the group with the name.
        let results = self
            .groups()
            .list_all(
                group, // query
                "",    // search
                "",    // expand
            )
            .await?;

        for result in results {
            let profile = result.profile.unwrap();
            if profile.name == group {
                let members = self.groups().list_all_users(&result.id).await?;
                for member in members {
                    if member.id == user.okta_id {
                        info!("user `{}` is already a member of Okta group `{}`", user.email, group);
                        return Ok(true);
                    }
                }
            }
        }

        Ok(false)
    }

    async fn add_user_to_group(&self, _company: &Company, user: &User, group: &str) -> Result<()> {
        if group == "Everyone" {
            // Return early we can't modify this group.
            return Ok(());
        }

        // Try to find the group with the name.
        let results = self
            .groups()
            .list_all(
                group, // query
                "",    // search
                "",    // expand
            )
            .await?;

        for result in results {
            let profile = result.profile.unwrap();
            if profile.name == group {
                // We found the group let's delete it.
                self.groups().add_user(&result.id, &user.okta_id).await?;

                info!("added user `{}` to Okta group `{}`", user.email, group);

                return Ok(());
            }
        }

        Ok(())
    }

    async fn remove_user_from_group(&self, _company: &Company, user: &User, group: &str) -> Result<()> {
        if group == "Everyone" {
            // Return early we can't modify this group.
            return Ok(());
        }

        // Try to find the group with the name.
        let results = self
            .groups()
            .list_all(
                group, // query
                "",    // search
                "",    // expand
            )
            .await?;

        for result in results {
            let profile = result.profile.unwrap();
            if profile.name == group {
                // We found the group let's delete it.
                self.groups().remove_user_from(&result.id, &user.okta_id).await?;

                info!("removed user `{}` from Okta group `{}`", user.email, group);

                return Ok(());
            }
        }

        Ok(())
    }

    async fn delete_user(&self, _db: &Database, _company: &Company, user: &User) -> Result<()> {
        if user.okta_id.is_empty() {
            // Return early.
            warn!(
                "could not deactivate user `{}` from okta because they don't have an okta_id",
                user.email
            );
            return Ok(());
        }

        // Deactivate the user.
        self.users().deactivate(&user.okta_id, true).await?;
        info!("deactivate user `{}` from Okta", user.email);

        Ok(())
    }

    async fn delete_group(&self, _company: &Company, group: &Group) -> Result<()> {
        if group.name == "Everyone" {
            // Return early we can't modify this group.
            return Ok(());
        }

        // Try to find the group with the name.
        let results = self
            .groups()
            .list_all(
                &group.name, // query
                "",          // search
                "",          // expand
            )
            .await?;

        for result in results {
            let profile = result.profile.unwrap();
            if profile.name == group.name {
                // We found the group let's delete it.
                self.groups().delete(&result.id).await?;
                return Ok(());
            }
        }

        Ok(())
    }
}

#[async_trait]
impl ProviderReadOps for okta::Client {
    type ProviderUser = okta::types::User;
    type ProviderGroup = okta::types::Group;

    async fn list_provider_users(&self, _company: &Company) -> Result<Vec<okta::types::User>> {
        self.users()
            .list_all(
                "", // query
                "", // filter
                "", // search
                "", // sort by
                "", // sort order
            )
            .await
    }

    async fn list_provider_groups(&self, _company: &Company) -> Result<Vec<okta::types::Group>> {
        self.groups()
            .list_all(
                "", // query
                "", // search
                "", // expand
            )
            .await
    }
}

#[async_trait]
impl ProviderWriteOps for zoom_api::Client {
    async fn ensure_user(&self, db: &Database, _company: &Company, user: &User) -> Result<String> {
        if user.denied_services.contains(&ExternalServices::Zoom) {
            log::info!(
                "User {} is denied access to {}. Exiting provisioning.",
                user.id,
                ExternalServices::Zoom
            );

            return Ok(String::new());
        }

        // Only do this if the user is full time.
        if !user.is_full_time() {
            return Ok(String::new());
        }

        if !user.zoom_id.is_empty() {
            // We have a zoom user.

            // Fixup the vanity URL to be their username.
            // We only do this here, since we can't do it until the
            // user has activated their account.
            // Get the user to check their vanity URL as it's not
            // given to us when we list users.
            let zu = self
                .users()
                .user(
                    &user.zoom_id,
                    zoom_api::types::LoginType::Noop, // We don't know their login type...
                    false,
                )
                .await?;

            // Check if the vanity URL is already either the username
            // or the github handle.
            if zu.user_response.vanity_url.is_empty()
                || (!zu
                    .user_response
                    .vanity_url
                    .contains(&format!("/{}?", user.username.to_lowercase()))
                    && !zu
                        .user_response
                        .vanity_url
                        .contains(&format!("/{}?", user.github.to_lowercase()))
                    && !zu.user_response.vanity_url.contains(&format!(
                        "/{}.{}?",
                        user.first_name.to_lowercase(),
                        user.last_name.to_lowercase()
                    )))
            {
                // Update the vanity URL for the user.
                // First try their username.
                // This should succeed _if_ we have a custom domain.
                match user
                    .update_zoom_vanity_name(db, self, &user.zoom_id, &zu, &user.username.to_lowercase())
                    .await
                {
                    Ok(_) => (),
                    Err(e) => {
                        // Try their github username.
                        info!(
                            "updating zoom vanity_url failed for username `{}`, will try with github handle `{}`: {}",
                            user.username.to_lowercase(),
                            user.github,
                            e
                        );

                        if !user.github.is_empty() {
                            match user
                                .update_zoom_vanity_name(db, self, &user.zoom_id, &zu, &user.github.to_lowercase())
                                .await
                            {
                                Ok(_) => (),
                                Err(e) => {
                                    // Try their {first_name}.{last_name}.
                                    info!(
                                        "updating zoom vanity_url failed for github handle `{}`, will try with `{}.{}`: {}",
                                        user.github.to_lowercase(), user.first_name, user.last_name, e
                                    );
                                    // Ignore the error if it does not work.
                                    if let Err(e) = user
                                        .update_zoom_vanity_name(
                                            db,
                                            self,
                                            &user.zoom_id,
                                            &zu,
                                            &format!(
                                                "{}.{}",
                                                user.first_name.to_lowercase(),
                                                user.last_name.to_lowercase()
                                            ),
                                        )
                                        .await
                                    {
                                        info!(
                                            "updating zoom vanity_url failed for `{}.{}`: {}",
                                            user.first_name, user.last_name, e
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }

            info!("updated zoom user `{}`", user.email);

            // Return early.
            return Ok(user.zoom_id.to_string());
        }

        // Only create the user if we don't already have a pending user.
        // We can know this if the zoom_id is empty.
        let zoom_user = self
            .users()
            .create(&zoom_api::types::UserCreateRequest {
                // User will get an email sent from Zoom.
                // There is a confirmation link in this email.
                // The user will then need to use the link to activate their Zoom account.
                // The user can then set or change their password.
                action: zoom_api::types::UserCreateRequestAction::Create,
                user_info: Some(zoom_api::types::UserInfo {
                    email: user.email.to_string(),
                    first_name: user.first_name.to_string(),
                    last_name: user.last_name.to_string(),
                    password: "".to_string(), // Leave blank.
                    // Create a licensed user.
                    type_: 2,
                }),
            })
            .await?;

        info!("created zoom user `{}`", user.email);

        Ok(zoom_user.id)
    }

    async fn ensure_group(&self, _db: &Database, _company: &Company, _group: &Group) -> Result<()> {
        Ok(())
    }

    async fn check_user_is_member_of_group(&self, _company: &Company, _user: &User, _group: &str) -> Result<bool> {
        Ok(false)
    }

    async fn add_user_to_group(&self, _company: &Company, _user: &User, _group: &str) -> Result<()> {
        Ok(())
    }

    async fn remove_user_from_group(&self, _company: &Company, _user: &User, _group: &str) -> Result<()> {
        Ok(())
    }

    async fn delete_user(&self, db: &Database, _company: &Company, user: &User) -> Result<()> {
        if user.zoom_id.is_empty() {
            // Return early.
            return Ok(());
        }

        self.users()
            .delete(
                &user.zoom_id, // ID of the user to delete.
                zoom_api::types::UserDeleteAction::Delete,
                &user.manager(db).await.email, // Email of the user's manager to transfer items to
                true,                          // Tranfer meetings
                true,                          // Transfer webinars
                true,                          // Transfer recordings
            )
            .await?;

        info!("deleted zoom user `{}`", user.email);

        Ok(())
    }

    async fn delete_group(&self, _company: &Company, _group: &Group) -> Result<()> {
        Ok(())
    }
}

#[async_trait]
impl ProviderReadOps for zoom_api::Client {
    type ProviderUser = zoom_api::types::UsersResponse;
    type ProviderGroup = ();

    async fn list_provider_users(&self, _company: &Company) -> Result<Vec<zoom_api::types::UsersResponse>> {
        self.users()
            .get_all(
                zoom_api::types::UsersStatus::Active,
                "", // role id
                zoom_api::types::UsersIncludeFields::HostKey,
            )
            .await
    }

    async fn list_provider_groups(&self, _company: &Company) -> Result<Vec<()>> {
        Ok(vec![])
    }
}

#[async_trait]
impl ProviderWriteOps for airtable_api::Airtable {
    async fn ensure_user(&self, _db: &Database, company: &Company, user: &User) -> Result<String> {
        if user.denied_services.contains(&ExternalServices::Airtable) {
            log::info!(
                "User {} is denied access to {}. Exiting provisioning.",
                user.id,
                ExternalServices::Airtable
            );

            return Ok(String::new());
        }

        if company.airtable_enterprise_account_id.is_empty() {
            // We don't have an enterprise account, we can't perform this function.
            return Ok(String::new());
        }

        // Only do this if the user is full time.
        if !user.is_full_time() {
            return Ok(String::new());
        }

        match self.get_enterprise_user(&user.email).await {
            Ok(airtable_user) => {
                // If we don't have the airtable user added to our workspace,
                // we need to add them.
                let mut has_access_to_workspace = false;
                let mut has_access_to_workspace_read_only = false;
                for collabs in airtable_user.collaborations.workspace_collaborations {
                    if collabs.workspace_id == company.airtable_workspace_id {
                        // We already have given the user the permissions.
                        has_access_to_workspace = true;
                    } else if collabs.workspace_id == company.airtable_workspace_read_only_id {
                        // We already have given the user the permissions, to the read
                        // only workspace.
                        has_access_to_workspace_read_only = true;
                    }
                }

                // Add the user, if we found out they did not already have permissions
                // to the workspace.
                if !has_access_to_workspace {
                    info!(
                        "giving `{}` access to airtable workspace `{}`",
                        user.email, company.airtable_workspace_id
                    );
                    self.add_collaborator_to_workspace(&company.airtable_workspace_id, &airtable_user.id, "create")
                        .await?;
                }
                if !has_access_to_workspace_read_only {
                    info!(
                        "giving `{}` comment access to airtable workspace read only `{}`",
                        user.email, company.airtable_workspace_read_only_id
                    );
                    self.add_collaborator_to_workspace(
                        &company.airtable_workspace_read_only_id,
                        &airtable_user.id,
                        // Giving comment access to the workspace means
                        // that they can create personal views.
                        // https://support.airtable.com/hc/en-us/articles/202887099-Permissions-overview
                        "comment",
                    )
                    .await?;
                }

                return Ok(airtable_user.id);
            }
            Err(e) => {
                if user.is_full_time() {
                    warn!("getting airtable enterprise user for `{}` failed: {}", user.email, e);
                }
            }
        }

        Ok(String::new())
    }

    async fn ensure_group(&self, _db: &Database, _company: &Company, _group: &Group) -> Result<()> {
        Ok(())
    }

    async fn check_user_is_member_of_group(&self, _company: &Company, _user: &User, _group: &str) -> Result<bool> {
        Ok(false)
    }

    async fn add_user_to_group(&self, _company: &Company, _user: &User, _group: &str) -> Result<()> {
        Ok(())
    }

    async fn remove_user_from_group(&self, _company: &Company, _user: &User, _group: &str) -> Result<()> {
        Ok(())
    }

    async fn delete_user(&self, _db: &Database, company: &Company, user: &User) -> Result<()> {
        if company.airtable_enterprise_account_id.is_empty() {
            // We don't have an enterprise account, we can't perform this function.
            return Ok(());
        }

        // If we have an enterprise airtable account, let's delete the user from
        // our Airtable.
        // We don't need a base id here since we are only using the enterprise api features.
        self.delete_internal_user_by_email(&user.email).await.map(|_| {
            info!("Deleted user {} from Airtable", user.id)
        }).or_else(|err| {
            let msg = format!("{:?}", err);

            // If the only error we encounter is that we failed to find an Airtable user to
            // remove then it is likely that we are handling a user that was only partially
            // provisioned or deprovisioned and therefore should not be considered an error.

            // Errors from the Airtable client are anyhow::Error and we do not know what the
            // underlying error actually is. As such the best we can do is to try and parse the
            // string representation of the error. This is extremely brittle, and requires rework
            // of the public API of the Airtable client to resolve. Additionally we can only perform
            // this check at all because we are only trying to delete a single record. In the
            // case of attempting to delete multiple records, this falls apart.
            if !msg.contains("type_: \"NOT_FOUND\"") {
                warn!("Failed to delete user {} from Airtable. err: {}", user.id, err);
                Err(err)
            } else {
                info!("Ignoring error for Airtable user {} delete", user.id);
                Ok(())
            }
        })
    }

    async fn delete_group(&self, _company: &Company, _group: &Group) -> Result<()> {
        Ok(())
    }
}

#[async_trait]
impl ProviderReadOps for airtable_api::Airtable {
    type ProviderUser = ();
    type ProviderGroup = ();

    async fn list_provider_users(&self, _company: &Company) -> Result<Vec<()>> {
        Ok(vec![])
    }

    async fn list_provider_groups(&self, _company: &Company) -> Result<Vec<()>> {
        Ok(vec![])
    }
}

/*
 *
 * Keep as empty boiler plate for now.

#[async_trait]
impl ProviderWriteOps<ramp_api::types::User, ()> for ramp_api::Client {
    async fn ensure_user(&self, _db: &Database, _company: &Company, _user: &User) -> Result<String> {
        Ok(String::new())
    }

    async fn ensure_group(&self, _db: &Database, _company: &Company, _group: &Group) -> Result<()> {
        Ok(())
    }

    async fn check_user_is_member_of_group(&self, _company: &Company, _user: &User, _group: &str) -> Result<bool> {
        Ok(false)
    }

    async fn add_user_to_group(&self, _company: &Company, _user: &User, _group: &str) -> Result<()> {
        Ok(())
    }

    async fn remove_user_from_group(&self, _company: &Company, _user: &User, _group: &str) -> Result<()> {
        Ok(())
    }

    async fn list_provider_users(&self, _company: &Company) -> Result<Vec<ramp_api::types::User>> {
        Ok(vec![])
    }

    async fn list_provider_groups(&self, _company: &Company) -> Result<Vec<()>> {
        Ok(vec![])
    }

    async fn delete_user(&self, _db: &Database, _company: &Company, _user: &User) -> Result<()> {
        Ok(())
    }

    async fn delete_group(&self, _company: &Company, _group: &Group) -> Result<()> {
        Ok(())
    }
}

*/
