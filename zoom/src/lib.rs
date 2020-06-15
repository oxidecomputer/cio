/**
 * A rust library for interacting with the Zoom v2 API.
 *
 * For more information, the Zoom v2 API is documented at [marketplace.zoom.us/docs/api-reference/zoom-api](https://marketplace.zoom.us/docs/api-reference/zoom-api).
 */
use std::env;
use std::error;
use std::fmt;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::rc::Rc;

use chrono::{Duration, Utc};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use reqwest::{get, header, Client, Method, Request, StatusCode, Url};
use serde::{Deserialize, Serialize};

use cio::{BuildingConfig, ResourceConfig};

/// Endpoint for the Zoom API.
const ENDPOINT: &str = "https://api.zoom.us/v2/";

/// Entrypoint for interacting with the Zoom API.
pub struct Zoom {
    key: String,
    secret: String,
    account_id: String,

    token: String,

    client: Rc<Client>,
}

impl Zoom {
    /// Create a new Zoom client struct. It takes a type that can convert into
    /// an &str (`String` or `Vec<u8>` for example). As long as the function is
    /// given a valid API Key and Secret your requests will work.
    pub fn new<K, S, A>(key: K, secret: S, account_id: A) -> Self
    where
        K: ToString,
        S: ToString,
        A: ToString,
    {
        // Get the token.
        let token = token(key.to_string(), secret.to_string());

        let client = Client::builder().build();
        match client {
            Ok(c) => Self {
                key: key.to_string(),
                secret: secret.to_string(),
                account_id: account_id.to_string(),
                token,
                client: Rc::new(c),
            },
            Err(e) => panic!("creating client failed: {:?}", e),
        }
    }

    /// Create a new Zoom client struct from environment variables. It takes a
    /// type that can convert into an &str (`String` or `Vec<u8>` for example).
    /// As long as the function is given a valid API Key and Secret your requests
    /// will work.
    pub fn new_from_env() -> Self {
        let key = env::var("ZOOM_API_KEY").unwrap();
        let secret = env::var("ZOOM_API_SECRET").unwrap();
        let account_id = env::var("ZOOM_ACCOUNT_ID").unwrap();

        Zoom::new(key, secret, account_id)
    }

    /// Get the currently set API key.
    pub fn get_key(&self) -> &str {
        &self.key
    }

    /// Get the currently set API secret.
    pub fn get_secret(&self) -> &str {
        &self.secret
    }

    /// Get the currently set authorization token.
    pub fn get_token(&self) -> &str {
        &self.token
    }

    fn request<B>(
        &self,
        method: Method,
        path: String,
        body: B,
        query: Option<Vec<(&str, String)>>,
    ) -> Request
    where
        B: Serialize,
    {
        // Get the url.
        let url = if !path.starts_with("http") {
            // Build the URL from our endpoint instead since a full URL was not
            // passed.
            let base = Url::parse(ENDPOINT).unwrap();
            base.join(&path).unwrap()
        } else {
            // Parse the full URL.
            Url::parse(&path).unwrap()
        };

        let bt = format!("Bearer {}", self.token);
        let bearer = header::HeaderValue::from_str(&bt).unwrap();

        // Set the default headers.
        let mut headers = header::HeaderMap::new();
        headers.append(header::AUTHORIZATION, bearer);
        headers.append(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/json"),
        );

        let mut rb = self.client.request(method.clone(), url).headers(headers);

        match query {
            None => (),
            Some(val) => {
                rb = rb.query(&val);
            }
        }

        // Add the body, this is to ensure our GET and DELETE calls succeed.
        if method != Method::GET && method != Method::DELETE {
            rb = rb.json(&body);
        }

        // Build the request.
        rb.build().unwrap()
    }

