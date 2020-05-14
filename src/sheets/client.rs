use std::rc::Rc;

use reqwest::blocking::{Client, Request};
use reqwest::{header, Method, StatusCode, Url};
use serde::Serialize;
use yup_oauth2::Token;

use crate::sheets::core::{UpdateValuesResponse, ValueRange};

const ENDPOINT: &str = "https://sheets.googleapis.com/v4/";

pub struct Sheets {
    token: Token,

    client: Rc<Client>,
}

impl Sheets {
    // Create a new Sheets client struct. It takes a type that can convert into
    // an &str (`String` or `Vec<u8>` for example). As long as the function is
    // given a valid API Key and Secret your requests will work.
    pub fn new(token: Token) -> Self {
        let client = Client::builder().build();
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
    ) -> Request
    where
        B: Serialize,
    {
        let base = Url::parse(ENDPOINT).unwrap();
        let url = base.join(&path).unwrap();

        // Check if the token is expired and panic.
        if self.token.expired() {
            panic!("token is expired");
        }

        let bt = format!("Bearer {}", self.token.access_token);
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

    pub fn get_values(&self, sheet_id: &str, range: String) -> ValueRange {
        // Build the request.
        let request = self.request(
            Method::GET,
            format!("spreadsheets/{}/values/{}", sheet_id.to_string(), range),
            {},
            Some(vec![
                ("valueRenderOption", "FORMATTED_VALUE".to_string()),
                ("dateTimeRenderOption", "FORMATTED_STRING".to_string()),
                ("majorDimension", "ROWS".to_string()),
            ]),
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => panic!(
                "received response status: {:?}\nbody: {}",
                s,
                resp.text().unwrap()
            ),
        };

        // Try to deserialize the response.
        let value_range: ValueRange = resp.json().unwrap();

        return value_range;
    }

    pub fn update_values(
        &self,
        sheet_id: &str,
        range: &str,
        value: String,
    ) -> UpdateValuesResponse {
        // Build the request.
        let request = self.request(
            Method::PUT,
            format!(
                "spreadsheets/{}/values/{}",
                sheet_id.to_string(),
                range.to_string()
            ),
            ValueRange {
                range: Some(range.to_string()),
                values: Some(vec![vec![value]]),
                major_dimension: None,
            },
            Some(vec![
                ("valueInputOption", "USER_ENTERED".to_string()),
                ("responseValueRenderOption", "FORMATTED_VALUE".to_string()),
                (
                    "responseDateTimeRenderOption",
                    "FORMATTED_STRING".to_string(),
                ),
            ]),
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => panic!(
                "received response status: {:?}\nbody: {}",
                s,
                resp.text().unwrap()
            ),
        };

        // Try to deserialize the response.
        let r: UpdateValuesResponse = resp.json().unwrap();

        return r;
    }
}
