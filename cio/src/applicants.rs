use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::{copy, stderr, stdout, Write};
use std::process::Command;

use google_drive::GoogleDrive;
use html2text::from_read;
use hubcaps::issues::{IssueListOptions, State};
use pandoc::OutputKind;
use sendgrid_api::SendGrid;
use serde::{Deserialize, Serialize};
use sheets::Sheets;

use crate::db::Database;
use crate::models::NewApplicant;
use crate::slack::{get_hiring_channel_post_url, post_to_channel};
use crate::utils::{authenticate_github, get_gsuite_token, github_org};

/// The data type for a Google Sheet applicant columns, we use this when
/// parsing the Google Sheets for applicants.
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct ApplicantSheetColumns {
    pub timestamp: usize,
    pub name: usize,
    pub email: usize,
    pub location: usize,
    pub phone: usize,
    pub github: usize,
    pub portfolio: usize,
    pub website: usize,
    pub linkedin: usize,
    pub resume: usize,
    pub materials: usize,
    pub status: usize,
    pub sent_email_received: usize,
    pub value_reflected: usize,
    pub value_violated: usize,
    pub value_in_tension_1: usize,
    pub value_in_tension_2: usize,
}

impl ApplicantSheetColumns {
    /// Parse the sheet columns from Google Sheets values.
    pub fn parse(values: &[Vec<String>]) -> Self {
        // Iterate over the columns.
        // TODO: make this less horrible
        let mut columns: ApplicantSheetColumns = Default::default();

        // Get the first row.
        let row = values.get(0).unwrap();

        for (index, col) in row.iter().enumerate() {
            let c = col.to_lowercase();

            if c.contains("timestamp") {
                columns.timestamp = index;
            }
            if c.contains("name") {
                columns.name = index;
            }
            if c.contains("email address") {
                columns.email = index;
            }
            if c.contains("location") {
                columns.location = index;
            }
            if c.contains("phone") {
                columns.phone = index;
            }
            if c.contains("github") {
                columns.github = index;
            }
            if c.contains("portfolio url") {
                columns.portfolio = index;
            }
            if c.contains("website") {
                columns.website = index;
            }
            if c.contains("linkedin") {
                columns.linkedin = index;
            }
            if c.contains("resume") {
                columns.resume = index;
            }
            if c.contains("materials") {
                columns.materials = index;
            }
            if c.contains("status") {
                columns.status = index;
            }
            if c.contains("value reflected") {
                columns.value_reflected = index;
            }
            if c.contains("value violated") {
                columns.value_violated = index;
            }
            if c.contains("value in tension [1") {
                columns.value_in_tension_1 = index;
            }
            if c.contains("value in tension [2") {
                columns.value_in_tension_2 = index;
            }
            if c.contains("sent email that we received their application") {
                columns.sent_email_received = index;
            }
        }
        columns
    }
}

