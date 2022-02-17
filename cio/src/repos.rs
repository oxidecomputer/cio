use std::{collections::BTreeMap};

use anyhow::{bail, Result};
use async_bb8_diesel::{AsyncRunQueryDsl};
use async_trait::async_trait;
use chrono::{offset::Utc, DateTime};
use diesel::{
    sql_types::Jsonb,
};
use log::info;
use macros::db;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    airtable::AIRTABLE_GITHUB_REPOS_TABLE, companies::Company, core::UpdateAirtableRecord, db::Database,
    github_prs::FromSimpleUser, schema::github_repos,
};

/// The data type for a GitHub user.
#[derive(Debug, Default, PartialEq, Clone, JsonSchema, FromSqlRow, AsExpression, Serialize, Deserialize)]
#[diesel(sql_type = Jsonb)]
pub struct GitHubUser {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub login: String,
    #[serde(default)]
    pub id: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub username: String,
    #[serde(
        default,
        deserialize_with = "octorust::utils::deserialize_null_string::deserialize",
        skip_serializing_if = "String::is_empty"
    )]
    pub email: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub avatar_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub gravatar_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub html_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub followers_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub following_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub gists_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub starred_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub subscriptions_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub organizations_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub repos_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub events_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub received_events_url: String,
    #[serde(default)]
    pub site_admin: bool,
}

/// The data type for a GitHub repository.
#[db {
    new_struct_name = "GithubRepo",
    airtable_base = "misc",
    airtable_table = "AIRTABLE_GITHUB_REPOS_TABLE",
    match_on = {
        "github_id" = "String",
    },
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[diesel(table_name = github_repos)]
pub struct NewRepo {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub github_id: String,
    pub owner: String,
    pub name: String,
    pub full_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default)]
    pub private: bool,
    #[serde(default)]
    pub fork: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub html_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub archive_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub assignees_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub blobs_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub branches_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub clone_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub collaborators_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub comments_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub commits_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub compare_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub contents_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub contributors_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub deployments_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub downloads_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub events_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub forks_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub git_commits_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub git_refs_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub git_tags_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub git_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub hooks_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub issue_comment_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub issue_events_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub issues_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub keys_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub labels_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub languages_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub merges_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub milestones_url: String,
    #[serde(
        default,
        deserialize_with = "octorust::utils::deserialize_null_string::deserialize",
        skip_serializing_if = "String::is_empty"
    )]
    pub mirror_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notifications_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pulls_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub releases_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ssh_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub stargazers_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub statuses_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub subscribers_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub subscription_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub svn_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tags_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub teams_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub trees_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub homepage: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub language: String,
    #[serde(default)]
    pub forks_count: i32,
    pub stargazers_count: i32,
    #[serde(default)]
    pub watchers_count: i32,
    #[serde(default)]
    pub size: i32,
    pub default_branch: String,
    #[serde(default)]
    pub open_issues_count: i32,
    #[serde(default)]
    pub has_issues: bool,
    #[serde(default)]
    pub has_wiki: bool,
    #[serde(default)]
    pub has_pages: bool,
    #[serde(default)]
    pub has_downloads: bool,
    #[serde(default)]
    pub archived: bool,
    #[serde(deserialize_with = "crate::configs::null_date_format::deserialize")]
    pub pushed_at: DateTime<Utc>,
    #[serde(deserialize_with = "crate::configs::null_date_format::deserialize")]
    pub created_at: DateTime<Utc>,
    #[serde(deserialize_with = "crate::configs::null_date_format::deserialize")]
    pub updated_at: DateTime<Utc>,
    /// The CIO company ID.
    #[serde(default)]
    pub cio_company_id: i32,
}

/// Implement updating the Airtable record for a GithubRepo.
#[async_trait]
impl UpdateAirtableRecord<GithubRepo> for GithubRepo {
    async fn update_airtable_record(&mut self, _record: GithubRepo) -> Result<()> {
        Ok(())
    }
}

