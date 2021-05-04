use std::collections::BTreeMap;

use handlebars::{Context, Handlebars, Helper, HelperResult, Output, RenderContext};
use hubcaps::repositories::Repository;
use hubcaps::Github;
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::configs::{Groups, UserConfig, Users};
use crate::db::Database;
use crate::shorturls::ShortUrl;
use crate::utils::{create_or_update_file_in_github_repo, github_org};

/// Helper function so the terraform names do not start with a number.
/// Otherwise terraform will fail.
fn terraform_name_helper(h: &Helper, _: &Handlebars, _: &Context, _rc: &mut RenderContext, out: &mut dyn Output) -> HelperResult {
    let p = h.param(0).unwrap().value().to_string();
    let param = p.trim_matches('"');

    // Check if the first character is a number.
    let first_char = param.chars().next().unwrap();
    if first_char.is_digit(10) {
        out.write(&("_".to_owned() + param))?;
    } else {
        out.write(&param)?;
    }
    Ok(())
}

/// Helper function so the terraform usernames do not have a period.
/// Otherwise terraform will fail.
#[allow(clippy::unnecessary_wraps)]
fn terraform_username_helper(h: &Helper, _: &Handlebars, _: &Context, _rc: &mut RenderContext, out: &mut dyn Output) -> HelperResult {
    let p = h.param(0).unwrap().value().to_string();
    let param = p.trim_matches('"');

    out.write(&param.replace(".", "")).unwrap();
    Ok(())
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct GitHubTeamMembers {
    pub team: String,
    pub members: Vec<UserConfig>,
}

/**
 * Generate Okta terraform configs that configure members of the
 * organization, groups, and group membership. We use terraform instead of calling out the
 * api ourselves because the diffs of the files after changes are more readable
 * than not having that functionality at all.
 *
 * This function uses the users.toml and the groups.toml file in the configs repo for information.
 */
#[instrument(skip(db))]
#[inline]
pub async fn generate_terraform_files_for_okta(github: &Github, db: &Database) {
    let users = Users::get_from_db(db);
    let groups = Groups::get_from_db(db);

    let repo = github.repo(github_org(), "configs");
    let r = repo.get().await.unwrap();

    // Set the paths for the files.
    let okta_path = "terraform/okta";

    // Initialize handlebars.
    let mut handlebars = Handlebars::new();
    handlebars.register_helper("terraformize", Box::new(terraform_username_helper));

    // Generate the members of the users file.
    let users_rendered = handlebars.render_template(&TEMPLATE_TERRAFORM_OKTA_USER, &users).unwrap();

    // Join it with the directory to save the files in.
    let users_file = format!("{}/generated.users.tf", okta_path);

    create_or_update_file_in_github_repo(&repo, &r.default_branch, &users_file, users_rendered.as_bytes().to_vec()).await;

    // Generate the members of the groups file.
    let groups_rendered = handlebars.render_template(&TEMPLATE_TERRAFORM_OKTA_GROUP, &groups).unwrap();

    // Join it with the directory to save the files in.
    let groups_file = format!("{}/generated.groups.tf", okta_path);

    create_or_update_file_in_github_repo(&repo, &r.default_branch, &groups_file, groups_rendered.as_bytes().to_vec()).await;
}

/**
 * Generate GitHub and AWS terraform configs that configure members of the
 * organization and team membership. We use terraform instead of calling out the
 * api ourselves because the diffs of the files after changes are more readable
 * than not having that functionality at all.
 *
 * This function uses the users.toml file in the configs repo for information.
 */
#[instrument]
#[inline]
pub async fn generate_terraform_files_for_aws_and_github(github: &Github, users: BTreeMap<String, UserConfig>) {
    let repo = github.repo(github_org(), "configs");
    let r = repo.get().await.unwrap();

    // Set the paths for the files.
    let github_path = "terraform/github";
    let aws_path = "terraform/aws";

    // Initialize handlebars.
    let mut handlebars = Handlebars::new();
    handlebars.register_helper("terraformize", Box::new(terraform_username_helper));

    // Generate the members of the GitHub org file.
    let github_rendered = handlebars.render_template(&TEMPLATE_TERRAFORM_GITHUB_ORG_MEMBERSHIP, &users).unwrap();

    // Join it with the directory to save the files in.
    let github_file = format!("{}/generated.organization-members.tf", github_path);

    create_or_update_file_in_github_repo(&repo, &r.default_branch, &github_file, github_rendered.as_bytes().to_vec()).await;

    // Generate the members of the AWS org file.
    let aws_rendered = handlebars.render_template(&TEMPLATE_TERRAFORM_AWS_ORG_MEMBERSHIP, &users).unwrap();

    // Join it with the directory to save the files in.
    let aws_file = format!("{}/generated.organization-members.tf", aws_path);

    create_or_update_file_in_github_repo(&repo, &r.default_branch, &aws_file, aws_rendered.as_bytes().to_vec()).await;

    // Generate the members of each GitHub team.
    // TODO: don't hard code these
    let teams = vec!["all", "eng", "consultants"];
    for team in teams {
        // Build the members array.
        let mut members: Vec<UserConfig> = Default::default();
        for user in users.values() {
            if user.groups.contains(&team.to_string()) {
                members.push(user.clone());
            }
        }

        // Generate the members of the team file.
        let rendered = handlebars
            .render_template(&TEMPLATE_TERRAFORM_GITHUB_TEAM_MEMBERSHIP, &GitHubTeamMembers { team: team.to_string(), members })
            .unwrap();

        // Join it with the directory to save the files in.
        let file = format!("{}/generated.team-members-{}.tf", github_path, team.to_string());

        create_or_update_file_in_github_repo(&repo, &r.default_branch, &file, rendered.as_bytes().to_vec()).await;
    }
}

/// Generate nginx and terraform files for shorturls.
/// This is used for short URL link generation like:
///   - {link}.corp.oxide.computer
///   - {repo}.git.oxide.computer
///   - {num}.rfd.oxide.computer
/// This function saves the generated files in the GitHub repository, in the
/// given path.
#[instrument(skip(repo))]
#[inline]
pub async fn generate_nginx_and_terraform_files_for_shorturls(repo: &Repository, shorturls: Vec<ShortUrl>) {
    if shorturls.is_empty() {
        println!("no shorturls in array");
        return;
    }

    let r = repo.get().await.unwrap();

    // Initialize handlebars.
    let mut handlebars = Handlebars::new();
    handlebars.register_helper("terraformize", Box::new(terraform_name_helper));

    // Get the subdomain from the first link.
    let subdomain = shorturls[0].subdomain.to_string();

    // Generate the subdomains nginx file.
    let nginx_file = format!("/nginx/conf.d/generated.{}.oxide.computer.conf", subdomain);
    // Add a warning to the top of the file that it should _never_
    // be edited by hand and generate it.
    let mut nginx_rendered = TEMPLATE_WARNING.to_owned() + &handlebars.render_template(&TEMPLATE_NGINX, &shorturls).unwrap();
    // Add the vim formating string.
    nginx_rendered += "# vi: ft=nginx";

    create_or_update_file_in_github_repo(repo, &r.default_branch, &nginx_file, nginx_rendered.as_bytes().to_vec()).await;

    // Generate the paths nginx file.
    let nginx_paths_file = format!("/nginx/conf.d/generated.{}.paths.oxide.computer.conf", subdomain);
    // Add a warning to the top of the file that it should _never_
    // be edited by hand and generate it.
    let mut nginx_paths_rendered = TEMPLATE_WARNING.to_owned() + &handlebars.render_template(&TEMPLATE_NGINX_PATHS, &shorturls).unwrap();
    // Add the vim formating string.
    nginx_paths_rendered += "# vi: ft=nginx";

    create_or_update_file_in_github_repo(repo, &r.default_branch, &nginx_paths_file, nginx_paths_rendered.as_bytes().to_vec()).await;

    generate_terraform_files_for_shorturls(repo, shorturls).await;
}

/// Generate terraform files for shorturls.
/// This function saves the generated files in the GitHub repository, in the
/// given path.
#[instrument(skip(repo))]
#[inline]
pub async fn generate_terraform_files_for_shorturls(repo: &Repository, shorturls: Vec<ShortUrl>) {
    if shorturls.is_empty() {
        println!("no shorturls in array");
        return;
    }

    let r = repo.get().await.unwrap();

    // Initialize handlebars.
    let mut handlebars = Handlebars::new();
    handlebars.register_helper("terraformize", Box::new(terraform_name_helper));

    // Get the subdomain from the first link.
    let subdomain = shorturls[0].subdomain.to_string();

    // Generate the terraform file.
    let terraform_file = format!("/terraform/cloudflare/generated.{}.oxide.computer.tf", subdomain);
    // Add a warning to the top of the file that it should _never_
    // be edited by hand and generate it.
    let terraform_rendered = TEMPLATE_WARNING.to_owned() + &handlebars.render_template(&TEMPLATE_CLOUDFLARE_TERRAFORM, &shorturls).unwrap();

    create_or_update_file_in_github_repo(repo, &r.default_branch, &terraform_file, terraform_rendered.as_bytes().to_vec()).await;
}

/// The warning for files that we automatically generate so folks don't edit them
/// all willy nilly.
pub static TEMPLATE_WARNING: &str = "# THIS FILE HAS BEEN GENERATED BY THE CIO REPO
# AND SHOULD NEVER BE EDITED BY HAND!!
# Instead change the link in configs/links.toml
";

/// Template for creating nginx conf files for the subdomain urls.
pub static TEMPLATE_NGINX: &str = r#"{{#each this}}
# Redirect {{this.link}} to {{this.name}}.{{this.subdomain}}.oxide.computer
# Description: {{this.description}}
server {
	listen      [::]:443 ssl http2;
	listen      443 ssl http2;
	server_name {{this.name}}.{{this.subdomain}}.oxide.computer;

	include ssl-params.conf;

	ssl_certificate			/etc/nginx/ssl/wildcard.{{this.subdomain}}.oxide.computer/fullchain.pem;
	ssl_certificate_key		/etc/nginx/ssl/wildcard.{{this.subdomain}}.oxide.computer/privkey.pem;
	ssl_trusted_certificate	    	/etc/nginx/ssl/wildcard.{{this.subdomain}}.oxide.computer/fullchain.pem;

	# Add redirect.
	location / {
		return 301 "{{this.link}}";
	}

	{{#if this.discussion}}# Redirect /discussion to {{this.discussion}}
	# Description: Discussion link for {{this.description}}
	location /discussion {
		return 301 {{this.discussion}};
	}
{{/if}}
}
{{/each}}
"#;

/// Template for creating nginx conf files for the paths urls.
pub static TEMPLATE_NGINX_PATHS: &str = r#"server {
	listen      [::]:443 ssl http2;
	listen      443 ssl http2;
	server_name {{this.0.subdomain}}.oxide.computer;

	include ssl-params.conf;

	# Note this certificate is NOT the wildcard, since these are paths.
	ssl_certificate			/etc/nginx/ssl/{{this.0.subdomain}}.oxide.computer/fullchain.pem;
	ssl_certificate_key		/etc/nginx/ssl/{{this.0.subdomain}}.oxide.computer/privkey.pem;
	ssl_trusted_certificate	        /etc/nginx/ssl/{{this.0.subdomain}}.oxide.computer/fullchain.pem;

	location = / {
		return 301 https://github.com/oxidecomputer/meta/tree/master/links;
	}

	{{#each this}}
	# Redirect {{this.subdomain}}.oxide.computer/{{this.name}} to {{this.link}}
	# Description: {{this.description}}
	location = /{{this.name}} {
		return 301 "{{this.link}}";
	}
{{#if this.discussion}}	# Redirect /{{this.name}}/discussion to {{this.discussion}}
	# Description: Discussion link for {{this.name}}
	location = /{{this.name}}/discussion {
		return 301 {{this.discussion}};
	}
{{/if}}
{{/each}}
}
"#;

/// Template for creating DNS records in our Cloudflare terraform configs.
pub static TEMPLATE_CLOUDFLARE_TERRAFORM: &str = r#"{{#each this}}
resource "cloudflare_record" "{{terraformize this.name}}_{{this.subdomain}}_oxide_computer" {
  zone_id  = var.zone_id-oxide_computer
  name     = "{{this.name}}.{{this.subdomain}}.oxide.computer"
  value    = {{{this.ip}}}
  type     = "A"
  ttl      = 1
  priority = 0
  proxied  = false
}
{{/each}}
"#;

/// Template for terraform GitHub org membership.
pub static TEMPLATE_TERRAFORM_GITHUB_ORG_MEMBERSHIP: &str = r#"# THIS IS A GENERATED FILE, DO NOT EDIT THIS FILE DIRECTLY.
# Define the members of the organization.
{{#each this}}{{#if this.github}}
# Add @{{this.github}} to the organization.
resource "github_membership" "{{this.github}}" {
  username = "{{this.github}}"
  role     = "{{#if this.is_group_admin}}admin{{else}}member{{/if}}"
}
{{/if}}{{/each}}
"#;

/// Template for terraform GitHub team membership.
pub static TEMPLATE_TERRAFORM_GITHUB_TEAM_MEMBERSHIP: &str = r#"# THIS IS A GENERATED FILE, DO NOT EDIT THIS FILE DIRECTLY.
# Define the members of the {{this.team}} team.
{{#each this.members}}{{#if this.github}}
# Add @{{this.github}} to {{../team}}.
resource "github_team_membership" "{{../team}}-{{this.github}}" {
  team_id  = github_team.{{../team}}.id
  username = "{{this.github}}"
  role     = "{{#if this.is_group_admin}}maintainer{{else}}member{{/if}}"
}
{{/if}}{{/each}}
"#;

/// Template for terraform AWS org membership.
pub static TEMPLATE_TERRAFORM_AWS_ORG_MEMBERSHIP: &str = r#"# THIS IS A GENERATED FILE, DO NOT EDIT THIS FILE DIRECTLY.
# Define the members of the organization.
{{#each this}}{{#if this.github}}
# Add @{{this.username}} to the organization.
resource "aws_iam_user" "{{terraformize this.username}}" {
  name = "{{terraformize this.username}}"
  path = "/users/"
}
# Add @{{this.username}} to the relevant groups.
resource "aws_iam_user_group_membership" "eng-{{terraformize this.username}}" {
  user = aws_iam_user.{{terraformize this.username}}.name
  groups = [
    aws_iam_group.eng.name,
    aws_iam_group.everyone.name
  ]
}
{{/if}}{{/each}}
"#;

/// Template for terraform Okta users.
pub static TEMPLATE_TERRAFORM_OKTA_USER: &str = r#"# THIS IS A GENERATED FILE, DO NOT EDIT THIS FILE DIRECTLY.
# Define the members of the organization.
{{#each this}}
# Add {{this.username}}@ to the organization.
resource "okta_user" "{{terraformize this.username}}" {
  first_name                = "{{this.first_name}}"
  last_name                 = "{{this.last_name}}"
  login                     = "{{this.username}}@oxidecomputer.com"
  email                     = "{{this.username}}@oxidecomputer.com"
  display_name              = "{{this.first_name}} {{this.last_name}}"{{#if this.recovery_email}}
  mobile_phone              = "{{this.recovery_phone}}"
  primary_phone             = "{{this.recovery_phone}}"{{/if}}{{#if this.recovery_email}}
  second_email              = "{{this.recovery_email}}"{{/if}}


  department         = "{{this.department}}"
  organization       = "Oxide Computer Company"
  manager            = "{{this.manager}}@oxidecomputer.com"

  {{#if this.home_address_formatted}}postal_address     = <<EOT
{{this.home_address_formatted}}
EOT
  {{/if}}street_address     = "{{this.home_address_street_1}}{{#if this.home_address_street_2}} {{this.home_address_street_2}}{{/if}}"
  city               = "{{this.home_address_city}}"
  state              = "{{this.home_address_state}}"
  zip_code           = "{{this.home_address_zipcode}}"
  country_code       = "{{this.home_address_country_code}}"{{#if (eq this.username "jess")}}

  admin_roles = [
    "SUPER_ADMIN",
  ]{{/if}}

  custom_profile_attributes = <<EOT
{
    "githubUsername": "{{this.github}}",
    "matrixUsername": "{{this.chat}}",
    "awsRole": "{{this.aws_role}}",
    "startDate": "{{this.start_date}}",
    "birthday": "{{this.birthday}}",
    "emailAliases": [{{#each this.aliases}}"{{this}}@oxidecomputer.com"{{#if @last}}{{else}},{{/if}}{{/each}}],
    "workPostalAddress": "{{this.work_address_formatted}}",
    "workStreetAddress": "{{this.work_address_street_1}}{{#if this.work_address_street_2}} {{this.work_address_street_2}}{{/if}}",
    "workCity": "{{this.work_address_city}}",
    "workState": "{{this.work_address_state}}",
    "workZipCode": "{{this.work_address_zipcode}}",
    "workCountryCode": "{{this.work_address_country_code}}"
}
EOT
}
{{#each this.groups}}
# Add {{../username}}@ to the {{this}}@ group.
resource "okta_group_membership" "{{terraformize ../username}}-{{terraformize this}}" {
  group_id = okta_group.{{terraformize this}}.id
  user_id  = okta_user.{{terraformize ../username}}.id
}
{{/each}}{{/each}}
"#;

/// Template for terraform Okta groups.
pub static TEMPLATE_TERRAFORM_OKTA_GROUP: &str = r#"# THIS IS A GENERATED FILE, DO NOT EDIT THIS FILE DIRECTLY.
# Define the groups in the organization.
{{#each this}}{{#if (eq this.name "everyone")}}{{else}}
# Add {{this.name}}@ as a group in the organization.
resource "okta_group" "{{terraformize this.name}}" {
  name        = "{{this.name}}"
  description = "{{this.description}}"
}
{{/if}}{{/each}}
"#;
