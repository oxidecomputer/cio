use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::{stderr, stdout, Write};
use std::process::Command;

use crate::utils::get_gsuite_token;
use chrono::offset::Utc;
use chrono::DateTime;
use chrono_humanize::HumanTime;
use google_drive::GoogleDrive;
use html2text::from_read;
use pandoc::OutputKind;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sheets::Sheets;

use crate::slack::{
    FormattedMessage, MessageAttachment, MessageBlock, MessageBlockText,
    MessageBlockType, MessageResponseType, MessageType,
};

// The line breaks that get parsed are weird thats why we have the random asterisks here.
static QUESTION_TECHNICALLY_CHALLENGING: &str = r"W(?s:.*)at work(?s:.*)ave you found mos(?s:.*)challenging(?s:.*)caree(?s:.*)wh(?s:.*)\?";
static QUESTION_WORK_PROUD_OF: &str = r"W(?s:.*)at work(?s:.*)ave you done that you(?s:.*)particularl(?s:.*)proud o(?s:.*)and why\?";
static QUESTION_HAPPIEST_CAREER: &str = r"W(?s:.*)en have you been happiest in your professiona(?s:.*)caree(?s:.*)and why\?";
static QUESTION_UNHAPPIEST_CAREER: &str = r"W(?s:.*)en have you been unhappiest in your professiona(?s:.*)caree(?s:.*)and why\?";
static QUESTION_VALUE_REFLECTED: &str = r"F(?s:.*)r one of Oxide(?s:.*)s values(?s:.*)describe an example of ho(?s:.*)it wa(?s:.*)reflected(?s:.*)particula(?s:.*)body(?s:.*)you(?s:.*)work\.";
static QUESTION_VALUE_VIOLATED: &str = r"F(?s:.*)r one of Oxide(?s:.*)s values(?s:.*)describe an example of ho(?s:.*)it wa(?s:.*)violated(?s:.*)you(?s:.*)organization o(?s:.*)work\.";
static QUESTION_VALUES_IN_TENSION: &str = r"F(?s:.*)r a pair of Oxide(?s:.*)s values(?s:.*)describe a time in whic(?s:.*)the tw(?s:.*)values(?s:.*)tensio(?s:.*)for(?s:.*)your(?s:.*)and how yo(?s:.*)resolved it\.";
static QUESTION_WHY_OXIDE: &str = r"W(?s:.*)y do you want to work for Oxide\?";

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
    pub received_application: usize,
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
                columns.received_application = index;
            }
        }
        columns
    }
}

/// The data type for an applicant.
#[derive(Debug, Clone)]
pub struct Applicant {
    pub submitted_time: DateTime<Utc>,
    pub name: String,
    pub email: String,
    pub location: String,
    pub phone: String,
    pub country_code: String,
    pub github: String,
    pub gitlab: String,
    pub portfolio: String,
    pub website: String,
    pub linkedin: String,
    pub resume: String,
    pub materials: String,
    pub status: String,
    pub received_application: bool,
    pub role: String,
    pub sheet_id: String,
    pub value_reflected: String,
    pub value_violated: String,
    pub values_in_tension: Vec<String>,
}

