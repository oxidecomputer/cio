use http::{
    header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE},
    StatusCode,
};
use reqwest::{Client, Method, RequestBuilder, Url};
use std::{error, fmt, str::FromStr};
use url::ParseError;

mod types;

pub use crate::types::*;

#[derive(Debug)]
pub enum AuthMode {
    Basic(BasicAuth),
}

#[derive(Debug)]
pub struct BasicAuth {
    auth_header: HeaderValue,
    endpoint: Url,
}

#[derive(Debug)]
struct MailChimpDataCenter(String);

impl FromStr for MailChimpDataCenter {
    type Err = MailChimpError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts = s.split('-');

        let _key = parts.next();
        let dc = parts.next();

        dc.map(|dc| Self(dc.to_string())).ok_or(MailChimpError::MalformedAPIKey)
    }
}

impl AuthMode {
    pub fn new_basic_auth<T>(key: T) -> Result<Self, MailChimpError>
    where
        T: AsRef<str>,
    {
        let encoded = base64::encode(format!("username:{}", key.as_ref()).as_bytes());
        let auth_header =
            HeaderValue::from_str(&format!("Basic {}", encoded)).map_err(|_| MailChimpError::MalformedAPIKey)?;

        let dc: MailChimpDataCenter = key.as_ref().parse()?;
        let url = format!("https://{}.api.mailchimp.com", dc.0);
        let endpoint = Url::parse(&url).map_err(MailChimpError::InvalidDataCenterEndpoint)?;

        Ok(AuthMode::Basic(BasicAuth { auth_header, endpoint }))
    }

    pub fn has_token(&self) -> bool {
        match self {
            AuthMode::Basic(_) => true,
        }
    }

    pub fn to_endpoint_url(&self) -> Result<Url, MailChimpError> {
        match self {
            AuthMode::Basic(auth) => Ok(auth.endpoint.clone()),
        }
    }

    pub fn to_authorization_header(&self) -> Result<HeaderValue, MailChimpError> {
        match self {
            AuthMode::Basic(auth) => Ok(auth.auth_header.clone()),
        }
    }
}

pub struct MailChimp {
    auth: AuthMode,
    client: Client,
}

impl MailChimp {
    pub fn new(auth: AuthMode) -> Self {
        Self {
            auth,
            client: Client::new(),
        }
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

        let url = self
            .auth
            .to_endpoint_url()
            .and_then(|url| url.join(&uri).map_err(MailChimpError::InvalidUri))?;

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
    MalformedAPIKey,
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

// This is important for other errors to wrap this one.
impl error::Error for MailChimpError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        // Generic error, underlying cause isn't tracked.
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{AuthMode, MailChimpError};
    use base64;
    use std::str::from_utf8;

    static VALID_FORMAT: &'static str = "5555555555555555-us6";
    static INVALID_FORMAT: &'static str = "5555555555555555us6";

    #[test]
    fn test_computes_datacenter() {
        let auth = AuthMode::new_basic_auth(VALID_FORMAT).unwrap();

        match auth {
            AuthMode::Basic(auth) => assert_eq!("https://us6.api.mailchimp.com/", auth.endpoint.as_str()),
        }
    }

    #[test]
    fn test_handles_malformed_datacenter() {
        let auth = AuthMode::new_basic_auth(INVALID_FORMAT);

        match auth {
            Err(MailChimpError::MalformedAPIKey) => (),
            other => panic!("Expected malformed api key error, but instead received {:?}", other),
        }
    }

    #[test]
    fn test_computes_valid_header() {
        let auth = AuthMode::new_basic_auth(VALID_FORMAT).unwrap();

        match auth {
            AuthMode::Basic(auth) => {
                let value = auth.auth_header.to_str().unwrap();
                let mut value_parts = value.split(' ');

                let label = value_parts.next().unwrap();

                assert_eq!("Basic", label);

                let basic = value_parts.next().unwrap();

                let decoded = base64::decode(basic).unwrap();
                let mut basic_parts = from_utf8(&decoded).unwrap().split(':');

                assert_eq!("username", basic_parts.next().unwrap());
                assert_eq!(VALID_FORMAT, basic_parts.next().unwrap());
            }
        }
    }
}
