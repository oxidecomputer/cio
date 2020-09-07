use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use hubcaps::http_cache::FileBasedCache;
use hubcaps::repositories::OrgRepoType;
use hubcaps::repositories::OrganizationRepoListOptions;
use hubcaps::{Credentials, Github};
use reqwest::Client;
use yup_oauth2::{
    read_service_account_key, AccessToken, ServiceAccountAuthenticator,
};

use crate::db::Database;
use crate::models::NewRepo;

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

// Sync the repos with our database.
pub async fn refresh_db_github_repos(github: &Github) {
    let github_repos = list_all_github_repos(github).await;

    // Initialize our database.
    let db = Database::new();

    // Sync github_repos.
    for github_repo in github_repos {
        db.upsert_github_repo(&github_repo);
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