/// Get the contexts of a file in Google Drive by it's URL as a text string.
pub async fn get_file_contents(
    drive_client: &GoogleDrive,
    url: &str,
) -> String {
    let id = url.replace("https://drive.google.com/open?id=", "");

    // Get information about the file.
    let drive_file = drive_client.get_file_by_id(&id).await.unwrap();
    let mime_type = drive_file.mime_type;
    let name = drive_file.name;

    let mut path = env::temp_dir();
    let mut output = env::temp_dir();

    let mut result: String = Default::default();

    if mime_type == "application/pdf" {
        // Get the PDF contents from Drive.
        let contents = drive_client.download_file_by_id(&id).await.unwrap();

        path.push(format!("{}.pdf", id));

        let mut file = fs::File::create(&path).unwrap();
        file.write_all(&contents).unwrap();

        result = read_pdf(&name, path.clone());
    } else if mime_type == "text/html" {
        let contents = drive_client.download_file_by_id(&id).await.unwrap();

        // Wrap lines at 80 characters.
        result = from_read(&contents[..], 80);
    } else if mime_type == "application/vnd.google-apps.document" {
        result = drive_client.get_file_contents_by_id(&id).await.unwrap();
    } else if name.ends_with(".zip") {
        // This is patrick :)
        // Get the ip contents from Drive.
        let contents = drive_client.download_file_by_id(&id).await.unwrap();

        path.push(format!("{}.zip", id));

        let mut file = fs::File::create(&path).unwrap();
        file.write_all(&contents).unwrap();
        file = fs::File::open(&path).unwrap();

        // Unzip the file.
        let mut archive = zip::ZipArchive::new(file).unwrap();
        for i in 0..archive.len() {
            let mut file = archive.by_index(i).unwrap();
            output = env::temp_dir();
            output.push("zip/");
            output.push(file.name());

            {
                let comment = file.comment();
                if !comment.is_empty() {
                    println!(
                        "[applicants] zip file {} comment: {}",
                        i, comment
                    );
                }
            }

            if (&*file.name()).ends_with('/') {
                println!(
                    "[applicants] zip file {} extracted to \"{}\"",
                    i,
                    output.as_path().display()
                );
                fs::create_dir_all(&output).unwrap();
            } else {
                println!(
                    "[applicants] zip file {} extracted to \"{}\" ({} bytes)",
                    i,
                    output.as_path().display(),
                    file.size()
                );

                if let Some(p) = output.parent() {
                    if !p.exists() {
                        fs::create_dir_all(&p).unwrap();
                    }
                }
                let mut outfile = fs::File::create(&output).unwrap();
                copy(&mut file, &mut outfile).unwrap();

                let file_name = output.to_str().unwrap();
                if (!output.is_dir())
                    && (file_name.ends_with("responses.pdf")
                        || file_name.ends_with("OxideQuestions.pdf")
                        || file_name.ends_with("Questionnaire.pdf"))
                {
                    // Concatenate all the zip files into our result.
                    result += &format!("====================== zip file: {} ======================\n\n",output.as_path().to_str().unwrap().replace(env::temp_dir().as_path().to_str().unwrap(), ""));
                    if output.as_path().extension().unwrap() == "pdf" {
                        result += &read_pdf(&name, output.clone());
                    } else {
                        result += &fs::read_to_string(&output).unwrap();
                    }
                    result += "\n\n\n";
                }
            }
        }
    } else if name.ends_with(".doc")
        || name.ends_with(".pptx")
        || name.ends_with(".jpg")
    // TODO: handle these formats
    {
        println!(
            "[applicants] unsupported doc format -- mime type: {}, name: {}, path: {}",
            mime_type,
            name,
            path.to_str().unwrap()
        );
    } else {
        let contents = drive_client.download_file_by_id(&id).await.unwrap();
        path.push(name.to_string());

        let mut file = fs::File::create(&path).unwrap();
        file.write_all(&contents).unwrap();

        output.push(format!("{}.txt", id));

        let mut pandoc = pandoc::new();
        pandoc.add_input(&path);
        pandoc.set_output(OutputKind::File(output.clone()));
        pandoc.execute().unwrap();

        result = fs::read_to_string(output.clone()).unwrap();
    }

    // Delete the temporary file, if it exists.
    for p in vec![path, output] {
        if p.exists() && !p.is_dir() {
            fs::remove_file(p).unwrap();
        }
    }

    result.trim().to_string()
}