impl Applicant {
    /// Parse the applicant from a Google Sheets row.
    pub fn parse(
        sheet_name: &str,
        sheet_id: &str,
        columns: &ApplicantSheetColumns,
        row: &[String],
    ) -> Self {
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
        let linkedin = if row.len() > columns.linkedin && columns.linkedin != 0
        {
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
        let website = if row.len() > columns.website && columns.website != 0 {
            row[columns.website].trim().to_lowercase()
        } else {
            "".to_lowercase()
        };

        // If the length of the row is greater than the value_reflected column
        // then we have a value_reflected.
        let value_reflected = if row.len() > columns.value_reflected
            && columns.value_reflected != 0
        {
            row[columns.value_reflected].trim().to_lowercase()
        } else {
            "".to_lowercase()
        };

        // If the length of the row is greater than the value_violated column
        // then we have a value_violated.
        let value_violated = if row.len() > columns.value_violated
            && columns.value_violated != 0
        {
            row[columns.value_violated].trim().to_lowercase()
        } else {
            "".to_lowercase()
        };

        let mut values_in_tension: Vec<String> = Default::default();
        // If the length of the row is greater than the value_in_tension1 column
        // then we have a value_in_tension1.
        if row.len() > columns.value_in_tension_1
            && columns.value_in_tension_1 != 0
        {
            values_in_tension
                .push(row[columns.value_in_tension_1].trim().to_lowercase());
        }
        // If the length of the row is greater than the value_in_tension2 column
        // then we have a value_in_tension2.
        if row.len() > columns.value_in_tension_2
            && columns.value_in_tension_2 != 0
        {
            values_in_tension
                .push(row[columns.value_in_tension_2].trim().to_lowercase());
        }

        // Check if we sent them an email that we received their application.
        let mut received_application = true;
        if row[columns.received_application]
            .to_lowercase()
            .contains("false")
        {
            received_application = false;
        }

        let mut github = "".to_string();
        let mut gitlab = "".to_string();
        if !row[columns.github].trim().is_empty() {
            github = format!(
                "@{}",
                row[columns.github]
                    .trim()
                    .to_lowercase()
                    .trim_start_matches("https://github.com/")
                    .trim_start_matches("http://github.com/")
                    .trim_start_matches("https://www.github.com/")
                    .trim_start_matches('@')
                    .trim_end_matches('/')
            );
            // Some people put a gitlab URL in the github form input,
            // parse those accordingly.
            if github.contains("https://gitlab.com") {
                github = "".to_string();

                gitlab = format!(
                    "@{}",
                    row[columns.github]
                        .trim()
                        .to_lowercase()
                        .trim_start_matches("https://gitlab.com/")
                        .trim_start_matches('@')
                        .trim_end_matches('/')
                );
            }
        }

        let location = row[columns.location].trim().to_string();

        let mut phone = row[columns.phone]
            .trim()
            .replace(" ", "")
            .replace("-", "")
            .replace("+", "")
            .replace("(", "")
            .replace(")", "");

        let mut country = phonenumber::country::US;
        if (location.to_lowercase().contains("uk")
            || location.to_lowercase().contains("london")
            || location.to_lowercase().contains("ipswich")
            || location.to_lowercase().contains("united kingdom")
            || location.to_lowercase().contains("england"))
            && phone.starts_with("44")
        {
            country = phonenumber::country::GB;
        } else if (location.to_lowercase().contains("czech republic")
            || location.to_lowercase().contains("prague"))
            && phone.starts_with("420")
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
        } else if location.to_lowercase().contains("romania")
            && phone.starts_with("40")
        {
            country = phonenumber::country::RO;
        } else if location.to_lowercase().contains("nigeria") {
            country = phonenumber::country::NG;
        } else if location.to_lowercase().contains("austria") {
            country = phonenumber::country::AT;
        } else if location.to_lowercase().contains("australia")
            && phone.starts_with("61")
        {
            country = phonenumber::country::AU;
        } else if location.to_lowercase().contains("sri lanka")
            && phone.starts_with("94")
        {
            country = phonenumber::country::LK;
        } else if location.to_lowercase().contains("slovenia")
            && phone.starts_with("386")
        {
            country = phonenumber::country::SI;
        } else if location.to_lowercase().contains("france")
            && phone.starts_with("33")
        {
            country = phonenumber::country::FR;
        } else if location.to_lowercase().contains("netherlands")
            && phone.starts_with("31")
        {
            country = phonenumber::country::NL;
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
        } else if location.to_lowercase().contains("benin")
            && phone.starts_with("229")
        {
            country = phonenumber::country::BJ;
        } else if location.to_lowercase().contains("israel") {
            country = phonenumber::country::IL;
        } else if location.to_lowercase().contains("spain") {
            country = phonenumber::country::ES;
        }

        let db = &phonenumber::metadata::DATABASE;
        let metadata = db.by_id(country.as_ref()).unwrap();
        let country_code = metadata.id().to_string().to_lowercase();

        // Get the last ten character of the string.
        if let Ok(phone_number) =
            phonenumber::parse(Some(country), phone.to_string())
        {
            if !phone_number.is_valid() {
                println!("phone number is invalid: {}", phone);
            }

            phone = format!(
                "{}",
                phone_number.format().mode(phonenumber::Mode::International)
            );
        }

        // Build and return the applicant information for the row.
        Applicant {
            submitted_time: time,
            name: row[columns.name].to_string(),
            email: row[columns.email].to_string(),
            location,
            phone,
            country_code,
            github,
            gitlab,
            linkedin,
            portfolio,
            website,
            resume: row[columns.resume].to_string(),
            materials: row[columns.materials].to_string(),
            status,
            received_application,
            role: sheet_name.to_string(),
            sheet_id: sheet_id.to_string(),
            value_reflected,
            value_violated,
            values_in_tension,
        }
    }

