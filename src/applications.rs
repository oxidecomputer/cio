use std::collections::BTreeMap;
use std::env;

use chrono::naive::NaiveDate;
use clap::{value_t, ArgMatches};
use hubcaps::issues::{Issue, IssueListOptions, IssueOptions, State};
use log::info;

use crate::core::{Applicant, SheetColumns};
use crate::slack::post_to_applications;
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
    let mut sheets: BTreeMap<&str, &str> = BTreeMap::new();
    sheets.insert(
        "Systems Engineering",
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

        let mut columns = SheetColumns {
            timestamp: 0,
            name: 0,
            email: 0,
            location: 0,
            phone: 0,
            github: 0,
            resume: 0,
            materials: 0,
            status: 0,
            received_application: 0,
        };
        // Iterate over the rows.
        for (i, row) in values.iter().enumerate() {
            if i == 0 {
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
            let time = NaiveDate::parse_from_str(
                &row[columns.timestamp],
                "%m/%d/%Y %H:%M:%S",
            )
            .unwrap();

            // If the length of the row is greater than the status column
            // then we have a status.
            let status = if row.len() > columns.status {
                &row[columns.status]
            } else {
                ""
            };

            // Build the applicant information for the row.
            let a = Applicant {
                submitted_time: time,
                name: row[columns.name].to_string(),
                email: row[columns.email].to_string(),
                location: row[columns.location].to_string(),
                phone: row[columns.phone].to_string(),
                github: format!(
                    "@{}",
                    row[columns.github]
                        .trim_start_matches("https://github.com/")
                        .trim_end_matches('/')
                ),
                resume: row[columns.resume].to_string(),
                materials: row[columns.materials].to_string(),
                status: status.to_string(),
            };

            // Check if we have sent them an email that we received their application.
            if row[columns.received_application]
                .to_lowercase()
                .contains("false")
            {
                // Initialize the SendGrid client.
                let sendgrid_client = SendGrid::new_from_env();

                // Send them an email.
                email_send_received_application(
                    &sendgrid_client,
                    a.clone().email,
                    domain.to_string(),
                )
                .await;

                // Send us an email notification for the application.
                email_send_new_applicant_notification(
                    &sendgrid_client,
                    a.clone(),
                    domain.to_string(),
                    sheet_name,
                )
                .await;

                // Send a message to the applications slack channel.
                post_to_applications(&format!(
                    r#"## New Application Received for {}: {}

Email: {}
Phone: {}
Location: {}
GitHub: {}
Resume: {}
Oxide Candidate Materials: {}


                        "#,
                    sheet_name,
                    a.name,
                    a.email,
                    a.phone,
                    a.location,
                    a.github,
                    a.resume,
                    a.materials,
                ))
                .await;

                // Mark the column as true not false.
                let mut colmn = "ABCDEFGHIJKLMNOPQRSTUVWXYZ".chars();
                let rng = format!(
                    "{}{}",
                    colmn
                        .nth(columns.received_application)
                        .unwrap()
                        .to_string(),
                    i + 1
                );

                sheets_client
                    .update_values(&sheet_id, &rng, "TRUE".to_string())
                    .await
                    .unwrap();

                info!(
                    "[sendgrid] sent email to {} that we received their application",
                    a.email
                );
            }

            // Check if their status is next steps.
            if status.to_lowercase().contains("next steps") {
                // Check if we already have an issue for this user.
                let exists =
                    check_if_github_issue_exists(&meta_issues, &a.name);
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
            if status.to_lowercase().contains("hired") {
                // Check if we already have an issue for this user.
                let exists =
                    check_if_github_issue_exists(&configs_issues, &a.name);
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
    sheet_name: &str,
) {
    // Create the message.
    let message = applicant_email(applicant.clone(), sheet_name);

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

fn applicant_email(applicant: Applicant, sheet_name: &str) -> String {
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
sheet_name,
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
