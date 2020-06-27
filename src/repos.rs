use std::collections::BTreeMap;
use std::env;

use clap::ArgMatches;
use hubcaps::branches::Protection;
use hubcaps::labels::{Label, LabelOptions};
use hubcaps::repositories::{
    OrgRepoType, OrganizationRepoListOptions, RepoEditOptions,
};
use hubcaps::teams::{Permission, Team};
use log::{info, warn};

use crate::utils::{authenticate_github, read_config_from_files};

/**
 * Set default configurations for all repos in the GitHub organization.
 *
 * The defaults are as follows:
 *
 * - Give the GitHub teams: "eng" and "all", push access to every repository.
 * - Adds and updates each of the labels from labels.toml.
 * - Turns off the wiki.
 * - Adds protection to the default branch to disallow force pushes.
 */
pub async fn cmd_repos_run(cli_matches: &ArgMatches<'_>) {
    // Get the config.
    let config = read_config_from_files(cli_matches);

    // Initialize Github.
    let github = authenticate_github();
    let github_org = env::var("GITHUB_ORG").unwrap();

    // Get the github repos for the organization.
    let repos = github
        .org_repos(github_org.to_string())
        .list(
            &OrganizationRepoListOptions::builder()
                .repo_type(OrgRepoType::All)
                .per_page(100)
                .build(),
        )
        .await
        .unwrap();

    // Set the array of default teams to add to the repo.
    // TODO: do not hard code these.
    let default_teams = vec!["all", "eng"];
    let mut default_team_ids: BTreeMap<u64, String> = Default::default();

    // Get the ids for the teams.
    let teams = github
        .org(github_org.to_string())
        .teams()
        .list()
        .await
        .unwrap();
    // Add the team to the ids if it is a match.
    for team in teams {
        if default_teams.contains(&team.name.as_str()) {
            default_team_ids.insert(team.id, team.name);
        }
    }

    // Iterate over the repos and set a number of default settings.
    for r in repos {
        // Skip archived repositories.
        if r.archived {
            continue;
        }

        // Get the repository object.
        let repo = github.repo(github_org.to_string(), r.name.to_string());

        // Update the repository settings.
        match repo
            .edit(&RepoEditOptions {
                name: r.name.to_string(),
                description: r.description,
                homepage: r.homepage,
                private: Some(r.private),
                has_issues: Some(r.has_issues),
                has_projects: None,
                has_wiki: Some(false),
                default_branch: Some(r.default_branch.to_string()),
                allow_squash_merge: Some(true),
                allow_merge_commit: Some(false),
                allow_rebase_merge: Some(true),
            })
            .await
        {
            Ok(_) => (),
            Err(e) => {
                if e.to_string().contains("empty repository") {
                    continue;
                }
                warn!("could not update repo {}: {}", r.name, e);
            }
        }

        // Get the branch protection for the repo.
        let mut default_branch = hubcaps::branches::Branch {
            name: "".to_string(),
            protected: None,
            protection_url: None,
        };
        match repo.branches().get(r.default_branch.to_string()).await {
            Ok(d) => default_branch = d,
            Err(e) => {
                if !e.to_string().contains("empty repository") {
                    warn!(
                        "could not default branch for repo {}: {}",
                        r.name, e
                    );
                }
            }
        }

        // Add branch protection to disallow force pushing to the default branch.
        // Only do this if it is not already protected.
        let is_protected = default_branch.protected.unwrap_or(false);
        if !is_protected {
            match repo
                .branches()
                .protection(
                    r.default_branch.to_string(),
                    &Protection {
                        required_status_checks: None,
                        enforce_admins: true,
                        required_pull_request_reviews: None,
                        restrictions: None,
                    },
                )
                .await
            {
                Ok(_) => (),
                Err(e) => {
                    if !e.to_string().contains("empty repository") {
                        warn!(
                            "could not update protection for repo {}: {}",
                            r.name, e
                        );
                    }
                }
            }
        }

        // Get the current labels for the repo.
        let ls = repo.labels().list().await.unwrap();
        // Create the BTreeMap of labelss.
        let mut labels: BTreeMap<String, Label> = Default::default();
        for l in ls {
            labels.insert(l.name.to_string(), l);
        }

        // For each label, add the label to the repo.
        for label in &config.labels {
            // Check if we already have this label, and remove it if so,
            // so that when we are all finished we can delete any labels that
            // exist in repos and should not be there.
            if let Some(val) = labels.remove(&label.name) {
                // Check if the description and color are the same.
                if val.description.as_ref() == Some(&label.description)
                    && val.color == label.color
                {
                    // We already have the label so continue through our loop.
                    continue;
                }
            }

            // Try to update the label, otherwise create the label.
            match repo
                .labels()
                .update(
                    &label.name,
                    &LabelOptions {
                        description: label.description.to_string(),
                        color: label.color.to_string(),
                        name: label.name.to_string(),
                    },
                )
                .await
            {
                // Continue early since we do not need to create a label now.
                Ok(_) => continue,
                Err(e) => {
                    // Ignore the error if it is a 404 since we will try to create the label below
                    // instead.
                    if !e.to_string().contains("404") {
                        warn!(
                            "updating label {} in repo {} failed: {}",
                            label.name,
                            r.name.to_string(),
                            e
                        );
                    }
                }
            }

            match github
                .repo(github_org.to_string(), r.name.to_string())
                .labels()
                .create(&LabelOptions {
                    name: label.name.to_string(),
                    description: label.description.to_string(),
                    color: label.color.to_string(),
                })
                .await
            {
                Ok(_) => (),
                Err(e) => warn!(
                    "creating label {} in repo {} failed: {}",
                    label.name, r.name, e
                ),
            }
        }

        // Iterate over the remaining labels for the repo and delete any that were not in our
        // config file.
        for (name, _label) in labels {
            warn!(
                "repo {} has label {} but that is not in the config file, DELETING",
                r.name, name
            );

            // Delete the label.
            repo.labels().delete(&name).await.unwrap();
        }

        info!("updated labels for repo {}", r.name);

        // Get this repository's teams.
        let ts = repo.teams().list().await.unwrap();
        // Create the BTreeMap of labelss.
        let mut teams: BTreeMap<u64, Team> = Default::default();
        for t in ts {
            teams.insert(t.id, t);
        }

        // For each team id, add the team to the permissions.
        for (team_id, team_name) in &default_team_ids {
            let perms = Permission::Push;

            // Check if the team already has the permission.
            if let Some(val) = teams.get(team_id) {
                if val.permission == perms.to_string() {
                    // Continue since they already have permission.
                    info!(
                        "team {} already has push access to {}/{}",
                        team_name, github_org, r.name
                    );

                    continue;
                }
            }

            github
                .org(github_org.to_string())
                .teams()
                .add_repo_permission(*team_id, r.name.to_string(), perms)
                .await
                .unwrap();

            info!(
                "gave team {} push access to {}/{}",
                team_name, github_org, r.name
            );
        }
    }
}
