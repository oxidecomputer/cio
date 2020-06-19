use std::collections::BTreeMap;
use std::env;

use chrono::offset::Utc;
use chrono::DateTime;
use clap::{value_t, ArgMatches};
use hubcaps::issues::{Issue, IssueListOptions, IssueOptions, State};
use log::info;

use crate::core::{Applicant, SheetColumns};
use crate::slack::{post_to_channel, HIRING_CHANNEL_POST_URL};
use crate::utils::{authenticate_github, get_gsuite_token};

use sendgrid::SendGrid;
use sheets::Sheets;

/**
 * Generate GitHub issues for various stages of the application process
 * based on their status in a Google Sheet.
 *
 * When a new application is submitted by an applicant, it gets added to
 * the Google sheet with the cell "received application" set to `false`.
 * This command will automatically send the applicant an email thanking
 * them for their application and let them know we are reading it over.
 *
 * An email is then sent to all@{domain} to notify everyone that there has
 * been a new application submitted with the applicant's information.
 *
 * The command then sets the "received application" cell to `true`, so we
 * can ensure we only send opurselves and them one email.
 *
 * The status for an application can be one of the following:
 *
 * - deferred (gray): For folks we want to keep around for a different
 *  stage of the company. This command ignores a status of "deferred"
 *  and does nothing.
 *
 * - declined (red): For folks who did not have an impressive application
 *  and we have declined their employment. This command ignores a
 *  status of "declined" and does nothing.
 *
 * - next steps (yellow): For folks we are working through next steps in
 *  the application process. This command will create a GitHub issue
 *  for the applicant in the meta repository so we can track their
 *  interview process.
 *
 * - hired (green): For folks we have decided to hire. This command will
 *  create an issue in the configs repository for tracking their
 *  on-boarding.
 */
