use std::env;

use clap::ArgMatches;
use handlebars::{Context, Handlebars, Helper, HelperResult, Output, RenderContext};
use hubcaps::repositories::{OrgRepoType, OrganizationRepoListOptions};
use log::warn;
use tokio::runtime::Runtime;

use crate::core::LinkConfig;
use crate::utils::{
    authenticate_github, get_rfds_from_repo, read_config_from_files, write_file, TEMPLATE_WARNING,
};

pub fn cmd_shorturls_run(cli_matches: &ArgMatches) {
    // Initialize the array of links.
    let mut links: Vec<LinkConfig> = Default::default();

    // Initialize Github and the runtime.
    let github = authenticate_github();
    let github_org = env::var("GITHUB_ORG").unwrap();
    let mut runtime = Runtime::new().unwrap();

    /* REPO LINKS GENERATION */
    let mut subdomain = "git";
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

    // Create the array of links.
    for repo in repos {
        let link = LinkConfig {
            name: Some(repo.name.to_string()),
            description: format!(
                "The GitHub repository at {}/{}",
                github_org.to_string(),
                repo.name.to_string()
            ),
            link: repo.html_url,
            subdomain: Some(subdomain.to_string()),
            aliases: None,
            discussion: None,
        };

        // Add the link.
        links.push(link.clone());
    }

    // Generate the files for the links.
    generate_files_for_links(links.clone());

    /* RFD LINKS GENERATION */
    subdomain = "rfd";
    // Reset the links array.
    links = Default::default();

    // Get the rfds from our the repo.
    let rfds = get_rfds_from_repo(github);
    for (_, rfd) in rfds {
        let link = LinkConfig {
            name: Some(rfd.number.to_string()),
            description: format!("RFD {} {}", rfd.number, rfd.title),
            link: rfd.link,
            subdomain: Some(subdomain.to_string()),
            aliases: None,
            discussion: Some(rfd.discussion),
        };

        // Add the link.
        links.push(link.clone());
    }

    // Generate the files for the links.
    generate_files_for_links(links.clone());

    /* CORP LINKS GENERATION */
    subdomain = "corp";
    // Reset the links array.
    links = Default::default();

    // Get the config.
    let config = read_config_from_files(cli_matches);

    // Create the array of links.
    for (name, link) in config.links {
        let mut l = link.clone();
        // Set the name.
        l.name = Some(name.to_string());
        // Set the subdomain.
        l.subdomain = Some(subdomain.to_string());

        // Add the link.
        links.push(l.clone());

        // Add any aliases.
        match link.aliases {
            Some(aliases) => {
                for alias in aliases {
                    // Set the name.
                    l.name = Some(alias);

                    // Add the link.
                    links.push(l.clone());
                }
            }
            None => (),
        }
    }

    // Generate the files for the links.
    generate_files_for_links(links.clone());
}

// Helper function so the terraform names do not start with a number.
// Otherwise terraform will fail.
fn terraform_name_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _rc: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let p = h.param(0).unwrap().value().to_string();
    let param = p.trim_matches('"');

    // Check if the first character is a number.
    let first_char = param.chars().next().unwrap();
    let numbers: Vec<char> = vec!['0', '1', '2', '3', '4', '5', '6', '7', '8', '9'];

    if numbers.contains(&first_char) {
        out.write(&("_".to_owned() + &param))?;
    } else {
        out.write(&param)?;
    }
    Ok(())
}

