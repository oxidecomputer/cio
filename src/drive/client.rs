use std::error;
use std::fmt;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use reqwest::blocking::{Client, Request};
use reqwest::{header, Method, StatusCode, Url};
use serde::Serialize;
use yup_oauth2::Token;

use crate::drive::core::{
    Drive as SharedDrive, DrivesResponse, File, FilesResponse,
};

const ENDPOINT: &str = "https://www.googleapis.com/drive/v3/";

pub struct Drive {
    token: Token,

    client: Rc<Client>,
}

impl Drive {
    // Create a new Drive client struct. It takes a type that can convert into
    // an &str (`String` or `Vec<u8>` for example). As long as the function is
    // given a valid API Key and Secret your requests will work.
    pub fn new(token: Token) -> Self {
        let client =
            Client::builder().timeout(Duration::from_secs(360)).build();
        match client {
            Ok(c) => Self {
                token: token,
                client: Rc::new(c),
            },
            Err(e) => panic!("creating client failed: {:?}", e),
        }
    }

    // Get the currently set authorization token.
    pub fn get_token(&self) -> &Token {
        &self.token
    }

    pub fn request<B>(
        &self,
        method: Method,
        path: String,
        body: B,
        query: Option<Vec<(&str, String)>>,
        content_length: u64,
        content: String,
        mime_type: &str,
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

        // Check if the token is expired and panic.
        if self.token.expired() {
            panic!("token is expired");
        }

        let bt = format!("Bearer {}", self.token.access_token);
        let bearer = header::HeaderValue::from_str(&bt).unwrap();

        // Set the default headers.
        let mut headers = header::HeaderMap::new();
        headers.append(header::AUTHORIZATION, bearer);
        if mime_type.len() < 1 {
            // Add the default mime type.
            headers.append(
                header::CONTENT_TYPE,
                header::HeaderValue::from_static(
                    "application/json; charset=UTF-8",
                ),
            );
        } else {
            // Add the mime type that was passed in.
            headers.append(
                header::CONTENT_TYPE,
                header::HeaderValue::from_bytes(mime_type.as_bytes()).unwrap(),
            );
        }

        if method == Method::POST && path == "files" && content_length > 0 {
            // We are likely uploading a file so add the right headers.
            headers.append(
                header::HeaderName::from_static("X-Upload-Content-Type"),
                header::HeaderValue::from_static("application/octet-stream"),
            );
            headers.append(
                header::HeaderName::from_static("X-Upload-Content-Length"),
                header::HeaderValue::from_bytes(
                    content_length.to_string().as_bytes(),
                )
                .unwrap(),
            );
        }

        let mut rb = self.client.request(method.clone(), url).headers(headers);

        match query {
            None => (),
            Some(val) => {
                rb = rb.query(&val);
            }
        }

        // Add the body, this is to ensure our GET and DELETE calls succeed.
        if method != Method::GET
            && method != Method::DELETE
            && content.len() < 1
        {
            rb = rb.json(&body);
        }

        if content.len() > 1 {
            // We are uploading a file so add that as the body.
            rb = rb.body(content);
        }

        // Build the request.
        let request = rb.build().unwrap();

        return request;
    }

    pub fn find_file_by_name(
        &self,
        drive_id: &str,
        name: &str,
    ) -> Result<Vec<File>, APIError> {
        // Build the request.
        let request = self.request(
            Method::GET,
            "files".to_string(),
            {},
            Some(vec![
                ("corpora", "drive".to_string()),
                ("supportsAllDrives", "true".to_string()),
                ("includeItemsFromAllDrives", "true".to_string()),
                ("driveId", drive_id.to_string()),
                ("q", format!("name = '{}'", name)),
            ]),
            0,
            "".to_string(),
            "",
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
        let files_response: FilesResponse = resp.json().unwrap();

        return Ok(files_response.files);
    }

    pub fn list_drives(&self) -> Result<Vec<SharedDrive>, APIError> {
        // Build the request.
        let request = self.request(
            Method::GET,
            "drives".to_string(),
            {},
            Some(vec![("useDomainAdminAccess", "true".to_string())]),
            0,
            "".to_string(),
            "",
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
        let drives_response: DrivesResponse = resp.json().unwrap();

        return Ok(drives_response.drives);
    }

    pub fn get_drive_by_name(
        &self,
        name: String,
    ) -> Result<SharedDrive, APIError> {
        let drives = self.list_drives().unwrap();

        for drive in drives {
            if drive.clone().name.unwrap() == name {
                return Ok(drive);
            }
        }

        return Err(APIError {
            status_code: StatusCode::NOT_FOUND,
            body: format!("could not find {}", name),
        });
    }

    pub fn create_folder(
        &self,
        drive_id: &str,
        parent_id: &str,
        name: &str,
    ) -> Result<String, APIError> {
        let folder_mime_type = "application/vnd.google-apps.folder";
        let mut file: File = Default::default();
        // Set the name,
        file.name = Some(name.to_string());
        file.mime_type = Some(folder_mime_type.to_string());
        if parent_id.len() > 0 {
            file.parents = Some(vec![parent_id.to_string()]);
        } else {
            file.parents = Some(vec![drive_id.to_string()]);
        }

        // Make the request and return the ID.
        let request = self.request(
            Method::POST,
            "files".to_string(),
            file,
            Some(vec![
                ("supportsAllDrives", "true".to_string()),
                ("includeItemsFromAllDrives", "true".to_string()),
            ]),
            0,
            "".to_string(),
            folder_mime_type,
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            StatusCode::CREATED => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().unwrap(),
                });
            }
        };

        // Try to deserialize the response.
        let response: File = resp.json().unwrap();

        return Ok(response.id.unwrap());
    }

    pub fn upload_file(
        &self,
        drive_id: &str,
        file: PathBuf,
        parent_id: &str,
        mime_type: &str,
    ) -> Result<(), APIError> {
        // Get the metadata for the file.
        let metadata = fs::metadata(file.clone()).unwrap();

        let mut f: File = Default::default();
        // Set the name,
        f.name = Some(file.file_name().unwrap().to_str().unwrap().to_string());
        f.mime_type = Some(mime_type.to_string());
        if parent_id.len() > 0 {
            f.parents = Some(vec![parent_id.to_string()]);
        } else {
            f.parents = Some(vec![drive_id.to_string()]);
        }

        // Build the request to get the URL upload location.
        let request = self.request(
            Method::POST,
            "https://www.googleapis.com/upload/drive/v3/files".to_string(),
            f,
            Some(vec![
                ("uploadType", "resumable".to_string()),
                ("supportsAllDrives", "true".to_string()),
                ("includeItemsFromAllDrives", "true".to_string()),
            ]),
            metadata.len(),
            "".to_string(),
            "",
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

        // Get the "Location" header.
        let location =
            resp.headers().get("Location").unwrap().to_str().unwrap();

        // Read the contents of the file.
        let contents = fs::read_to_string(file).unwrap();

        // Now upload the file to that location.
        let request = self.request(
            Method::PUT,
            location.to_string(),
            {},
            None,
            metadata.len(),
            contents,
            mime_type,
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            StatusCode::CREATED => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().unwrap(),
                });
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
