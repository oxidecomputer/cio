/**
 * A rust library for interacting with the Google Drive v3 API.
 *
 * For more information, the Google Drive v3 API is documented at [developers.google.com/drive/api/v3/reference](https://developers.google.com/drive/api/v3/reference).
 */
use std::collections::HashMap;
use std::error;
use std::fmt;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use reqwest::{header, Client, Method, Request, StatusCode, Url};
use serde::{Deserialize, Serialize};
use yup_oauth2::AccessToken;

/// The endpoint for the Google Drive API.
const ENDPOINT: &str = "https://www.googleapis.com/drive/v3/";

/// Entrypoint for interacting with the Google Drive API.
pub struct GoogleDrive {
    token: AccessToken,

    client: Rc<Client>,
}

impl GoogleDrive {
    /// Create a new Drive client struct. It takes a type that can convert into
    /// an &str (`String` or `Vec<u8>` for example). As long as the function is
    /// given a valid API Key and Secret your requests will work.
    pub fn new(token: AccessToken) -> Self {
        let client =
            Client::builder().timeout(Duration::from_secs(360)).build();
        match client {
            Ok(c) => Self {
                token,
                client: Rc::new(c),
            },
            Err(e) => panic!("creating client failed: {:?}", e),
        }
    }

    /// Get the currently set authorization token.
    pub fn get_token(&self) -> &AccessToken {
        &self.token
    }

    fn request<B>(
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
        let url = if !path.starts_with("http") {
            // Build the URL from our endpoint instead since a full URL was not
            // passed.
            let base = Url::parse(ENDPOINT).unwrap();
            base.join(&path).unwrap()
        } else {
            // Parse the full URL.
            Url::parse(&path).unwrap()
        };

        // Check if the token is expired and panic.
        if self.token.is_expired() {
            panic!("token is expired");
        }

        let bt = format!("Bearer {}", self.token.as_str());
        let bearer = header::HeaderValue::from_str(&bt).unwrap();

        // Set the default headers.
        let mut headers = header::HeaderMap::new();
        headers.append(header::AUTHORIZATION, bearer);
        if mime_type.is_empty() {
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
            && content.is_empty()
        {
            rb = rb.json(&body);
        }

        if content.len() > 1 {
            // We are uploading a file so add that as the body.
            rb = rb.body(content);
        }

        // Build the request.
        rb.build().unwrap()
    }

    /// Get a file by it's name.
    pub async fn get_file_by_name(
        &self,
        drive_id: &str,
        name: &str,
    ) -> Result<Vec<File>, APIError> {
        // Build the request.
        let request = self.request(
            Method::GET,
            "files".to_string(),
            (),
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
        let files_response: FilesResponse = resp.json().await.unwrap();

        Ok(files_response.files)
    }

    /// List drives.
    pub async fn list_drives(&self) -> Result<Vec<Drive>, APIError> {
        // Build the request.
        let request = self.request(
            Method::GET,
            "drives".to_string(),
            (),
            Some(vec![("useDomainAdminAccess", "true".to_string())]),
            0,
            "".to_string(),
            "",
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
        let drives_response: DrivesResponse = resp.json().await.unwrap();

        Ok(drives_response.drives)
    }

    /// Get a drive by it's name.
    pub async fn get_drive_by_name(
        &self,
        name: String,
    ) -> Result<Drive, APIError> {
        let drives = self.list_drives().await.unwrap();

        for drive in drives {
            if drive.clone().name.unwrap() == name {
                return Ok(drive);
            }
        }

        Err(APIError {
            status_code: StatusCode::NOT_FOUND,
            body: format!("could not find {}", name),
        })
    }

    /// Create a folder.
    pub async fn create_folder(
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
        if !parent_id.is_empty() {
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

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
            StatusCode::CREATED => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                });
            }
        };

        // Try to deserialize the response.
        let response: File = resp.json().await.unwrap();

        Ok(response.id.unwrap())
    }