    /// List users.
    pub async fn list_users(&self) -> Result<Vec<User>, APIError> {
        // Build the request.
        let request = self.request(
            Method::GET,
            "users".to_string(),
            (),
            Some(vec![
                ("page_size", "100".to_string()),
                ("page_number", "1".to_string()),
            ]),
        );

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                })
            }
        };

        // Try to deserialize the response.
        let r: APIResponse = resp.json().await.unwrap();

        Ok(r.users.unwrap())
    }

    async fn get_user_with_login(
        &self,
        email: String,
        login_type: LoginType,
    ) -> Result<User, APIError> {
        // Build the request.
        let request = self.request(
            Method::GET,
            format!("users/{}", email),
            (),
            Some(vec![("login_type", login_type.to_string())]),
        );

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                })
            }
        };

        // Try to deserialize the response.
        let user: User = resp.json().await.unwrap();

        Ok(user)
    }

    /// Get a user.
    pub async fn get_user(&self, email: String) -> Result<User, APIError> {
        // By default try the Zoom login type.
        match self.get_user_with_login(email.to_string(), LoginType::Zoom).await {
            Ok(user)=> Ok(user),
            Err(_) => {
                // Try this request again with Google login type.
                return self.get_user_with_login(email.to_string(), LoginType::Google).await;
            }
        }
    }

    /// Create a user.
    pub async fn create_user(
        &self,
        first_name: String,
        last_name: String,
        email: String,
    ) -> Result<User, APIError> {
        // Build the request.
        let request = self.request(
            Method::POST,
            "users".to_string(),
            CreateUserOpts {
                action: "create".to_string(),
                user_info: UserInfo {
                    first_name,
                    last_name,
                    email,
                    // Type Pro.
                    typev: 2,
                },
            },
            None,
        );

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::CREATED => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                })
            }
        };

        // Try to deserialize the response.
        let user: User = resp.json().await.unwrap();

        Ok(user)
    }

    /// Update a user.
    pub async fn update_user(
        &self,
        first_name: String,
        last_name: String,
        email: String,
        use_pmi: bool,
        vanity_name: String,
    ) -> Result<(), APIError> {
        // Build the request.
        let request = self.request(
            Method::PATCH,
            format!("users/{}", email),
            UpdateUserOpts {
                first_name,
                last_name,
                use_pmi,
                vanity_name,
            },
            Some(vec![("login_type", "100".to_string())]),
        );

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::NO_CONTENT => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                })
            }
        };

        Ok(())
    }

    /// List rooms.
    pub async fn list_rooms(&self) -> Result<Vec<Room>, APIError> {
        // Build the request.
        let request = self.request(
            Method::GET,
            "rooms".to_string(),
            (),
            Some(vec![("page_size", "100".to_string())]),
        );

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                })
            }
        };

        // Try to deserialize the response.
        let r: APIResponse = resp.json().await.unwrap();

        Ok(r.rooms.unwrap())
    }

    /// Update a room.
    pub async fn update_room(&self, room: Room) -> Result<(), APIError> {
        let id = room.clone().id.unwrap();

        // Build the request.
        let request = self.request(
            Method::PATCH,
            format!("rooms/{}", id),
            UpdateRoomRequest { basic: room },
            None,
        );

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::NO_CONTENT => (),
            s => {
                let body = resp.text().await.unwrap();

                if body.contains(
                    "This conference room already has a Zoom Room account",
                ) {
                    // Ignore the duplicate error.
                    return Ok(());
                }

                return Err(APIError {
                    status_code: s,
                    body,
                });
            }
        };

        Ok(())
    }

    /// Create a room.
    pub async fn create_room(&self, room: Room) -> Result<Room, APIError> {
        // Build the request.
        let request =
            self.request(Method::POST, "rooms".to_string(), room, None);

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::CREATED => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                });
            }
        };

        // Try to deserialize the response.
        Ok(resp.json().await.unwrap())
    }

    /// List buildings.
    pub async fn list_buildings(&self) -> Result<Vec<Building>, APIError> {
        // Build the request.
        let request = self.request(
            Method::GET,
            "rooms/locations".to_string(),
            (),
            Some(vec![
                ("page_size", "100".to_string()),
                ("type", "building".to_string()),
            ]),
        );

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                });
            }
        };

        // Try to deserialize the response.
        let r: APIResponse = resp.json().await.unwrap();

        Ok(r.locations.unwrap())
    }

    /// Create a building.
    pub async fn create_building(
        &self,
        mut building: Building,
    ) -> Result<Building, APIError> {
        // Set the parent location to the account id.
        // That is the root.
        building.parent_location_id = Some(self.account_id.to_string());

        // Build the request.
        let request = self.request(
            Method::POST,
            "rooms/locations".to_string(),
            building,
            None,
        );

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::CREATED => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                });
            }
        };

        // Try to deserialize the response.
        Ok(resp.json().await.unwrap())
    }

    /// Update a building.
    pub async fn update_building(
        &self,
        mut building: Building,
    ) -> Result<(), APIError> {
        let id = building.clone().id.unwrap();

        // Set the parent location to the account id.
        // That is the root.
        building.parent_location_id = Some(self.account_id.to_string());

        // Build the request.
        let request = self.request(
            Method::PATCH,
            format!("rooms/locations/{}", id),
            UpdateBuildingRequest { basic: building },
            None,
        );

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::NO_CONTENT => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                })
            }
        };

        Ok(())
    }

    /// List cloud recordings available on an account.
    /// From: https://marketplace.zoom.us/docs/api-reference/zoom-api/cloud-recording/getaccountcloudrecording
    /// This assumes the caller is an admin.
    pub async fn list_recordings_as_admin(&self) -> Result<Vec<Meeting>, APIError> {
        let now = Utc::now();
        let weeks = Duration::weeks(3);

        // Build the request.
        let request = self.request(
            Method::GET,
            "accounts/me/recordings".to_string(),
            (),
            Some(vec![
                ("page_size", "100".to_string()),
                ("from", now.checked_sub_signed(weeks).unwrap().to_rfc3339()),
                ("to", now.to_rfc3339()),
            ]),
        );

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                })
            }
        };

        // Try to deserialize the response.
        let r: APIResponse = resp.json().await.unwrap();

        Ok(r.meetings.unwrap())
    }

    /// Download a recording to a file.
    pub async fn download_recording_to_file(
        &self,
        download_url: String,
        file: PathBuf,
    ) -> Result<(), APIError> {
        // Build the request.
        // TODO: add this back in if Zoom add auth to recordings... WOW.
        // let request = self.request(Method::GET, download_url, {}, None);

        // let resp = self.client.execute(request).await.unwrap();
        let resp = get(&download_url).await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                })
            }
        };

        // Create each directory.
        fs::create_dir_all(file.parent().unwrap()).unwrap();

        // Write to the file.
        let mut f = fs::File::create(file).unwrap();
        f.write_all(resp.text().await.unwrap().as_bytes()).unwrap();
        Ok(())
    }

    /// Delete all the recordings for a meeting.
    pub async fn delete_meeting_recordings(
        &self,
        meeting_id: i64,
    ) -> Result<(), APIError> {
        // Build the request.
        let request = self.request(
            Method::DELETE,
            format!("meetings/{}/recordings", meeting_id),
            (),
            None,
        );

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::NO_CONTENT => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                })
            }
        };

        Ok(())
    }
}

