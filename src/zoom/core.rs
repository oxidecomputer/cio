use std::fmt;

use serde::{Deserialize, Serialize};

use crate::core::{BuildingConfig, ResourceConfig};

#[derive(Debug, Serialize, Deserialize)]
pub struct APIResponse {
    /// The number of pages returned for the request made.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_count: Option<i64>,
    /// The current page number of returned records.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_number: Option<i64>,
    /// The number of records returned within a single API call.
    pub page_size: i64,
    /// The total number of all the records available across pages.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_records: Option<i64>,
    /// The next page token is used to paginate through large result sets.
    /// A next page token will be returned whenever the set of available
    /// results exceeds the current page size. The expiration period for
    /// this token is 15 minutes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,

    /// List of room objects.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rooms: Option<Vec<Room>>,

    /// List of user objects.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub users: Option<Vec<User>>,

    /// List of building objects.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locations: Option<Vec<Building>>,

    /// List of meeting objects.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meetings: Option<Vec<Meeting>>,
}

/// From: https://marketplace.zoom.us/docs/api-reference/zoom-api/users/user
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: Option<String>,
    pub first_name: String,
    pub last_name: String,
    pub email: String,
    #[serde(rename = "type")]
    pub typev: i64,
    pub status: Option<String>,
    pub pmi: Option<i64>,
    pub timezone: Option<String>,
    pub dept: Option<String>,
    pub created_at: Option<String>,
    pub last_login_time: Option<String>,
    pub last_client_version: Option<String>,
    pub verified: Option<i64>,
    pub role_name: Option<String>,
    pub use_pmi: Option<bool>,
    pub language: Option<String>,
    pub vanity_url: Option<String>,
    pub personal_meeting_url: Option<String>,
    pub pic_url: Option<String>,
    pub account_id: Option<String>,
    pub host_key: Option<String>,
    pub job_title: Option<String>,
    pub company: Option<String>,
    pub location: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LoginType {
    Facebook = 0,
    Google = 1,
    API = 99,
    Zoom = 100,
    SSO = 101,
}

impl Default for LoginType {
    fn default() -> Self {
        LoginType::Zoom
    }
}