fn generate_files_for_links(links: Vec<LinkConfig>) {
    if links.len() < 1 {
        warn!("no links in array");
        return;
    }

    // Initialize handlebars.
    let mut handlebars = Handlebars::new();
    handlebars.register_helper("terraformize", Box::new(terraform_name_helper));

    // Get the subdomain from the first link.
    let subdomain = links[0].subdomain.as_ref().unwrap();

    // Get the current working directory.
    let curdir = env::current_dir().unwrap();

    // Generate the subdomains nginx file.
    let nginx_file = curdir.join(format!(
        "nginx/conf.d/generated.{}.oxide.computer.conf",
        subdomain
    ));
    // Add a warning to the top of the file that it should _never_
    // be edited by hand and generate it.
    let mut nginx_rendered =
        TEMPLATE_WARNING.to_owned() + &handlebars.render_template(&TEMPLATE_NGINX, &links).unwrap();
    // Add the vim formating string.
    nginx_rendered += "# vi: ft=nginx";

    write_file(nginx_file, nginx_rendered);

    // Generate the paths nginx file.
    let nginx_paths_file = curdir.join(format!(
        "nginx/conf.d/generated.{}.paths.oxide.computer.conf",
        subdomain
    ));
    // Add a warning to the top of the file that it should _never_
    // be edited by hand and generate it.
    let mut nginx_paths_rendered = TEMPLATE_WARNING.to_owned()
        + &handlebars
            .render_template(&TEMPLATE_NGINX_PATHS, &links)
            .unwrap();
    // Add the vim formating string.
    nginx_paths_rendered += "# vi: ft=nginx";

    write_file(nginx_paths_file, nginx_paths_rendered);

    // Generate the terraform file.
    let terraform_file = env::current_dir().unwrap().join(format!(
        "terraform/cloudflare/generated.{}.oxide.computer.tf",
        subdomain
    ));
    // Add a warning to the top of the file that it should _never_
    // be edited by hand and generate it.
    let terraform_rendered = TEMPLATE_WARNING.to_owned()
        + &handlebars
            .render_template(&TEMPLATE_CLOUDFLARE_TERRAFORM, &links)
            .unwrap();

    write_file(terraform_file, terraform_rendered);
}

static TEMPLATE_NGINX: &'static str = "{{#each this}}
# Redirect {{this.link}} to {{this.name}}.{{this.subdomain}}.oxide.computer
# Description: {{this.description}}
server {
	listen      [::]:443 ssl http2;
	listen      443 ssl http2;
	server_name {{this.name}}.{{this.subdomain}}.oxide.computer;

	include ssl-params.conf;

	ssl_certificate			/mnt/disks/letsencrypt/live/{{this.subdomain}}.oxide.computer/fullchain.pem;
	ssl_certificate_key		/mnt/disks/letsencrypt/live/{{this.subdomain}}.oxide.computer/privkey.pem;
	ssl_trusted_certificate	/mnt/disks/letsencrypt/live/{{this.subdomain}}.oxide.computer/fullchain.pem;

	# Add redirect.
	location / {
		return 301 {{this.link}};
	}

	{{#if this.discussion}}# Redirect /discussion to {{this.discussion}}
	# Description: Discussion link for {{this.description}}
	location /discussion {
		return 301 {{this.discussion}};
	}
{{/if}}
	root /etc/nginx/static/other;

	location ~ /.well-known {
		allow all;
	}
}
{{/each}}
";

static TEMPLATE_NGINX_PATHS: &'static str = "server {
	listen      [::]:443 ssl http2;
	listen      443 ssl http2;
	server_name {{this.0.subdomain}}.oxide.computer;

	include ssl-params.conf;

	ssl_certificate			/mnt/disks/letsencrypt/live/{{this.0.subdomain}}.oxide.computer-0001/fullchain.pem;
	ssl_certificate_key		/mnt/disks/letsencrypt/live/{{this.0.subdomain}}.oxide.computer-0001/privkey.pem;
	ssl_trusted_certificate	/mnt/disks/letsencrypt/live/{{this.0.subdomain}}.oxide.computer-0001/fullchain.pem;

	location = / {
		return 301 https://github.com/oxidecomputer/meta/tree/master/links;
	}

	{{#each this}}
	# Redirect {{this.subdomain}}.oxide.computer/{{this.name}} to {{this.link}}
	# Description: {{this.description}}
	location = /{{this.name}} {
		return 301 {{this.link}};
	}
{{#if this.discussion}}	# Redirect /{{this.name}}/discussion to {{this.discussion}}
	# Description: Discussion link for {{this.name}}
	location = /{{this.name}}/discussion {
		return 301 {{this.discussion}};
	}
{{/if}}
{{/each}}
	root /etc/nginx/static/other;

	location ~ /.well-known {
		allow all;
	}
}
";

static TEMPLATE_CLOUDFLARE_TERRAFORM: &'static str = r#"{{#each this}}
resource "cloudflare_record" "{{terraformize this.name}}_{{this.subdomain}}_oxide_computer" {
  zone_id  = var.zone_id-oxide_computer
  name     = "{{this.name}}.{{this.subdomain}}.oxide.computer"
  value    = var.maverick_ip
  type     = "A"
  ttl      = 1
  priority = 0
  proxied  = false
}
{{/each}}
"#;
