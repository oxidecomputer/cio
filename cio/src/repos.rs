use std::{collections::BTreeMap, io::Write};

use async_trait::async_trait;
use chrono::{offset::Utc, DateTime};
use diesel::{
    deserialize::{self, FromSql},
    pg::Pg,
    serialize::{self, Output, ToSql},
    sql_types::Jsonb,
};
use macros::db;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    airtable::AIRTABLE_GITHUB_REPOS_TABLE, companies::Company, core::UpdateAirtableRecord,
    db::Database, schema::github_repos,
};

/// The data type for a GitHub user.
#[derive(
    Debug, Default, PartialEq, Clone, JsonSchema, FromSqlRow, AsExpression, Serialize, Deserialize,
)]
#[sql_type = "Jsonb"]
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
        deserialize_with = "deserialize_null_string::deserialize",
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

impl FromSql<Jsonb, Pg> for GitHubUser {
    fn from_sql(bytes: Option<&[u8]>) -> deserialize::Result<Self> {
        let value = <serde_json::Value as FromSql<Jsonb, Pg>>::from_sql(bytes)?;
        Ok(serde_json::from_value(value).unwrap())
    }
}

impl ToSql<Jsonb, Pg> for GitHubUser {
    fn to_sql<W: Write>(&self, out: &mut Output<W, Pg>) -> serialize::Result {
        let value = serde_json::to_value(self).unwrap();
        <serde_json::Value as ToSql<Jsonb, Pg>>::to_sql(&value, out)
    }
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
#[table_name = "github_repos"]
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
        deserialize_with = "deserialize_null_string::deserialize",
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
    async fn update_airtable_record(&mut self, _record: GithubRepo) {}
}

