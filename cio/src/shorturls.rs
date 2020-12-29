use hubcaps::repositories::Repository;
use serde::Serialize;
use tracing::instrument;

use crate::db::Database;
use crate::templates::generate_nginx_and_terraform_files_for_shorturls;
use crate::utils::{authenticate_github_jwt, github_org};

/// Generate the files for the GitHub repository short URLs.
#[instrument(skip(repo))]
#[inline]
pub async fn generate_shorturls_for_repos(repo: &Repository) {
    let subdomain = "git";
    // Initialize the array of links.
    let mut links: Vec<ShortUrl> = Default::default();
    let db = Database::new();

    // Get the github repos from the database.
    let repos = db.get_github_repos();

    // Create the array of links.
    for repo in repos {
        let link = ShortUrl {
            name: repo.name.to_string(),
            description: format!("The GitHub repository at {}/{}", repo.owner.login.to_string(), repo.name.to_string()),
            link: repo.html_url.to_string(),
            subdomain: subdomain.to_string(),
            aliases: Default::default(),
            discussion: Default::default(),
        };

        // Add the link.
        links.push(link.clone());
    }

    // Generate the files for the links.
    generate_nginx_and_terraform_files_for_shorturls(repo, links.clone()).await;
}

/// Generate the files for the RFD short URLs.
#[instrument(skip(repo))]
#[inline]
pub async fn generate_shorturls_for_rfds(repo: &Repository) {
    let subdomain = "rfd";
    // Initialize the array of links.
    let mut links: Vec<ShortUrl> = Default::default();
    let db = Database::new();

    // Get the rfds from the database.
    let rfds = db.get_rfds();
    for rfd in rfds {
        let mut link = ShortUrl {
            name: rfd.number.to_string(),
            description: format!("RFD {} {}", rfd.number_string, rfd.title),
            link: rfd.link,
            subdomain: subdomain.to_string(),
            aliases: Default::default(),
            discussion: rfd.discussion,
        };

        // Add the link.
        links.push(link.clone());

        // Add the number string as well with leading zeroes.
        link.name = rfd.number_string.to_string();
        links.push(link.clone());
    }

    // Generate the files for the links.
    generate_nginx_and_terraform_files_for_shorturls(repo, links.clone()).await;
}

/// Generate the files for the configs links.
#[instrument(skip(repo))]
#[inline]
pub async fn generate_shorturls_for_configs_links(repo: &Repository) {
    let subdomain = "corp";
    // Initialize the array of links.
    let mut links: Vec<ShortUrl> = Default::default();
    let db = Database::new();

    // Get the config.
    let configs_links = db.get_links();

    // Create the array of links.
    for link in configs_links {
        let mut l = ShortUrl {
            name: link.name.to_string(),
            description: link.description,
            link: link.link,
            subdomain: subdomain.to_string(),
            aliases: Default::default(),
            discussion: Default::default(),
        };

        // Add the link.
        links.push(l.clone());

        // Add any aliases.
        for alias in link.aliases {
            // Set the name.
            l.name = alias;

            // Add the link.
            links.push(l.clone());
        }
    }

    // Generate the files for the links.
    generate_nginx_and_terraform_files_for_shorturls(repo, links).await;
}

/// Update all the short URLs.
#[instrument]
#[inline]
pub async fn refresh_shorturls() {
    let github = authenticate_github_jwt();
    let repo = github.repo(github_org(), "configs");

    generate_shorturls_for_repos(&repo).await;
    generate_shorturls_for_rfds(&repo).await;
    generate_shorturls_for_configs_links(&repo).await;
}

/// The data type for a short URL that will be used in a template.
#[derive(Debug, Serialize, Clone)]
pub struct ShortUrl {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    pub description: String,
    pub link: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub subdomain: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub discussion: String,
}

#[cfg(test)]
mod tests {
    use crate::shorturls::refresh_shorturls;

    #[ignore]
    #[tokio::test(threaded_scheduler)]
    async fn test_cron_shorturls() {
        refresh_shorturls().await;
    }
}
