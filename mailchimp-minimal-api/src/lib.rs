use http::{
    header::{HeaderMap, HeaderValue, InvalidHeaderValue, AUTHORIZATION, CONTENT_TYPE},
    StatusCode,
};
use reqwest::{Client, Method, RequestBuilder, Url};
use std::{error, fmt};
use url::ParseError;

mod types;

pub use crate::types::*;

pub enum AuthMode {
    Basic(HeaderValue),
}

impl AuthMode {
    pub fn new_basic_auth<T>(key: T) -> Result<Self, MailChimpError>
    where
        T: AsRef<str>,
    {
        let encoded = base64::encode(format!("username:{}", key.as_ref()).as_bytes());
        let auth_header =
            HeaderValue::from_str(&format!("Basic {}", encoded)).map_err(MailChimpError::MalformedAPIKey)?;

        Ok(AuthMode::Basic(auth_header))
    }

    pub fn has_token(&self) -> bool {
        match self {
            AuthMode::Basic(_) => true,
        }
    }

    pub fn to_authorization_header(&self) -> Result<HeaderValue, MailChimpError> {
        match self {
            AuthMode::Basic(header) => Ok(header.clone()),
        }
    }
}

pub struct MailChimp {
    auth: AuthMode,
    client: Client,
    dc_endpoint: Url,
}

impl MailChimp {
    pub fn new<T>(endpoint: T, auth: AuthMode) -> Result<Self, MailChimpError>
    where
        T: AsRef<str>,
    {
        Ok(Self {
            auth,
            client: Client::new(),
            dc_endpoint: Url::parse(endpoint.as_ref()).map_err(MailChimpError::InvalidDataCenterEndpoint)?,
        })
    }

    fn request<P>(&self, method: Method, path: P) -> Result<RequestBuilder, MailChimpError>
    where
        P: AsRef<str>,
    {
        let mut uri = path.as_ref().to_string();

        // Make sure we have the leading "/".
        if !uri.starts_with('/') {
            uri = format!("/{}", uri);
        }

        let url = self.dc_endpoint.join(&uri).map_err(MailChimpError::InvalidUri)?;

        // Set the default headers.
        let mut headers = HeaderMap::new();
        headers.append(AUTHORIZATION, self.auth.to_authorization_header()?);
        headers.append(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        Ok(self.client.request(method, url).headers(headers))
    }

    pub async fn get_subscribers<T>(&self, list_id: T) -> Result<Vec<crate::types::Member>, MailChimpError>
    where
        T: AsRef<str>,
    {
        let per_page = 500;
        let mut offset: usize = 0;
        let mut members: Vec<Member> = Default::default();
        let mut has_more_rows = true;

        while has_more_rows {
            // Build the request.
            let rb = self.request(
                Method::GET,
                &format!(
                    "3.0/lists/{}/members?count={}&offset={}",
                    list_id.as_ref(),
                    per_page,
                    offset
                ),
            )?;

            let request = rb.build()?;

            let resp = self.client.execute(request).await?;

            match resp.status() {
                StatusCode::OK => {
                    let mut list: ListMembersResponse = resp.json().await?;

                    has_more_rows = !list.members.is_empty();
                    offset += list.members.len();

                    members.append(&mut list.members)
                }
                status => {
                    return Err(MailChimpError::APIError(MailChimpAPIError {
                        status_code: status,
                        body: resp.text().await?,
                    }))
                }
            }
        }

        Ok(members)
    }
}

#[derive(Debug)]
pub struct MailChimpAPIError {
    pub status_code: StatusCode,
    pub body: String,
}

#[derive(Debug)]
pub enum MailChimpError {
    APIError(MailChimpAPIError),
    InternalError(reqwest::Error),
    InvalidDataCenterEndpoint(ParseError),
    InvalidUri(ParseError),
    MalformedAPIKey(InvalidHeaderValue),
    MissingAuthentication,
}

impl From<reqwest::Error> for MailChimpError {
    fn from(error: reqwest::Error) -> Self {
        MailChimpError::InternalError(error)
    }
}

impl fmt::Display for MailChimpError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "MailChimp client error: {:?}", self)
    }
}

// impl fmt::Debug for MailChimpError {
//     fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
//         match self {
//             APIError(api_error) => write!(f, "{:?}", ),
//             InternalError(reqwest::Error),
//             InvalidDataCenterEndpoint(ParseError),
//             InvalidUri(ParseError),
//             MalformedAPIKey(InvalidHeaderValue),
//             MissingAuthentication,
//         }
//         write!(f, "MailChimp client error: {:?}", self)
//     }
// }

// This is important for other errors to wrap this one.
impl error::Error for MailChimpError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        // Generic error, underlying cause isn't tracked.
        None
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
