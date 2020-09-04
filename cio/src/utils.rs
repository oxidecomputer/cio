use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use hubcaps::http_cache::FileBasedCache;
use hubcaps::repositories::OrgRepoType;
use hubcaps::repositories::OrganizationRepoListOptions;
use hubcaps::repositories::Repo as GithubRepo;
use hubcaps::{Credentials, Github};
use reqwest::Client;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use yup_oauth2::{
    read_service_account_key, AccessToken, ServiceAccountAuthenticator,
};

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

#[derive(Debug, Default, Clone, JsonSchema, Deserialize, Serialize)]
pub struct User {
    pub login: String,
    pub id: u64,
    pub avatar_url: String,
    pub gravatar_id: String,
    pub url: String,
    pub html_url: String,
    pub followers_url: String,
    pub following_url: String,
    pub gists_url: String,
    pub starred_url: String,
    pub subscriptions_url: String,
    pub organizations_url: String,
    pub repos_url: String,
    pub events_url: String,
    pub received_events_url: String,
    pub site_admin: bool,
}

#[derive(Debug, Default, Clone, JsonSchema, Deserialize, Serialize)]
pub struct Repo {
    pub id: u64,
    pub owner: User,
    pub name: String,
    pub full_name: String,
    pub description: Option<String>,
    pub private: bool,
    pub fork: bool,
    pub url: String,
    pub html_url: String,
    pub archive_url: String,
    pub assignees_url: String,
    pub blobs_url: String,
    pub branches_url: String,
    pub clone_url: String,
    pub collaborators_url: String,
    pub comments_url: String,
    pub commits_url: String,
    pub compare_url: String,
    pub contents_url: String,
    pub contributors_url: String,
    pub deployments_url: String,
    pub downloads_url: String,
    pub events_url: String,
    pub forks_url: String,
    pub git_commits_url: String,
    pub git_refs_url: String,
    pub git_tags_url: String,
    pub git_url: String,
    pub hooks_url: String,
    pub issue_comment_url: String,
    pub issue_events_url: String,
    pub issues_url: String,
    pub keys_url: String,
    pub labels_url: String,
    pub languages_url: String,
    pub merges_url: String,
    pub milestones_url: String,
    pub mirror_url: Option<String>,
    pub notifications_url: String,
    pub pulls_url: String,
    pub releases_url: String,
    pub ssh_url: String,
    pub stargazers_url: String,
    pub statuses_url: String,
    pub subscribers_url: String,
    pub subscription_url: String,
    pub svn_url: String,
    pub tags_url: String,
    pub teams_url: String,
    pub trees_url: String,
    pub homepage: Option<String>,
    pub language: Option<String>,
    pub forks_count: u64,
    pub stargazers_count: u64,
    pub watchers_count: u64,
    pub size: u64,
    pub default_branch: String,
    pub open_issues_count: u64,
    pub has_issues: bool,
    pub has_wiki: bool,
    pub has_pages: bool,
    pub has_downloads: bool,
    pub archived: bool,
    pub pushed_at: String,
    pub created_at: String,
    pub updated_at: String,
}

impl Repo {
    pub async fn new(r: GithubRepo) -> Self {
        // TODO: get the languages as well
        // https://docs.rs/hubcaps/0.6.1/hubcaps/repositories/struct.Repo.html
        Repo {
            id: r.id,
            owner: User {
                login: r.owner.login,
                id: r.owner.id,
                avatar_url: r.owner.avatar_url,
                gravatar_id: r.owner.gravatar_id,
                url: r.owner.url,
                html_url: r.owner.html_url,
                followers_url: r.owner.followers_url,
                following_url: r.owner.following_url,
                gists_url: r.owner.gists_url,
                starred_url: r.owner.starred_url,
                subscriptions_url: r.owner.subscriptions_url,
                organizations_url: r.owner.organizations_url,
                repos_url: r.owner.repos_url,
                events_url: r.owner.events_url,
                received_events_url: r.owner.received_events_url,
                site_admin: r.owner.site_admin,
            },
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
            forks_count: r.forks_count,
            stargazers_count: r.stargazers_count,
            watchers_count: r.watchers_count,
            size: r.size,
            default_branch: r.default_branch,
            open_issues_count: r.open_issues_count,
            has_issues: r.has_issues,
            has_wiki: r.has_wiki,
            has_pages: r.has_pages,
            has_downloads: r.has_downloads,
            archived: r.archived,
            pushed_at: r.pushed_at,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

/// List all the GitHub repositories for our org.
pub async fn list_all_github_repos(github: &Github) -> Vec<Repo> {
    let github_org = env::var("GITHUB_ORG").unwrap();

    // TODO: paginate.
    let github_repos = github
        .org_repos(github_org)
        .list(
            &OrganizationRepoListOptions::builder()
                .per_page(100)
                .repo_type(OrgRepoType::All)
                .build(),
        )
        .await
        .unwrap();

    let mut repos: Vec<Repo> = Default::default();
    for r in github_repos {
        repos.push(Repo::new(r).await);
    }

    repos
}
