/*!
 * A rust library for interacting with the Giphy API.
 *
 * For more information, the Giphy API is documented at
 * [developers.giphy.com/docs/api#quick-start-guide](https://developers.giphy.com/docs/api#quick-start-guide).
 *
 * Example:
 *
 * ```
 * use giphy_api::Giphy;
 *
 * async fn get_gif() {
 *     // Initialize the Giphy client.
 *     let giphy_client = Giphy::new_from_env();
 *
 *     // Get a list of gifs based on a search.
 *     let gifs = giphy_client.search_gifs("toddlers and tiaras", 5, "pg-13").await.unwrap();
 *
 *     for gif in gifs {
 *         println!("{:?}", gif);
 *     }
 * }
 * ```
 */
use std::env;
use std::error;
use std::fmt;
use std::sync::Arc;

use reqwest::{header, Client, Method, Request, StatusCode, Url};
use serde::{Deserialize, Serialize};

/// Endpoint for the Giphy API.
const ENDPOINT: &str = "https://api.giphy.com/v1/";

/// Entrypoint for interacting with the Giphy API.
pub struct Giphy {
    key: String,

    client: Arc<Client>,
}

impl Giphy {
    /// Create a new Giphy client struct. It takes a type that can convert into
    /// an &str (`String` or `Vec<u8>` for example). As long as the function is
    /// given a valid API Key your requests will work.
    pub fn new<K>(key: K) -> Self
    where
        K: ToString,
    {
        let client = Client::builder().build();
        match client {
            Ok(c) => Self {
                key: key.to_string(),

                client: Arc::new(c),
            },
            Err(e) => panic!("creating client failed: {:?}", e),
        }
    }

    /// Create a new Giphy client struct from environment variables. It
    /// takes a type that can convert into
    /// an &str (`String` or `Vec<u8>` for example). As long as the function is
    /// given a valid API Key your requests will work.
    pub fn new_from_env() -> Self {
        let key = env::var("GIPHY_API_KEY").unwrap();

        Giphy::new(key)
    }

    /// Get the currently set API key.
    pub fn get_key(&self) -> &str {
        &self.key
    }

    fn request<B>(&self, method: Method, path: String, body: B, query: Option<Vec<(&'static str, String)>>) -> Request
    where
        B: Serialize,
    {
        let base = Url::parse(ENDPOINT).unwrap();
        let url = base.join(&path).unwrap();

        // Set the default headers.
        let mut headers = header::HeaderMap::new();
        headers.append(header::CONTENT_TYPE, header::HeaderValue::from_static("application/json"));

        let mut rb = self.client.request(method.clone(), url).headers(headers);
        rb = rb.query(&[("api_key", self.key.to_string())]);

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

    /// Search gifs, defaults to pg-13.
    pub async fn search_gifs(&self, query: &str, limit: i32, rating: &str) -> Result<Vec<Gif>, APIError> {
        // Build the request.
        let request = self.request(
            Method::GET,
            "gifs/search".to_string(),
            (),
            Some(vec![("q", query.to_string()), ("rating", rating.to_string()), ("limit", format!("{}", limit))]),
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
        let r: Response = resp.json().await.unwrap();
        Ok(r.data)
    }
}

/// Error type returned by our library.
pub struct APIError {
    pub status_code: StatusCode,
    pub body: String,
}

impl fmt::Display for APIError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "APIError: status code -> {}, body -> {}", self.status_code.to_string(), self.body)
    }
}

impl fmt::Debug for APIError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "APIError: status code -> {}, body -> {}", self.status_code.to_string(), self.body)
    }
}

// This is important for other errors to wrap this one.
impl error::Error for APIError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        // Generic error, underlying cause isn't tracked.
        None
    }
}

/// Response object.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Response {
    pub data: Vec<Gif>,
}

/// An Giphy record.
/// FROM: https://developers.giphy.com/docs/api/schema/#gif-object
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Gif {
    #[serde(alias = "type")]
    pub gif_type: String,
    pub id: String,
    pub slug: String,
    pub url: String,
    pub bitly_url: String,
    pub embed_url: String,
    pub username: String,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rating: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<User>,
    pub source_tld: String,
    pub source_post_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_datetime: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_datetime: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub import_datetime: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trending_datetime: Option<String>,
    pub images: Images,
    pub title: String,
}

/// A Giphy user.
///
/// FROM: https://developers.giphy.com/docs/#user-object
#[derive(Default, Clone, Serialize, Deserialize, Debug)]
pub struct User {
    pub avatar_url: String,
    pub banner_url: String,
    pub profile_url: String,
    pub username: String,
    pub display_name: String,
    pub twitter: Option<String>,
}

/// Giphy Animated `Images` object representation.
///
/// FROM: https://developers.giphy.com/docs/#images-object
#[derive(Default, Clone, Serialize, Deserialize, Debug)]
pub struct ImageAnimated {
    pub url: Option<String>,
    pub width: String,
    pub height: String,
    pub size: Option<String>,
    pub mp4: Option<String>,
    pub mp4_size: Option<String>,
    pub webp: Option<String>,
    pub webp_size: Option<String>,
}

/// Giphy Still `Images` object representation.
///
/// FROM: https://developers.giphy.com/docs/#images-object
#[derive(Default, Clone, Serialize, Deserialize, Debug)]
pub struct ImageStill {
    pub url: String,
    pub width: String,
    pub height: String,
}

/// Giphy Looping `Images` object representation.
///
/// FROM: https://developers.giphy.com/docs/#images-object
#[derive(Default, Clone, Serialize, Deserialize, Debug)]
pub struct ImageLooping {
    pub mp4: String,
}

/// Giphy MP4 Preview `Images` object representation.
///
/// FROM: https://developers.giphy.com/docs/#images-object
#[derive(Default, Clone, Serialize, Deserialize, Debug)]
pub struct ImagePreviewMp4 {
    pub mp4: String,
    pub mp4_size: String,
    pub width: String,
    pub height: String,
}

/// Giphy GIF Preview `Images` object representation.
///
/// FROM: https://developers.giphy.com/docs/#images-object
#[derive(Default, Clone, Serialize, Deserialize, Debug)]
pub struct ImagePreviewGif {
    pub url: String,
    pub size: String,
    pub width: String,
    pub height: String,
}

/// Giphy `Images` object representation.
///
/// FROM: https://developers.giphy.com/docs/#images-object
#[derive(Default, Clone, Serialize, Deserialize, Debug)]
pub struct Images {
    pub fixed_height: ImageAnimated,
    pub fixed_height_still: ImageStill,
    pub fixed_height_downsampled: ImageAnimated,
    pub fixed_width: ImageAnimated,
    pub fixed_width_still: ImageStill,
    pub fixed_width_downsampled: ImageAnimated,
    pub fixed_height_small: ImageAnimated,
    pub fixed_height_small_still: ImageStill,
    pub fixed_width_small: ImageAnimated,
    pub fixed_width_small_still: ImageStill,
    pub downsized: ImageAnimated,
    pub downsized_still: ImageStill,
    pub downsized_large: ImageAnimated,
    pub downsized_medium: ImageAnimated,
    pub downsized_small: ImageAnimated,
    pub original: ImageAnimated,
    pub original_still: ImageStill,
    pub looping: ImageLooping,
    pub preview: ImagePreviewMp4,
    pub preview_gif: ImagePreviewGif,
}