impl NewRepo {
    pub fn new_from_full(r: octorust::types::FullRepository, cio_company_id: i32) -> Self {
        NewRepo {
            github_id: r.id.to_string(),
            owner: r.owner.to_string(),
            name: r.name,
            full_name: r.full_name,
            description: r.description,
            private: r.private,
            fork: r.fork,
            url: r.url.to_string(),
            html_url: r.html_url.to_string(),
            archive_url: r.archive_url.to_string(),
            assignees_url: r.assignees_url.to_string(),
            blobs_url: r.blobs_url.to_string(),
            branches_url: r.branches_url.to_string(),
            clone_url: r.clone_url.to_string(),
            collaborators_url: r.collaborators_url.to_string(),
            comments_url: r.comments_url.to_string(),
            commits_url: r.commits_url.to_string(),
            compare_url: r.compare_url.to_string(),
            contents_url: r.contents_url.to_string(),
            contributors_url: r.contributors_url.to_string(),
            deployments_url: r.deployments_url.to_string(),
            downloads_url: r.downloads_url.to_string(),
            events_url: r.events_url.to_string(),
            forks_url: r.forks_url.to_string(),
            git_commits_url: r.git_commits_url.to_string(),
            git_refs_url: r.git_refs_url.to_string(),
            git_tags_url: r.git_tags_url.to_string(),
            git_url: r.git_url.to_string(),
            hooks_url: r.hooks_url.to_string(),
            issue_comment_url: r.issue_comment_url.to_string(),
            issue_events_url: r.issue_events_url.to_string(),
            issues_url: r.issues_url.to_string(),
            keys_url: r.keys_url.to_string(),
            labels_url: r.labels_url.to_string(),
            languages_url: r.languages_url.to_string(),
            merges_url: r.merges_url.to_string(),
            milestones_url: r.milestones_url.to_string(),
            mirror_url: r.mirror_url.to_string(),
            notifications_url: r.notifications_url.to_string(),
            pulls_url: r.pulls_url.to_string(),
            releases_url: r.releases_url.to_string(),
            ssh_url: r.ssh_url.to_string(),
            stargazers_url: r.stargazers_url.to_string(),
            statuses_url: r.statuses_url.to_string(),
            subscribers_url: r.subscribers_url.to_string(),
            subscription_url: r.subscription_url.to_string(),
            svn_url: r.svn_url.to_string(),
            tags_url: r.tags_url.to_string(),
            teams_url: r.teams_url.to_string(),
            trees_url: r.trees_url.to_string(),
            homepage: r.homepage.to_string(),
            language: r.language,
            forks_count: r.forks_count as i32,
            stargazers_count: r.stargazers_count as i32,
            watchers_count: r.watchers_count as i32,
            size: r.size as i32,
            default_branch: r.default_branch,
            open_issues_count: r.open_issues_count as i32,
            has_issues: r.has_issues,
            has_wiki: r.has_wiki,
            has_pages: r.has_pages,
            has_downloads: r.has_downloads,
            archived: r.archived,
            pushed_at: r.pushed_at.unwrap(),
            created_at: r.created_at.unwrap(),
            updated_at: r.updated_at.unwrap(),
            cio_company_id,
        }
    }