impl NewRepo {
    pub fn new(r: octorust::types::MinimalRepository, cio_company_id: i32) -> Self {
        NewRepo {
            github_id: r.id.to_string(),
            // TODO: figure out why octorust thinks this is empty.
            owner: "".to_string(),
            name: r.name,
            full_name: r.full_name,
            description: r.description,
            private: r.private,
            fork: r.fork,
            url: r.url,
            html_url: r.html_url,
            archive_url: r.archive_url,
            assignees_url: r.assignees_url,
            blobs_url: r.blobs_url,
            branches_url: r.branches_url,
            clone_url: r.clone_url,
            collaborators_url: r.collaborators_url,
            comments_url: r.comments_url,
            commits_url: r.commits_url,
            compare_url: r.compare_url,
            contents_url: r.contents_url,
            contributors_url: r.contributors_url,
            deployments_url: r.deployments_url,
            downloads_url: r.downloads_url,
            events_url: r.events_url,
            forks_url: r.forks_url,
            git_commits_url: r.git_commits_url,
            git_refs_url: r.git_refs_url,
            git_tags_url: r.git_tags_url,
            git_url: r.git_url,
            hooks_url: r.hooks_url,
            issue_comment_url: r.issue_comment_url,
            issue_events_url: r.issue_events_url,
            issues_url: r.issues_url,
            keys_url: r.keys_url,
            labels_url: r.labels_url,
            languages_url: r.languages_url,
            merges_url: r.merges_url,
            milestones_url: r.milestones_url,
            mirror_url: r.mirror_url,
            notifications_url: r.notifications_url,
            pulls_url: r.pulls_url,
            releases_url: r.releases_url,
            ssh_url: r.ssh_url,
            stargazers_url: r.stargazers_url,
            statuses_url: r.statuses_url,
            subscribers_url: r.subscribers_url,
            subscription_url: r.subscription_url,
            svn_url: r.svn_url,
            tags_url: r.tags_url,
            teams_url: r.teams_url,
            trees_url: r.trees_url,
            homepage: r.homepage,
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

/// List all the GitHub repositories for our org.
pub async fn list_all_github_repos(github: &octorust::Client, company: &Company) -> Vec<NewRepo> {
    let github_repos = github
        .repos()
        .list_all_for_org(
            &company.github_org,
            octorust::types::ReposListOrgType::All,
            octorust::types::ReposListOrgSort::Created,
            octorust::types::Direction::Desc,
        )
        .await
        .unwrap();

    let mut repos: Vec<NewRepo> = Default::default();
    for r in github_repos {
        repos.push(NewRepo::new(r, company.id));
    }

    repos
}

/// Sync the repos with our database.
pub async fn refresh_db_github_repos(db: &Database, github: &octorust::Client, company: &Company) {
    let github_repos = list_all_github_repos(github, company).await;

    // Get all the repos.
    let db_repos = GithubRepos::get_from_db(db, company.id);

    // Create a BTreeMap
    let mut repo_map: BTreeMap<String, GithubRepo> = Default::default();
    for r in db_repos {
        repo_map.insert(r.name.to_string(), r);
    }

    // Sync github_repos.
    for github_repo in github_repos {
        github_repo.upsert(db).await;

        // Remove the repo from the map.
        repo_map.remove(&github_repo.name);
    }

    // Remove any repos that should no longer be in the database.
    // This is found by the remaining repos that are in the map since we removed
    // the existing repos from the map above.
    for (_, repo) in repo_map {
        repo.delete(db).await;
    }
}

pub mod deserialize_null_string {
    use serde::{self, Deserialize, Deserializer};

    // The signature of a deserialize_with function must follow the pattern:
    //
    //    fn deserialize<'de, D>(D) -> Result<T, D::Error>
    //    where
    //        D: Deserializer<'de>
    //
    // although it may also be generic over the output types T.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<String, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer).unwrap_or_default();

        Ok(s)
    }
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
pub async fn sync_repo_settings(db: &Database, github: &octorust::Client, company: &Company) {
    let repos = GithubRepos::get_from_db(db, company.id);

    // Set the array of default teams to add to the repo.
    // TODO: do not hard code these.
    let default_teams = vec!["all".to_string(), "eng".to_string()];

    // Iterate over the repos and set a number of default settings.
    for r in repos {
        // Skip archived repositories.
        if r.archived {
            continue;
        }

        // Skip "fluffy-tribble"
        if r.name == "fluffy-tribble" {
            continue;
        }

        // Get the branch protection for the repo.
        if let Ok(default_branch) = github
            .repos()
            .get_branch(&company.github_org, &r.name, &r.default_branch)
            .await
        {
            // Add branch protection to disallow force pushing to the default branch.
            // Only do this if it is not already protected.
            if !default_branch.protected {
                match github
                .repos()
                .update_branch_protection(
                    &company.github_org,
                    &r.name,
                    &r.default_branch,
                    &octorust::types::ReposUpdateBranchProtectionRequest {
                        allow_deletions:Default::default(),
                        allow_force_pushes: Default::default(),
                        enforce_admins: true,
                        required_conversation_resolution:Default::default(),
                        required_linear_history: Default::default(),
                        required_pull_request_reviews:
                            octorust::types::ReposUpdateBranchProtectionRequestRequiredPullReviews {
                                dismiss_stale_reviews:Default::default(),
                                dismissal_restrictions: Default::default(),
                                require_code_owner_reviews:Default::default(),
                                required_approving_review_count: Default::default(),
                            },
                        required_status_checks:
                            octorust::types::ReposUpdateBranchProtectionRequestRequiredStatusChecks {
                                contexts: Default::default(),
                                strict: Default::default(),
                            },
                        restrictions: octorust::types::Restrictions {
                            apps: Default::default(),
                            teams: Default::default(),
                            users: Default::default(),
                        },
                    },
                )
                .await
            {
                Ok(_) => (),
                Err(e) => {
                    if !e.to_string().contains("empty repository") {
                        println!("could not update protection for repo {}: {}", r.name, e);
                    }
                }
            }
            }
        } else {
            println!("could not get default branch for repo {}", r.name);
        }

        // Get this repository's teams.
        let mut ts: Vec<octorust::types::Team> = Default::default();
        match github
            .repos()
            .list_all_teams(&company.github_org, &r.name)
            .await
        {
            Ok(v) => (ts = v),
            Err(e) => {
                if !e.to_string().contains("404") {
                    println!("could not list teams for repo {}: {}", r.name, e);
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
                if val.permission == perms.to_string() || val.permission.to_lowercase() == *"admin"
                {
                    // Continue since they already have permission.
                    println!(
                        "team {} already has push access to {}/{}",
                        team_name, company.github_org, r.name
                    );

                    continue;
                }
            }

            match github
                .teams()
                .add_or_update_repo_permissions_in_org(
                    &company.github_org,
                    &team_name,
                    &company.github_org,
                    &r.name,
                    &octorust::types::TeamsAddUpdateRepoPermissionsInOrgRequest {
                        permission: Some(perms),
                    },
                )
                .await
            {
                Ok(_) => (),
                Err(e) => println!(
                    "adding repo permission for team {} in repo {} failed: {}",
                    team_name, r.name, e
                ),
            }

            println!(
                "gave team {} push access to {}/{}",
                team_name, company.github_org, r.name
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        companies::Companys,
        db::Database,
        repos::{refresh_db_github_repos, sync_repo_settings, GithubRepos},
    };

    #[ignore]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_cron_github_repos() {
        // Initialize our database.
        let db = Database::new();
        let companies = Companys::get_from_db(&db, 1);
        // Iterate over the companies and update.
        for company in companies {
            let github = company.authenticate_github();

            sync_repo_settings(&db, &github, &company).await;
            refresh_db_github_repos(&db, &github, &company).await;

            GithubRepos::get_from_db(&db, company.id)
                .update_airtable(&db)
                .await;
        }
    }
}
