use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::str::from_utf8;

use futures_util::TryStreamExt;
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
    let mut gsuite_credential_file =
        env::var("GADMIN_CREDENTIAL_FILE").unwrap_or("".to_string());
    let gsuite_key = env::var("GSUITE_KEY").unwrap_or("".to_string());

    if gsuite_credential_file.is_empty() && !gsuite_key.is_empty() {
        // Save the gsuite key to a tmp file.
        let mut file_path = env::temp_dir();
        file_path.push("gsuite_key.json");

        // Create the file and write to it.
        let mut file = fs::File::create(file_path.clone()).unwrap();
        file.write_all(gsuite_key.as_bytes()).unwrap();

        // Set the GSuite credential file to the temp path.
        gsuite_credential_file = file_path.to_str().unwrap().to_string();
    }

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

/// Create or update a file in a GitHub repository.
/// If the file does not exist, it will be created.
/// If the file exists, it will be updated _only if_ the content of the file has changed.
pub async fn create_or_update_file_in_github_repo(
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

            // When the pdfs are generated they change the modified time that is
            // encoded in the file. We want to get that diff and see if it is
            // the only change so that we are not always updating those files.
            let diff = diffy::create_patch_bytes(&decoded, &content);
            let bdiff = diff.to_bytes();
            let str_diff = from_utf8(&bdiff).unwrap_or("");
            if str_diff.contains("-/ModDate")
                && str_diff.contains("-/CreationDate")
                && str_diff.contains("+/ModDate")
                && str_diff.contains("-/CreationDate")
                && str_diff.contains("@@ -5,8 +5,8 @@")
            {
                // The binary contents are the same so we can return early.
                // The only thing that changed was the modified time and creation date.
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
            let error_string = format!("{:?}", e);
            if error_string.contains("RateLimit") {
                // Return early.
                return;
            }
            if error_string.contains("too_large") {
                // The file is too big for us to get it's contents through this API.
                // The error suggests we use the Git Data API but we need the file sha for
                // that.
                // TODO: make this less awful.
                // Get all the items in the directory and try to find our file and get the sha
                // for it so we can update it.
                let mut path = PathBuf::from(file_path);
                path.pop();

                for item in repo
                    .content()
                    .iter(path.to_str().unwrap(), "master")
                    .try_collect::<Vec<hubcaps::content::DirectoryItem>>()
                    .await
                    .unwrap()
                {
                    if file_path.trim_start_matches('/') != item.path {
                        // Continue early.
                        continue;
                    }

                    // Otherwise, this is our file.
                    // We have the sha we can see if the files match using the
                    // Git Data API.
                    let blob = repo.git().blob(&item.sha).await.unwrap();
                    // Base64 decode the contents.
                    // TODO: move this logic to hubcaps.
                    let v = blob.content.replace("\n", "");
                    let decoded =
                        base64::decode_config(&v, base64::STANDARD).unwrap();
                    let file_content: Vec<u8> = decoded.into();
                    let decoded_content = file_content.trim();

                    // Compare the content to the decoded content and see if we need to update them.
                    if content == decoded_content {
                        // They are the same so we can return early, we do not need to update the
                        // file.
                        println!("[github content] File contents at {} are the same, no update needed", file_path);
                        return;
                    }

                    // We can actually update the file since we have the sha.
                    repo.content().update(
                                    file_path,
                                    &content,
                                    &format!("Updating file content {} programatically\n\nThis is done from the cio repo utils::create_or_update_file function.",file_path),
                                    &item.sha).await
                            .ok();

                    println!("[github content] Updated file at {}", file_path);

                    // We can break the loop now.
                    break;
                }

                return;
            }
            println!(
                "[github content] Getting the file at {} failed: {:?}",
                file_path, e
            );

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

pub fn default_date() -> chrono::naive::NaiveDate {
    chrono::naive::NaiveDate::parse_from_str("1970-01-01", "%Y-%m-%d").unwrap()
}

#[cfg(test)]
mod tests {
    use crate::utils::authenticate_github;
    use crate::utils::refresh_db_github_repos;

    #[tokio::test(threaded_scheduler)]
    async fn test_cron_github_repos() {
        let github = authenticate_github();
        refresh_db_github_repos(&github).await;
    }
}
