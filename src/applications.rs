use std::env;

use chrono::naive::NaiveDate;
use clap::ArgMatches;
use hubcaps::issues::{Issue, IssueListOptions, IssueOptions, State};
use log::info;
use tokio::runtime::Runtime;

use crate::core::{Applicant, SheetColumns};
use crate::email::client::SendGrid;
use crate::sheets::client::Sheets;
use crate::utils::{authenticate_github, get_gsuite_token};

pub fn cmd_applications_run(cli_matches: &ArgMatches) {
    let sheets: Vec<String>;
    match cli_matches.values_of("sheet") {
        None => panic!("no Google sheets IDs specified"),
        Some(val) => {
            sheets = val.map(|s| s.to_string()).collect();
        }
    };

    if sheets.len() < 1 {
        panic!("must provide IDs of google sheets to update applications from")
    }

    // Initialize Github and the runtime.
    let github = authenticate_github();
    let github_org = env::var("GITHUB_ORG").unwrap();
    let mut runtime = Runtime::new().unwrap();

    // Get all the hiring issues on the meta repository.
    let meta_issues = runtime
        .block_on(
            github.repo(github_org.to_string(), "meta").issues().list(
                &IssueListOptions::builder()
                    .per_page(100)
                    .state(State::All)
                    .labels(vec!["hiring"])
                    .build(),
            ),
        )
        .unwrap();

    // Get all the hiring issues on the configs repository.
    let configs_issues = runtime
        .block_on(
            github
                .repo(github_org.to_string(), "configs")
                .issues()
                .list(
                    &IssueListOptions::builder()
                        .per_page(100)
                        .state(State::All)
                        .labels(vec!["hiring"])
                        .build(),
                ),
        )
        .unwrap();

    // Get the GSuite token.
    let token = get_gsuite_token();

    // Initialize the GSuite sheets client.
    let sheets_client = Sheets::new(token);

    // Iterate over the Google sheets and create or update GitHub issues
    // depending on the application status.
    for sheet_id in sheets {
        // Get the values in the sheet.
        let sheet_values =
            sheets_client.get_values(&sheet_id, "Form Responses 1!A1:N1000".to_string());
        let values = sheet_values.values.unwrap();

        if values.len() < 1 {
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
                    if col
                        .to_lowercase()
                        .contains("sent email that we received their application")
                    {
                        columns.received_application = index;
                    }
                }

                // Continue the loop since we were on the header row.
                continue;
            } // End get header information.

            // Break the loop early if we reached an empty row.
            if row[columns.email].len() < 1 {
                break;
            }
            // Parse the time.
            let time =
                NaiveDate::parse_from_str(&row[columns.timestamp], "%m/%d/%Y %H:%M:%S").unwrap();

            let mut status = "";
            // If the length of the row is greater than the status column
            // then we have a status.
            if row.len() > columns.status {
                status = &row[columns.status];
            }

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
                        .trim_end_matches("/")
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
                sendgrid_client.send_received_application(&a.email, &a.name);

                // Send us an email notification for the application.
                sendgrid_client.send_new_applicant_notification(a.clone());

                // Mark the column as true not false.
                let mut colmn = "ABCDEFGHIJKLMNOPQRSTUVWXYZ".chars();
                let rng = format!(
                    "{}{}",
                    colmn.nth(columns.received_application).unwrap().to_string(),
                    i + 1
                );

                sheets_client.update_values(&sheet_id, &rng, "TRUE".to_string());

                info!(
                    "[sendgrid] sent email to {} that we received their application",
                    a.email
                );
            }

            // Check if their status is next steps.
            if status.to_lowercase().contains("next steps") {
                // Check if we already have an issue for this user.
                let exists = check_if_github_issue_exists(&meta_issues, a.name.clone());
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
                runtime
                    .block_on(github.repo(github_org.to_string(), "meta").issues().create(
                        &IssueOptions {
                            title: title,
                            body: Some(body),
                            assignee: Some("jessfraz".to_string()),
                            labels: labels,
                            milestone: None,
                        },
                    ))
                    .unwrap();

                info!("[github]: created hiring issue for {}", a.email);

                continue;
            }

            // Check if their status is hired.
            if status.to_lowercase().contains("hired") {
                // Check if we already have an issue for this user.
                let exists = check_if_github_issue_exists(&configs_issues, a.name.clone());
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
                runtime
                    .block_on(
                        github
                            .repo(github_org.to_string(), "configs")
                            .issues()
                            .create(&IssueOptions {
                                title: title,
                                body: Some(body),
                                assignee: Some("jessfraz".to_string()),
                                labels: labels,
                                milestone: None,
                            }),
                    )
                    .unwrap();

                info!("[github]: created on-boarding issue for {}", a.email);

                continue;
            }
        }
    }
}

fn check_if_github_issue_exists(issues: &Vec<Issue>, search: String) -> bool {
    for i in issues {
        if i.title.contains(&search) {
            return true;
        }
    }

    return false;
}