    /// Upload a file.
    pub async fn upload_file(
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
        if !parent_id.is_empty() {
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

        // Get the "Location" header.
        let location =
            resp.headers().get("Location").unwrap().to_str().unwrap();

        // Read the contents of the file.
        let contents = fs::read_to_string(file).unwrap();

        // Now upload the file to that location.
        let request = self.request(
            Method::PUT,
            location.to_string(),
            (),
            None,
            metadata.len(),
            contents,
            mime_type,
        );

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
            StatusCode::CREATED => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                });
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

/// From: https://developers.google.com/drive/api/v3/reference/files/list
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
struct FilesResponse {
    /// Identifies what kind of resource this is. Value: the fixed string "drive#fileList".
    pub kind: String,
    /// The page token for the next page of files. This will be absent if the end of the files list has been reached. If the token is rejected for any reason, it should be discarded, and pagination should be restarted from the first page of results.
    #[serde(rename = "nextPageToken", skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
    /// Whether the search process was incomplete. If true, then some search results may be missing, since all documents were not searched. This may occur when searching multiple drives with the "allDrives" corpora, but all corpora could not be searched. When this happens, it is suggested that clients narrow their query by choosing a different corpus such as "user" or "drive".
    #[serde(rename = "incompleteSearch")]
    pub incomplete_search: bool,
    /// The list of files. If nextPageToken is populated, then this list may be incomplete and an additional page of results should be fetched.
    pub files: Vec<File>,
}

/// From: https://developers.google.com/drive/api/v3/reference/drives/list
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
struct DrivesResponse {
    /// Identifies what kind of resource this is. Value: the fixed string "drive#driveList".
    pub kind: String,
    /// The page token for the next page of shared drives. This will be absent if the end of the list has been reached. If the token is rejected for any reason, it should be discarded, and pagination should be restarted from the first page of results.
    #[serde(rename = "nextPageToken", skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
    /// The list of shared drives. If nextPageToken is populated, then this list may be incomplete and an additional page of results should be fetched.
    pub drives: Vec<Drive>,
}

/// An image file and cropping parameters from which a background image for this shared drive is set. This is a write only field; it can only be set on drive.drives.update requests that don't set themeId. When specified, all fields of the backgroundImageFile must be set.
///
/// This type is not used in any activity, and only used as *part* of another schema.
///
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct DriveBackgroundImageFile {
    /// The width of the cropped image in the closed range of 0 to 1. This value represents the width of the cropped image divided by the width of the entire image. The height is computed by applying a width to height aspect ratio of 80 to 9. The resulting image must be at least 1280 pixels wide and 144 pixels high.
    pub width: Option<f32>,
    /// The Y coordinate of the upper left corner of the cropping area in the background image. This is a value in the closed range of 0 to 1. This value represents the vertical distance from the top side of the entire image to the top side of the cropping area divided by the height of the entire image.
    #[serde(rename = "yCoordinate")]
    pub y_coordinate: Option<f32>,
    /// The ID of an image file in Google Drive to use for the background image.
    pub id: Option<String>,
    /// The X coordinate of the upper left corner of the cropping area in the background image. This is a value in the closed range of 0 to 1. This value represents the horizontal distance from the left side of the entire image to the left side of the cropping area divided by the width of the entire image.
    #[serde(rename = "xCoordinate")]
    pub x_coordinate: Option<f32>,
}

/// A drive.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct Drive {
    /// A set of restrictions that apply to this shared drive or items inside this shared drive.
    pub restrictions: Option<DriveRestrictions>,
    /// The color of this shared drive as an RGB hex string. It can only be set on a drive.drives.update request that does not set themeId.
    #[serde(rename = "colorRgb")]
    pub color_rgb: Option<String>,
    /// A short-lived link to this shared drive's background image.
    #[serde(rename = "backgroundImageLink")]
    pub background_image_link: Option<String>,
    /// The name of this shared drive.
    pub name: Option<String>,
    /// The ID of the theme from which the background image and color will be set. The set of possible driveThemes can be retrieved from a drive.about.get response. When not specified on a drive.drives.create request, a random theme is chosen from which the background image and color are set. This is a write-only field; it can only be set on requests that don't set colorRgb or backgroundImageFile.
    #[serde(rename = "themeId")]
    pub theme_id: Option<String>,
    /// Identifies what kind of resource this is. Value: the fixed string "drive#drive".
    pub kind: Option<String>,
    /// Capabilities the current user has on this shared drive.
    pub capabilities: Option<DriveCapabilities>,
    /// An image file and cropping parameters from which a background image for this shared drive is set. This is a write only field; it can only be set on drive.drives.update requests that don't set themeId. When specified, all fields of the backgroundImageFile must be set.
    #[serde(rename = "backgroundImageFile")]
    pub background_image_file: Option<DriveBackgroundImageFile>,
    /// The time at which the shared drive was created (RFC 3339 date-time).
    #[serde(rename = "createdTime")]
    pub created_time: Option<String>,
    /// Whether the shared drive is hidden from default view.
    pub hidden: Option<bool>,
    /// The ID of this shared drive which is also the ID of the top level folder of this shared drive.
    pub id: Option<String>,
}

/// A set of restrictions that apply to this shared drive or items inside this shared drive.
///
/// This type is not used in any activity, and only used as *part* of another schema.
///
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct DriveRestrictions {
    /// Whether administrative privileges on this shared drive are required to modify restrictions.
    #[serde(rename = "adminManagedRestrictions")]
    pub admin_managed_restrictions: Option<bool>,
    /// Whether the options to copy, print, or download files inside this shared drive, should be disabled for readers and commenters. When this restriction is set to true, it will override the similarly named field to true for any file inside this shared drive.
    #[serde(rename = "copyRequiresWriterPermission")]
    pub copy_requires_writer_permission: Option<bool>,
    /// Whether access to this shared drive and items inside this shared drive is restricted to users of the domain to which this shared drive belongs. This restriction may be overridden by other sharing policies controlled outside of this shared drive.
    #[serde(rename = "domainUsersOnly")]
    pub domain_users_only: Option<bool>,
    /// Whether access to items inside this shared drive is restricted to its members.
    #[serde(rename = "driveMembersOnly")]
    pub drive_members_only: Option<bool>,
}

