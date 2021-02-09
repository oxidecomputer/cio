#![allow(clippy::from_over_into)]
use std::collections::{BTreeMap, HashMap};
use std::env;
use std::fs;
use std::io::{copy, Write};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use google_drive::GoogleDrive;
use gsuite_api::GSuite;
use lopdf::{Bookmark, Document, Object, ObjectId};
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
                    if a.name.is_empty() {
                        // Sometimes we get back an empty name somehow...?
                        continue;
                    }
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

            if interview.interviewers.is_empty() {
                // Continue early.
                // We only care about interviews where the candidate has interviewers.
                continue;
            }
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
                if !materials_url.is_empty() {
                    // Sometimes we get back an empty materials url somehow...?
                    break;
                }
            }
        }

        if materials_url.is_empty() {
            println!("[interviews] could not find applicant with email {}", employee.recovery_email);
            continue;
        }

        // Let's download the contents of their materials locally.
        download_materials(&drive_client, &materials_url, &employee.username).await;
    }

    let interviews = ApplicantInterviews::get_from_db(&db);

    // Let's group the interviewers into each interview.
    let mut interviewers: HashMap<String, Vec<(User, DateTime<Tz>, DateTime<Tz>)>> = HashMap::new();
    for interview in interviews.clone() {
        let mut existing: Vec<(User, DateTime<Tz>, DateTime<Tz>)> = Default::default();
        if let Some(v) = interviewers.get(&interview.email) {
            existing = v.clone();
        }
        for interviewer in interview.interviewers {
            let username = interviewer.trim_end_matches(GSUITE_DOMAIN).trim_end_matches(DOMAIN).trim_end_matches('@').trim().to_string();
            let u = User::get_from_db(db, username.to_string());
            if u.is_none() {
                // Likeley means the person quit or no longer works here.
                continue;
            }
            let user = u.unwrap();
            existing.push((
                user,
                interview.start_time.with_timezone(&chrono_tz::US::Pacific),
                interview.end_time.with_timezone(&chrono_tz::US::Pacific),
            ));
            // Sort the interviewers by the meeting start_time.
            existing.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
            interviewers.insert(interview.email.to_string(), existing.clone());
        }
    }

    // So we have everyone's materials stored locally @ /{temp_dir}/{username}.pdf.
    // Let's compile the materials for candidates into one file.
    let mut packets: HashMap<String, Vec<String>> = HashMap::new();
    for (email, itrs) in interviewers {
        let mut args: Vec<String> = Default::default();

        // Create the cover page.
        let mut user_html = "".to_string();
        for (i, start_time, end_time) in itrs.clone() {
            user_html += &format!(
                "<tr><td>{}</td><td>{} - {}</td></tr>",
                i.full_name(),
                start_time.format("%A, %B %e from %l:%M%P"),
                end_time.format("%l:%M%P %Z")
            );
        }
        let cover_html = format!(
            r#"<html>
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
{}
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
</html>"#,
            user_html
        );

        // Generate a cover for the packet.
        let mut cover_path = env::temp_dir();
        cover_path.push(format!("{}.html", email.to_string()));
        let mut file = fs::File::create(&cover_path).unwrap();
        file.write_all(&cover_html.as_bytes()).unwrap();
        let mut cover_output = env::temp_dir();
        cover_output.push(format!("{}.pdf", email.to_string()));
        // Convert it to a PDF with pandoc.
        let mut pandoc = pandoc::new();
        pandoc.add_input(&cover_path);
        pandoc.set_output(OutputKind::File(cover_output.clone()));
        pandoc.execute().unwrap();

        // Add the header to our string.
        args.push(cover_output.to_str().unwrap().to_string());

        // Iterate over the interviewees and add their packet to our list of packets.
        for (i, start_time, end_time) in itrs {
            let username = i.username.to_string();

            // Generate a header for the interviewee.
            let mut html_path = env::temp_dir();
            html_path.push(format!("{}-{}.html", email.to_string(), username));
            let mut file = fs::File::create(&html_path).unwrap();
            // TODO: add the date and time and the real name here.
            file.write_all(
                &format!(
                    "<html><body><table><tr><td><h1>{}</h1></td></tr><tr><td><p>{} - {}</p></td></tr></table></html>",
                    i.full_name(),
                    start_time.format("%A, %B %e from %l:%M%P"),
                    end_time.format("%l:%M%P %Z")
                )
                .as_bytes(),
            )
            .unwrap();
            let mut header_output = env::temp_dir();
            header_output.push(format!("{}-{}.pdf", email.to_string(), username));
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
            packets.insert(email.to_string(), args.to_vec());
        }
    }

    // Concatenate all the files.
    for (applicant_email, packet_args) in packets {
        for (_, sheet_id) in get_sheets_map() {
            let a = Applicant::get_from_db(&db, applicant_email.to_string(), sheet_id.to_string());
            if let Some(mut applicant) = a {
                if !applicant.name.is_empty() {
                    // Sometimes we get back an empty name somehow...?
                    continue;
                }

                let filename = format!("Interview Packet - {}.pdf", applicant.name);

                let buffer = combine_pdfs(packet_args);

                // Create or update the file in the google_drive.
                drive_client.create_or_upload_file(&drive_id, &parent_id, &filename, "application/pdf", &buffer).await.unwrap();

                // Get the file in drive.
                let files = drive_client.get_file_by_name(&drive_id, &filename).await.unwrap();
                if !files.is_empty() {
                    applicant.interview_packet = format!("https://drive.google.com/open?id={}", files.get(0).unwrap().id);
                }
                applicant.update(&db).await;
                break;
            }
        }
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

/// Combine multiple pdfs into one pdf and return the byte stream of it.
#[instrument]
#[inline]
pub fn combine_pdfs(pdfs: Vec<String>) -> Vec<u8> {
    // Define a starting max_id (will be used as start index for object_ids)
    let mut max_id = 1;
    let mut pagenum = 1;
    // Collect all Documents Objects grouped by a map
    let mut documents_pages = BTreeMap::new();
    let mut documents_objects = BTreeMap::new();
    let mut document = Document::with_version("1.5");

    for pdf in pdfs {
        // Load the pdf as a file.
        let mut doc = Document::load(&pdf).unwrap();

        let mut first = false;
        doc.renumber_objects_with(max_id);

        max_id = doc.max_id + 1;

        documents_pages.extend(
            doc.get_pages()
                .into_iter()
                .map(|(_, object_id)| {
                    if !first {
                        let bookmark = Bookmark::new(format!("Page_{}", pagenum), [0.0, 0.0, 1.0], 0, object_id);
                        document.add_bookmark(bookmark, None);
                        first = true;
                        pagenum += 1;
                    }

                    (object_id, doc.get_object(object_id).unwrap().to_owned())
                })
                .collect::<BTreeMap<ObjectId, Object>>(),
        );
        documents_objects.extend(doc.objects);
    }

    // Catalog and Pages are mandatory
    let mut catalog_object: Option<(ObjectId, Object)> = None;
    let mut pages_object: Option<(ObjectId, Object)> = None;

    // Process all objects except "Page" type
    for (object_id, object) in documents_objects.iter() {
        // We have to ignore "Page" (as are processed later), "Outlines" and "Outline" objects
        // All other objects should be collected and inserted into the main Document
        match object.type_name().unwrap_or("") {
            "Catalog" => {
                // Collect a first "Catalog" object and use it for the future "Pages"
                catalog_object = Some((if let Some((id, _)) = catalog_object { id } else { *object_id }, object.clone()));
            }
            "Pages" => {
                // Collect and update a first "Pages" object and use it for the future "Catalog"
                // We have also to merge all dictionaries of the old and the new "Pages" object
                if let Ok(dictionary) = object.as_dict() {
                    let mut dictionary = dictionary.clone();
                    if let Some((_, ref object)) = pages_object {
                        if let Ok(old_dictionary) = object.as_dict() {
                            dictionary.extend(old_dictionary);
                        }
                    }

                    pages_object = Some((if let Some((id, _)) = pages_object { id } else { *object_id }, Object::Dictionary(dictionary)));
                }
            }
            "Page" => {}     // Ignored, processed later and separately
            "Outlines" => {} // Ignored, not supported yet
            "Outline" => {}  // Ignored, not supported yet
            _ => {
                document.objects.insert(*object_id, object.clone());
            }
        }
    }

    // If no "Pages" found abort
    if pages_object.is_none() {
        println!("[merge-pdfs] pages root not found.");

        return Default::default();
    }

    // Iter over all "Page" and collect with the parent "Pages" created before
    for (object_id, object) in documents_pages.iter() {
        if let Ok(dictionary) = object.as_dict() {
            let mut dictionary = dictionary.clone();
            dictionary.set("Parent", pages_object.as_ref().unwrap().0);

            document.objects.insert(*object_id, Object::Dictionary(dictionary));
        }
    }

    // If no "Catalog" found abort
    if catalog_object.is_none() {
        println!("[merge-pdfs] atalog root not found.");

        return Default::default();
    }

    let catalog_object = catalog_object.unwrap();
    let pages_object = pages_object.unwrap();

    // Build a new "Pages" with updated fields
    if let Ok(dictionary) = pages_object.1.as_dict() {
        let mut dictionary = dictionary.clone();

        // Set new pages count
        dictionary.set("Count", documents_pages.len() as u32);

        // Set new "Kids" list (collected from documents pages) for "Pages"
        dictionary.set("Kids", documents_pages.into_iter().map(|(object_id, _)| Object::Reference(object_id)).collect::<Vec<_>>());

        document.objects.insert(pages_object.0, Object::Dictionary(dictionary));
    }

    // Build a new "Catalog" with updated fields
    if let Ok(dictionary) = catalog_object.1.as_dict() {
        let mut dictionary = dictionary.clone();
        dictionary.set("Pages", pages_object.0);
        dictionary.remove(b"Outlines"); // Outlines not supported in merged PDFs

        document.objects.insert(catalog_object.0, Object::Dictionary(dictionary));
    }

    document.trailer.set("Root", catalog_object.0);

    // Update the max internal ID as wasn't updated before due to direct objects insertion
    document.max_id = document.objects.len() as u32;

    // Reorder all new Document objects
    document.renumber_objects();

    //Set any Bookmarks to the First child if they are not set to a page
    document.adjust_zero_pages();

    //Set all bookmarks to the PDF Object tree then set the Outlines to the Bookmark content map.
    if let Some(n) = document.build_outline() {
        if let Ok(x) = document.get_object_mut(catalog_object.0) {
            if let Object::Dictionary(ref mut dict) = x {
                dict.set("Outlines", Object::Reference(n));
            }
        }
    }

    document.compress();

    // Save the merged PDF
    let mut buffer = Vec::new();
    document.save_to(&mut buffer).unwrap();
    buffer
}

#[cfg(test)]
mod tests {
    use crate::db::Database;
    use crate::interviews::{compile_packets, refresh_interviews};

    #[ignore]
    #[tokio::test(threaded_scheduler)]
    async fn test_interviews() {
        let db = Database::new();
        refresh_interviews(&db).await;
        compile_packets(&db).await;
    }
}