impl fmt::Display for LoginType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateUserOpts {
    pub action: String,
    pub user_info: UserInfo,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UserInfo {
    pub first_name: String,
    pub last_name: String,
    pub email: String,
    #[serde(rename = "type")]
    pub typev: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateUserOpts {
    pub first_name: String,
    pub last_name: String,
    pub use_pmi: bool,
    pub vanity_name: String,
}

/// From: https://marketplace.zoom.us/docs/api-reference/zoom-api/rooms/getzrprofile
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Room {
    /// Unique Identifier for the Zoom Room.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Name of the Zoom Room.
    pub name: String,
    /// Activation Code is the code that is used to complete the setup of the
    /// Zoom Room.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activation_code: Option<String>,
    /// Type of the Zoom Room.
    /// Allowed values: ZoomRoom, SchedulingDisplayOnly, DigitalSignageOnly
    #[serde(skip_serializing_if = "Option::is_none", rename = "type")]
    pub typev: Option<String>,
    /// Status of the Zoom Room.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// The email address to be used for reporting Zoom Room issues.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub support_email: Option<String>,
    /// The phone number to be used for reporting Zoom Room issues.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub support_phone: Option<String>,
    /// 1-16 digit number or characters that is used to secure your Zoom Rooms
    /// application. This code must be entered on your Zoom Room Controller to
    /// change settings or sign out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room_passcode: Option<String>,
    /// Require code to exit out of Zoom Rooms application to switch between
    /// other apps.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_code_to_ext: Option<bool>,
    /// Hide this Zoom Room from your Contact List.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hide_in_room_contacts: Option<bool>,
    /// Location ID of the lowest level location in the location hierarchy
    /// where the Zoom Room is to be added. For instance if the structure of
    /// the location hierarchy is set up as “country, states, city, campus,
    /// building, floor”, a room can only be added under the floor level
    /// location.
    /// See: https://support.zoom.us/hc/en-us/articles/115000342983-Zoom-Rooms-Location-Hierarchy
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location_id: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct UpdateRoomRequest {
    pub basic: Room,
}

impl Room {
    pub fn update(
        mut self,
        resource: ResourceConfig,
        passcode: String,
        location_id: String,
    ) -> Room {
        self.name = resource.name;
        self.room_passcode = Some(passcode);
        self.required_code_to_ext = Some(true);
        self.typev = Some("ZoomRoom".to_string());
        self.location_id = Some(location_id);
        self.hide_in_room_contacts = Some(false);

        return self;
    }
}

/// From: https://marketplace.zoom.us/docs/api-reference/zoom-api/rooms-location/getzrlocationprofile
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Building {
    /// Unique Identifier of the location.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Name of the location.
    pub name: String,
    /// Description about the location.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// ID (Unique Identifier) of the parent location. For instance, if a Zoom
    /// Room is located in Floor 1 of Building A, the location of Building A
    /// will be the parent location of Floor 1 and the parent_location_id of
    /// Floor 1 will be the ID of Building A.
    /// The value of parent_location_id of the top-level location (country)
    /// is the Account ID of the Zoom account.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_location_id: Option<String>,
    /// Type of location.
    #[serde(skip_serializing_if = "Option::is_none", rename = "type")]
    pub typev: Option<String>,
    /// Address of the location.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    /// The email address to be used for reporting Zoom Room issues.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub support_email: Option<String>,
    /// The phone number to be used for reporting Zoom Room issues.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub support_phone: Option<String>,
    /// 1-16 digit number or characters that is used to secure your Zoom Rooms
    /// application. This code must be entered on your Zoom Room Controller to
    /// change settings or sign out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room_passcode: Option<String>,
    /// Require code to exit out of Zoom Rooms application to switch between
    /// other apps.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_code_to_ext: Option<bool>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct UpdateBuildingRequest {
    pub basic: Building,
}

impl Building {
    pub fn update(
        mut self,
        building: BuildingConfig,
        passcode: String,
    ) -> Building {
        self.name = building.name;
        self.description = Some(building.description);
        self.address = Some(format!(
            "{}
{}, {} {} {}",
            building.address,
            building.city,
            building.state,
            building.zipcode,
            building.country
        ));
        self.room_passcode = Some(passcode);
        self.required_code_to_ext = Some(true);
        self.typev = Some("building".to_string());

        return self;
    }
}

/// From: https://marketplace.zoom.us/docs/api-reference/zoom-api/cloud-recording/getaccountcloudrecording
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Meeting {
    /// Universally Unique Identifier of a meeting instance. Each meeting instance will have its own meeting UUID.
    pub uuid: String,
    /// Meeting ID - Unique Identifier for a meeting, also known as Meeting Number.
    pub id: i64,
    /// User ID of the user who is set as the host of the meeting.
    pub host_id: String,
    /// Meeting topic.
    pub topic: String,
    /// The date and time at which the meeting started.
    pub start_time: String,
    /// The scheduled duration of the meeting.
    pub duration: i64,
    /// The total size of the meeting in bytes.
    pub total_size: i64,
    /// The total number of recordings retrieved from the account.
    pub recording_count: i32,
    pub recording_files: Vec<Recording>,
}

/// From: https://marketplace.zoom.us/docs/api-reference/zoom-api/cloud-recording/getaccountcloudrecording
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Recording {
    /// Recording ID. Identifier for the recording..
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// The date and time at which the recording started.
    pub recording_start: String,
    /// The date and time at which the recording ended.
    pub recording_end: String,
    /// The recording file type.
    pub file_type: FileType,
    /// The size of the recording file in bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_size: Option<i64>,
    /// The URL using which recording can be played.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub play_url: Option<String>,
    /// The URL using which the recording can be downloaded.
    pub download_url: String,
    /// The status of the recording.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// The recording type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recording_type: Option<String>,
    /// Universally unique identifier of the meeting instance that was being recorded.
    pub meeting_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub enum FileType {
    /// Video file of the recording.
    MP4,
    /// Audio-only file of the recording.
    M4A,
    /// Timestamp file of the recording.
    Timeline,
    /// Transcription file of the recording.
    Transcript,
    /// A TXT file containing in-meeting chat messages that were sent during
    /// the meeting.
    Chat,
    /// File containing closed captions of the recording.
    CC,
}

impl Default for FileType {
    fn default() -> Self {
        FileType::MP4
    }
}

impl FileType {
    // Returns the extension for each file type.
    pub fn to_extension(&self) -> String {
        match self {
            FileType::MP4 => return "-video.mp4".to_string(),
            FileType::M4A => return "-audio.m4a".to_string(),
            FileType::Timeline => return "-timeline.txt".to_string(),
            FileType::Transcript => return "-transcription.txt".to_string(),
            FileType::Chat => return "-chat.txt".to_string(),
            FileType::CC => return "-closed-captions.txt".to_string(),
        }
    }

    // Returns the mime type for each file type.
    pub fn get_mime_type(&self) -> String {
        match self {
            FileType::MP4 => return "video/mp4".to_string(),
            FileType::M4A => return "audio/m4a".to_string(),
            FileType::Timeline => return "text/plain".to_string(),
            FileType::Transcript => return "text/plain".to_string(),
            FileType::Chat => return "text/plain".to_string(),
            FileType::CC => return "text/plain".to_string(),
        }
    }
}
