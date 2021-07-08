use hubcaps::repositories::Repository;
use serde::Serialize;

use crate::companies::{Company, Companys};
use crate::configs::Links;
use crate::db::Database;
use crate::repos::GithubRepos;
use crate::rfds::RFDs;
use crate::templates::{generate_nginx_and_terraform_files_for_shorturls, generate_terraform_files_for_shorturls};

/// Generate the files for the GitHub repository short URLs.
pub async fn generate_shorturls_for_repos(db: &Database, repo: &Repository, cio_company_id: i32) {
    let company = Company::get_by_id(db, cio_company_id);
    let subdomain = "git";
    // Initialize the array of links.
    let mut links: Vec<ShortUrl> = Default::default();

    // Get the github repos from the database.
    let repos = GithubRepos::get_from_db(db, cio_company_id);

    // Create the array of links.
    for repo in repos {
        let link = ShortUrl {
            name: repo.name.to_string(),
            description: format!("The GitHub repository at {}/{}", repo.owner.to_string(), repo.name.to_string()),
            link: repo.html_url.to_string(),
            ip: "var.maverick_ip".to_string(),
            subdomain: subdomain.to_string(),
            domain: company.domain.to_string(),
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
pub async fn generate_shorturls_for_rfds(db: &Database, repo: &Repository, cio_company_id: i32) {
    let company = Company::get_by_id(db, cio_company_id);
    let subdomain = "rfd";
    // Initialize the array of links.
    let mut links: Vec<ShortUrl> = Default::default();

    // Get the rfds from the database.
    let rfds = RFDs::get_from_db(db, cio_company_id);
    for rfd in rfds {
        let mut link = ShortUrl {
            name: rfd.number.to_string(),
            description: format!("RFD {} {}", rfd.number_string, rfd.title),
            link: rfd.link,
            ip: "var.maverick_ip".to_string(),
            subdomain: subdomain.to_string(),
            domain: company.domain.to_string(),
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
pub async fn generate_shorturls_for_configs_links(db: &Database, repo: &Repository, cio_company_id: i32) {
    let company = Company::get_by_id(db, cio_company_id);
    let subdomain = "corp";
    // Initialize the array of links.
    let mut links: Vec<ShortUrl> = Default::default();

    // Get the config.
    let configs_links = Links::get_from_db(db, cio_company_id);

    // Create the array of links.
    for link in configs_links {
        let mut l = ShortUrl {
            name: link.name.to_string(),
            description: link.description,
            link: link.link,
            ip: "var.maverick_ip".to_string(),
            subdomain: subdomain.to_string(),
            domain: company.domain.to_string(),
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

/// Generate the cloudflare terraform files for the tailscale devices.
pub async fn generate_dns_for_tailscale_devices(repo: &Repository, company: &Company) {
    let subdomain = "internal";
    // Initialize the array of links.
    let mut links: Vec<ShortUrl> = Default::default();

    // Initialize the Tailscale API.
    let tailscale = company.authenticate_tailscale();

    // Get the devices.
    let devices = tailscale.list_devices().await.unwrap();

    // Create the array of links.
    for device in devices {
        if device.addresses.is_empty() || device.hostname.is_empty() || device.hostname.starts_with("console-git-") {
            // Skip over the domains we generate for the console.
            // Continue early.
            continue;
        }

        let hostname = device
            .name
            .trim()
            .trim_end_matches(".local")
            .trim_end_matches(&company.gsuite_domain)
            .trim_end_matches(&company.domain)
            .trim_end_matches('.')
            .to_lowercase();

        let l = ShortUrl {
            name: hostname.to_string(),
            description: format!("Route for Tailscale IP for {}", hostname),
            link: Default::default(),
            ip: json!(device.addresses.get(0).unwrap()).to_string(),
            subdomain: subdomain.to_string(),
            domain: company.domain.to_string(),
            aliases: Default::default(),
            discussion: Default::default(),
        };

        // Add the link.
        links.push(l.clone());

        if hostname == "cio-api" {
            // Alias this to "api" as well.
            let l = ShortUrl {
                name: "api".to_string(),
                description: format!("Route for Tailscale IP for {}", "api"),
                link: Default::default(),
                ip: json!(device.addresses.get(0).unwrap()).to_string(),
                subdomain: subdomain.to_string(),
                domain: company.domain.to_string(),
                aliases: Default::default(),
                discussion: Default::default(),
            };

            // Add the link.
            links.push(l.clone());
        }
    }

    // Generate the files for the links.
    generate_terraform_files_for_shorturls(repo, links).await;
}

/// Update all the short URLs and DNS.
pub async fn refresh_shorturls() {
    let db = Database::new();

    let companies = Companys::get_from_db(&db, 1);

    // Iterate over the companies and update.
    for company in companies {
        let github = company.authenticate_github();
        let repo = github.repo(&company.github_org, "configs");
        generate_shorturls_for_repos(&db, &repo, company.id).await;
        generate_shorturls_for_rfds(&db, &repo, company.id).await;
        generate_shorturls_for_configs_links(&db, &repo, company.id).await;

        // Only do this if we can auth with Tailscale.
        if !company.tailscale_api_key.is_empty() {
            generate_dns_for_tailscale_devices(&repo, &company).await;
        }
    }
}

/// The data type for a short URL that will be used in a template.
#[derive(Debug, Serialize, Clone)]
pub struct ShortUrl {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    pub description: String,
    pub link: String,
    pub ip: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub subdomain: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub domain: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub discussion: String,
}

#[cfg(test)]
mod tests {
    use crate::shorturls::refresh_shorturls;

    #[ignore]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_shorturls() {
        refresh_shorturls().await;
    }
}