/// Capabilities the current user has on this Team Drive.
///
/// This type is not used in any activity, and only used as *part* of another schema.
///
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct TeamDriveCapabilities {
    /// Whether the current user can read the revisions resource of files in this Team Drive.
    #[serde(rename = "canReadRevisions")]
    pub can_read_revisions: Option<bool>,
    /// Whether the current user can copy files in this Team Drive.
    #[serde(rename = "canCopy")]
    pub can_copy: Option<bool>,
    /// Whether the current user can change the copyRequiresWriterPermission restriction of this Team Drive.
    #[serde(rename = "canChangeCopyRequiresWriterPermissionRestriction")]
    pub can_change_copy_requires_writer_permission_restriction: Option<bool>,
    /// Whether the current user can trash children from folders in this Team Drive.
    #[serde(rename = "canTrashChildren")]
    pub can_trash_children: Option<bool>,
    /// Whether the current user can change the domainUsersOnly restriction of this Team Drive.
    #[serde(rename = "canChangeDomainUsersOnlyRestriction")]
    pub can_change_domain_users_only_restriction: Option<bool>,
    /// Whether the current user can delete this Team Drive. Attempting to delete the Team Drive may still fail if there are untrashed items inside the Team Drive.
    #[serde(rename = "canDeleteTeamDrive")]
    pub can_delete_team_drive: Option<bool>,
    /// Whether the current user can rename this Team Drive.
    #[serde(rename = "canRenameTeamDrive")]
    pub can_rename_team_drive: Option<bool>,
    /// Whether the current user can comment on files in this Team Drive.
    #[serde(rename = "canComment")]
    pub can_comment: Option<bool>,
    /// Whether the current user can list the children of folders in this Team Drive.
    #[serde(rename = "canListChildren")]
    pub can_list_children: Option<bool>,
    /// Whether the current user can rename files or folders in this Team Drive.
    #[serde(rename = "canRename")]
    pub can_rename: Option<bool>,
    /// Whether the current user can delete children from folders in this Team Drive.
    #[serde(rename = "canDeleteChildren")]
    pub can_delete_children: Option<bool>,
    /// Whether the current user can add children to folders in this Team Drive.
    #[serde(rename = "canAddChildren")]
    pub can_add_children: Option<bool>,
    /// Whether the current user can share files or folders in this Team Drive.
    #[serde(rename = "canShare")]
    pub can_share: Option<bool>,
    /// Whether the current user can add members to this Team Drive or remove them or change their role.
    #[serde(rename = "canManageMembers")]
    pub can_manage_members: Option<bool>,
    /// Whether the current user can download files in this Team Drive.
    #[serde(rename = "canDownload")]
    pub can_download: Option<bool>,
    /// Whether the current user can change the teamMembersOnly restriction of this Team Drive.
    #[serde(rename = "canChangeTeamMembersOnlyRestriction")]
    pub can_change_team_members_only_restriction: Option<bool>,
    /// Whether the current user can change the background of this Team Drive.
    #[serde(rename = "canChangeTeamDriveBackground")]
    pub can_change_team_drive_background: Option<bool>,
    /// Deprecated - use canDeleteChildren or canTrashChildren instead.
    #[serde(rename = "canRemoveChildren")]
    pub can_remove_children: Option<bool>,
    /// Whether the current user can edit files in this Team Drive
    #[serde(rename = "canEdit")]
    pub can_edit: Option<bool>,
}

/// An image file and cropping parameters from which a background image for this Team Drive is set. This is a write only field; it can only be set on drive.teamdrives.update requests that don't set themeId. When specified, all fields of the backgroundImageFile must be set.
///
/// This type is not used in any activity, and only used as *part* of another schema.
///
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct TeamDriveBackgroundImageFile {
    /// The width of the cropped image in the closed range of 0 to 1. This value represents the width of the cropped image divided by the width of the entire image. The height is computed by applying a width to height aspect ratio of 80 to 9. The resulting image must be at least 1280 pixels wide and 144 pixels high.
    pub width: Option<f32>,
    /// The Y coordinate of the upper left corner of the cropping area in the background image. This is a value in the closed range of 0 to 1. This value represents the vertical distance from the top side of the entire image to the top side of the cropping area divided by the height of the entire image.
    #[serde(rename = "yCoordinate")]
    pub y_coordinate: Option<f32>,
    /// The ID of an image file in Drive to use for the background image.
    pub id: Option<String>,
    /// The X coordinate of the upper left corner of the cropping area in the background image. This is a value in the closed range of 0 to 1. This value represents the horizontal distance from the left side of the entire image to the left side of the cropping area divided by the width of the entire image.
    #[serde(rename = "xCoordinate")]
    pub x_coordinate: Option<f32>,
}

