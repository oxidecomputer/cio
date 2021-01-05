use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::Write;
use std::ops::Add;
use std::path::PathBuf;
use std::str::from_utf8;
use std::thread;
use std::time;

use futures_util::stream::TryStreamExt;
use hubcaps::http_cache::FileBasedCache;
use hubcaps::issues::Issue;
use hubcaps::repositories::{OrgRepoType, OrganizationRepoListOptions, Repository};
use hubcaps::{Credentials, Github, InstallationTokenGenerator, JWTCredentials};
use reqwest::get;
use reqwest::Client;
use tracing::instrument;
use yup_oauth2::{read_service_account_key, AccessToken, ServiceAccountAuthenticator};

use crate::db::Database;
use crate::models::{GithubRepo, GithubRepos, NewRepo};

pub static DOMAIN: &str = "oxide.computer";
pub static GSUITE_DOMAIN: &str = "oxidecomputer.com";

/// Write a file.
#[instrument]
#[inline]
pub fn write_file(file: &PathBuf, contents: &str) {
    // create each directory.
    fs::create_dir_all(file.parent().unwrap()).unwrap();

    // Write to the file.
    let mut f = fs::File::create(file.clone()).unwrap();
    f.write_all(contents.as_bytes()).unwrap();

    println!("wrote file: {}", file.to_str().unwrap());
}

