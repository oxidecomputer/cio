use std::collections::BTreeMap;
use std::env;

use chrono::DateTime;
use clap::{value_t, ArgMatches};
use log::{info, warn};

use crate::drive::client::Drive;
use crate::email::client::SendGrid;
use crate::utils::{get_gsuite_token, read_config_from_files};
use crate::zoom::client::Zoom;
use crate::zoom::core::{Building as ZoomBuilding, Room, User as ZoomUser};

/// The Zoom passcode for overriding the room configuration when you are in a room.
pub static ZOOM_PASSCODE: &'static str = "6274";

/**
 * Sync the configuration files with Zoom.
 *
 * This command does the following:
 *
 * - Create or update user's accounts
 * - Create or update Zoom buildings
 * - Create or update Zoom rooms
 * - Download any Zoom recordings and automatically upload them to Google drive.
 */
pub fn cmd_zoom_run(cli_matches: &ArgMatches) {
    let domain = value_t!(cli_matches, "domain", String).unwrap();

    // Initialize the Zoom client.
    let zoom = Zoom::new_from_env();

    // Get the GSuite token.
    let token = get_gsuite_token();

    // Get the GSuite Drive client.
    let drive = Drive::new(token.clone());

    // Get the current zoom users.
    info!("[zoom] getting current users...");
    let zu = zoom.list_users().unwrap();
    let mut zoom_users: BTreeMap<String, ZoomUser> = BTreeMap::new();
    for z in zu {
        zoom_users.insert(z.email.to_string(), z);
    }

    // Get the current zoom rooms.
    info!("[zoom] getting current rooms...");
    let zr = zoom.list_rooms().unwrap();
    let mut zoom_rooms: BTreeMap<String, Room> = BTreeMap::new();
    for z in zr {
        zoom_rooms.insert(z.name.replace(" ", "").to_string(), z);
    }

    // Get the current zoom buildings.
    info!("[zoom] getting current buildings...");
    let zb = zoom.list_buildings().unwrap();
    let mut zoom_buildings: BTreeMap<String, ZoomBuilding> = BTreeMap::new();
    for z in zb {
        zoom_buildings.insert(z.name.replace(" ", "").to_string(), z);
    }

    // Get the config.
    let config = read_config_from_files(cli_matches);

    // Create or update Zoom buildings.
    // TODO: make sure we delete any Zoom buildings not in the config file.
    for (id, building) in config.buildings {
        // Try to find the building in our list of zoom buildings.
        let mut zoom_building: ZoomBuilding = Default::default();
        match zoom_buildings.get(&id) {
            Some(val) => zoom_building = val.clone(),
            None => {
                // Create the Zoom building.
                zoom_building = zoom_building
                    .clone()
                    .update(building.clone(), ZOOM_PASSCODE.to_string());

                zoom_building = zoom.create_building(zoom_building).unwrap();

                info!("[zoom] created building: {}", id);
            }
        }

        // Update the Zoom building.
        zoom_building = zoom_building
            .clone()
            .update(building.clone(), ZOOM_PASSCODE.to_string());

        zoom.update_building(zoom_building).unwrap();

        info!("[zoom] updated building: {}", id);
    }

    // Create or update Zoom rooms.
    // TODO: make sure we delete any Zoom rooms not in the config file.
    for (id, resource) in config.resources {
        // If the resource is not a Zoom room, return early.
        match resource.is_zoom_room {
            Some(is_zoom_room) => {
                if !is_zoom_room {
                    return;
                }
            }
            None => return,
        }

        // Get the location id for the Zoom room.
        let mut location_id: String = "".to_string();
        match zoom_buildings.get(&resource.clone().building) {
            Some(val) => location_id = val.clone().id.unwrap(),
            None => warn!(
                "could not get zoom id for building: {}",
                resource.clone().building
            ),
        }

        // Try to find the room in our list of Zoom rooms.
        let mut zoom_room: Room = Default::default();
        match zoom_rooms.get(&id) {
            Some(val) => zoom_room = val.clone(),
            None => {
                // Create the Zoom room.
                zoom_room = zoom_room.clone().update(
                    resource.clone(),
                    ZOOM_PASSCODE.to_string(),
                    location_id.to_string(),
                );

                zoom_room = zoom.create_room(zoom_room).unwrap();

                info!("[zoom] created room: {}", id);
            }
        }

        // Update the Zoom room.
        zoom_room = zoom_room.clone().update(
            resource.clone(),
            ZOOM_PASSCODE.to_string(),
            location_id.to_string(),
        );

        zoom.update_room(zoom_room).unwrap();

        info!("[zoom] updated room: {}", id);
    }

    // Create or update Zoom users.
    // TODO: make sure we delete any Zoom users not in the config file.
    for (_, user) in config.users {
        match user.github {
            None => {
                // Continue early here.
                continue;
            }
            // The user has a GitHub so we can create them a Zoom account.
            Some(_) => (),
        }

        let email = format!("{}@{}", user.username, domain);

        // Check if we have that user already in Zoom.
        match zoom_users.get(&email) {
            Some(_) => (),
            None => {
                // Create the zoom user.
                zoom.create_user(
                    user.first_name.to_string(),
                    user.last_name.to_string(),
                    email.to_string(),
                )
                .unwrap();

                info!("[zoom]: created user for {}", email);
            }
        }

        // Update the Zoom user.
        let mut vanity = user.clone().github.unwrap().to_lowercase();
        if vanity.len() < 5 {
            if user.first_name.len() < 5 {
                vanity = format!(
                    "{}.{}",
                    user.first_name.to_string(),
                    user.last_name.to_string()
                )
                .to_lowercase();
            } else {
                vanity = user.first_name.to_lowercase();
            }
        }

        // Get the user to check their vanity URL.
        let current_zoom_user = zoom.get_user(email.to_string()).unwrap();
        match current_zoom_user.vanity_url {
            None => (),
            Some(val) => {
                if val.ends_with(&format!("/{}", vanity)) {
                    // Return early since the user already has the vanity URL.
                    return;
                }
            }
        }

        match current_zoom_user.last_login_time {
            Some(_) => (),
            None => {
                //  The user is pending so warn on that and return early.
                warn!(
                    "[zoom] account has not been activated by user {}",
                    email
                );
                return;
            }
        }

        // Update the user's vanity URL.
        zoom.update_user(
            user.first_name.to_string(),
            user.last_name.to_string(),
            email.to_string(),
            true,
            vanity.to_string(),
        )
        .unwrap();

        info!(
            "[zoom]: updated user for {} with vanity url {}",
            email, vanity
        );
    }

    // Get all the recordings for the Zoom account to move them to Google Drive
    // to be sorted.
    let meetings = zoom.list_recordings_as_admin().unwrap();
    for meeting in meetings {
        // Get the date of the meeting.
        let date = DateTime::parse_from_rfc3339(&meeting.start_time).unwrap();
        let date_str = date.format("%Y-%m-%d");

        // Create the name for the folder in drive for this meeting.
        let drive_folder_name = format!(
            "{}-{}",
            date.to_rfc3339(),
            meeting
                .topic
                .replace(" ", "-")
                .replace("'", "")
                .to_lowercase(),
        );

        // Get the path to the shared drive so we can upload to it.
        let d = drive
            .get_drive_by_name("Video Call Recordings".to_string())
            .unwrap();
        let drive_id = d.id.unwrap();

        // Get the correct parent folder for the uploads as the top-level parent.
        let folders = drive
            .find_file_by_name(&drive_id, "Zoom Dumps to be Sorted")
            .unwrap();

        if folders.len() < 1 {
            panic!("could not find the google drive folder for zoom dumps");
        }

        if folders.len() > 1 {
            panic!("found more than one matching folder: {:?}", folders);
        }

        // Create the directory for this meeting's uploads.
        let parent_id = drive
            .create_folder(
                &drive_id,
                folders[0].id.as_ref().unwrap(),
                &drive_folder_name,
            )
            .unwrap();

        // Download the recording files.
        for recording in meeting.recording_files {
            let recording_name =
                format!("{}{}", date_str, recording.file_type.to_extension());

            // Create a temporary directory for the file.
            let tmp_dir = env::temp_dir();
            let download_path = tmp_dir.join(recording_name);

            // Download the file.
            // TODO: add a progress bar.
            info!(
                "[zoom] meeting \"{}\" -> downloading recording {} to {}\nThis might take a bit...",
                meeting.topic,
                recording.download_url,
                download_path.to_str().unwrap()
            );
            zoom.download_recording_to_file(
                recording.download_url,
                download_path.clone(),
            )
            .unwrap();

            // Get the mime type.
            let mime_type = recording.file_type.get_mime_type();

            // Upload the recording to Google drive.
            // TODO: add a progress bar.
            info!(
                "[drive] uploading recording {} to Google drive at \"{}\"\nThis might take a bit...",
                download_path.to_str().unwrap(),
                drive_folder_name.to_string(),
            );
            drive
                .upload_file(&drive_id, download_path, &parent_id, &mime_type)
                .unwrap();
        }

        // Delete the recording in Zoom so that we do not upload it again.
        zoom.delete_meeting_recordings(meeting.id).unwrap();
        info!(
            "[zoom] deleted meeting records in Zoom since they are now in Google drive at {}",
            drive_folder_name
        );

        // Send email.
        let drive_url =
            "https://drive.google.com/drive/u/1/folders/11IzpZL9zJB5mZs53e-gYH0IIizVx-Giw";
        // Initialize the SendGrid client.
        let sendgrid_client = SendGrid::new_from_env();
        sendgrid_client.send_uploaded_zoom_dump(drive_url);
    }
}