/// Capabilities the current user has on this shared drive.
///
/// This type is not used in any activity, and only used as *part* of another schema.
///
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct DriveCapabilities {
    /// Whether the current user can read the revisions resource of files in this shared drive.
    #[serde(rename = "canReadRevisions")]
    pub can_read_revisions: Option<bool>,
    /// Whether the current user can copy files in this shared drive.
    #[serde(rename = "canCopy")]
    pub can_copy: Option<bool>,
    /// Whether the current user can delete this shared drive. Attempting to delete the shared drive may still fail if there are untrashed items inside the shared drive.
    #[serde(rename = "canDeleteDrive")]
    pub can_delete_drive: Option<bool>,
    /// Whether the current user can change the copyRequiresWriterPermission restriction of this shared drive.
    #[serde(rename = "canChangeCopyRequiresWriterPermissionRestriction")]
    pub can_change_copy_requires_writer_permission_restriction: Option<bool>,
    /// Whether the current user can trash children from folders in this shared drive.
    #[serde(rename = "canTrashChildren")]
    pub can_trash_children: Option<bool>,
    /// Whether the current user can change the driveMembersOnly restriction of this shared drive.
    #[serde(rename = "canChangeDriveMembersOnlyRestriction")]
    pub can_change_drive_members_only_restriction: Option<bool>,
    /// Whether the current user can change the background of this shared drive.
    #[serde(rename = "canChangeDriveBackground")]
    pub can_change_drive_background: Option<bool>,
    /// Whether the current user can comment on files in this shared drive.
    #[serde(rename = "canComment")]
    pub can_comment: Option<bool>,
    /// Whether the current user can delete children from folders in this shared drive.
    #[serde(rename = "canDeleteChildren")]
    pub can_delete_children: Option<bool>,
    /// Whether the current user can list the children of folders in this shared drive.
    #[serde(rename = "canListChildren")]
    pub can_list_children: Option<bool>,
    /// Whether the current user can rename files or folders in this shared drive.
    #[serde(rename = "canRename")]
    pub can_rename: Option<bool>,
    /// Whether the current user can rename this shared drive.
    #[serde(rename = "canRenameDrive")]
    pub can_rename_drive: Option<bool>,
    /// Whether the current user can add children to folders in this shared drive.
    #[serde(rename = "canAddChildren")]
    pub can_add_children: Option<bool>,
    /// Whether the current user can share files or folders in this shared drive.
    #[serde(rename = "canShare")]
    pub can_share: Option<bool>,
    /// Whether the current user can add members to this shared drive or remove them or change their role.
    #[serde(rename = "canManageMembers")]
    pub can_manage_members: Option<bool>,
    /// Whether the current user can download files in this shared drive.
    #[serde(rename = "canDownload")]
    pub can_download: Option<bool>,
    /// Whether the current user can change the domainUsersOnly restriction of this shared drive.
    #[serde(rename = "canChangeDomainUsersOnlyRestriction")]
    pub can_change_domain_users_only_restriction: Option<bool>,
    /// Whether the current user can edit files in this shared drive
    #[serde(rename = "canEdit")]
    pub can_edit: Option<bool>,
}

