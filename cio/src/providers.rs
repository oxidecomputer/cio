use anyhow::{bail, Result};
use async_trait::async_trait;

use crate::{
    companies::Company,
    configs::{Group, User},
};

/// This trait defines how to implement a provider for a vendor that manages users
/// and groups.
#[async_trait]
pub trait ProviderOps<U, G> {
    async fn create_user(&self, company: &Company, user: User) -> Result<()>;

    async fn create_group(&self, company: &Company, group: Group) -> Result<()>;

    async fn check_user_is_member_of_group(&self, company: &Company, user: User, group: &str) -> Result<bool>;

    async fn add_user_to_group(&self, company: &Company, user: User, group: &str) -> Result<()>;

    async fn remove_user_from_group(&self, company: &Company, user: User, group: &str) -> Result<()>;

    async fn list_provider_users(&self, company: &Company) -> Result<Vec<U>>;

    async fn list_provider_groups(&self, company: &Company) -> Result<Vec<G>>;

    async fn delete_user(&self, company: &Company, user: User) -> Result<()>;

    async fn delete_group(&self, company: &Company, group: Group) -> Result<()>;
}

#[async_trait]
impl ProviderOps<octorust::types::SimpleUser, octorust::types::Team> for octorust::Client {
    async fn create_user(&self, company: &Company, user: User) -> Result<()> {
        let role = if user.is_group_admin {
            octorust::types::OrgsSetMembershipUserRequestRole::Admin
        } else {
            octorust::types::OrgsSetMembershipUserRequestRole::Member
        };

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

        Ok(())
    }

    async fn create_group(&self, company: &Company, group: Group) -> Result<()> {
        // Create the team.
        let team = octorust::types::TeamsCreateRequest {
            name: group.name.to_string(),
            description: group.description.to_string(),
            maintainers: Default::default(),
            privacy: Some(octorust::types::Privacy::Closed),
            permission: None, // This is depreciated, so just pass none.
            parent_team_id: 0,
            repo_names: group.repos,
        };

        self.teams().create(&company.github_org, &team).await?;

        Ok(())
    }

    async fn check_user_is_member_of_group(&self, company: &Company, user: User, group: &str) -> Result<bool> {
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

    async fn add_user_to_group(&self, company: &Company, user: User, group: &str) -> Result<()> {
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

        Ok(())
    }

    async fn remove_user_from_group(&self, company: &Company, user: User, group: &str) -> Result<()> {
        self.teams()
            .remove_membership_for_user_in_org(&company.github_org, group, &user.github)
            .await
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

    async fn delete_user(&self, company: &Company, user: User) -> Result<()> {
        // Delete the user from the GitHub org.
        // Removing a user from this list will remove them from all teams and
        // they will no longer have any access to the organization’s repositories.
        self.orgs().remove_member(&company.github_org, &user.github).await
    }

    async fn delete_group(&self, company: &Company, group: Group) -> Result<()> {
        self.teams().delete_in_org(&company.github_org, &group.name).await
    }
}
