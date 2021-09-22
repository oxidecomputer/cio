use anyhow::{bail, Result};
use async_trait::async_trait;
use log::info;

use crate::{
    companies::Company,
    configs::{Group, User},
    db::Database,
};

/// This trait defines how to implement a provider for a vendor that manages users
/// and groups.
#[async_trait]
pub trait ProviderOps<U, G> {
    /// Ensure the user exists and has the correct information.
    async fn ensure_user(&self, db: &Database, company: &Company, user: &User) -> Result<String>;

    /// Ensure the group exists and has the correct information.
    async fn ensure_group(&self, company: &Company, group: &Group) -> Result<()>;

    async fn check_user_is_member_of_group(&self, company: &Company, user: &User, group: &str) -> Result<bool>;

    async fn add_user_to_group(&self, company: &Company, user: &User, group: &str) -> Result<()>;

    async fn remove_user_from_group(&self, company: &Company, user: &User, group: &str) -> Result<()>;

    async fn list_provider_users(&self, company: &Company) -> Result<Vec<U>>;

    async fn list_provider_groups(&self, company: &Company) -> Result<Vec<G>>;

    async fn delete_user(&self, company: &Company, user: &User) -> Result<()>;

    async fn delete_group(&self, company: &Company, group: &Group) -> Result<()>;
}

#[async_trait]
impl ProviderOps<ramp_api::types::User, ()> for ramp_api::Client {
    async fn ensure_user(&self, db: &Database, _company: &Company, user: &User) -> Result<String> {
        // TODO: this is wasteful find another way to do this.
        let departments = self.departments().get_all().await?;

        // Invite the new ramp user.
        let mut ramp_user = ramp_api::types::PostUsersDeferredRequest {
            email: user.email.to_string(),
            first_name: user.first_name.to_string(),
            last_name: user.last_name.to_string(),
            phone: user.recovery_phone.to_string(),
            role: ramp_api::types::Role::BusinessUser,
            // Add the manager.
            direct_manager_id: user.manager(db).ramp_id,
            department_id: String::new(),
            location_id: String::new(),
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

        // TODO(should we?): Create them a card.

        Ok(r.id)
    }

    // Ramp does not have groups so this is a no-op.
    async fn ensure_group(&self, _company: &Company, _group: &Group) -> Result<()> {
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

    async fn delete_user(&self, _company: &Company, _user: &User) -> Result<()> {
        // TODO: Suspend the user from Ramp.
        Ok(())
    }

    // Ramp does not have groups so this is a no-op.
    async fn delete_group(&self, _company: &Company, _group: &Group) -> Result<()> {
        Ok(())
    }
}

#[async_trait]
impl ProviderOps<octorust::types::SimpleUser, octorust::types::Team> for octorust::Client {
    async fn ensure_user(&self, _db: &Database, company: &Company, user: &User) -> Result<String> {
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
        match self
            .orgs()
            .get_membership_for_user(&company.github_org, &user.github)
            .await
        {
            Ok(membership) => {
                if membership.role.to_string() == role.to_string() {
                    info!(
                        "user `{}` is already a member of the github org `{}` with role `{}`",
                        user.github, company.github_org, role
                    );
                    // We can return early, they have the right perms.
                    return Ok(String::new());
                }
            }
            Err(e) => {
                // If the error is Not Found we need to add them.
                if !e.to_string().contains("404") {
                    // Otherwise bail.
                    bail!(
                        "checking if user `{}` is a member of the github org `{}` failed: {}",
                        user.github,
                        company.github_org,
                        e
                    );
                }
            }
        }

        // We need to add the user to the org or update their role, do it now.
        self.orgs()
            .set_membership_for_user(
                &company.github_org,
                &user.github,
                &octorust::types::OrgsSetMembershipUserRequest {
                    role: Some(role.clone()),
                },
            )
            .await?;

        info!(
            "updated user `{}` as a member of the github org `{}` with role `{}`",
            user.github, company.github_org, role
        );

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

    async fn ensure_group(&self, company: &Company, group: &Group) -> Result<()> {
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

    async fn delete_user(&self, company: &Company, user: &User) -> Result<()> {
        if user.github.is_empty() {
            // Return early.
            return Ok(());
        }

        // Delete the user from the GitHub org.
        // Removing a user from this list will remove them from all teams and
        // they will no longer have any access to the organization’s repositories.
        self.orgs().remove_member(&company.github_org, &user.github).await?;

        info!(
            "deleted user `{}` from github org `{}`",
            user.github, company.github_org
        );

        Ok(())
    }

    async fn delete_group(&self, company: &Company, group: &Group) -> Result<()> {
        self.teams().delete_in_org(&company.github_org, &group.name).await?;

        info!("deleted group `{}` in github org `{}`", group.name, company.github_org);

        Ok(())
    }
}

/*
 *
 * Keep as empty boiler plate for now.

#[async_trait]
impl ProviderOps<ramp_api::types::User, ()> for ramp_api::Client {
    async fn ensure_user(&self, company: &Company, user: &User) -> Result<()> {
        Ok(())
    }

    async fn ensure_group(&self, company: &Company, group: &Group) -> Result<()> {
        Ok(())
    }

    async fn check_user_is_member_of_group(&self, company: &Company, user: &User, group: &str) -> Result<bool> {
        Ok(false)
    }

    async fn add_user_to_group(&self, company: &Company, user: &User, group: &str) -> Result<()> {
        Ok(())
    }

    async fn remove_user_from_group(&self, company: &Company, user: &User, group: &str) -> Result<()> {
        Ok(())
    }

    async fn list_provider_users(&self, company: &Company) -> Result<Vec<ramp_api::types::User>> {
        Ok(vec![])
    }

    async fn list_provider_groups(&self, company: &Company) -> Result<Vec<()>> {
        Ok(vec![])
    }

    async fn delete_user(&self, company: &Company, user: &User) -> Result<()> {
        Ok(())
    }

    async fn delete_group(&self, company: &Company, group: &Group) -> Result<()> {
        Ok(())
    }
}

*/