/// A file.
///
/// From: https://developers.google.com/drive/api/v3/reference/files#resource
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct File {
    /// Whether this file has a thumbnail. This does not indicate whether the requesting app has access to the thumbnail. To check access, look for the presence of the thumbnailLink field.
    #[serde(rename = "hasThumbnail")]
    pub has_thumbnail: Option<bool>,
    /// The MIME type of the file.
    /// Google Drive will attempt to automatically detect an appropriate value from uploaded content if no value is provided. The value cannot be changed unless a new revision is uploaded.
    /// If a file is created with a Google Doc MIME type, the uploaded content will be imported if possible. The supported import formats are published in the About resource.
    #[serde(rename = "mimeType")]
    pub mime_type: Option<String>,
    /// The last time the file was modified by the user (RFC 3339 date-time).
    #[serde(rename = "modifiedByMeTime")]
    pub modified_by_me_time: Option<String>,
    /// A short-lived link to the file's thumbnail, if available. Typically lasts on the order of hours. Only populated when the requesting app can access the file's content.
    #[serde(rename = "thumbnailLink")]
    pub thumbnail_link: Option<String>,
    /// The thumbnail version for use in thumbnail cache invalidation.
    #[serde(rename = "thumbnailVersion")]
    pub thumbnail_version: Option<String>,
    /// Whether the file has been explicitly trashed, as opposed to recursively trashed from a parent folder.
    #[serde(rename = "explicitlyTrashed")]
    pub explicitly_trashed: Option<bool>,
    /// Whether the file was created or opened by the requesting app.
    #[serde(rename = "isAppAuthorized")]
    pub is_app_authorized: Option<bool>,
    /// Whether users with only writer permission can modify the file's permissions. Not populated for items in shared drives.
    #[serde(rename = "writersCanShare")]
    pub writers_can_share: Option<bool>,
    /// Whether the user owns the file. Not populated for items in shared drives.
    #[serde(rename = "ownedByMe")]
    pub owned_by_me: Option<bool>,
    /// The last time the file was viewed by the user (RFC 3339 date-time).
    #[serde(rename = "viewedByMeTime")]
    pub viewed_by_me_time: Option<String>,
    /// The ID of the file.
    pub id: Option<String>,
    /// The user who shared the file with the requesting user, if applicable.
    #[serde(rename = "sharingUser")]
    pub sharing_user: Option<User>,
    /// The size of the file's content in bytes. This is only applicable to files with binary content in Google Drive.
    pub size: Option<String>,
    /// Additional metadata about video media. This may not be available immediately upon upload.
    #[serde(rename = "videoMediaMetadata")]
    pub video_media_metadata: Option<FileVideoMediaMetadata>,
    /// The last user to modify the file.
    #[serde(rename = "lastModifyingUser")]
    pub last_modifying_user: Option<User>,
    /// The color for a folder as an RGB hex string. The supported colors are published in the folderColorPalette field of the About resource.
    /// If an unsupported color is specified, the closest color in the palette will be used instead.
    #[serde(rename = "folderColorRgb")]
    pub folder_color_rgb: Option<String>,
    /// A collection of arbitrary key-value pairs which are private to the requesting app.
    /// Entries with null values are cleared in update and copy requests.
    #[serde(rename = "appProperties")]
    pub app_properties: Option<HashMap<String, String>>,
    /// Capabilities the current user has on this file. Each capability corresponds to a fine-grained action that a user may take.
    pub capabilities: Option<FileCapabilities>,
    /// A collection of arbitrary key-value pairs which are visible to all apps.
    /// Entries with null values are cleared in update and copy requests.
    pub properties: Option<HashMap<String, String>>,
    /// A link for opening the file in a relevant Google editor or viewer in a browser.
    #[serde(rename = "webViewLink")]
    pub web_view_link: Option<String>,
    /// A monotonically increasing version number for the file. This reflects every change made to the file on the server, even those not visible to the user.
    pub version: Option<String>,
    /// The IDs of the parent folders which contain the file.
    /// If not specified as part of a create request, the file will be placed directly in the user's My Drive folder. If not specified as part of a copy request, the file will inherit any discoverable parents of the source file. Update requests must use the addParents and removeParents parameters to modify the parents list.
    pub parents: Option<Vec<String>>,
    /// The MD5 checksum for the content of the file. This is only applicable to files with binary content in Google Drive.
    #[serde(rename = "md5Checksum")]
    pub md5_checksum: Option<String>,
    /// Links for exporting Google Docs to specific formats.
    #[serde(rename = "exportLinks")]
    pub export_links: Option<HashMap<String, String>>,
    /// Whether the file has been shared. Not populated for items in shared drives.
    pub shared: Option<bool>,
    /// Whether the options to copy, print, or download this file, should be disabled for readers and commenters.
    #[serde(rename = "copyRequiresWriterPermission")]
    pub copy_requires_writer_permission: Option<bool>,
    /// The full file extension extracted from the name field. May contain multiple concatenated extensions, such as "tar.gz". This is only available for files with binary content in Google Drive.
    /// This is automatically updated when the name field changes, however it is not cleared if the new name does not contain a valid extension.
    #[serde(rename = "fullFileExtension")]
    pub full_file_extension: Option<String>,
    /// The original filename of the uploaded content if available, or else the original value of the name field. This is only available for files with binary content in Google Drive.
    #[serde(rename = "originalFilename")]
    pub original_filename: Option<String>,
    /// Additional metadata about image media, if available.
    #[serde(rename = "imageMediaMetadata")]
    pub image_media_metadata: Option<FileImageMediaMetadata>,
    /// A short description of the file.
    pub description: Option<String>,
    /// The last time the file was modified by anyone (RFC 3339 date-time).
    /// Note that setting modifiedTime will also update modifiedByMeTime for the user.
    #[serde(rename = "modifiedTime")]
    pub modified_time: Option<String>,
    /// Whether the file has been viewed by this user.
    #[serde(rename = "viewedByMe")]
    pub viewed_by_me: Option<bool>,
    /// Whether the file has been modified by this user.
    #[serde(rename = "modifiedByMe")]
    pub modified_by_me: Option<bool>,
    /// Identifies what kind of resource this is. Value: the fixed string "drive#file".
    pub kind: Option<String>,
    /// The time at which the file was created (RFC 3339 date-time).
    #[serde(rename = "createdTime")]
    pub created_time: Option<String>,
    /// The number of storage quota bytes used by the file. This includes the head revision as well as previous revisions with keepForever enabled.
    #[serde(rename = "quotaBytesUsed")]
    pub quota_bytes_used: Option<String>,
    /// Deprecated - use driveId instead.
    #[serde(rename = "teamDriveId")]
    pub team_drive_id: Option<String>,
    /// The time that the item was trashed (RFC 3339 date-time). Only populated for items in shared drives.
    #[serde(rename = "trashedTime")]
    pub trashed_time: Option<String>,
    /// The time at which the file was shared with the user, if applicable (RFC 3339 date-time).
    #[serde(rename = "sharedWithMeTime")]
    pub shared_with_me_time: Option<String>,
    /// A static, unauthenticated link to the file's icon.
    #[serde(rename = "iconLink")]
    pub icon_link: Option<String>,
    /// Deprecated - use copyRequiresWriterPermission instead.
    #[serde(rename = "viewersCanCopyContent")]
    pub viewers_can_copy_content: Option<bool>,
    /// The owners of the file. Currently, only certain legacy files may have more than one owner. Not populated for items in shared drives.
    pub owners: Option<Vec<User>>,
    /// The name of the file. This is not necessarily unique within a folder. Note that for immutable items such as the top level folders of shared drives, My Drive root folder, and Application Data folder the name is constant.
    pub name: Option<String>,
    /// A link for downloading the content of the file in a browser. This is only available for files with binary content in Google Drive.
    #[serde(rename = "webContentLink")]
    pub web_content_link: Option<String>,
    /// If the file has been explicitly trashed, the user who trashed it. Only populated for items in shared drives.
    #[serde(rename = "trashingUser")]
    pub trashing_user: Option<User>,
    /// ID of the shared drive the file resides in. Only populated for items in shared drives.
    #[serde(rename = "driveId")]
    pub drive_id: Option<String>,
    /// The list of spaces which contain the file. The currently supported values are 'drive', 'appDataFolder' and 'photos'.
    pub spaces: Option<Vec<String>>,
    /// List of permission IDs for users with access to this file.
    #[serde(rename = "permissionIds")]
    pub permission_ids: Option<Vec<String>>,
    /// Whether the file has been trashed, either explicitly or from a trashed parent folder. Only the owner may trash a file, and other users cannot see files in the owner's trash.
    pub trashed: Option<bool>,
    /// Additional information about the content of the file. These fields are never populated in responses.
    #[serde(rename = "contentHints")]
    pub content_hints: Option<FileContentHints>,
    /// The final component of fullFileExtension. This is only available for files with binary content in Google Drive.
    #[serde(rename = "fileExtension")]
    pub file_extension: Option<String>,
    /// Whether any users are granted file access directly on this file. This field is only populated for shared drive files.
    #[serde(rename = "hasAugmentedPermissions")]
    pub has_augmented_permissions: Option<bool>,
    /// Whether the user has starred the file.
    pub starred: Option<bool>,
    /// The ID of the file's head revision. This is currently only available for files with binary content in Google Drive.
    #[serde(rename = "headRevisionId")]
    pub head_revision_id: Option<String>,
    /// The full list of permissions for the file. This is only available if the requesting user can share the file. Not populated for items in shared drives.
    pub permissions: Option<Vec<Permission>>,
}