    /// Convert an applicant into the format for Airtable.
    pub async fn to_airtable_fields(
        &self,
        drive_client: &GoogleDrive,
    ) -> ApplicantFields {
        let mut status = "Needs to be triaged";

        if self.status.to_lowercase().contains("next steps") {
            status = "Next steps";
        } else if self.status.to_lowercase().contains("deferred") {
            status = "Deferred";
        } else if self.status.to_lowercase().contains("declined") {
            status = "Declined";
        } else if self.status.to_lowercase().contains("hired") {
            status = "Hired";
        }

        let mut location = None;
        if !self.location.is_empty() {
            location = Some(self.location.to_string());
        }

        let mut github = None;
        if !self.github.is_empty() {
            github = Some(
                "https://github.com/".to_owned()
                    + &self.github.replace("@", ""),
            );
        }

        let mut linkedin = None;
        if !self.linkedin.is_empty() {
            linkedin = Some(self.linkedin.to_string());
        }

        let mut portfolio = None;
        if !self.portfolio.is_empty() {
            portfolio = Some(self.portfolio.to_string());
        }

        let mut website = None;
        if !self.website.is_empty() {
            website = Some(self.website.to_string());
        }

        let mut value_reflected = None;
        if !self.value_reflected.is_empty() {
            value_reflected = Some(self.value_reflected.to_string());
        }

        let mut value_violated = None;
        if !self.value_violated.is_empty() {
            value_violated = Some(self.value_violated.to_string());
        }

        let mut values_in_tension = None;
        if !self.values_in_tension.is_empty() {
            values_in_tension = Some(self.values_in_tension.clone());
        }

        // Read the file contents.
        let rc = get_file_contents(drive_client, self.resume.to_string()).await;
        let mc =
            get_file_contents(drive_client, self.materials.to_string()).await;

        let mut resume_contents = None;
        if !rc.is_empty() {
            resume_contents = Some(rc);
        }

        let mut materials_contents = None;
        if !mc.is_empty() {
            materials_contents = Some(mc);
        }

        let mut applicant = ApplicantFields {
            name: self.name.to_string(),
            position: self.role.to_string(),
            status: status.to_string(),
            timestamp: self.submitted_time,
            email: self.email.to_string(),
            phone: self.phone.to_string(),
            location,
            github,
            linkedin,
            portfolio,
            website,
            resume: self.resume.to_string(),
            materials: self.materials.to_string(),
            value_reflected,
            value_violated,
            values_in_tension,
            resume_contents,
            materials_contents,
            work_samples: None,
            writing_samples: None,
            analysis_samples: None,
            presentation_samples: None,
            exploratory_samples: None,
            question_technically_challenging: None,
            question_proud_of: None,
            question_happiest: None,
            question_unhappiest: None,
            question_value_reflected: None,
            question_value_violated: None,
            question_values_in_tension: None,
            question_why_oxide: None,
        };

        // Parse the materials.
        applicant.parse_materials();

        applicant
    }

    /// Get the human duration of time since the application was submitted.
    pub fn human_duration(&self) -> HumanTime {
        let mut dur = self.submitted_time - Utc::now();
        if dur.num_seconds() > 0 {
            dur = -dur;
        }

        HumanTime::from(dur)
    }

