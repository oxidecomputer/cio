use std::env;
use std::error;
use std::fmt;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::rc::Rc;

use chrono::{Duration, Utc};
use reqwest::blocking::{get, Client, Request};
use reqwest::{header, Method, StatusCode, Url};
use serde::Serialize;

use crate::zoom::auth;
use crate::zoom::core::{
    APIResponse, Building, CreateUserOpts, LoginType, Meeting, Room, UpdateBuildingRequest,
    UpdateRoomRequest, UpdateUserOpts, User, UserInfo,
};

const ENDPOINT: &str = "https://api.zoom.us/v2/";

pub struct Zoom {
    key: String,
    secret: String,
    account_id: String,

    token: String,

    client: Rc<Client>,
}

impl Zoom {
    // Create a new Zoom client struct. It takes a type that can convert into
    // an &str (`String` or `Vec<u8>` for example). As long as the function is
    // given a valid API Key and Secret your requests will work.
    pub fn new<K, S, A>(key: K, secret: S, account_id: A) -> Self
    where
        K: ToString,
        S: ToString,
        A: ToString,
    {
        // Get the token.
        let token = auth::token(key.to_string(), secret.to_string());

        let client = Client::builder().build();
        match client {
            Ok(c) => Self {
                key: key.to_string(),
                secret: secret.to_string(),
                account_id: account_id.to_string(),
                token: token.to_string(),
                client: Rc::new(c),
            },
            Err(e) => panic!("creating client failed: {:?}", e),
        }
    }

    pub fn new_from_env() -> Self {
        let key = env::var("ZOOM_API_KEY").unwrap();
        let secret = env::var("ZOOM_API_SECRET").unwrap();
        let account_id = env::var("ZOOM_ACCOUNT_ID").unwrap();

        return Zoom::new(key, secret, account_id);
    }

    // Get the currently set API key.
    pub fn get_key(&self) -> &str {
        &self.key
    }

    // Get the currently set API secret.
    pub fn get_secret(&self) -> &str {
        &self.secret
    }

    // Get the currently set authorization token.
    pub fn get_token(&self) -> &str {
        &self.token
    }

    pub fn request<B>(
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
        let url: Url;
        if !path.starts_with("http") {
            // Build the URL from our endpoint instead since a full URL was not
            // passed.
            let base = Url::parse(ENDPOINT).unwrap();
            url = base.join(&path).unwrap();
        } else {
            // Parse the full URL.
            url = Url::parse(&path).unwrap();
        }

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
        let request = rb.build().unwrap();

        return request;
    }

