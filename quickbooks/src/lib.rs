/*!
 * A rust library for interacting with the QuickBooks API.
 *
 * For more information, you can check out their documentation at:
 * https://developer.intuit.com/app/developer/qbo/docs/develop
 *
 * Example:
 *
 * ```
 * use quickbooks::QuickBooks;
 * use serde::{Deserialize, Serialize};
 *
 * async fn list_invoices() {
 *     // Initialize the QuickBooks client.
 *     let quickbooks = QuickBooks::new_from_env().await;
 *
 *     let payments = quickbooks.list_invoices().await.unwrap();
 *
 *     println!("{:?}", payments);
 * }
 * ```
 */
use std::env;
use std::error;
use std::fmt;
use std::sync::Arc;

use reqwest::{header, Client, Method, Request, StatusCode, Url};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Endpoint for the QuickBooks API.
const ENDPOINT: &str = "https://quickbooks.api.intuit.com/v3/";

/// Entrypoint for interacting with the QuickBooks API.
#[derive(Debug, Clone)]
pub struct QuickBooks {
    token: String,
    // This expires in 101 days. It is hardcoded in the GitHub Actions secrets,
    // We might want something a bit better like storing it in the database.
    refresh_token: String,
    client_id: String,
    client_secret: String,
    redirect_uri: String,
    company_id: String,

    client: Arc<Client>,
}

impl QuickBooks {
    /// Create a new QuickBooks client struct. It takes a type that can convert into
    /// an &str (`String` or `Vec<u8>` for example). As long as the function is
    /// given a valid API key your requests will work.
    pub async fn new<I, K, B, R>(client_id: I, client_secret: K, company_id: B, redirect_uri: R) -> Self
    where
        I: ToString,
        K: ToString,
        B: ToString,
        R: ToString,
    {
        let client = Client::builder().build();
        match client {
            Ok(c) => {
                let mut qb = QuickBooks {
                    client_id: client_id.to_string(),
                    client_secret: client_secret.to_string(),
                    company_id: company_id.to_string(),
                    redirect_uri: redirect_uri.to_string(),
                    token: env::var("QUICKBOOKS_TOKEN").unwrap_or_default(),
                    refresh_token: env::var("QUICKBOOKS_REFRESH_TOKEN").unwrap_or_default(),

                    client: Arc::new(c),
                };

                if qb.token.is_empty() || qb.refresh_token.is_empty() {
                    // This is super hacky and a work around since there is no way to
                    // auth without using the browser.
                    println!("quickbooks consent URL: {}", qb.user_consent_url());
                }
                qb.refresh_access_token().await.unwrap();

                qb
            }
            Err(e) => panic!("creating client failed: {:?}", e),
        }
    }

    /// Create a new QuickBooks client struct from environment variables. It
    /// takes a type that can convert into
    /// an &str (`String` or `Vec<u8>` for example). As long as the function is
    /// given a valid API key and your requests will work.
    pub async fn new_from_env() -> Self {
        let client_id = env::var("QUICKBOOKS_CLIENT_ID").unwrap();
        let client_secret = env::var("QUICKBOOKS_CLIENT_SECRET").unwrap();
        let company_id = env::var("QUICKBOOKS_COMPANY_ID").unwrap();
        let redirect_uri = env::var("QUICKBOOKS_REDIRECT_URI").unwrap();

        QuickBooks::new(client_id, client_secret, company_id, redirect_uri).await
    }