    pub fn new(r: octorust::types::MinimalRepository, cio_company_id: i32) -> Self {
        NewRepo {
            github_id: r.id.to_string(),
            owner: r.owner.unwrap().login,
            name: r.name,
            full_name: r.full_name,
            description: r.description,
            private: r.private,
            fork: r.fork,
            url: r.url.to_string(),
            html_url: r.html_url.to_string(),
            archive_url: r.archive_url.to_string(),
            assignees_url: r.assignees_url.to_string(),
            blobs_url: r.blobs_url.to_string(),
            branches_url: r.branches_url.to_string(),
            clone_url: r.clone_url.to_string(),
            collaborators_url: r.collaborators_url.to_string(),
            comments_url: r.comments_url.to_string(),
            commits_url: r.commits_url.to_string(),
            compare_url: r.compare_url.to_string(),
            contents_url: r.contents_url.to_string(),
            contributors_url: r.contributors_url.to_string(),
            deployments_url: r.deployments_url.to_string(),
            downloads_url: r.downloads_url.to_string(),
            events_url: r.events_url.to_string(),
            forks_url: r.forks_url.to_string(),
            git_commits_url: r.git_commits_url.to_string(),
            git_refs_url: r.git_refs_url.to_string(),
            git_tags_url: r.git_tags_url.to_string(),
            git_url: r.git_url.to_string(),
            hooks_url: r.hooks_url.to_string(),
            issue_comment_url: r.issue_comment_url.to_string(),
            issue_events_url: r.issue_events_url.to_string(),
            issues_url: r.issues_url.to_string(),
            keys_url: r.keys_url.to_string(),
            labels_url: r.labels_url.to_string(),
            languages_url: r.languages_url.to_string(),
            merges_url: r.merges_url.to_string(),
            milestones_url: r.milestones_url.to_string(),
            mirror_url: r.mirror_url.to_string(),
            notifications_url: r.notifications_url.to_string(),
            pulls_url: r.pulls_url.to_string(),
            releases_url: r.releases_url.to_string(),
            ssh_url: r.ssh_url.to_string(),
            stargazers_url: r.stargazers_url.to_string(),
            statuses_url: r.statuses_url.to_string(),
            subscribers_url: r.subscribers_url.to_string(),
            subscription_url: r.subscription_url.to_string(),
            svn_url: r.svn_url.to_string(),
            tags_url: r.tags_url.to_string(),
            teams_url: r.teams_url.to_string(),
            trees_url: r.trees_url.to_string(),
            homepage: r.homepage.to_string(),
            language: r.language,
            forks_count: r.forks_count as i32,
            stargazers_count: r.stargazers_count as i32,
            watchers_count: r.watchers_count as i32,
            size: r.size as i32,
            default_branch: r.default_branch,
            open_issues_count: r.open_issues_count as i32,
            has_issues: r.has_issues,
            has_wiki: r.has_wiki,
            has_pages: r.has_pages,
            has_downloads: r.has_downloads,
            archived: r.archived,
            pushed_at: r.pushed_at.unwrap(),
            created_at: r.created_at.unwrap(),
            updated_at: r.updated_at.unwrap(),
            cio_company_id,
        }
    }
}

impl GithubRepo {
    /**
     * Set default configurations for the repo in the GitHub organization.
     *
     * The defaults are as follows:
     *
     * - Give the GitHub teams: "eng" and "all", push access to every repository.
     * - Turns off the wiki.
     * - Adds protection to the default branch to disallow force pushes.
     * - Adds outside collaborators to their specified repositories.
     */
    pub async fn sync_settings(&self, github: &octorust::Client, company: &Company) -> Result<()> {
        // Skip archived repositories.
        if self.archived {
            return Ok(());
        }

        // Skip "fluffy-tribble"
        if self.name == "fluffy-tribble" {
            return Ok(());
        }

        // Set the array of default teams to add to the repo.
        // TODO: do not hard code these.
        let default_teams = vec!["all".to_string(), "eng".to_string()];

        // Get the branch protection for the repo.
        let branch = github
            .repos()
            .get_branch(&company.github_org, &self.name, &self.default_branch)
            .await;
        if let Err(e) = branch {
            if !e.to_string().contains("404") && !e.to_string().contains("Not Found") {
                bail!("could not get branch {} repo {}: {}", self.default_branch, self.name, e);
            } else {
                // Return early. Likely the repo no longer exists.
                return Ok(());
            }
        }
        let default_branch = branch?;

        // Add branch protection to disallow force pushing to the default branch.
        // Only do this if it is not already protected.
        if !default_branch.protected {
            match github
                .repos()
                .update_branch_protection(
                    &company.github_org,
                    &self.name,
                    &self.default_branch,
                    &octorust::types::ReposUpdateBranchProtectionRequest {
                        allow_deletions: Default::default(),
                        allow_force_pushes: Default::default(),
                        enforce_admins: Some(true),
                        required_conversation_resolution: Default::default(),
                        required_linear_history: Default::default(),
                        required_pull_request_reviews: None,
                        required_status_checks: None,
                        restrictions: None,
                    },
                )
                .await
            {
                Ok(_) => (),
                Err(e) => {
                    if !e.to_string().contains("empty repository") {
                        bail!("could not update protection for repo {}: {}", self.name, e);
                    }
                }
            }
        }

        // Get this repository's teams.
        let mut ts: Vec<octorust::types::Team> = Default::default();
        match github.repos().list_all_teams(&company.github_org, &self.name).await {
            Ok(v) => (ts = v),
            Err(e) => {
                // If we get a 404 for teams then likely the repo is new, we can just move on and
                // add the teams.
                if !e.to_string().contains("404") && !e.to_string().contains("Not Found") {
                    bail!("could not list teams for repo {}: {}", self.name, e);
                }
            }
        }
        // Create the BTreeMap of teams.
        let mut teams: BTreeMap<String, octorust::types::Team> = Default::default();
        for t in ts {
            teams.insert(t.name.to_string(), t);
        }

        // For each team id, add the team to the permissions.
        for team_name in &default_teams {
            let perms = octorust::types::TeamsAddUpdateRepoPermissionsInOrgRequestPermission::Push;

            // Check if the team already has the permission.
            if let Some(val) = teams.get(team_name) {
                if val.permission == perms.to_string() || val.permission.to_lowercase() == *"admin" {
                    // Continue since they already have permission.
                    info!(
                        "team {} already has push access to {}/{}",
                        team_name, company.github_org, self.name
                    );

                    continue;
                }
            }

            match github
                .teams()
                .add_or_update_repo_permissions_in_org(
                    &company.github_org,
                    team_name,
                    &company.github_org,
                    &self.name,
                    &octorust::types::TeamsAddUpdateRepoPermissionsInOrgRequest {
                        permission: Some(perms),
                    },
                )
                .await
            {
                Ok(_) => (),
                Err(e) => bail!(
                    "adding repo permission for team {} in repo {} failed: {}",
                    team_name,
                    self.name,
                    e
                ),
            }

            info!(
                "gave team {} push access to {}/{}",
                team_name, company.github_org, self.name
            );
        }

        Ok(())
    }
}

