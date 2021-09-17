use anyhow::{bail, Result};
use async_trait::async_trait;

use crate::{
    companies::Company,
    configs::{Group, User},
    db::Database,
};

/// This trait defines how to implement a provider for a vendor that manages users
/// and groups.
#[async_trait]
pub trait ProviderOps<U, G> {
    async fn create_user(&self, db: &Database, company: &Company, user: User) -> Result<String>;

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
impl ProviderOps<ramp_api::types::User, ()> for ramp_api::Client {
    async fn create_user(&self, db: &Database, _company: &Company, user: User) -> Result<String> {
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
    async fn create_group(&self, _company: &Company, _group: Group) -> Result<()> {
        Ok(())
    }

    // Ramp does not have groups so this is a no-op.
    async fn check_user_is_member_of_group(&self, _company: &Company, _user: User, _group: &str) -> Result<bool> {
        Ok(false)
    }

    // Ramp does not have groups so this is a no-op.
    async fn add_user_to_group(&self, _company: &Company, _user: User, _group: &str) -> Result<()> {
        Ok(())
    }

    // Ramp does not have groups so this is a no-op.
    async fn remove_user_from_group(&self, _company: &Company, _user: User, _group: &str) -> Result<()> {
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

    async fn delete_user(&self, _company: &Company, _user: User) -> Result<()> {
        // TODO: Suspend the user from Ramp.
        Ok(())
    }

    // Ramp does not have groups so this is a no-op.
    async fn delete_group(&self, _company: &Company, _group: Group) -> Result<()> {
        Ok(())
    }
}

#[async_trait]
impl ProviderOps<octorust::types::SimpleUser, octorust::types::Team> for octorust::Client {
    async fn create_user(&self, _db: &Database, company: &Company, user: User) -> Result<String> {
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

        // We don't need to store the user id, so just return an empty string here.
        Ok(String::new())
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

/*
 *
 * Keep as empty boiler plate for now.

#[async_trait]
impl ProviderOps<ramp_api::types::User, ()> for ramp_api::Client {
    async fn create_user(&self, company: &Company, user: User) -> Result<()> {
        Ok(())
    }

    async fn create_group(&self, company: &Company, group: Group) -> Result<()> {
        Ok(())
    }

    async fn check_user_is_member_of_group(&self, company: &Company, user: User, group: &str) -> Result<bool> {
        Ok(false)
    }

    async fn add_user_to_group(&self, company: &Company, user: User, group: &str) -> Result<()> {
        Ok(())
    }

    async fn remove_user_from_group(&self, company: &Company, user: User, group: &str) -> Result<()> {
        Ok(())
    }

    async fn list_provider_users(&self, company: &Company) -> Result<Vec<ramp_api::types::User>> {
        Ok(vec![])
    }

    async fn list_provider_groups(&self, company: &Company) -> Result<Vec<()>> {
        Ok(vec![])
    }

    async fn delete_user(&self, company: &Company, user: User) -> Result<()> {
        Ok(())
    }

    async fn delete_group(&self, company: &Company, group: Group) -> Result<()> {
        Ok(())
    }
}

*/