    pub fn list_users(&self) -> Result<Vec<User>, APIError> {
        // Build the request.
        let request = self.request(
            Method::GET,
            "users".to_string(),
            {},
            Some(vec![
                ("page_size", "100".to_string()),
                ("page_number", "1".to_string()),
            ]),
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().unwrap(),
                })
            }
        };

        // Try to deserialize the response.
        let r: APIResponse = resp.json().unwrap();

        return Ok(r.users.unwrap());
    }

    pub fn get_user_with_login(
        &self,
        email: String,
        login_type: LoginType,
    ) -> Result<User, APIError> {
        // Build the request.
        let request = self.request(
            Method::GET,
            format!("users/{}", email),
            {},
            Some(vec![("login_type", login_type.to_string())]),
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                let body = resp.text().unwrap();

                if body.contains("1001") && login_type != LoginType::Google {
                    // Try this request again with Google login type.
                    return self.get_user_with_login(email, LoginType::Google);
                }

                return Err(APIError {
                    status_code: s,
                    body: body,
                });
            }
        };

        // Try to deserialize the response.
        let user: User = resp.json().unwrap();

        return Ok(user);
    }

    pub fn get_user(&self, email: String) -> Result<User, APIError> {
        // By default try the Zoom login type.
        return self.get_user_with_login(email, LoginType::Zoom);
    }

    pub fn create_user(
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
                    first_name: first_name,
                    last_name: last_name,
                    email: email,
                    // Type Pro.
                    typev: 2,
                },
            },
            None,
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::CREATED => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().unwrap(),
                })
            }
        };

        // Try to deserialize the response.
        let user: User = resp.json().unwrap();

        return Ok(user);
    }

    pub fn update_user(
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
                first_name: first_name,
                last_name: last_name,
                use_pmi: use_pmi,
                vanity_name: vanity_name,
            },
            Some(vec![("login_type", "100".to_string())]),
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::NO_CONTENT => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().unwrap(),
                })
            }
        };

        Ok(())
    }

    pub fn list_rooms(&self) -> Result<Vec<Room>, APIError> {
        // Build the request.
        let request = self.request(
            Method::GET,
            "rooms".to_string(),
            {},
            Some(vec![("page_size", "100".to_string())]),
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().unwrap(),
                })
            }
        };

        // Try to deserialize the response.
        let r: APIResponse = resp.json().unwrap();

        return Ok(r.rooms.unwrap());
    }

    pub fn update_room(&self, room: Room) -> Result<(), APIError> {
        let id = room.clone().id.unwrap();

        // Build the request.
        let request = self.request(
            Method::PATCH,
            format!("rooms/{}", id),
            UpdateRoomRequest { basic: room },
            None,
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::NO_CONTENT => (),
            s => {
                let body = resp.text().unwrap();

                if body.contains("This conference room already has a Zoom Room account") {
                    // Ignore the duplicate error.
                    return Ok(());
                }

                return Err(APIError {
                    status_code: s,
                    body: body,
                });
            }
        };

        Ok(())
    }

    pub fn create_room(&self, room: Room) -> Result<Room, APIError> {
        // Build the request.
        let request = self.request(Method::POST, "rooms".to_string(), room, None);

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::CREATED => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().unwrap(),
                });
            }
        };

        // Try to deserialize the response.
        return Ok(resp.json().unwrap());
    }

    pub fn list_buildings(&self) -> Result<Vec<Building>, APIError> {
        // Build the request.
        let request = self.request(
            Method::GET,
            "rooms/locations".to_string(),
            {},
            Some(vec![
                ("page_size", "100".to_string()),
                ("type", "building".to_string()),
            ]),
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().unwrap(),
                });
            }
        };

        // Try to deserialize the response.
        let r: APIResponse = resp.json().unwrap();

        return Ok(r.locations.unwrap());
    }

    pub fn create_building(&self, mut building: Building) -> Result<Building, APIError> {
        // Set the parent location to the account id.
        // That is the root.
        building.parent_location_id = Some(self.account_id.to_string());

        // Build the request.
        let request = self.request(Method::POST, "rooms/locations".to_string(), building, None);

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::CREATED => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().unwrap(),
                });
            }
        };

        // Try to deserialize the response.
        return Ok(resp.json().unwrap());
    }

    pub fn update_building(&self, mut building: Building) -> Result<(), APIError> {
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

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::NO_CONTENT => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().unwrap(),
                })
            }
        };

        Ok(())
    }

    /// List cloud recordings available on an account.
    /// From: https://marketplace.zoom.us/docs/api-reference/zoom-api/cloud-recording/getaccountcloudrecording
    /// This assumes the caller is an admin.
    pub fn list_recordings_as_admin(&self) -> Result<Vec<Meeting>, APIError> {
        let now = Utc::now();
        let weeks = Duration::weeks(3);

        // Build the request.
        let request = self.request(
            Method::GET,
            "accounts/me/recordings".to_string(),
            {},
            Some(vec![
                ("page_size", "100".to_string()),
                ("from", now.checked_sub_signed(weeks).unwrap().to_rfc3339()),
                ("to", now.to_rfc3339()),
            ]),
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().unwrap(),
                })
            }
        };

        // Try to deserialize the response.
        let r: APIResponse = resp.json().unwrap();

        return Ok(r.meetings.unwrap());
    }

    pub fn download_recording_to_file(
        &self,
        download_url: String,
        file: PathBuf,
    ) -> Result<(), APIError> {
        // Build the request.
        // TODO: add this back in if Zoom add auth to recordings... WOW.
        // let request = self.request(Method::GET, download_url, {}, None);

        // let resp = self.client.execute(request).unwrap();
        let resp = get(&download_url).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().unwrap(),
                })
            }
        };

        // Create each directory.
        fs::create_dir_all(file.parent().unwrap()).unwrap();

        // Write to the file.
        let mut f = fs::File::create(file.clone()).unwrap();
        f.write_all(resp.text().unwrap().as_bytes()).unwrap();
        Ok(())
    }

    pub fn delete_meeting_recordings(&self, meeting_id: i64) -> Result<(), APIError> {
        // Build the request.
        let request = self.request(
            Method::DELETE,
            format!("meetings/{}/recordings", meeting_id),
            {},
            None,
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::NO_CONTENT => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().unwrap(),
                })
            }
        };

        Ok(())
    }
}

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