    fn request<B>(&self, method: Method, path: &str, body: B, query: Option<&[(&str, &str)]>) -> Request
    where
        B: Serialize,
    {
        let base = Url::parse(ENDPOINT).unwrap();
        let url = base.join(path).unwrap();

        let bt = format!("Bearer {}", self.token);
        let bearer = header::HeaderValue::from_str(&bt).unwrap();

        // Set the default headers.
        let mut headers = header::HeaderMap::new();
        headers.append(header::AUTHORIZATION, bearer);
        headers.append(header::CONTENT_TYPE, header::HeaderValue::from_static("application/json"));
        headers.append(header::ACCEPT, header::HeaderValue::from_static("application/json"));

        let mut rb = self.client.request(method.clone(), url).headers(headers);

        if let Some(val) = query {
            rb = rb.query(&val);
        }

        // Add the body, this is to ensure our GET and DELETE calls succeed.
        if method != Method::GET && method != Method::DELETE {
            rb = rb.json(&body);
        }

        // Build the request.
        rb.build().unwrap()
    }

    pub fn user_consent_url(&self) -> String {
        format!(
            "https://appcenter.intuit.com/connect/oauth2?client_id={}&response_type=code&scope=com.intuit.quickbooks.accounting&redirect_uri={}&state=some_state",
            self.client_id, self.redirect_uri
        )
    }

    pub async fn refresh_access_token(&mut self) -> Result<(), APIError> {
        let mut headers = header::HeaderMap::new();
        headers.append(header::ACCEPT, header::HeaderValue::from_static("application/json"));

        let params = [("grant_type", "refresh_token"), ("refresh_token", &self.refresh_token)];
        let client = reqwest::Client::new();
        let resp = client
            .post("https://oauth.platform.intuit.com/oauth2/v1/tokens/bearer")
            .headers(headers)
            .basic_auth(&self.client_id, Some(&self.client_secret))
            .form(&params)
            .send()
            .await
            .unwrap();

        // Unwrap the response.
        let t: AccessToken = resp.json().await.unwrap();

        self.token = t.access_token.to_string();
        self.refresh_token = t.refresh_token;

        Ok(())
    }

    pub async fn get_access_token(&mut self, code: &str) -> Result<(), APIError> {
        let mut headers = header::HeaderMap::new();
        headers.append(header::ACCEPT, header::HeaderValue::from_static("application/json"));

        let params = [("grant_type", "authorization_code"), ("code", code), ("redirect_uri", &self.redirect_uri)];
        let client = reqwest::Client::new();
        let resp = client
            .post("https://oauth.platform.intuit.com/oauth2/v1/tokens/bearer")
            .headers(headers)
            .basic_auth(&self.client_id, Some(&self.client_secret))
            .form(&params)
            .send()
            .await
            .unwrap();

        // Unwrap the response.
        let t: AccessToken = resp.json().await.unwrap();

        self.token = t.access_token.to_string();
        self.refresh_token = t.refresh_token;

        Ok(())
    }

    pub async fn list_invoices(&self) -> Result<(), APIError> {
        // Build the request.
        let request = self.request(Method::GET, &format!("company/{}/query", self.company_id), (), Some(&[("query", "SELECT COUNT(*) FROM Invoice")]));

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

        let r: CountResponse = resp.json().await.unwrap();

        println!("{}", r.query_response.total_count);

        Ok(())
    }

    pub async fn list_items(&self) -> Result<(), APIError> {
        // Build the request.
        let request = self.request(
            Method::GET,
            &format!("company/{}/query", self.company_id),
            (),
            Some(&[("query", "SELECT * FROM Item MAXRESULTS 10000")]),
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

        println!("{}", resp.text().await.unwrap());

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

#[derive(Debug, JsonSchema, Clone, Default, Serialize, Deserialize)]
pub struct AccessToken {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub access_token: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub token_type: String,
    #[serde(default)]
    pub expires_in: i64,
    #[serde(default)]
    pub x_refresh_token_expires_in: i64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub refresh_token: String,
}

#[derive(Debug, JsonSchema, Clone, Default, Serialize, Deserialize)]
pub struct CountResponse {
    #[serde(default, rename = "QueryResponse")]
    pub query_response: QueryResponse,
}

#[derive(Debug, JsonSchema, Clone, Default, Serialize, Deserialize)]
pub struct QueryResponse {
    #[serde(default, rename = "TotalCount")]
    pub total_count: i64,
}
