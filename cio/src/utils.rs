use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use hubcaps::http_cache::FileBasedCache;
use hubcaps::issues::Issue;
use hubcaps::repositories::{
    OrgRepoType, OrganizationRepoListOptions, Repository,
};
use hubcaps::{Credentials, Github};
use reqwest::get;
use reqwest::Client;
use yup_oauth2::{
    read_service_account_key, AccessToken, ServiceAccountAuthenticator,
};

use crate::db::Database;
use crate::models::{GithubRepo, NewRepo};

/// Write a file.
pub fn write_file(file: PathBuf, contents: String) {
    // create each directory.
    fs::create_dir_all(file.parent().unwrap()).unwrap();

    // Write to the file.
    let mut f = fs::File::create(file.clone()).unwrap();
    f.write_all(contents.as_bytes()).unwrap();

    println!("wrote file: {}", file.to_str().unwrap());
}

/// Get a GSuite token.
pub async fn get_gsuite_token() -> AccessToken {
    // Get the GSuite credentials file.
    let gsuite_credential_file = env::var("GADMIN_CREDENTIAL_FILE").unwrap();
    let gsuite_subject = env::var("GADMIN_SUBJECT").unwrap();
    let gsuite_secret = read_service_account_key(gsuite_credential_file)
        .await
        .expect("failed to read gsuite credential file");
    let auth = ServiceAccountAuthenticator::builder(gsuite_secret)
        .subject(gsuite_subject.to_string())
        .build()
        .await
        .expect("failed to create authenticator");

    // Add the scopes to the secret and get the token.
    let token = auth
        .token(&[
            "https://www.googleapis.com/auth/admin.directory.group",
            "https://www.googleapis.com/auth/admin.directory.resource.calendar",
            "https://www.googleapis.com/auth/admin.directory.user",
            "https://www.googleapis.com/auth/apps.groups.settings",
            "https://www.googleapis.com/auth/spreadsheets",
            "https://www.googleapis.com/auth/drive",
        ])
        .await
        .expect("failed to get token");

    if token.as_str().is_empty() {
        panic!("empty token is not valid");
    }

    token
}

/// Check if a GitHub issue already exists.
pub fn check_if_github_issue_exists(issues: &[Issue], search: &str) -> bool {
    issues.iter().any(|i| i.title.contains(search))
}

/// Return a user's public ssh key's from GitHub by their GitHub handle.
pub async fn get_github_user_public_ssh_keys(handle: &str) -> Vec<String> {
    let body = get(&format!("https://github.com/{}.keys", handle))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    body.lines()
        .filter_map(|key| {
            let kt = key.trim();
            if !kt.is_empty() {
                Some(kt.to_string())
            } else {
                None
            }
        })
        .collect()
}

/// Authenticate with GitHub.
pub fn authenticate_github() -> Github {
    // Initialize the github client.
    let github_token = env::var("GITHUB_TOKEN").unwrap();
    // Get the current working directory.
    let curdir = env::current_dir().unwrap();
    // Create the HTTP cache.
    let http_cache =
        Box::new(FileBasedCache::new(curdir.join(".cache/github")));
    Github::custom(
        "https://api.github.com",
        concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")),
        Credentials::Token(github_token),
        Client::builder().build().unwrap(),
        http_cache,
    )
}

pub fn github_org() -> String {
    env::var("GITHUB_ORG").unwrap()
}

/// List all the GitHub repositories for our org.
pub async fn list_all_github_repos(github: &Github) -> Vec<NewRepo> {
    // TODO: paginate.
    let github_repos = github
        .org_repos(github_org())
        .list(
            &OrganizationRepoListOptions::builder()
                .per_page(100)
                .repo_type(OrgRepoType::All)
                .build(),
        )
        .await
        .unwrap();

    let mut repos: Vec<NewRepo> = Default::default();
    for r in github_repos {
        repos.push(NewRepo::new(r).await);
    }

    repos
}

/// Sync the repos with our database.
pub async fn refresh_db_github_repos(github: &Github) {
    let github_repos = list_all_github_repos(github).await;

    // Initialize our database.
    let db = Database::new();

    // Get all the repos.
    let db_repos = db.get_github_repos();
    // Create a BTreeMap
    let mut repo_map: BTreeMap<String, GithubRepo> = Default::default();
    for r in db_repos {
        repo_map.insert(r.name.to_string(), r);
    }

    // Sync github_repos.
    for github_repo in github_repos {
        db.upsert_github_repo(&github_repo);

        // Remove the repo from the map.
        repo_map.remove(&github_repo.name);
    }

    // Remove any repos that should no longer be in the database.
    // This is found by the remaining repos that are in the map since we removed
    // the existing repos from the map above.
    for (name, _) in repo_map {
        db.delete_github_repo_by_name(&name);
    }
}

/// Update or create a file in a repository.
pub async fn create_or_update_file(
    repo: &Repository,
    file_path: &str,
    new_content: Vec<u8>,
) {
    let content = new_content.trim();

    // Try to get the content for the file from the repo.
    match repo.content().file(file_path, "master").await {
        Ok(file) => {
            let file_content: Vec<u8> = file.content.into();
            let decoded = file_content.trim();

            // Compare the content to the decoded content and see if we need to update them.
            if content == decoded {
                // They are the same so we can return early, we do not need to update the
                // file.
                println!("[github content] File contents at {} are the same, no update needed", file_path);
                return;
            }

            // We need to update the file. Ignore failure.
            repo.content().update(
                                    file_path,
                                    &content,
                                    &format!("Updating file content {} programatically\n\nThis is done from the cio repo utils::create_or_update_file function.",file_path),
                                    &file.sha).await
                            .ok();

            println!("[github content] Updated file at {}", file_path);
        }
        Err(e) => {
            println!(
                "[github content] Getting the file at {} failed: {:?}",
                file_path, e
            );
            if e.to_string().contains("RateLimit") {
                // Return early.
                return;
            }

            // Create the file in the repo. Ignore failure.
            repo.content().create(
                                    file_path,
                                    &content,
                                    &format!("Creating file content {} programatically\n\nThis is done from the cio repo utils::create_or_update_file function.",file_path),
                            ).await.ok();

            println!("[github content] Created file at {}", file_path);
        }
    }
}

trait SliceExt {
    fn trim(&self) -> Self;
}

impl SliceExt for Vec<u8> {
    fn trim(&self) -> Vec<u8> {
        fn is_whitespace(c: &u8) -> bool {
            c == &b'\t' || c == &b' '
        }

        fn is_not_whitespace(c: &u8) -> bool {
            !is_whitespace(c)
        }

        if let Some(first) = self.iter().position(is_not_whitespace) {
            if let Some(last) = self.iter().rposition(is_not_whitespace) {
                self[first..last + 1].to_vec()
            } else {
                unreachable!();
            }
        } else {
            vec![]
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::utils::authenticate_github;
    use crate::utils::refresh_db_github_repos;

    #[tokio::test(threaded_scheduler)]
    async fn test_github_repos() {
        let github = authenticate_github();
        refresh_db_github_repos(&github).await;
    }
}