pub async fn cmd_applications_run(cli_matches: &ArgMatches<'_>) {
    // Get the domain.
    let domain = value_t!(cli_matches, "domain", String).unwrap();

    // Initialize Github.
    let github = authenticate_github();
    let github_org = env::var("GITHUB_ORG").unwrap();

    // Get all the hiring issues on the meta repository.
    let meta_issues = github
        .repo(github_org.to_string(), "meta")
        .issues()
        .list(
            &IssueListOptions::builder()
                .per_page(100)
                .state(State::All)
                .labels(vec!["hiring"])
                .build(),
        )
        .await
        .unwrap();

    // Get all the hiring issues on the configs repository.
    let configs_issues = github
        .repo(github_org.to_string(), "configs")
        .issues()
        .list(
            &IssueListOptions::builder()
                .per_page(100)
                .state(State::All)
                .labels(vec!["hiring"])
                .build(),
        )
        .await
        .unwrap();

    let applicants = iterate_over_applications(domain, &do_applicant).await;

    for a in applicants {
        // Check if their status is next steps.
        if a.status.contains("next steps") {
            // Check if we already have an issue for this user.
            let exists = check_if_github_issue_exists(&meta_issues, &a.name);
            if exists {
                // Return early we don't want to update the issue because it will overwrite
                // any changes we made.
                continue;
            }

            // Create an issue for the applicant.
            let title = format!("Hiring: {}", a.name);
            let labels = vec!["hiring".to_string()];
            let body = format!("- [ ] Schedule follow up meetings
- [ ] Schedule sync to discuss

## Candidate Information

Submitted Date: {}
Email: {}
Phone: {}
Location: {}
GitHub: {}
Resume: {}
Oxide Candidate Materials: {}

## Reminder

To view the all the candidates refer to the following Google
spreadsheets:
- [Engineering Applications](https://docs.google.com/spreadsheets/d/1FHA-otHCGwe5fCRpcl89MWI7GHiFfN3EWjO6K943rYA/edit?usp=sharing)
- [Product Engineering and Design Applications](https://docs.google.com/spreadsheets/d/1VkRgmr_ZdR-y_1NJc8L0Iv6UVqKaZapt3T_Bq_gqPiI/edit?usp=sharing)

cc @jessfraz @sdtuck @bcantrill",
		a.submitted_time,
		a.email,
		a.phone,
		a.location,
		a.github,
		a.resume,
		a.materials);

            // Create the issue.
            github
                .repo(github_org.to_string(), "meta")
                .issues()
                .create(&IssueOptions {
                    title,
                    body: Some(body),
                    assignee: Some("jessfraz".to_string()),
                    labels,
                    milestone: None,
                })
                .await
                .unwrap();

            info!("[github]: created hiring issue for {}", a.email);

            continue;
        }

        // Check if their status is hired.
        if a.status.contains("hired") {
            // Check if we already have an issue for this user.
            let exists = check_if_github_issue_exists(&configs_issues, &a.name);
            if exists {
                // Return early we don't want to update the issue because it will overwrite
                // any changes we made.
                continue;
            }

            // Create an issue for the applicant.
            let title = format!("On-boarding: {}", a.name);
            let labels = vec!["hiring".to_string()];
            let body = format!(
                "- [ ] Add to users.toml
- [ ] Add to matrix chat

Start Date: [START DATE (ex. Monday, January 20th, 2020)]
Personal Email: {}
Twitter: [TWITTER HANDLE]
GitHub: {}
Phone: {}

cc @jessfraz @sdtuck @bcantrill",
                a.email, a.github, a.phone,
            );

            // Create the issue.
            github
                .repo(github_org.to_string(), "configs")
                .issues()
                .create(&IssueOptions {
                    title,
                    body: Some(body),
                    assignee: Some("jessfraz".to_string()),
                    labels,
                    milestone: None,
                })
                .await
                .unwrap();

            info!("[github]: created on-boarding issue for {}", a.email);

            continue;
        }
    }
}

/// Check if a GitHub issue already exists.
fn check_if_github_issue_exists(issues: &[Issue], search: &str) -> bool {
    issues.iter().any(|i| i.title.contains(search))
}

async fn email_send_received_application(
    sendgrid: &SendGrid,
    email: String,
    domain: String,
) {
    // Send the message.
    sendgrid.send_mail(
        "Oxide Computer Company Application Received!".to_string(),
                "Thank you for submitting your application materials! We really appreciate all
the time and thought everyone puts into their application. We will be in touch
within the next couple weeks with more information.

Sincerely,
  The Oxide Team".to_string(),
  vec![email],
        vec![format!("careers@{}",domain)],
        vec![],
    format!("careers@{}", domain),
    ).await;
}

async fn email_send_new_applicant_notification(
    sendgrid: &SendGrid,
    applicant: Applicant,
    domain: String,
) {
    // Create the message.
    let message = applicant_email(applicant.clone());

    // Send the message.
    sendgrid
        .send_mail(
            format!("New Application: {}", applicant.name),
            message,
            vec![format!("all@{}", domain)],
            vec![],
            vec![],
            format!("applications@{}", domain),
        )
        .await;
}

fn applicant_email(applicant: Applicant) -> String {
    return format!(
                        "## Applicant Information for {}

Submitted Date: {}
Name: {}
Email: {}
Phone: {}
Location: {}
GitHub: {}
Resume: {}
Oxide Candidate Materials: {}

## Reminder

To view the all the candidates refer to the following Google spreadsheets:

- Engineering Applications: https://applications-engineering.corp.oxide.computer
- Product Engineering and Design Applications: https://applications-product.corp.oxide.computer
- Technical Program Manager Applications: https://applications-tpm.corp.oxide.computer
",
applicant.role,
                        applicant.submitted_time,
                        applicant.name,
                        applicant.email,
                        applicant.phone,
                        applicant.location,
                        applicant.github,
                        applicant.resume,
                        applicant.materials,
                    );
}

pub async fn iterate_over_applications(
    domain: String,
    f: &dyn Fn(
        &Sheets,
        String,
        Applicant,
        usize,
        &SheetColumns,
    ) -> Option<Applicant>,
) -> Vec<Applicant> {
    let mut applicants: Vec<Applicant> = Default::default();
    let mut sheets: BTreeMap<&str, &str> = BTreeMap::new();
    sheets.insert(
        "Engineering",
        "1FHA-otHCGwe5fCRpcl89MWI7GHiFfN3EWjO6K943rYA",
    );
    sheets.insert(
        "Product Engineering and Design",
        "1VkRgmr_ZdR-y_1NJc8L0Iv6UVqKaZapt3T_Bq_gqPiI",
    );
    sheets.insert(
        "Technical Program Management",
        "1Z9sNUBW2z-Tlie0ci8xiet4Nryh-F0O82TFmQ1rQqlU",
    );

    // Get the GSuite token.
    let token = get_gsuite_token().await;

    // Initialize the GSuite sheets client.
    let sheets_client = Sheets::new(token);

    // Iterate over the Google sheets and create or update GitHub issues
    // depending on the application status.
    for (sheet_name, sheet_id) in sheets {
        // Get the values in the sheet.
        let sheet_values = sheets_client
            .get_values(&sheet_id, "Form Responses 1!A1:N1000".to_string())
            .await
            .unwrap();
        let values = sheet_values.values.unwrap();

        if values.is_empty() {
            panic!("unable to retrieve any data values from Google sheet")
        }

        let mut columns: SheetColumns = Default::default();
        // Iterate over the rows.
        for (row_index, row) in values.iter().enumerate() {
            if row_index == 0 {
                // Get the header information.
                // Iterate over the columns.
                // TODO: make this less horrible
                for (index, col) in row.iter().enumerate() {
                    if col.to_lowercase().contains("timestamp") {
                        columns.timestamp = index;
                    }
                    if col.to_lowercase().contains("name") {
                        columns.name = index;
                    }
                    if col.to_lowercase().contains("email address") {
                        columns.email = index;
                    }
                    if col.to_lowercase().contains("location") {
                        columns.location = index;
                    }
                    if col.to_lowercase().contains("phone") {
                        columns.phone = index;
                    }
                    if col.to_lowercase().contains("github") {
                        columns.github = index;
                    }
                    if col.to_lowercase().contains("portfolio url") {
                        columns.portfolio = index;
                    }
                    if col.to_lowercase().contains("website") {
                        columns.website = index;
                    }
                    if col.to_lowercase().contains("linkedin") {
                        columns.linkedin = index;
                    }
                    if col.to_lowercase().contains("resume") {
                        columns.resume = index;
                    }
                    if col.to_lowercase().contains("materials") {
                        columns.materials = index;
                    }
                    if col.to_lowercase().contains("status") {
                        columns.status = index;
                    }
                    if col.to_lowercase().contains(
                        "sent email that we received their application",
                    ) {
                        columns.received_application = index;
                    }
                }

                // Continue the loop since we were on the header row.
                continue;
            } // End get header information.

            // Break the loop early if we reached an empty row.
            if row[columns.email].is_empty() {
                break;
            }
            // Parse the time.
            let time_str = row[columns.timestamp].to_string() + " -08:00";
            let time =
                DateTime::parse_from_str(&time_str, "%m/%d/%Y %H:%M:%S  %:z")
                    .unwrap()
                    .with_timezone(&Utc);

            // If the length of the row is greater than the status column
            // then we have a status.
            let status = if row.len() > columns.status {
                row[columns.status].trim().to_lowercase()
            } else {
                "".to_string()
            };

            // If the length of the row is greater than the linkedin column
            // then we have a linkedin.
            let linkedin =
                if row.len() > columns.linkedin && columns.linkedin != 0 {
                    row[columns.linkedin].trim().to_lowercase()
                } else {
                    "".to_string()
                };

            // If the length of the row is greater than the portfolio column
            // then we have a portfolio.
            let portfolio =
                if row.len() > columns.portfolio && columns.portfolio != 0 {
                    row[columns.portfolio].trim().to_lowercase()
                } else {
                    "".to_lowercase()
                };

            // If the length of the row is greater than the website column
            // then we have a website.
            let website = if row.len() > columns.website && columns.website != 0
            {
                row[columns.website].trim().to_lowercase()
            } else {
                "".to_lowercase()
            };

            // Check if we sent them an email that we received their application.
            let mut received_application = true;
            if row[columns.received_application]
                .to_lowercase()
                .contains("false")
            {
                received_application = false;
            }

            let mut github = "".to_string();
            if !row[columns.github].trim().is_empty() {
                github = format!(
                    "@{}",
                    row[columns.github]
                        .trim()
                        .to_lowercase()
                        .trim_start_matches("https://github.com/")
                        .trim_start_matches('@')
                        .trim_end_matches('/')
                );
            }

            let location = row[columns.location].trim().to_string();

            let mut phone = row[columns.phone]
                .trim()
                .replace(" ", "")
                .replace("-", "")
                .replace("+", "")
                .replace("(", "")
                .replace(")", "")
                .to_string();

            let mut country = phonenumber::country::US;
            if (location.to_lowercase().contains("uk")
                || location.to_lowercase().contains("london")
                || location.to_lowercase().contains("ipswich"))
                && phone.starts_with("44")
            {
                country = phonenumber::country::GB;
            } else if location.to_lowercase().contains("czech republic")
                || location.to_lowercase().contains("prague")
            {
                country = phonenumber::country::CZ;
            } else if (location.to_lowercase().contains("mumbai")
                || location.to_lowercase().contains("india")
                || location.to_lowercase().contains("bangalore"))
                && phone.starts_with("91")
            {
                country = phonenumber::country::IN;
            } else if location.to_lowercase().contains("brazil") {
                country = phonenumber::country::BR;
            } else if location.to_lowercase().contains("belgium") {
                country = phonenumber::country::BE;
            } else if location.to_lowercase().contains("romania") {
                country = phonenumber::country::RO;
            } else if location.to_lowercase().contains("nigeria") {
                country = phonenumber::country::NG;
            } else if location.to_lowercase().contains("austria") {
                country = phonenumber::country::AT;
            } else if location.to_lowercase().contains("taiwan") {
                country = phonenumber::country::TW;
            } else if location.to_lowercase().contains("new zealand") {
                country = phonenumber::country::NZ;
            } else if location.to_lowercase().contains("maragno")
                || location.to_lowercase().contains("italy")
            {
                country = phonenumber::country::IT;
            } else if location.to_lowercase().contains("nairobi")
                || location.to_lowercase().contains("kenya")
            {
                country = phonenumber::country::KE;
            } else if location.to_lowercase().contains("dubai") {
                country = phonenumber::country::AE;
            } else if location.to_lowercase().contains("poland") {
                country = phonenumber::country::PL;
            } else if location.to_lowercase().contains("portugal") {
                country = phonenumber::country::PT;
            } else if location.to_lowercase().contains("berlin")
                || location.to_lowercase().contains("germany")
            {
                country = phonenumber::country::DE;
            } else if location.to_lowercase().contains("benin") {
                country = phonenumber::country::BJ;
            } else if location.to_lowercase().contains("israel") {
                country = phonenumber::country::IL;
            } else if location.to_lowercase().contains("spain") {
                country = phonenumber::country::ES;
            }

            // Get the last ten character of the string.
            if let Ok(phone_number) =
                phonenumber::parse(Some(country), phone.to_string())
            {
                phone = format!(
                    "{}",
                    phone_number
                        .format()
                        .mode(phonenumber::Mode::International)
                );
            }

            // Build the applicant information for the row.
            let a = Applicant {
                submitted_time: time,
                name: row[columns.name].to_string(),
                email: row[columns.email].to_string(),
                location,
                phone,
                github,
                linkedin,
                portfolio,
                website,
                resume: row[columns.resume].to_string(),
                materials: row[columns.materials].to_string(),
                status,
                received_application,
                role: sheet_name.to_string(),
                sheet_id: sheet_id.to_string(),
            };

            info!("{:?}", a);

            // Run the function passed on the applicant.
            // TODO: make domain global so we don't need to pass it.
            if let Some(applicant) =
                f(&sheets_client, domain.to_string(), a, row_index, &columns)
            {
                applicants.push(applicant);
            }
        }
    }

    applicants
}

// TODO: make this function async.
fn do_applicant(
    sheets_client: &Sheets,
    domain: String,
    a: Applicant,
    row_index: usize,
    columns: &SheetColumns,
) -> Option<Applicant> {
    // Check if we have sent them an email that we received their application.
    if !a.received_application {
        // Initialize the SendGrid client.
        let sendgrid_client = SendGrid::new_from_env();

        // Send them an email.
        futures::executor::block_on(email_send_received_application(
            &sendgrid_client,
            a.clone().email,
            domain.to_string(),
        ));

        // Send us an email notification for the application.
        futures::executor::block_on(email_send_new_applicant_notification(
            &sendgrid_client,
            a.clone(),
            domain,
        ));

        // Form the Slack message.
        let msg = format!("*NEW* :inbox_tray: {}", a.as_slack_msg(false));

        // Send a message to the applications slack channel.
        futures::executor::block_on(post_to_channel(
            HIRING_CHANNEL_POST_URL,
            &msg,
        ));

        // Mark the column as true not false.
        let mut colmn = "ABCDEFGHIJKLMNOPQRSTUVWXYZ".chars();
        let rng = format!(
            "{}{}",
            colmn.nth(columns.received_application).unwrap().to_string(),
            row_index + 1
        );

        futures::executor::block_on(sheets_client.update_values(
            &a.sheet_id,
            &rng,
            "TRUE".to_string(),
        ))
        .unwrap();

        info!(
            "[sendgrid] sent email to {} that we received their application",
            a.email
        );
    }

    if a.status.contains("hired") || a.status.contains("next steps") {
        // Return the applicant so we can iterate over them after.
        return Some(a);
    }

    None
}
