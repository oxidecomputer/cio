use std::collections::BTreeMap;
use std::env;

use clap::ArgMatches;
use hubcaps::branches::Protection;
use hubcaps::labels::{Label, LabelOptions};
use hubcaps::repositories::{OrgRepoType, OrganizationRepoListOptions, RepoEditOptions};
use hubcaps::teams::{Permission, Team};
use log::{info, warn};
use tokio::runtime::Runtime;

use crate::utils::{authenticate_github, read_config_from_files, write_file};

pub fn cmd_repos_run(cli_matches: &ArgMatches) {
    // Get the config.
    let mut config = read_config_from_files(cli_matches);

    // Initialize Github and the runtime.
    let github = authenticate_github();
    let github_org = env::var("GITHUB_ORG").unwrap();
    let mut runtime = Runtime::new().unwrap();

    // Get the github repos for the organization.
    let repos = runtime
        .block_on(
            github.org_repos(github_org.to_string()).list(
                &OrganizationRepoListOptions::builder()
                    .repo_type(OrgRepoType::All)
                    .per_page(100)
                    .build(),
            ),
        )
        .unwrap();

    // Set the array of default teams to add to the repo.
    // TODO: do not hard code these.
    let default_teams = vec!["all", "eng"];
    let mut default_team_ids: Vec<u64> = Default::default();

    // Get the ids for the teams.
    let teams = runtime
        .block_on(github.org(github_org.to_string()).teams().list())
        .unwrap();
    // Add the team to the ids if it is a match.
    for team in teams {
        if default_teams.contains(&team.name.as_str()) {
            default_team_ids.push(team.id);
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
        runtime
            .block_on(repo.edit(&RepoEditOptions {
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
            }))
            .unwrap();

        // Get the branch protection for the repo.
        let default_branch = runtime
            .block_on(repo.branches().get(r.default_branch.to_string()))
            .unwrap();

        // Add branch protection to disallow force pushing to the default branch.
        // Only do this if it is not already protected.
        let mut is_protected = false;
        match default_branch.protected {
            Some(val) => {
                if val {
                    is_protected = true
                }
            }
            None => (),
        }
        if !is_protected {
            runtime
                .block_on(repo.branches().protection(
                    r.default_branch.to_string(),
                    &Protection {
                        required_status_checks: None,
                        enforce_admins: true,
                        required_pull_request_reviews: None,
                        restrictions: None,
                    },
                ))
                .unwrap();
        }

        // Get the current labels for the repo.
        let ls = runtime.block_on(repo.labels().list()).unwrap();
        // Create the BTreeMap of labelss.
        let mut labels: BTreeMap<String, Label> = Default::default();
        for l in ls {
            labels.insert(l.clone().name, l.clone());
        }

        // For each label, add the label to the repo.
        // TODO: delete any labels that are not in the config file but present in GitHub.
        for label in &config.labels {
            // Get the new name of the label if there is one.
            let mut new_name = label.name.to_string();
            match &label.new_name {
                Some(val) => {
                    new_name = val.to_string();
                }
                None => (),
            }

            // Check if we already have this label.
            match labels.get(&new_name) {
                Some(val) => {
                    // Check if the description and color are the same.
                    let mut description = "";
                    match &val.description {
                        Some(d) => {
                            description = d;
                        }
                        None => (),
                    }
                    if description == &label.description.to_string()
                        && val.color == label.color.to_string()
                    {
                        // We already have the label so continue through our loop.
                        continue;
                    }
                }
                None => (),
            }

            // Try to update the label, otherwise create the label.
            match runtime.block_on(repo.labels().update(
                &label.name,
                &LabelOptions {
                    description: label.description.to_string(),
                    color: label.color.to_string(),
                    name: new_name.to_string(),
                },
            )) {
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

            match runtime.block_on(
                github
                    .repo(github_org.to_string(), r.name.to_string())
                    .labels()
                    .create(&LabelOptions {
                        name: label.name.to_string(),
                        description: label.description.to_string(),
                        color: label.color.to_string(),
                    }),
            ) {
                Ok(_) => (),
                Err(e) => warn!(
                    "creating label {} in repo {} failed: {}",
                    label.name, r.name, e
                ),
            }
        }

        info!("updated labels for repo {}", r.name);

        // Get this repository's teams.
        let ts = runtime.block_on(repo.teams().list()).unwrap();
        // Create the BTreeMap of labelss.
        let mut teams: BTreeMap<u64, Team> = Default::default();
        for t in ts {
            teams.insert(t.clone().id, t.clone());
        }

        // For each team id, add the team to the permissions.
        for team_id in &default_team_ids {
            let perms = Permission::Push;

            // Check if the team already has the permission.
            match teams.get(team_id) {
                Some(val) => {
                    if val.permission == perms.to_string() {
                        // Continue since they already have permission.
                        info!(
                            "team {} already has push access to {}/{}",
                            team_id, github_org, r.name
                        );

                        continue;
                    }
                }
                None => (),
            }

            runtime
                .block_on(
                    github
                        .org(github_org.to_string())
                        .teams()
                        .add_repo_permission(*team_id, r.name.to_string(), perms),
                )
                .unwrap();

            info!(
                "gave team {} push access to {}/{}",
                team_id, github_org, r.name
            );
        }
    }

    // For each label, update the label then write back out to the file.
    for (i, label) in config.labels.clone().iter().enumerate() {
        // Get the new name of the label if there is one.
        let mut new_name = label.name.to_string();
        match &label.new_name {
            Some(val) => {
                new_name = val.to_string();
            }
            None => (),
        }
        // If the label has a new name, then make that new name the actual
        // label name.
        // This will help us re-write the file back out again.
        if new_name != label.name {
            config.labels[i].new_name = None;
            config.labels[i].name = new_name;
        }
    }

    // Get the current working directory.
    let curdir = env::current_dir().unwrap();

    // Now that we have updated all the labels, re-write the file back out.
    let toml = toml::to_string(&config.labels)
        .unwrap()
        .replace("[[]]", "[[labels]]");
    write_file(curdir.join("configs/labels.new.toml"), toml);
}
