use anyhow::Result;
use handlebars::{Context, Handlebars, Helper, HelperResult, Output, RenderContext};
use serde::{Deserialize, Serialize};

use crate::{
    configs::User,
    shorturls::ShortUrl,
    utils::{create_or_update_file_in_github_repo, SliceExt},
};

/// Helper function so the terraform names do not start with a number.
/// Otherwise terraform will fail.
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
    if first_char.is_ascii_digit() {
        out.write(&("_".to_owned() + &param.replace('.', "")))?;
    } else {
        out.write(&param.replace('.', ""))?;
    }
    Ok(())
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct GitHubTeamMembers {
    pub team: String,
    pub members: Vec<User>,
}

/// Generate nginx files for shorturls.
/// This is used for short URL link generation like:
///   - {link}.corp.oxide.computer
///   - {repo}.git.oxide.computer
///   - {num}.rfd.oxide.computer
/// This function saves the generated files in the GitHub repository, in the
/// given path.
pub async fn generate_nginx_files_for_shorturls(
    github: &octorust::Client,
    owner: &str,
    repos: &[String],
    shorturls: Vec<ShortUrl>,
) -> Result<()> {
    if shorturls.is_empty() {
        return Ok(());
    }

    // Initialize handlebars.
    let mut handlebars = Handlebars::new();
    handlebars.register_helper("terraformize", Box::new(terraform_name_helper));

    // Get the subdomain from the first link.
    let subdomain = shorturls[0].subdomain.to_string();
    let domain = shorturls[0].domain.to_string();

    // Generate the subdomains nginx file.
    let nginx_file = format!("/nginx/conf.d/generated.{}.{}.conf", subdomain, domain);
    // Add a warning to the top of the file that it should _never_
    // be edited by hand and generate it.
    let mut nginx_rendered = TEMPLATE_WARNING.to_owned() + &handlebars.render_template(TEMPLATE_NGINX, &shorturls)?;
    // Add the vim formating string.
    nginx_rendered += "# vi: ft=nginx";

    for repo in repos {
        create_or_update_file_in_github_repo(
            github,
            owner,
            repo,
            "", // leaving the branch blank gives us the default branch
            &nginx_file,
            nginx_rendered.as_bytes().to_vec().trim(),
        )
        .await?;
    }

    // Generate the paths nginx file.
    let nginx_paths_file = format!("/nginx/conf.d/generated.{}.paths.{}.conf", subdomain, domain);
    // Add a warning to the top of the file that it should _never_
    // be edited by hand and generate it.
    let mut nginx_paths_rendered =
        TEMPLATE_WARNING.to_owned() + &handlebars.render_template(TEMPLATE_NGINX_PATHS, &shorturls)?;
    // Add the vim formating string.
    nginx_paths_rendered += "# vi: ft=nginx";

    for repo in repos {
        create_or_update_file_in_github_repo(
            github,
            owner,
            repo,
            "", // leaving the branch blank gives us the default branch
            &nginx_paths_file,
            nginx_paths_rendered.as_bytes().to_vec().trim(),
        )
        .await?;
    }

    Ok(())
}

/// The warning for files that we automatically generate so folks don't edit them
/// all willy nilly.
pub static TEMPLATE_WARNING: &str = "# THIS FILE HAS BEEN GENERATED BY THE CIO REPO
# AND SHOULD NEVER BE EDITED BY HAND!!
# Instead change the link in configs/links.toml
";

/// Template for creating nginx conf files for the subdomain urls.
pub static TEMPLATE_NGINX: &str = r#"{{#each this}}
# Redirect {{this.link}} to {{this.name}}.{{this.subdomain}}.{{this.domain}}
# Description: {{this.description}}
server {
	listen      80;
	server_name {{this.name}}.{{this.subdomain}};

	# Add redirect.
	location / {
		return 302 {{#if this.discussion}}{{this.link}}$request_uri{{else}}"{{this.link}}"{{/if}};
	}

	{{#if this.discussion}}# Redirect /discussion to {{this.discussion}}
	# Description: Discussion link for {{this.description}}
	location /discussion {
		return 302 {{this.discussion}};
	}
{{/if}}
}

server {
	listen      [::]:443 ssl http2;
	listen      443 ssl http2;
	server_name {{this.name}}.{{this.subdomain}} {{this.name}}.{{this.subdomain}}.{{this.domain}};

	include ssl-params.conf;

	ssl_certificate			/etc/nginx/ssl/wildcard.{{this.subdomain}}.{{this.domain}}/fullchain.pem;
	ssl_certificate_key		/etc/nginx/ssl/wildcard.{{this.subdomain}}.{{this.domain}}/privkey.pem;
	ssl_trusted_certificate	    	/etc/nginx/ssl/wildcard.{{this.subdomain}}.{{this.domain}}/fullchain.pem;

	# Add redirect.
	location / {
		return 302 {{#if this.discussion}}{{this.link}}$request_uri{{else}}"{{this.link}}"{{/if}};
	}

	{{#if this.discussion}}# Redirect /discussion to {{this.discussion}}
	# Description: Discussion link for {{this.description}}
	location /discussion {
		return 302 {{this.discussion}};
	}
{{/if}}
}
{{/each}}
"#;

/// Template for creating nginx conf files for the paths urls.
pub static TEMPLATE_NGINX_PATHS: &str = r#"server {
	listen      80;
	server_name {{this.0.subdomain}};

	location = / {
		return 302 https://119.rfd.{{this.0.domain}};
	}

	{{#each this}}
	# Redirect {{this.subdomain}}.{{this.domain}}/{{this.name}} to {{this.link}}
	# Description: {{this.description}}
	location = /{{this.name}} {
		return 302 "{{this.link}}";
	}
{{#if this.discussion}}	# Redirect /{{this.name}}/discussion to {{this.discussion}}
	# Description: Discussion link for {{this.name}}
	location = /{{this.name}}/discussion {
		return 302 {{this.discussion}};
	}
{{/if}}
{{/each}}
}

server {
	listen      [::]:443 ssl http2;
	listen      443 ssl http2;
	server_name {{this.0.subdomain}} {{this.0.subdomain}}.{{this.0.domain}};

	include ssl-params.conf;

	# Note this certificate is NOT the wildcard, since these are paths.
	ssl_certificate			/etc/nginx/ssl/{{this.0.subdomain}}.{{this.0.domain}}/fullchain.pem;
	ssl_certificate_key		/etc/nginx/ssl/{{this.0.subdomain}}.{{this.0.domain}}/privkey.pem;
	ssl_trusted_certificate	        /etc/nginx/ssl/{{this.0.subdomain}}.{{this.0.domain}}/fullchain.pem;

	location = / {
		return 302 https://119.rfd.{{this.0.domain}};
	}

	{{#each this}}
	# Redirect {{this.subdomain}}.{{this.domain}}/{{this.name}} to {{this.link}}
	# Description: {{this.description}}
	location = /{{this.name}} {
		return 302 "{{this.link}}";
	}
{{#if this.discussion}}	# Redirect /{{this.name}}/discussion to {{this.discussion}}
	# Description: Discussion link for {{this.name}}
	location = /{{this.name}}/discussion {
		return 302 {{this.discussion}};
	}
{{/if}}
{{/each}}
}
"#;