/// Get a GSuite token.
#[instrument]
#[inline]
pub async fn get_gsuite_token() -> AccessToken {
    let gsuite_key = env::var("GSUITE_KEY_ENCODED").unwrap_or_default();
    // Get the GSuite credentials file.
    let mut gsuite_credential_file = env::var("GADMIN_CREDENTIAL_FILE").unwrap_or_default();

    if gsuite_credential_file.is_empty() && !gsuite_key.is_empty() {
        let b = base64::decode(gsuite_key).unwrap();

        // Save the gsuite key to a tmp file.
        let mut file_path = env::temp_dir();
        file_path.push("gsuite_key.json");

        // Create the file and write to it.
        let mut file = fs::File::create(file_path.clone()).unwrap();
        file.write_all(&b).unwrap();

        // Set the GSuite credential file to the temp path.
        gsuite_credential_file = file_path.to_str().unwrap().to_string();
    }

    let gsuite_subject = env::var("GADMIN_SUBJECT").unwrap();
    let gsuite_secret = read_service_account_key(gsuite_credential_file).await.expect("failed to read gsuite credential file");
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
#[instrument]
#[inline]
pub fn check_if_github_issue_exists(issues: &[Issue], search: &str) -> Option<Issue> {
    for i in issues {
        if i.title.contains(search) {
            return Some(i.clone());
        }
    }

    None
}

/// Return a user's public ssh key's from GitHub by their GitHub handle.
#[instrument]
#[inline]
pub async fn get_github_user_public_ssh_keys(handle: &str) -> Vec<String> {
    let body = get(&format!("https://github.com/{}.keys", handle)).await.unwrap().text().await.unwrap();

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
#[instrument]
#[inline]
pub fn authenticate_github() -> Github {
    // Initialize the github client.
    let github_token = env::var("GITHUB_TOKEN").unwrap();
    // Create the HTTP cache.
    let http_cache = Box::new(FileBasedCache::new(format!("{}/.cache/github", env::var("HOME").unwrap())));
    Github::custom(
        "https://api.github.com",
        concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")),
        Credentials::Token(github_token),
        Client::builder().build().unwrap(),
        http_cache,
    )
}

/// Authenticate GitHub with JSON web token credentials.
#[instrument]
#[inline]
pub fn authenticate_github_jwt() -> Github {
    // Parse our env variables.
    let app_id_str = env::var("GH_APP_ID").unwrap();
    let app_id = app_id_str.parse::<u64>().unwrap();
    let installation_id_str = env::var("GH_INSTALLATION_ID").unwrap();
    let installation_id = installation_id_str.parse::<u64>().unwrap();
    let encoded_private_key = env::var("GH_PRIVATE_KEY").unwrap();
    let private_key = base64::decode(encoded_private_key).unwrap();

    // Decode the key.
    let key = nom_pem::decode_block(&private_key).unwrap();

    // Get the JWT credentials.
    let jwt = JWTCredentials::new(app_id, key.data).unwrap();

    // Create the HTTP cache.
    let http_cache = Box::new(FileBasedCache::new(format!("{}/.cache/github", env::var("HOME").unwrap())));

    let token_generator = InstallationTokenGenerator::new(installation_id, jwt);

    Github::custom(
        "https://api.github.com",
        concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")),
        Credentials::InstallationToken(token_generator),
        Client::builder().build().unwrap(),
        http_cache,
    )
}

#[instrument]
#[inline]
pub fn github_org() -> String {
    env::var("GITHUB_ORG").unwrap()
}

/// List all the GitHub repositories for our org.
#[instrument]
#[inline]
pub async fn list_all_github_repos(github: &Github) -> Vec<NewRepo> {
    let github_repos = github
        .org_repos(github_org())
        .iter(&OrganizationRepoListOptions::builder().per_page(100).repo_type(OrgRepoType::All).build())
        .try_collect::<Vec<hubcaps::repositories::Repo>>()
        .await
        .unwrap();

    let mut repos: Vec<NewRepo> = Default::default();
    for r in github_repos {
        repos.push(NewRepo::new(r));
    }

    repos
}

/// Sync the repos with our database.
#[instrument(skip(db))]
#[inline]
pub async fn refresh_db_github_repos(db: &Database, github: &Github) {
    let github_repos = list_all_github_repos(github).await;

    // Get all the repos.
    let db_repos = GithubRepos::get_from_db(db);

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

/// Get a files content from a repo.
/// It returns a tuple of the bytes of the file content and the sha of the file.
pub async fn get_file_content_from_repo(repo: &Repository, branch: &str, path: &str) -> (Vec<u8>, String) {
    // Add the starting "/" so this works.
    // TODO: figure out why it doesn't work without it.
    let mut file_path = path.to_string();
    if !path.starts_with('/') {
        file_path = "/".to_owned() + path;
    }

    // Try to get the content for the file from the repo.
    match repo.content().file(&file_path, branch).await {
        Ok(file) => return (file.content.into(), file.sha),
        Err(e) => {
            match e {
                hubcaps::errors::Error::RateLimit { reset } => {
                    // We got a rate limit error.
                    println!("got rate limited, sleeping for {}s", reset.as_secs());
                    thread::sleep(reset.add(time::Duration::from_secs(5)));
                }
                hubcaps::errors::Error::Fault { code: _, error } => {
                    if error.message.contains("too_large") {
                        // The file is too big for us to get it's contents through this API.
                        // The error suggests we use the Git Data API but we need the file sha for
                        // that.
                        // Get all the items in the directory and try to find our file and get the sha
                        // for it so we can update it.
                        let mut path = PathBuf::from(&file_path);
                        path.pop();

                        for item in repo.content().iter(path.to_str().unwrap(), branch).try_collect::<Vec<hubcaps::content::DirectoryItem>>().await.unwrap() {
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
                            let decoded = base64::decode_config(&v, base64::STANDARD).unwrap();
                            return (decoded.trim(), item.sha.to_string());
                        }
                    }
                }
                _ => {
                    println!("[github content] Getting the file at {} on branch {} failed: {:?}", file_path, branch, e);
                }
            }
        }
    }

    // By default return nothing. This only happens if we could not get the file for some reason.
    return (vec![], "".to_string());
}

/// Create or update a file in a GitHub repository.
/// If the file does not exist, it will be created.
/// If the file exists, it will be updated _only if_ the content of the file has changed.
#[instrument(skip(repo))]
#[inline]
pub async fn create_or_update_file_in_github_repo(repo: &Repository, branch: &str, path: &str, new_content: Vec<u8>) {
    let content = new_content.trim();
    // Add the starting "/" so this works.
    // TODO: figure out why it doesn't work without it.
    let mut file_path = path.to_string();
    if !path.starts_with('/') {
        file_path = "/".to_owned() + path;
    }

    // Try to get the content for the file from the repo.
    let (existing_content, sha) = get_file_content_from_repo(repo, branch, path).await;

    if !existing_content.is_empty() {
        if content == existing_content {
            // They are the same so we can return early, we do not need to update the
            // file.
            println!("[github content] File contents at {} are the same, no update needed", file_path);
            return;
        }

        // When the pdfs are generated they change the modified time that is
        // encoded in the file. We want to get that diff and see if it is
        // the only change so that we are not always updating those files.
        let diff = diffy::create_patch_bytes(&existing_content, &content);
        let bdiff = diff.to_bytes();
        let str_diff = from_utf8(&bdiff).unwrap_or("");
        if str_diff.contains("-/ModDate") && str_diff.contains("-/CreationDate") && str_diff.contains("+/ModDate") && str_diff.contains("-/CreationDate") && str_diff.contains("@@ -5,8 +5,8 @@") {
            // The binary contents are the same so we can return early.
            // The only thing that changed was the modified time and creation date.
            println!("[github content] File contents at {} are the same, no update needed", file_path);
            return;
        }

        // We need to update the file. Ignore failure.
        match repo
            .content()
            .update(
                &file_path,
                &content,
                &format!(
                    "Updating file content {} programatically\n\nThis is done from the cio repo utils::create_or_update_file function.",
                    file_path
                ),
                &sha,
                branch,
            )
            .await
        {
            Ok(_) => (),
            Err(e) => {
                println!("[github content] updating file at {} on branch {} failed: {}", file_path, branch, e);
                return;
            }
        }

        println!("[github content] Updated file at {}", file_path);
        return;
    }

    // Create the file in the repo. Ignore failure.
    match repo
        .content()
        .create(
            &file_path,
            &content,
            &format!(
                "Creating file content {} programatically\n\nThis is done from the cio repo utils::create_or_update_file function.",
                file_path
            ),
            branch,
        )
        .await
    {
        Ok(_) => (),
        Err(e) => {
            println!("[github content] creating file at {} on branch {} failed: {}", file_path, branch, e);
            return;
        }
    }

    println!("[github content] Created file at {}", file_path);
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

#[instrument]
#[inline]
pub fn default_date() -> chrono::naive::NaiveDate {
    chrono::naive::NaiveDate::parse_from_str("1970-01-01", "%Y-%m-%d").unwrap()
}

#[cfg(test)]
mod tests {
    use crate::db::Database;
    use crate::models::GithubRepos;
    use crate::utils::{authenticate_github_jwt, refresh_db_github_repos};

    #[ignore]
    #[tokio::test(threaded_scheduler)]
    async fn test_cron_github_repos() {
        let github = authenticate_github_jwt();

        // Initialize our database.
        let db = Database::new();

        refresh_db_github_repos(&db, &github).await;

        GithubRepos::get_from_db(&db).update_airtable().await;
    }
}