    /// Convert the applicant into JSON for a Slack message.
    pub fn as_slack_msg(&self) -> Value {
        let mut color = "#805AD5";
        match self.role.as_str() {
            "Product Engineering and Design" => color = "#48D597",
            "Technical Program Management" => color = "#667EEA",
            _ => (),
        }

        let time = self.human_duration();

        let mut status_msg = format!("<https://docs.google.com/spreadsheets/d/{}|{}> Applicant | applied {}", self.sheet_id, self.role, time);
        if !self.status.is_empty() {
            status_msg += &format!(" | status: *{}*", self.status);
        }

        let mut values_msg = "".to_string();
        if !self.value_reflected.is_empty() {
            values_msg +=
                &format!("values reflected: *{}*", self.value_reflected);
        }
        if !self.value_violated.is_empty() {
            values_msg += &format!(" | violated: *{}*", self.value_violated);
        }
        for (k, tension) in self.values_in_tension.iter().enumerate() {
            if k == 0 {
                values_msg += &format!(" | in tension: *{}*", tension);
            } else {
                values_msg += &format!(" *& {}*", tension);
            }
        }
        if values_msg.is_empty() {
            values_msg = "values not yet populated".to_string();
        }

        let mut intro_msg =
            format!("*{}*  <mailto:{}|{}>", self.name, self.email, self.email,);
        if !self.location.is_empty() {
            intro_msg += &format!("  {}", self.location);
        }

        let mut info_msg = format!(
            "<{}|resume> | <{}|materials>",
            self.resume, self.materials,
        );
        if !self.phone.is_empty() {
            info_msg += &format!(" | <tel:{}|{}>", self.phone, self.phone);
        }
        if !self.github.is_empty() {
            info_msg += &format!(
                " | <https://github.com/{}|github:{}>",
                self.github.trim_start_matches('@'),
                self.github,
            );
        }
        if !self.gitlab.is_empty() {
            info_msg += &format!(
                " | <https://gitlab.com/{}|gitlab:{}>",
                self.gitlab.trim_start_matches('@'),
                self.gitlab,
            );
        }
        if !self.linkedin.is_empty() {
            info_msg += &format!(" | <{}|linkedin>", self.linkedin,);
        }
        if !self.portfolio.is_empty() {
            info_msg += &format!(" | <{}|portfolio>", self.portfolio,);
        }
        if !self.website.is_empty() {
            info_msg += &format!(" | <{}|website>", self.website,);
        }

        json!(FormattedMessage {
            response_type: Some(MessageResponseType::InChannel),
            channel: None,
            blocks: None,
            attachments: Some(vec![MessageAttachment {
                color: Some(color.to_string()),
                blocks: Some(vec![
                    MessageBlock {
                        block_type: MessageBlockType::Section,
                        text: MessageBlockText {
                            text_type: MessageType::Markdown,
                            text: intro_msg,
                        },
                        accessory: None,
                        block_id: None,
                        fields: None,
                    },
                    MessageBlock {
                        block_type: MessageBlockType::Context,
                        text: MessageBlockText {
                            text_type: MessageType::Markdown,
                            text: info_msg,
                        },
                        accessory: None,
                        block_id: None,
                        fields: None,
                    },
                    MessageBlock {
                        block_type: MessageBlockType::Context,
                        text: MessageBlockText {
                            text_type: MessageType::Markdown,
                            text: values_msg,
                        },
                        accessory: None,
                        block_id: None,
                        fields: None,
                    },
                    MessageBlock {
                        block_type: MessageBlockType::Context,
                        text: MessageBlockText {
                            text_type: MessageType::Markdown,
                            text: status_msg,
                        },
                        accessory: None,
                        block_id: None,
                        fields: None,
                    }
                ]),
                author_icon: None,
                author_link: None,
                author_name: None,
                fallback: None,
                fields: None,
                footer: None,
                footer_icon: None,
                image_url: None,
                pretext: None,
                text: None,
                thumb_url: None,
                title: None,
                title_link: None,
            }])
        })
    }