fn read_pdf(name: &str, path: std::path::PathBuf) -> String {
    let mut output = env::temp_dir();
    output.push("tempfile.txt");

    // Extract the text from the PDF
    let cmd_output = Command::new("pdftotext")
        .args(&[
            "-enc",
            "UTF-8",
            path.to_str().unwrap(),
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    let result = match fs::read_to_string(output.clone()) {
        Ok(r) => r,
        Err(e) => {
            println!(
                "[applicants] running pdf2text failed: {} | name: {}, path: {}",
                e,
                name,
                path.as_path().display()
            );
            stdout().write_all(&cmd_output.stdout).unwrap();
            stderr().write_all(&cmd_output.stderr).unwrap();

            "".to_string()
        }
    };

    // Delete the temporary file, if it exists.
    for p in vec![path, output] {
        if p.exists() && !p.is_dir() {
            fs::remove_file(p).unwrap();
        }
    }

    result
}

fn get_sheets_map() -> BTreeMap<&'static str, &'static str> {
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

    sheets
}

/// Return a vector of all the raw applicants and add all the metadata.
pub async fn get_raw_applicants() -> Vec<NewApplicant> {
    let mut applicants: Vec<NewApplicant> = Default::default();
    let sheets = get_sheets_map();

    // Get the GSuite token.
    let token = get_gsuite_token().await;

    // Initialize the GSuite sheets client.
    let sheets_client = Sheets::new(token.clone());

    // Initialize the GSuite sheets client.
    let drive_client = GoogleDrive::new(token.clone());

    let github = authenticate_github();

    // Get all the hiring issues on the meta repository.
    let meta_issues = github
        .repo(github_org(), "meta")
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
        .repo(github_org(), "configs")
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

    let sendgrid_client = SendGrid::new_from_env();

    // Iterate over the Google sheets and create or update GitHub issues
    // depending on the application status.
    for (sheet_name, sheet_id) in sheets {
        // Get the values in the sheet.
        let sheet_values = sheets_client
            .get_values(&sheet_id, "Form Responses 1!A1:S1000".to_string())
            .await
            .unwrap();
        let values = sheet_values.values.unwrap();

        if values.is_empty() {
            panic!(
                "unable to retrieve any data values from Google sheet {} {}",
                sheet_id, sheet_name
            );
        }

        // Parse the sheet columns.
        let columns = ApplicantSheetColumns::parse(&values);

        // Iterate over the rows.
        for (row_index, row) in values.iter().enumerate() {
            if row_index == 0 {
                // Continue the loop since we were on the header row.
                continue;
            } // End get header information.

            // Break the loop early if we reached an empty row.
            if row[columns.email].is_empty() {
                break;
            }

            // Parse the applicant out of the row information.
            let (applicant, is_new_applicant) = NewApplicant::parse(
                &drive_client,
                &sheets_client,
                sheet_name,
                sheet_id,
                &columns,
                &row,
                row_index,
            )
            .await;

            applicant
                .create_github_next_steps_issue(&github, &meta_issues)
                .await;
            applicant
                .create_github_onboarding_issue(&github, &configs_issues)
                .await;

            if is_new_applicant {
                // Post to Slack.
                post_to_channel(
                    get_hiring_channel_post_url(),
                    applicant.as_slack_msg(),
                )
                .await;

                // Send a company-wide email.
                email_send_new_applicant_notification(
                    &sendgrid_client,
                    applicant.clone(),
                    "oxide.computer",
                )
                .await;
            }

            applicants.push(applicant);
        }
    }

    applicants
}

pub async fn email_send_received_application(
    sendgrid: &SendGrid,
    email: &str,
    domain: &str,
) {
    // Send the message.
    sendgrid.send_mail(
        "Oxide Computer Company Application Received!".to_string(),
                "Thank you for submitting your application materials! We really appreciate all
the time and thought everyone puts into their application. We will be in touch
within the next couple weeks with more information.
Sincerely,
  The Oxide Team".to_string(),
  vec![email.to_string()],
        vec![format!("careers@{}",domain)],
        vec![],
    format!("careers@{}", domain),
    ).await;
}

pub async fn email_send_new_applicant_notification(
    sendgrid: &SendGrid,
    applicant: NewApplicant,
    domain: &str,
) {
    // Create the message.
    let message = applicant.clone().as_company_notification_email();

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

// Sync the applicants with our database.
pub async fn refresh_db_applicants() {
    let applicants = get_raw_applicants().await;

    // Initialize our database.
    let db = Database::new();

    // Sync applicants.
    for applicant in applicants {
        db.upsert_applicant(&applicant);
    }
}

#[cfg(test)]
mod tests {
    use crate::applicants::refresh_db_applicants;
    use crate::db::Database;
    use crate::models::Applicants;

    #[tokio::test(threaded_scheduler)]
    async fn test_cron_applicants() {
        refresh_db_applicants().await;
    }

    #[tokio::test(threaded_scheduler)]
    async fn test_cron_applicants_airtable() {
        // Initialize our database.
        let db = Database::new();

        let applicants = db.get_applicants();
        // Update applicants in airtable.
        Applicants(applicants).update_airtable().await;
    }
}
