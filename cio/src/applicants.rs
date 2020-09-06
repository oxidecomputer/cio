use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::{stderr, stdout, Write};
use std::process::Command;

use google_drive::GoogleDrive;
use html2text::from_read;
use pandoc::OutputKind;
use serde::{Deserialize, Serialize};
use sheets::Sheets;

use crate::models::NewApplicant;
use crate::utils::get_gsuite_token;

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
    let mime_type = drive_file.mime_type.unwrap();
    let name = drive_file.name.unwrap();

    let mut path = env::temp_dir();
    let mut output = env::temp_dir();

    let mut result: String = Default::default();

    if mime_type == "application/pdf" {
        // Get the PDF contents from Drive.
        let contents = drive_client.download_file_by_id(&id).await.unwrap();

        path.push(format!("{}.pdf", id));

        let mut file = fs::File::create(path.clone()).unwrap();
        file.write_all(&contents).unwrap();

        output.push(format!("{}.txt", id));

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

        result = match fs::read_to_string(output.clone()) {
            Ok(r) => r,
            Err(e) => {
                println!(
                    "[applicants] running pdf2text failed: {} | name: {}, path: {}",
                    e,
                    name,
                    path.to_str().unwrap()
                );
                stdout().write_all(&cmd_output.stdout).unwrap();
                stderr().write_all(&cmd_output.stderr).unwrap();

                "".to_string()
            }
        };
    } else if mime_type == "text/html" {
        let contents = drive_client.download_file_by_id(&id).await.unwrap();

        // Wrap lines at 80 characters.
        result = from_read(&contents[..], 80);
    } else if mime_type == "application/vnd.google-apps.document" {
        result = drive_client.get_file_contents_by_id(&id).await.unwrap();
    } else if name.ends_with(".doc")
        || name.ends_with(".pptx")
        || name.ends_with(".jpg")
        || name.ends_with(".zip")
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

        let mut file = fs::File::create(path.clone()).unwrap();
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
            let applicant = NewApplicant::parse(
                &drive_client,
                sheet_name,
                sheet_id,
                &columns,
                &row,
            )
            .await;

            applicants.push(applicant);
        }
    }

    applicants
}