/// Capabilities the current user has on this file. Each capability corresponds to a fine-grained action that a user may take.
///
/// This type is not used in any activity, and only used as *part* of another schema.
///
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct FileCapabilities {
    /// Whether the current user can move this item outside of this drive by changing its parent. Note that a request to change the parent of the item may still fail depending on the new parent that is being added.
    #[serde(rename = "canMoveItemOutOfDrive")]
    pub can_move_item_out_of_drive: Option<bool>,
    /// Whether the current user can restore this file from trash.
    #[serde(rename = "canUntrash")]
    pub can_untrash: Option<bool>,
    /// Whether the current user can copy this file. For an item in a shared drive, whether the current user can copy non-folder descendants of this item, or this item itself if it is not a folder.
    #[serde(rename = "canCopy")]
    pub can_copy: Option<bool>,
    /// Whether the current user can move this item within this shared drive. Note that a request to change the parent of the item may still fail depending on the new parent that is being added. Only populated for items in shared drives.
    #[serde(rename = "canMoveItemWithinDrive")]
    pub can_move_item_within_drive: Option<bool>,
    /// Whether the current user can read the revisions resource of this file. For a shared drive item, whether revisions of non-folder descendants of this item, or this item itself if it is not a folder, can be read.
    #[serde(rename = "canReadRevisions")]
    pub can_read_revisions: Option<bool>,
    /// Deprecated - use canMoveItemOutOfDrive instead.
    #[serde(rename = "canMoveItemIntoTeamDrive")]
    pub can_move_item_into_team_drive: Option<bool>,
    /// Deprecated - use canMoveItemWithinDrive instead.
    #[serde(rename = "canMoveItemWithinTeamDrive")]
    pub can_move_item_within_team_drive: Option<bool>,
    /// Deprecated - use canMoveItemOutOfDrive instead.
    #[serde(rename = "canMoveItemOutOfTeamDrive")]
    pub can_move_item_out_of_team_drive: Option<bool>,
    /// Whether the current user can delete children of this folder. This is false when the item is not a folder. Only populated for items in shared drives.
    #[serde(rename = "canDeleteChildren")]
    pub can_delete_children: Option<bool>,
    /// Whether the current user can change the copyRequiresWriterPermission restriction of this file.
    #[serde(rename = "canChangeCopyRequiresWriterPermission")]
    pub can_change_copy_requires_writer_permission: Option<bool>,
    /// Whether the current user can download this file.
    #[serde(rename = "canDownload")]
    pub can_download: Option<bool>,
    /// Whether the current user can edit this file.
    #[serde(rename = "canEdit")]
    pub can_edit: Option<bool>,
    /// Deprecated - use canMoveChildrenWithinDrive instead.
    #[serde(rename = "canMoveChildrenWithinTeamDrive")]
    pub can_move_children_within_team_drive: Option<bool>,
    /// Whether the current user can comment on this file.
    #[serde(rename = "canComment")]
    pub can_comment: Option<bool>,
    /// Whether the current user can list the children of this folder. This is always false when the item is not a folder.
    #[serde(rename = "canListChildren")]
    pub can_list_children: Option<bool>,
    /// Whether the current user can rename this file.
    #[serde(rename = "canRename")]
    pub can_rename: Option<bool>,
    /// Whether the current user can move this file to trash.
    #[serde(rename = "canTrash")]
    pub can_trash: Option<bool>,
    /// Whether the current user can delete this file.
    #[serde(rename = "canDelete")]
    pub can_delete: Option<bool>,
    /// Whether the current user can read the shared drive to which this file belongs. Only populated for items in shared drives.
    #[serde(rename = "canReadDrive")]
    pub can_read_drive: Option<bool>,
    /// Deprecated - use canMoveItemWithinDrive or canMoveItemOutOfDrive instead.
    #[serde(rename = "canMoveTeamDriveItem")]
    pub can_move_team_drive_item: Option<bool>,
    /// Whether the current user can add children to this folder. This is always false when the item is not a folder.
    #[serde(rename = "canAddChildren")]
    pub can_add_children: Option<bool>,
    /// Whether the current user can modify the sharing settings for this file.
    #[serde(rename = "canShare")]
    pub can_share: Option<bool>,
    /// Whether the current user can trash children of this folder. This is false when the item is not a folder. Only populated for items in shared drives.
    #[serde(rename = "canTrashChildren")]
    pub can_trash_children: Option<bool>,
    /// Deprecated
    #[serde(rename = "canChangeViewersCanCopyContent")]
    pub can_change_viewers_can_copy_content: Option<bool>,
    /// Whether the current user can move children of this folder outside of the shared drive. This is false when the item is not a folder. Only populated for items in shared drives.
    #[serde(rename = "canMoveChildrenOutOfDrive")]
    pub can_move_children_out_of_drive: Option<bool>,
    /// Deprecated - use canMoveChildrenOutOfDrive instead.
    #[serde(rename = "canMoveChildrenOutOfTeamDrive")]
    pub can_move_children_out_of_team_drive: Option<bool>,
    /// Whether the current user can remove children from this folder. This is always false when the item is not a folder. For a folder in a shared drive, use canDeleteChildren or canTrashChildren instead.
    #[serde(rename = "canRemoveChildren")]
    pub can_remove_children: Option<bool>,
    /// Deprecated - use canReadDrive instead.
    #[serde(rename = "canReadTeamDrive")]
    pub can_read_team_drive: Option<bool>,
    /// Whether the current user can move children of this folder within the shared drive. This is false when the item is not a folder. Only populated for items in shared drives.
    #[serde(rename = "canMoveChildrenWithinDrive")]
    pub can_move_children_within_drive: Option<bool>,
}

