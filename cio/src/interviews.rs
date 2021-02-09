use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{copy, stderr, stdout, Read, Write};
use std::process::Command;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use google_drive::GoogleDrive;
use gsuite_api::GSuite;
use macros::db;
use pandoc::OutputKind;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::airtable::{AIRTABLE_BASE_ID_RECURITING_APPLICATIONS, AIRTABLE_INTERVIEWS_TABLE};
use crate::applicants::{get_sheets_map, Applicant};
use crate::configs::{User, Users};
use crate::core::UpdateAirtableRecord;
use crate::db::Database;
use crate::schema::{applicant_interviews, users};
use crate::utils::{get_gsuite_token, DOMAIN, GSUITE_DOMAIN};

#[db {
    new_struct_name = "ApplicantInterview",
    airtable_base_id = "AIRTABLE_BASE_ID_RECURITING_APPLICATIONS",
    airtable_table = "AIRTABLE_INTERVIEWS_TABLE",
    match_on = {
        "google_event_id" = "String",
    },
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[table_name = "applicant_interviews"]
pub struct NewApplicantInterview {
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub interviewers: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub google_event_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub event_link: String,
    /// link to another table in Airtable
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub applicant: Vec<String>,
}

/// Implement updating the Airtable record for a ApplicantInterview.
#[async_trait]
impl UpdateAirtableRecord<ApplicantInterview> for ApplicantInterview {
    #[instrument]
    #[inline]
    async fn update_airtable_record(&mut self, _record: ApplicantInterview) {}
}

/// Sync interviews.
#[instrument(skip(db))]
#[inline]
pub async fn refresh_interviews(db: &Database) {
    let gsuite_customer = env::var("GADMIN_ACCOUNT_ID").unwrap();
    let token = get_gsuite_token("").await;
    let gsuite = GSuite::new(&gsuite_customer, GSUITE_DOMAIN, token.clone());

    // Get the list of our calendars.
    let calendars = gsuite.list_calendars().await.unwrap();

    // Iterate over the calendars.
    for calendar in calendars {
        // Ignore any calandar that is not the interviews calendar.
        if calendar.summary != "Interviews" {
            continue;
        }

        // Let's get all the events on this calendar and try and see if they
        // have a meeting recorded.
        println!("Getting events for {}", calendar.id);
        let events = gsuite.list_calendar_events(&calendar.id).await.unwrap();

        for event in events {
            // Create the interview event.
            let mut interview = NewApplicantInterview {
                start_time: event.start.date_time.unwrap(),
                end_time: event.end.date_time.unwrap(),

                name: "".to_string(),
                email: "".to_string(),
                interviewers: Default::default(),

                google_event_id: event.id.to_string(),
                event_link: event.html_link.to_string(),
                applicant: Default::default(),
            };

            for attendee in event.attendees {
                // Skip the organizer, this is the Interviews calendar.
                if attendee.organizer || attendee.email.ends_with("@group.calendar.google.com") {
                    continue;
                }

                let end = &format!("({})", attendee.display_name);
                // TODO: Sometimes Dave and Nils use their personal email, find a better way to do this other than
                // a one-off.
                if attendee.email.ends_with(GSUITE_DOMAIN)
                    || attendee.email.ends_with(DOMAIN)
                    || event.summary.ends_with(end)
                    || attendee.email.starts_with("dave.pacheco")
                    || attendee.email.starts_with("nils.nieuwejaar")
                {
                    // This is the interviewer.
                    let mut email = attendee.email.to_string();

                    // If the email is not their oxide computer email, let's firgure it out based
                    // on the information from their user.
                    if !email.ends_with(GSUITE_DOMAIN) && !email.ends_with(DOMAIN) {
                        match users::dsl::users.filter(users::dsl::recovery_email.eq(email.to_string())).limit(1).load::<User>(&db.conn()) {
                            Ok(r) => {
                                if !r.is_empty() {
                                    let record = r.get(0).unwrap().clone();
                                    email = record.email();
                                }
                            }
                            Err(e) => {
                                println!("[db] we don't have the record in the database: {}", e);
                            }
                        }
                    }

                    interview.interviewers.push(email.to_string());
                    continue;
                }

                // It must be the person being interviewed.
                // See if we can get the Applicant record ID for them.
                interview.email = attendee.email.to_string();
            }

            for (_, sheet_id) in get_sheets_map() {
                let applicant = Applicant::get_from_db(&db, interview.email.to_string(), sheet_id.to_string());
                if let Some(a) = applicant {
                    interview.applicant = vec![a.airtable_record_id];
                    interview.name = a.name.to_string();
                    break;
                }
            }

            let name = interview.name.to_string();
            let mut interviewers = interview.interviewers.clone();
            interviewers
                .iter_mut()
                .for_each(|x| *x = x.trim_end_matches(GSUITE_DOMAIN).trim_end_matches(DOMAIN).trim_end_matches('@').to_string());

            interview.name = format!("{} ({})", name, interviewers.join(", "));

            interview.upsert(&db).await;
        }
    }

    ApplicantInterviews::get_from_db(&db).update_airtable().await;
}