/// List all the GitHub repositories for our org.
pub async fn list_all_github_repos(github: &octorust::Client, company: &Company) -> Result<Vec<NewRepo>> {
    let github_repos = github
        .repos()
        .list_all_for_org(
            &company.github_org,
            octorust::types::ReposListOrgType::All,
            octorust::types::ReposListOrgSort::Created,
            octorust::types::Order::Desc,
        )
        .await?;

    let mut repos: Vec<NewRepo> = Default::default();
    for r in github_repos {
        repos.push(NewRepo::new(r, company.id));
    }

    Ok(repos)
}

/// Sync the repos with our database.
pub async fn refresh_db_github_repos(db: &Database, github: &octorust::Client, company: &Company) -> Result<()> {
    let github_repos = list_all_github_repos(github, company).await?;

    // Get all the repos.
    let db_repos = GithubRepos::get_from_db(db, company.id).await?;

    // Create a BTreeMap
    let mut repo_map: BTreeMap<String, GithubRepo> = Default::default();
    for r in db_repos {
        repo_map.insert(r.name.to_string(), r);
    }

    // Sync github_repos.
    for github_repo in github_repos {
        github_repo.upsert(db).await?;

        // Remove the repo from the map.
        repo_map.remove(&github_repo.name);
    }

    // Remove any repos that should no longer be in the database.
    // This is found by the remaining repos that are in the map since we removed
    // the existing repos from the map above.
    for (_, repo) in repo_map {
        repo.delete(db).await?;
    }

    GithubRepos::get_from_db(db, company.id)
        .await?
        .update_airtable(db)
        .await?;

    Ok(())
}

/**
 * Set default configurations for all repos in the GitHub organization.
 *
 * The defaults are as follows:
 *
 * - Give the GitHub teams: "eng" and "all", push access to every repository.
 * - Turns off the wiki.
 * - Adds protection to the default branch to disallow force pushes.
 * - Adds outside collaborators to their specified repositories.
 */
pub async fn sync_all_repo_settings(db: &Database, github: &octorust::Client, company: &Company) -> Result<()> {
    let repos = GithubRepos::get_from_db(db, company.id).await?;

    // Iterate over the repos and set a number of default settings.
    for r in repos {
        r.sync_settings(github, company).await?;
    }

    Ok(())
}

pub trait FromUrl {
    fn to_string(&self) -> String;
}

impl FromUrl for Option<url::Url> {
    fn to_string(&self) -> String {
        if let Some(i) = self {
            i.to_string()
        } else {
            String::new()
        }
    }
}
