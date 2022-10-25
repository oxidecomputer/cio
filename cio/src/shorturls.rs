use anyhow::{bail, Result};
use serde::Serialize;

use crate::{
    companies::Company,
    configs::Links,
    db::Database,
    dns_providers::{DNSProviderOps, DnsRecord, DnsRecordType, DnsUpdateMode},
    repos::GithubRepos,
    rfd::RFDs,
    templates::generate_nginx_files_for_shorturls,
};

/// Generate the files for the GitHub repository short URLs.
pub async fn generate_shorturls_for_repos<C>(
    db: &Database,
    github: &octorust::Client,
    company: &Company,
    dns: &C,
    repo: &str,
) -> Result<()>
where
    C: DNSProviderOps,
{
    let owner = &company.github_org;
    let subdomain = "git";
    // Initialize the array of links.
    let mut links: Vec<ShortUrl> = Default::default();

    // Get the github repos from the database.
    let repos = GithubRepos::get_from_db(db, company.id).await?;

    // Create the array of links.
    for repo in repos {
        let link = ShortUrl {
            name: repo.name.to_string(),
            description: format!("The GitHub repository at {}/{}", repo.owner, repo.name),
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

    log::info!("Collected {} repo links to check", links.len());

    // Generate the files for the links.
    generate_nginx_files_for_shorturls(github, owner, repo, links.clone()).await?;

    create_dns_records_for_links(dns, company, links).await?;

    Ok(())
}

/// Generate the files for the RFD short URLs.
pub async fn generate_shorturls_for_rfds<C>(
    db: &Database,
    github: &octorust::Client,
    company: &Company,
    dns: &C,
    repo: &str,
) -> Result<()>
where
    C: DNSProviderOps,
{
    let owner = &company.github_org;
    let subdomain = "rfd";
    // Initialize the array of links.
    let mut links: Vec<ShortUrl> = Default::default();

    // Get the rfds from the database.
    let rfds = RFDs::get_from_db(db, company.id).await?;
    for rfd in rfds {
        let mut link = ShortUrl {
            name: rfd.number.to_string(),
            description: format!("RFD {} {}", rfd.number_string, rfd.title),
            link: rfd
                .link
                .trim_end_matches("README.adoc")
                .trim_end_matches("README.md")
                .to_string(),
            ip: "var.maverick_ip".to_string(),
            subdomain: subdomain.to_string(),
            domain: company.domain.to_string(),
            aliases: Default::default(),
            discussion: rfd.discussion,
        };

        if !link.discussion.starts_with("https://") && !link.discussion.is_empty() {
            log::warn!("RFD {} has bad discussion URL: {}", rfd.number, link.discussion);
            continue;
        }

        // Add the link.
        links.push(link.clone());

        // Add the number string as well with leading zeroes.
        link.name = rfd.number_string.to_string();
        links.push(link.clone());
    }

    log::info!("Collected {} rfd links to check", links.len());

    // Generate the files for the links.
    generate_nginx_files_for_shorturls(github, owner, repo, links.clone()).await?;

    create_dns_records_for_links(dns, company, links).await?;

    Ok(())
}

/// Generate the files for the configs links.
pub async fn generate_shorturls_for_configs_links<C>(
    db: &Database,
    github: &octorust::Client,
    company: &Company,
    dns: &C,
    repo: &str,
) -> Result<()>
where
    C: DNSProviderOps,
{
    let owner = &company.github_org;
    let subdomain = "corp";
    // Initialize the array of links.
    let mut links: Vec<ShortUrl> = Default::default();

    // Get the config.
    let configs_links = Links::get_from_db(db, company.id).await?;

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
            // Skip it if they repeated the name as an alias.
            if alias == link.name {
                continue;
            }

            // Set the name.
            l.name = alias;

            // Add the link.
            links.push(l.clone());
        }
    }

    log::info!("Collected {} config links to check", links.len());

    // Generate the files for the links.
    generate_nginx_files_for_shorturls(github, owner, repo, links.clone()).await?;

    create_dns_records_for_links(dns, company, links).await?;

    Ok(())
}

/// Generate the cloudflare terraform files for the tailscale devices.
pub async fn generate_dns_for_tailscale_devices<C>(company: &Company, dns: &C) -> Result<()>
where
    C: DNSProviderOps,
{
    let subdomain = "internal";
    // Initialize the array of links.
    let mut links: Vec<ShortUrl> = Default::default();

    // Initialize the Tailscale API.
    let tailscale = company.authenticate_tailscale();

    // Get the devices.
    let devices = tailscale.list_devices().await?;

    // Create the array of links.
    for device in devices {
        if device.addresses.is_empty()
            || device.hostname.is_empty()
            || device.hostname.contains("-git-")
            || device.hostname.starts_with("github-")
        {
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

    create_dns_records_for_links(dns, company, links).await?;

    Ok(())
}

/// Update all the short URLs and DNS.
pub async fn refresh_shorturls(db: &Database, company: &Company) -> Result<()> {
    let github = company.authenticate_github()?;
    let provider = company.authenticate_dns_providers().await?;

    generate_shorturls_for_repos(db, &github, company, &provider, "configs").await?;
    generate_shorturls_for_rfds(db, &github, company, &provider, "configs").await?;
    generate_shorturls_for_configs_links(db, &github, company, &provider, "configs").await?;

    Ok(())
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

async fn create_dns_records_for_links<C>(dns_client: &C, company: &Company, shorturls: Vec<ShortUrl>) -> Result<()>
where
    C: DNSProviderOps,
{
    for s in shorturls {
        // Make sure the name does not start with a dot ".".
        let mut name = if s.name.starts_with('.') {
            s.name.trim_start_matches('.').to_string()
        } else {
            s.name.to_string()
        };

        name = format!("{}.{}.{}", name, s.subdomain, company.domain);
        if dns_client
            .ensure_record(
                DnsRecord {
                    name: name.to_string(),
                    type_: DnsRecordType::A,
                    content: company.nginx_ip.parse()?,
                },
                DnsUpdateMode::Replace,
            )
            .await
            .is_err()
        {
            // Try it again, it might just have been a time out error.
            if let Err(e) = dns_client
                .ensure_record(
                    DnsRecord {
                        name: name.to_string(),
                        type_: DnsRecordType::A,
                        content: company.nginx_ip.parse()?,
                    },
                    DnsUpdateMode::Replace,
                )
                .await
            {
                bail!("Error creating DNS record for `{}`: {}", name, e);
            }
        }
    }

    Ok(())
}