/// Compile interview packets for each interviewee.
#[instrument(skip(db))]
#[inline]
pub async fn compile_packets(db: &Database) {
    // Get gsuite token.
    let token = get_gsuite_token("").await;

    // Initialize the Google Drive client.
    let drive_client = GoogleDrive::new(token);
    // Figure out where our directory is.
    // It should be in the shared drive : "Automated Documents"/"rfds"
    let shared_drive = drive_client.get_drive_by_name("Automated Documents").await.unwrap();
    let drive_id = shared_drive.id.to_string();

    // Get the directory by the name.
    let drive_rfd_dir = drive_client.get_file_by_name(&drive_id, "interview_packets").await.unwrap();
    let parent_id = drive_rfd_dir.get(0).unwrap().id.to_string();

    // Iterate over each user we have in gsuite and download their materials
    // locally.
    let employees = Users::get_from_db(&db);
    for employee in employees {
        if employee.is_system_account {
            continue;
        }

        // Get their application materials.
        let mut materials_url = "".to_string();
        for (_, sheet_id) in get_sheets_map() {
            let applicant = Applicant::get_from_db(&db, employee.recovery_email.to_string(), sheet_id.to_string());
            if let Some(a) = applicant {
                materials_url = a.materials;
                break;
            }
        }

        if materials_url.is_empty() {
            println!("[interviews] could not find applicant with email {}", employee.recovery_email);
            continue;
        }

        // Let's download the contents of their materials locally.
        download_materials(&drive_client, &materials_url, &employee.username).await;
    }

    let mut packets: HashMap<String, Vec<String>> = HashMap::new();
    let interviews = ApplicantInterviews::get_from_db(&db);
    // So we have everyone's materials stored locally @ /{temp_dir}/{username}.pdf.
    // Let's compile the materials for candidates into one file.
    for interview in interviews {
        let mut args: Vec<String> = Default::default();
        if let Some(v) = packets.get(&interview.email) {
            args = v.clone();
        }

        if args.is_empty() {
            // Create the cover page.
            // TODO: add the right data here.
            let cover_html = r#"<html>
<body>
<p>
Thank you for your interest in Oxide!  We have enjoyed reading the materials
you submitted to Oxide, and we are looking forward to having more conversations
with you.
<p>
At Oxide, we believe that you should be choosing to work with us as much as
we are choosing to work with you: teamwork is one of our values, and the
inspiration that we draw from our colleagues forms an important part of our
motivation.  Because every Oxide employee (including the founders!) has
submitted written answers to the same questions, we are afforded a unique
opportunity to inform our conversations with you: by sharing an employees' Oxide
materials with you, <b>you can get to know Oxide employees</b> as much as we
get to know you.
<p>
In this document, you will find the Oxide materials of the people with whom
you will be talking:
<p>
<table>
thing1
thing2
thing3
</table>
<p>
It should go without saying that you should treat these materials in confidence,
but they are open within the walls of Oxide.
(That is, we have all read one another's materials.)
Feel free to print this packet out and refer to it during your conversations
with Oxide.
<p>
Let us know if you have any questions, and thank you again for your interest
in Oxide!
<p>
Sincerely,<br>
The Oxide Team
</body>
</html>"#;

            // Generate a cover for the packet.
            let mut cover_path = env::temp_dir();
            cover_path.push(format!("{}.html", interview.email.to_string()));
            let mut file = fs::File::create(&cover_path).unwrap();
            file.write_all(&cover_html.as_bytes()).unwrap();
            let mut cover_output = env::temp_dir();
            cover_output.push(format!("{}.pdf", interview.email.to_string()));
            // Convert it to a PDF with pandoc.
            let mut pandoc = pandoc::new();
            pandoc.add_input(&cover_path);
            pandoc.set_output(OutputKind::File(cover_output.clone()));
            pandoc.execute().unwrap();

            // Add the header to our string.
            args = vec![cover_output.to_str().unwrap().to_string()];
        }

        // Iterate over the interviewees and add their packet to our list of packets.
        for interviewer in interview.interviewers {
            let username = interviewer.trim_end_matches(GSUITE_DOMAIN).trim_end_matches(DOMAIN).trim_end_matches('@').trim().to_string();

            // Generate a header for the interviewee.
            let mut html_path = env::temp_dir();
            html_path.push(format!("{}-{}.html", interview.email.to_string(), username));
            let mut file = fs::File::create(&html_path).unwrap();
            // TODO: add the date and time and the real name here.
            file.write_all(&format!("<html><body><table><tr><td><h1>{}</h1></table></html>", username).as_bytes()).unwrap();
            let mut header_output = env::temp_dir();
            header_output.push(format!("{}-{}.pdf", interview.email.to_string(), username));
            // Convert it to a PDF with pandoc.
            let mut pandoc = pandoc::new();
            pandoc.add_input(&html_path);
            pandoc.set_output(OutputKind::File(header_output.clone()));
            pandoc.execute().unwrap();

            // Add the header to our string.
            args.push(header_output.to_str().unwrap().to_string());

            // Get the path to the materials.
            let mut materials = env::temp_dir();
            materials.push(format!("{}.pdf", username));
            args.push(materials.to_str().unwrap().to_string());

            // Push it onto our array.
            packets.insert(interview.email.to_string(), args.to_vec());
        }
    }

    // Concatenate all the files.
    for (applicant_email, mut packet_args) in packets {
        let mut applicant_name = String::new();
        for (_, sheet_id) in get_sheets_map() {
            let applicant = Applicant::get_from_db(&db, applicant_email.to_string(), sheet_id.to_string());
            if let Some(a) = applicant {
                applicant_name = a.name;
                break;
            }
        }
        let mut output = env::temp_dir();
        output.push(format!("Interview Packet - {}.pdf", applicant_name));
        let filename = output.to_str().unwrap().to_string();
        packet_args.push(filename.to_string());

        // Extract the text from the PDF
        let cmd_output = Command::new("pdfunite").args(&packet_args).output().unwrap();

        match fs::read_to_string(output.clone()) {
            Ok(_) => (),
            Err(e) => {
                println!("[applicants] running pdfunite failed: {} ", e);
                stdout().write_all(&cmd_output.stdout).unwrap();
                stderr().write_all(&cmd_output.stderr).unwrap();
            }
        };

        let mut f = fs::File::open(&output).unwrap();
        let mut buffer = Vec::new();
        // read the whole file
        f.read_to_end(&mut buffer).unwrap();

        // Create or update the file in the google_drive.
        drive_client.create_or_upload_file(&drive_id, &parent_id, &filename, "application/pdf", &buffer).await.unwrap();
    }
}

/// Download materials file from Google drive and save it as a pdf under the persons username.
#[instrument(skip(drive_client))]
#[inline]
pub async fn download_materials(drive_client: &GoogleDrive, url: &str, username: &str) {
    let id = url.replace("https://drive.google.com/open?id=", "");

    // Get information about the file.
    let drive_file = drive_client.get_file_by_id(&id).await.unwrap();
    let mime_type = drive_file.mime_type;
    let name = drive_file.name;

    let mut path = env::temp_dir();
    let mut output = env::temp_dir();

    if mime_type == "application/pdf" {
        // Get the PDF contents from Drive.
        let contents = drive_client.download_file_by_id(&id).await.unwrap();

        path.push(format!("{}.pdf", username));

        let mut file = fs::File::create(&path).unwrap();
        file.write_all(&contents).unwrap();
        return;
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
                    println!("[applicants] zip file {} comment: {}", i, comment);
                }
            }

            if (&*file.name()).ends_with('/') {
                println!("[applicants] zip file {} extracted to \"{}\"", i, output.as_path().display());
                fs::create_dir_all(&output).unwrap();
            } else {
                println!("[applicants] zip file {} extracted to \"{}\" ({} bytes)", i, output.as_path().display(), file.size());

                if let Some(p) = output.parent() {
                    if !p.exists() {
                        fs::create_dir_all(&p).unwrap();
                    }
                }
                let mut outfile = fs::File::create(&output).unwrap();
                copy(&mut file, &mut outfile).unwrap();

                let file_name = output.to_str().unwrap();
                if (!output.is_dir()) && (file_name.ends_with("responses.pdf") || file_name.ends_with("OxideQuestions.pdf") || file_name.ends_with("Questionnaire.pdf")) {
                    let mut new_path = env::temp_dir();
                    new_path.push(format!("{}.pdf", username));
                    // Move the file to what we really want the output file to be.
                    fs::rename(&output, &new_path).unwrap();
                }
            }
        }
        return;
    }

    // Anything else let's use pandoc to convert it to a pdf.
    println!("Converting `{}` to a PDF", name);
    let contents = drive_client.download_file_by_id(&id).await.unwrap();
    path.push(name.to_string());

    let mut file = fs::File::create(&path).unwrap();
    file.write_all(&contents).unwrap();

    output.push(format!("{}.pdf", username));

    let mut pandoc = pandoc::new();
    pandoc.add_input(&path);
    pandoc.set_output(OutputKind::File(output.clone()));
    pandoc.execute().unwrap();
}

#[cfg(test)]
mod tests {
    use crate::db::Database;
    use crate::interviews::{compile_packets, refresh_interviews};

    #[ignore]
    #[tokio::test(threaded_scheduler)]
    async fn test_cron_interviews() {
        let db = Database::new();
        //refresh_interviews(&db).await;
        compile_packets(&db).await;
    }
}