/// Additional metadata about video media. This may not be available immediately upon upload.
///
/// This type is not used in any activity, and only used as *part* of another schema.
///
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct FileVideoMediaMetadata {
    /// The width of the video in pixels.
    pub width: Option<i32>,
    /// The duration of the video in milliseconds.
    #[serde(rename = "durationMillis")]
    pub duration_millis: Option<String>,
    /// The height of the video in pixels.
    pub height: Option<i32>,
}

/// Additional metadata about image media, if available.
///
/// This type is not used in any activity, and only used as *part* of another schema.
///
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct FileImageMediaMetadata {
    /// The exposure bias of the photo (APEX value).
    #[serde(rename = "exposureBias")]
    pub exposure_bias: Option<f32>,
    /// The length of the exposure, in seconds.
    #[serde(rename = "exposureTime")]
    pub exposure_time: Option<f32>,
    /// The smallest f-number of the lens at the focal length used to create the photo (APEX value).
    #[serde(rename = "maxApertureValue")]
    pub max_aperture_value: Option<f32>,
    /// The color space of the photo.
    #[serde(rename = "colorSpace")]
    pub color_space: Option<String>,
    /// The height of the image in pixels.
    pub height: Option<i32>,
    /// The lens used to create the photo.
    pub lens: Option<String>,
    /// The aperture used to create the photo (f-number).
    pub aperture: Option<f32>,
    /// The rotation in clockwise degrees from the image's original orientation.
    pub rotation: Option<i32>,
    /// The white balance mode used to create the photo.
    #[serde(rename = "whiteBalance")]
    pub white_balance: Option<String>,
    /// The model of the camera used to create the photo.
    #[serde(rename = "cameraModel")]
    pub camera_model: Option<String>,
    /// Whether a flash was used to create the photo.
    #[serde(rename = "flashUsed")]
    pub flash_used: Option<bool>,
    /// The make of the camera used to create the photo.
    #[serde(rename = "cameraMake")]
    pub camera_make: Option<String>,
    /// The focal length used to create the photo, in millimeters.
    #[serde(rename = "focalLength")]
    pub focal_length: Option<f32>,
    /// The exposure mode used to create the photo.
    #[serde(rename = "exposureMode")]
    pub exposure_mode: Option<String>,
    /// The distance to the subject of the photo, in meters.
    #[serde(rename = "subjectDistance")]
    pub subject_distance: Option<i32>,
    /// The width of the image in pixels.
    pub width: Option<i32>,
    /// The metering mode used to create the photo.
    #[serde(rename = "meteringMode")]
    pub metering_mode: Option<String>,
    /// Geographic location information stored in the image.
    pub location: Option<FileImageMediaMetadataLocation>,
    /// The date and time the photo was taken (EXIF DateTime).
    pub time: Option<String>,
    /// The ISO speed used to create the photo.
    #[serde(rename = "isoSpeed")]
    pub iso_speed: Option<i32>,
    /// The type of sensor used to create the photo.
    pub sensor: Option<String>,
}

/// Geographic location information stored in the image.
///
/// This type is not used in any activity, and only used as *part* of another schema.
///
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct FileImageMediaMetadataLocation {
    /// The latitude stored in the image.
    pub latitude: Option<f64>,
    /// The altitude stored in the image.
    pub altitude: Option<f64>,
    /// The longitude stored in the image.
    pub longitude: Option<f64>,
}

