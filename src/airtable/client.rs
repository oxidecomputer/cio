use std::env;
use std::error;
use std::fmt;
use std::rc::Rc;

use reqwest::blocking::{Client, Request};
use reqwest::{header, Method, StatusCode, Url};
use serde::Serialize;

use crate::airtable::core::{APICall, Record};

const ENDPOINT: &str = "https://api.airtable.com/v0/";

pub struct Airtable {
    key: String,
    base_id: String,

    client: Rc<Client>,
}

impl Airtable {
    // Create a new Airtable client struct. It takes a type that can convert into
    // an &str (`String` or `Vec<u8>` for example). As long as the function is
    // given a valid API Key and Base ID your requests will work.
    pub fn new<K, B>(key: K, base_id: B) -> Self
    where
        K: ToString,
        B: ToString,
    {
        let client = Client::builder().build();
        match client {
            Ok(c) => Self {
                key: key.to_string(),
                base_id: base_id.to_string(),

                client: Rc::new(c),
            },
            Err(e) => panic!("creating client failed: {:?}", e),
        }
    }

    pub fn new_from_env() -> Self {
        let key = env::var("AIRTABLE_API_KEY").unwrap();
        let base_id = env::var("AIRTABLE_BASE_ID").unwrap();

        return Airtable::new(key, base_id);
    }

    // Get the currently set API key.
    pub fn get_key(&self) -> &str {
        &self.key
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
        let base = Url::parse(ENDPOINT).unwrap();
        let url = base
            .join(&(self.base_id.to_string() + "/" + &path))
            .unwrap();

        let bt = format!("Bearer {}", self.key);
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

    pub fn list_records(
        &self,
        table: &str,
        view: &str,
    ) -> Result<Vec<Record>, APIError> {
        // Build the request.
        let request = self.request(
            Method::GET,
            table.to_string(),
            {},
            Some(vec![
                ("maxRecords", "100".to_string()),
                ("view", view.to_string()),
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
        let r: APICall = resp.json().unwrap();

        return Ok(r.records);
    }

    /// Can only bulk create 10 records at a time.
    pub fn create_records(
        &self,
        table: &str,
        records: Vec<Record>,
    ) -> Result<Vec<Record>, APIError> {
        // Build the request.
        let request = self.request(
            Method::POST,
            table.to_string(),
            APICall {
                records: records,
                offset: None,
                typecast: Some(true),
            },
            None,
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
        let r: APICall = resp.json().unwrap();

        return Ok(r.records);
    }

    /// Can only bulk update 10 records at a time.
    pub fn update_records(
        &self,
        table: &str,
        records: Vec<Record>,
    ) -> Result<Vec<Record>, APIError> {
        // Build the request.
        let request = self.request(
            Method::PATCH,
            table.to_string(),
            APICall {
                records: records.clone(),
                offset: None,
                typecast: Some(true),
            },
            None,
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
        let r: APICall = resp.json().unwrap();

        return Ok(r.records);
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
