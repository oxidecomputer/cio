/*!
 * A rust library for interacting with the Tailscale API.
 *
 * For more information, the Tailscale API is still in beta. The docs are
 * here: https://github.com/tailscale/tailscale/blob/main/api.md
 *
 * Example:
 *
 * ```ignore
 * use serde::{Deserialize, Serialize};
 * use tailscale_api::Tailscale;
 *
 * async fn get_devices() {
 *     // Initialize the Tailscale client.
 *     let tailscale = Tailscale::new_from_env();
 *
 *     // List the devices.
 *     let devices = tailscale.list_devices().await.unwrap();
 *
 *     println!("{:?}", devices);
 * }
 * ```
 */
#![allow(clippy::field_reassign_with_default)]
use std::{env, error, fmt, sync::Arc};

use chrono::{offset::Utc, DateTime};
use reqwest::{header, Client, Method, Request, StatusCode, Url};
use serde::{Deserialize, Serialize};

/// Endpoint for the Tailscale API.
const ENDPOINT: &str = "https://api.tailscale.com/api/v2/";

/// Entrypoint for interacting with the Tailscale API.
pub struct Tailscale {
    key: String,
    domain: String,

    client: Arc<Client>,
}

impl Tailscale {
    /// Create a new Tailscale client struct. It takes a type that can convert into
    /// an &str (`String` or `Vec<u8>` for example). As long as the function is
    /// given a valid API key your requests will work.
    pub fn new<K, D>(key: K, domain: D) -> Self
    where
        K: ToString,
        D: ToString,
    {
        let client = Client::builder().build();
        match client {
            Ok(c) => Self {
                key: key.to_string(),
                domain: domain.to_string(),

                client: Arc::new(c),
            },
            Err(e) => panic!("creating client failed: {:?}", e),
        }
    }

    /// Create a new Tailscale client struct from environment variables. It
    /// takes a type that can convert into
    /// an &str (`String` or `Vec<u8>` for example). As long as the function is
    /// given a valid API key and domain and your requests will work.
    pub fn new_from_env() -> Self {
        let key = env::var("TAILSCALE_API_KEY").unwrap();
        let domain = env::var("TAILSCALE_DOMAIN").unwrap();

        Tailscale::new(key, domain)
    }

    fn request<B>(&self, method: Method, path: &str, body: B, query: Option<Vec<(&str, String)>>) -> Request
    where
        B: Serialize,
    {
        let base = Url::parse(ENDPOINT).unwrap();
        let url = base.join(path).unwrap();

        // Set the default headers.
        let mut headers = header::HeaderMap::new();
        headers.append(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/json"),
        );

        let mut rb = self
            .client
            .request(method.clone(), url)
            .headers(headers)
            .basic_auth(&self.key, Some(""));

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

    /// List devices.
    pub async fn list_devices(&self) -> Result<Vec<Device>, APIError> {
        // Build the request.
        // TODO: paginate.
        let request = self.request(Method::GET, &format!("domain/{}/devices", self.domain), (), None);

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

        let r: APIResponse = resp.json().await.unwrap();

        Ok(r.devices)
    }

    /// Delete device.
    pub async fn delete_device(&self, device_id: &str) -> Result<(), APIError> {
        let request = self.request(Method::DELETE, &format!("device/{}", device_id), (), None);

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
            self.status_code, self.body
        )
    }
}

impl fmt::Debug for APIError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "APIError: status code -> {}, body -> {}",
            self.status_code, self.body
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

/// The data type for an API response.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct APIResponse {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub devices: Vec<Device>,
}

/// The data type for a device.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Device {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub addresses: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "allowedIPs")]
    pub allowed_ips: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "extraIPs")]
    pub extra_ips: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub endpoints: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub derp: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "clientVersion")]
    pub client_version: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub os: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    pub created: DateTime<Utc>,
    #[serde(rename = "lastSeen")]
    pub last_seen: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub hostname: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "machineKey")]
    pub machine_key: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "nodeKey")]
    pub node_key: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "displayNodeKey")]
    pub display_node_key: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "logID")]
    pub log_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub user: String,
    pub expires: DateTime<Utc>,
    #[serde(default, rename = "neverExpires")]
    pub never_expires: bool,
    #[serde(default)]
    pub authorized: bool,
    #[serde(default, rename = "isExternal")]
    pub is_external: bool,
    #[serde(default, rename = "updateAvailable")]
    pub update_available: bool,
    #[serde(default, rename = "routeAll")]
    pub route_all: bool,
    #[serde(default, rename = "hasSubnet")]
    pub has_subnet: bool,
}