/// A permission for a file. A permission grants a user, group, domain or the world access to a file or a folder hierarchy.
///
/// This type is not used in any activity, and only used as *part* of another schema.
///
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct Permission {
    /// The domain to which this permission refers.
    pub domain: Option<String>,
    /// A displayable name for users, groups or domains.
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    /// Whether the permission allows the file to be discovered through search. This is only applicable for permissions of type domain or anyone.
    #[serde(rename = "allowFileDiscovery")]
    pub allow_file_discovery: Option<bool>,
    /// Whether the account associated with this permission has been deleted. This field only pertains to user and group permissions.
    pub deleted: Option<bool>,
    /// Identifies what kind of resource this is. Value: the fixed string "drive#permission".
    pub kind: Option<String>,
    /// The email address of the user or group to which this permission refers.
    #[serde(rename = "emailAddress")]
    pub email_address: Option<String>,
    /// A link to the user's profile photo, if available.
    #[serde(rename = "photoLink")]
    pub photo_link: Option<String>,
    /// Details of whether the permissions on this shared drive item are inherited or directly on this item. This is an output-only field which is present only for shared drive items.
    #[serde(rename = "permissionDetails")]
    pub permission_details: Option<Vec<PermissionPermissionDetails>>,
    /// Deprecated - use permissionDetails instead.
    #[serde(rename = "teamDrivePermissionDetails")]
    pub team_drive_permission_details:
        Option<Vec<PermissionTeamDrivePermissionDetails>>,
    /// The time at which this permission will expire (RFC 3339 date-time). Expiration times have the following restrictions:
    /// - They can only be set on user and group permissions
    /// - The time must be in the future
    /// - The time cannot be more than a year in the future
    #[serde(rename = "expirationTime")]
    pub expiration_time: Option<String>,
    /// The role granted by this permission. While new values may be supported in the future, the following are currently allowed:
    /// - owner
    /// - organizer
    /// - fileOrganizer
    /// - writer
    /// - commenter
    /// - reader
    pub role: Option<String>,
    /// The type of the grantee. Valid values are:
    /// - user
    /// - group
    /// - domain
    /// - anyone
    #[serde(rename = "type")]
    pub type_: Option<String>,
    /// The ID of this permission. This is a unique identifier for the grantee, and is published in User resources as permissionId.
    pub id: Option<String>,
}

/// Deprecated - use permissionDetails instead.
///
/// This type is not used in any activity, and only used as *part* of another schema.
///
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct PermissionTeamDrivePermissionDetails {
    /// Deprecated - use permissionDetails/inherited instead.
    pub inherited: Option<bool>,
    /// Deprecated - use permissionDetails/permissionType instead.
    #[serde(rename = "teamDrivePermissionType")]
    pub team_drive_permission_type: Option<String>,
    /// Deprecated - use permissionDetails/role instead.
    pub role: Option<String>,
    /// Deprecated - use permissionDetails/inheritedFrom instead.
    #[serde(rename = "inheritedFrom")]
    pub inherited_from: Option<String>,
}

/// Details of whether the permissions on this shared drive item are inherited or directly on this item. This is an output-only field which is present only for shared drive items.
///
/// This type is not used in any activity, and only used as *part* of another schema.
///
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct PermissionPermissionDetails {
    /// Whether this permission is inherited. This field is always populated. This is an output-only field.
    pub inherited: Option<bool>,
    /// The permission type for this user. While new values may be added in future, the following are currently possible:
    /// - file
    /// - member
    #[serde(rename = "permissionType")]
    pub permission_type: Option<String>,
    /// The primary role for this user. While new values may be added in the future, the following are currently possible:
    /// - organizer
    /// - fileOrganizer
    /// - writer
    /// - commenter
    /// - reader
    pub role: Option<String>,
    /// The ID of the item from which this permission is inherited. This is an output-only field and is only populated for members of the shared drive.
    #[serde(rename = "inheritedFrom")]
    pub inherited_from: Option<String>,
}

/// Additional information about the content of the file. These fields are never populated in responses.
///
/// This type is not used in any activity, and only used as *part* of another schema.
///
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct FileContentHints {
    /// Text to be indexed for the file to improve fullText queries. This is limited to 128KB in length and may contain HTML elements.
    #[serde(rename = "indexableText")]
    pub indexable_text: Option<String>,
    /// A thumbnail for the file. This will only be used if Google Drive cannot generate a standard thumbnail.
    pub thumbnail: Option<FileContentHintsThumbnail>,
}

/// A thumbnail for the file. This will only be used if Google Drive cannot generate a standard thumbnail.
///
/// This type is not used in any activity, and only used as *part* of another schema.
///
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct FileContentHintsThumbnail {
    /// The MIME type of the thumbnail.
    #[serde(rename = "mimeType")]
    pub mime_type: Option<String>,
    /// The thumbnail data encoded with URL-safe Base64 (RFC 4648 section 5).
    pub image: Option<String>,
}

/// Information about a Drive user.
///
/// This type is not used in any activity, and only used as *part* of another schema.
///
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct User {
    /// Whether this user is the requesting user.
    pub me: Option<bool>,
    /// Identifies what kind of resource this is. Value: the fixed string "drive#user".
    pub kind: Option<String>,
    /// A plain text displayable name for this user.
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    /// A link to the user's profile photo, if available.
    #[serde(rename = "photoLink")]
    pub photo_link: Option<String>,
    /// The email address of the user. This may not be present in certain contexts if the user has not made their email address visible to the requester.
    #[serde(rename = "emailAddress")]
    pub email_address: Option<String>,
    /// The user's ID as visible in Permission resources.
    #[serde(rename = "permissionId")]
    pub permission_id: Option<String>,
}