    /// Get the applicant's information in the form of the body of an email for a
    /// company wide notification that we received a new application.
    pub fn as_company_notification_email(&self) -> String {
        let time = self.human_duration();

        let mut msg = format!(
            "## Applicant Information for {}

Submitted {}
Name: {}
Email: {}",
            self.role, time, self.name, self.email
        );

        if !self.location.is_empty() {
            msg += &format!("\nLocation: {}", self.location);
        }
        if !self.phone.is_empty() {
            msg += &format!("\nPhone: {}", self.phone);
        }

        if !self.github.is_empty() {
            msg += &format!(
                "\nGitHub: {} (https://github.com/{})",
                self.github,
                self.github.trim_start_matches('@')
            );
        }
        if !self.gitlab.is_empty() {
            msg += &format!(
                "\nGitLab: {} (https://gitlab.com/{})",
                self.gitlab,
                self.gitlab.trim_start_matches('@')
            );
        }
        if !self.linkedin.is_empty() {
            msg += &format!("\nLinkedIn: {}", self.linkedin);
        }
        if !self.portfolio.is_empty() {
            msg += &format!("\nPortfolio: {}", self.portfolio);
        }
        if !self.website.is_empty() {
            msg += &format!("\nWebsite: {}", self.website);
        }

        msg+=&format!("\nResume: {}
Oxide Candidate Materials: {}

## Reminder

To view the all the candidates refer to the following Google spreadsheets:

- Engineering Applications: https://applications-engineering.corp.oxide.computer
- Product Engineering and Design Applications: https://applications-product.corp.oxide.computer
- Technical Program Manager Applications: https://applications-tpm.corp.oxide.computer
",
                        self.resume,
                        self.materials,
                    );

        msg
    }
}

/// The Airtable fields type for an applicant.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ApplicantFields {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Position")]
    pub position: String,
    #[serde(rename = "Status")]
    pub status: String,
    #[serde(rename = "Timestamp")]
    pub timestamp: DateTime<Utc>,
    #[serde(rename = "Email Address")]
    pub email: String,
    #[serde(rename = "Phone Number")]
    pub phone: String,
    #[serde(rename = "Location")]
    pub location: Option<String>,
    #[serde(rename = "GitHub")]
    pub github: Option<String>,
    #[serde(rename = "LinkedIn")]
    pub linkedin: Option<String>,
    #[serde(rename = "Portfolio")]
    pub portfolio: Option<String>,
    #[serde(rename = "Website")]
    pub website: Option<String>,
    #[serde(rename = "Resume")]
    pub resume: String,
    #[serde(rename = "Oxide Materials")]
    pub materials: String,
    #[serde(rename = "Value Reflected")]
    pub value_reflected: Option<String>,
    #[serde(rename = "Value Violated")]
    pub value_violated: Option<String>,
    #[serde(rename = "Values in Tension")]
    pub values_in_tension: Option<Vec<String>>,
    #[serde(rename = "Resume Contents")]
    pub resume_contents: Option<String>,
    #[serde(rename = "Oxide Materials Contents")]
    pub materials_contents: Option<String>,
    #[serde(rename = "Work samples")]
    pub work_samples: Option<String>,
    #[serde(rename = "Writing samples")]
    pub writing_samples: Option<String>,
    #[serde(rename = "Analysis samples")]
    pub analysis_samples: Option<String>,
    #[serde(rename = "Presentation samples")]
    pub presentation_samples: Option<String>,
    #[serde(rename = "Exploratory samples")]
    pub exploratory_samples: Option<String>,
    #[serde(
        rename = "What work have you found most technically challenging in your career and why?"
    )]
    pub question_technically_challenging: Option<String>,
    #[serde(
        rename = "What work have you done that you were particularly proud of and why?"
    )]
    pub question_proud_of: Option<String>,
    #[serde(
        rename = "When have you been happiest in your professional career and why?"
    )]
    pub question_happiest: Option<String>,
    #[serde(
        rename = "When have you been unhappiest in your professional career and why?"
    )]
    pub question_unhappiest: Option<String>,
    #[serde(
        rename = "For one of Oxide's values, describe an example of how it was reflected in a particular body of your work."
    )]
    pub question_value_reflected: Option<String>,
    #[serde(
        rename = "For one of Oxide's values, describe an example of how it was violated in your organization or work."
    )]
    pub question_value_violated: Option<String>,
    #[serde(
        rename = "For a pair of Oxide's values, describe a time in which the two values came into tension for you or your work, and how you resolved it."
    )]
    pub question_values_in_tension: Option<String>,
    #[serde(rename = "Why do you want to work for Oxide?")]
    pub question_why_oxide: Option<String>,
}