/// Error type returned by our library.
pub struct APIError {
    pub status_code: StatusCode,
    pub body: String,
}

impl fmt::Display for APIError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "APIError: status code -> {}, body -> {}",
            self.status_code.to_string(),
            self.body
        )
    }
}

impl fmt::Debug for APIError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "APIError: status code -> {}, body -> {}",
            self.status_code.to_string(),
            self.body
        )
    }
}

// This is important for other errors to wrap this one.
impl error::Error for APIError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        // Generic error, underlying cause isn't tracked.
        None
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    iss: String,
    exp: usize,
}

fn token(key: String, secret: String) -> String {
    let claims = Claims {
        iss: key,
        exp: 10_000_000_000, // TODO: make this a value in seconds.
    };

    let mut header = Header::default();
    header.kid = Some("signing_key".to_owned());
    header.alg = Algorithm::HS256;

    match encode(&header, &claims, &EncodingKey::from_secret(secret.as_ref())) {
        Ok(t) => t,
        Err(e) => panic!("creating jwt failed: {}", e), // TODO: return the error.
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct APIResponse {
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

/// A user.
///
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

/// The login type for the user.
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
struct CreateUserOpts {
    pub action: String,
    pub user_info: UserInfo,
}

#[derive(Debug, Serialize, Deserialize)]
struct UserInfo {
    pub first_name: String,
    pub last_name: String,
    pub email: String,
    #[serde(rename = "type")]
    pub typev: i64,
}

#[derive(Debug, Serialize, Deserialize)]
struct UpdateUserOpts {
    pub first_name: String,
    pub last_name: String,
    pub use_pmi: bool,
    pub vanity_name: String,
}

/// A room.
///
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

impl Room {
    /// Update a room from a configuration.
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

        self
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct UpdateRoomRequest {
    pub basic: Room,
}

/// A building.
///
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
struct UpdateBuildingRequest {
    pub basic: Building,
}

impl Building {
    /// Update a building from a configuration.
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

        self
    }
}

/// A meeting.
///
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

/// A recording.
///
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

/// The type of file.
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
    /// Returns the extension for each file type.
    pub fn to_extension(&self) -> String {
        match self {
            FileType::MP4 => "-video.mp4".to_string(),
            FileType::M4A => "-audio.m4a".to_string(),
            FileType::Timeline => "-timeline.txt".to_string(),
            FileType::Transcript => "-transcription.txt".to_string(),
            FileType::Chat => "-chat.txt".to_string(),
            FileType::CC => "-closed-captions.txt".to_string(),
        }
    }

    /// Returns the mime type for each file type.
    pub fn get_mime_type(&self) -> String {
        match self {
            FileType::MP4 => "video/mp4".to_string(),
            FileType::M4A => "audio/m4a".to_string(),
            FileType::Timeline => "text/plain".to_string(),
            FileType::Transcript => "text/plain".to_string(),
            FileType::Chat => "text/plain".to_string(),
            FileType::CC => "text/plain".to_string(),
        }
    }
}