impl PartialEq for ApplicantFields {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.position == other.position
            && self.status == other.status
            && self.timestamp == other.timestamp
            && self.email == other.email
            && self.phone == other.phone
            && self.location == other.location
            && self.github == other.github
            && self.linkedin == other.linkedin
            && self.portfolio == other.portfolio
            && self.website == other.website
            && self.resume == other.resume
            && self.materials == other.materials
            && self.value_reflected == other.value_reflected
            && self.value_violated == other.value_violated
            && self.values_in_tension == other.values_in_tension
            && self.resume_contents == other.resume_contents
            && self.materials_contents == other.materials_contents
            && self.work_samples == other.work_samples
            && self.writing_samples == other.writing_samples
            && self.analysis_samples == other.analysis_samples
            && self.presentation_samples == other.presentation_samples
            && self.exploratory_samples == other.exploratory_samples
            && self.question_technically_challenging
                == other.question_technically_challenging
            && self.question_proud_of == other.question_proud_of
            && self.question_happiest == other.question_happiest
            && self.question_unhappiest == other.question_unhappiest
            && self.question_value_reflected == other.question_value_reflected
            && self.question_value_violated == other.question_value_violated
            && self.question_values_in_tension
                == other.question_values_in_tension
            && self.question_why_oxide == other.question_why_oxide
    }
}

impl ApplicantFields {
    // TODO: probably a better way to do regexes here, but hey it works.
    /// Parse the materials as text for the applicant fields.
    fn parse_materials(&mut self) {
        let materials_contents;
        match &self.materials_contents {
            Some(m) => materials_contents = m,
            None => return,
        }

        let mut work_samples = parse_question(
            r"Work sample\(s\)",
            "Writing samples",
            materials_contents,
        );
        if work_samples == None {
            work_samples = parse_question(
                r"If(?s:.*)his work is entirely proprietary(?s:.*)please describe it as fully as y(?s:.*)can, providing necessary context\.",
                "Writing samples",
                materials_contents,
            );
            if work_samples == None {
                // Try to parse work samples for TPM role.
                work_samples = parse_question(
                    r"What would you have done differently\?",
                    "Exploratory samples",
                    materials_contents,
                );

                if work_samples == None {
                    work_samples = parse_question(
                        r"Some questions(?s:.*)o have in mind as you describe them:",
                        "Exploratory samples",
                        materials_contents,
                    );

                    if work_samples == None {
                        work_samples = parse_question(
                            r"Work samples",
                            "Exploratory samples",
                            materials_contents,
                        );
                    }
                }
            }
        }
        self.work_samples = work_samples;

        let mut writing_samples = parse_question(
            r"Writing sample\(s\)",
            "Analysis samples",
            materials_contents,
        );
        if writing_samples == None {
            writing_samples = parse_question(
                r"Please submit at least one writing sample \(and no more tha(?s:.*)three\) that you feel represent(?s:.*)you(?s:.*)providin(?s:.*)links if(?s:.*)necessary\.",
                "Analysis samples",
                materials_contents,
            );
            if writing_samples == None {
                writing_samples = parse_question(
                    r"Writing samples",
                    "Analysis samples",
                    materials_contents,
                );
            }
        }
        self.writing_samples = writing_samples;

        let mut analysis_samples = parse_question(
            r"Analysis sample\(s\)$",
            "Presentation samples",
            materials_contents,
        );
        if analysis_samples == None {
            analysis_samples = parse_question(
                r"please recount a(?s:.*)incident(?s:.*)which you analyzed syste(?s:.*)misbehavior(?s:.*)including as much technical detail as you can recall\.",
                "Presentation samples",
                materials_contents,
            );
            if analysis_samples == None {
                analysis_samples = parse_question(
                    r"Analysis samples",
                    "Presentation samples",
                    materials_contents,
                );
            }
        }
        self.analysis_samples = analysis_samples;

        let mut presentation_samples = parse_question(
            r"Presentation sample\(s\)",
            "Questionnaire",
            materials_contents,
        );
        if presentation_samples == None {
            presentation_samples = parse_question(
                r"I(?s:.*)you don’t have a publicl(?s:.*)available presentation(?s:.*)pleas(?s:.*)describe a topic on which you have presented in th(?s:.*)past\.",
                "Questionnaire",
                materials_contents,
            );
            if presentation_samples == None {
                presentation_samples = parse_question(
                    r"Presentation samples",
                    "Questionnaire",
                    materials_contents,
                );
            }
        }
        self.presentation_samples = presentation_samples;

        let mut exploratory_samples = parse_question(
            r"Exploratory sample\(s\)",
            "Questionnaire",
            materials_contents,
        );
        if exploratory_samples == None {
            exploratory_samples = parse_question(
                r"What’s an example o(?s:.*)something that you needed to explore, reverse engineer, decipher or otherwise figure out a(?s:.*)part of a program or project and how did you do it\? Please provide as much detail as you ca(?s:.*)recall\.",
                "Questionnaire",
                materials_contents,
            );
            if exploratory_samples == None {
                exploratory_samples = parse_question(
                    r"Exploratory samples",
                    "Questionnaire",
                    materials_contents,
                );
            }
        }
        self.exploratory_samples = exploratory_samples;

        let question_technically_challenging = parse_question(
            QUESTION_TECHNICALLY_CHALLENGING,
            QUESTION_WORK_PROUD_OF,
            materials_contents,
        );
        self.question_technically_challenging =
            question_technically_challenging;

        let question_proud_of = parse_question(
            QUESTION_WORK_PROUD_OF,
            QUESTION_HAPPIEST_CAREER,
            materials_contents,
        );
        self.question_proud_of = question_proud_of;

        let question_happiest = parse_question(
            QUESTION_HAPPIEST_CAREER,
            QUESTION_UNHAPPIEST_CAREER,
            materials_contents,
        );
        self.question_happiest = question_happiest;

        let question_unhappiest = parse_question(
            QUESTION_UNHAPPIEST_CAREER,
            QUESTION_VALUE_REFLECTED,
            materials_contents,
        );
        self.question_unhappiest = question_unhappiest;

        let question_value_reflected = parse_question(
            QUESTION_VALUE_REFLECTED,
            QUESTION_VALUE_VIOLATED,
            materials_contents,
        );
        self.question_value_reflected = question_value_reflected;

        let question_value_violated = parse_question(
            QUESTION_VALUE_VIOLATED,
            QUESTION_VALUES_IN_TENSION,
            materials_contents,
        );
        self.question_value_violated = question_value_violated;

        let question_values_in_tension = parse_question(
            QUESTION_VALUES_IN_TENSION,
            QUESTION_WHY_OXIDE,
            materials_contents,
        );
        self.question_values_in_tension = question_values_in_tension;

        let question_why_oxide =
            parse_question(QUESTION_WHY_OXIDE, "", materials_contents);
        self.question_why_oxide = question_why_oxide;
    }
}

fn parse_question(
    q1: &str,
    q2: &str,
    materials_contents: &str,
) -> Option<String> {
    let re = Regex::new(&(q1.to_owned() + r"(?s)(.*)" + q2)).unwrap();
    let result: Option<String> = if let Some(q) =
        re.captures(materials_contents)
    {
        let val = q.get(1).unwrap();
        let s = val
            .as_str()
            .replace("________________", "")
            .replace("Oxide Candidate Materials: Technical Program Manager", "")
            .replace("Oxide Candidate Materials", "")
            .replace("Work sample(s)", "")
            .trim_start_matches(':')
            .trim()
            .to_string();

        if s.is_empty() {
            return None;
        }

        Some(s)
    } else {
        None
    };

    result
}

/// Get the contexts of a file in Google Drive by it's URL as a text string.
async fn get_file_contents(drive_client: &GoogleDrive, url: String) -> String {
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
                    "running pdf2text failed: {} | name: {}, path: {}",
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
            "unsupported doc format -- mime type: {}, name: {}, path: {}",
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

/// Return a vector of all the applicants.
pub async fn get_all_applicants() -> Vec<Applicant> {
    let mut applicants: Vec<Applicant> = Default::default();
    let sheets = get_sheets_map();

    // Get the GSuite token.
    let token = get_gsuite_token().await;

    // Initialize the GSuite sheets client.
    let sheets_client = Sheets::new(token);

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
            let applicant =
                Applicant::parse(sheet_name, sheet_id, &columns, &row);

            applicants.push(applicant);
        }
    }

    applicants
}
