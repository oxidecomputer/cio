/*!
 * A rust library for interacting with the Google Sheets v4 API.
 *
 * For more information, the Google Sheets v4 API is documented at [developers.google.com/sheets/api/reference/rest](https://developers.google.com/sheets/api/reference/rest).
 *
 * Example:
 *
 * ```
 * use std::env;
 *
 * use sheets::Sheets;
 * use yup_oauth2::{read_service_account_key, ServiceAccountAuthenticator};
 *
 * async fn get_sheet_values() {
 *     // Get the GSuite credentials file.
 *     let gsuite_credential_file = env::var("GADMIN_CREDENTIAL_FILE").unwrap();
 *     let gsuite_subject = env::var("GADMIN_SUBJECT").unwrap();
 *     let gsuite_secret = read_service_account_key(gsuite_credential_file).await.expect("failed to read gsuite credential file");
 *     let auth = ServiceAccountAuthenticator::builder(gsuite_secret)
 *         .subject(gsuite_subject.to_string())
 *         .build()
 *         .await
 *         .expect("failed to create authenticator");
 *
 *     // Add the scopes to the secret and get the token.
 *     let token = auth.token(&["https://www.googleapis.com/auth/spreadsheets"]).await.expect("failed to get token");
 *
 *     if token.as_str().is_empty() {
 *         panic!("empty token is not valid");
 *     }
 *
 *     // Initialize the Google Sheets client.
 *     let sheets_client = Sheets::new(token);
 *
 *     // Get the values in the sheet.
 *     let sheet_values = sheets_client.get_values("sheet_id", "Form Responses 1!A1:S1000".to_string()).await.unwrap();
 *     let values = sheet_values.values.unwrap();
 *
 *     if values.is_empty() {
 *         panic!("unable to retrieve any data values from Google sheet")
 *     }
 *
 *     // Iterate over the rows.
 *     for (row_index, row) in values.iter().enumerate() {
 *         println!("{}: {:?}", row_index, row);
 *     }
 * }
 * ```
 */
use std::error;
use std::fmt;
use std::sync::Arc;

use reqwest::{header, Client, Method, Request, StatusCode, Url};
use serde::{Deserialize, Serialize};
use yup_oauth2::AccessToken;

/// Endpoint for the Google Sheets API.
const ENDPOINT: &str = "https://sheets.googleapis.com/v4/";

/// Entrypoint for interacting with the Google Sheets API.
pub struct Sheets {
    token: AccessToken,

    client: Arc<Client>,
}

impl Sheets {
    /// Create a new Sheets client struct. It takes a type that can convert into
    /// an &str (`String` or `Vec<u8>` for example). As long as the function is
    /// given a valid API Key and Secret your requests will work.
    pub fn new(token: AccessToken) -> Self {
        let client = Client::builder().build();
        match client {
            Ok(c) => Self { token, client: Arc::new(c) },
            Err(e) => panic!("creating client failed: {:?}", e),
        }
    }

    /// Get the currently set authorization token.
    pub fn get_token(&self) -> &AccessToken {
        &self.token
    }

    fn request<B>(&self, method: Method, path: String, body: B, query: Option<Vec<(&str, String)>>) -> Request
    where
        B: Serialize,
    {
        let base = Url::parse(ENDPOINT).unwrap();
        let url = base.join(&path).unwrap();

        // Check if the token is expired and panic.
        if self.token.is_expired() {
            panic!("token is expired");
        }

        let bt = format!("Bearer {}", self.token.as_str());
        let bearer = header::HeaderValue::from_str(&bt).unwrap();

        // Set the default headers.
        let mut headers = header::HeaderMap::new();
        headers.append(header::AUTHORIZATION, bearer);
        headers.append(header::CONTENT_TYPE, header::HeaderValue::from_static("application/json"));

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
        rb.build().unwrap()
    }

    /// Get values.
    pub async fn get_values(&self, sheet_id: &str, range: String) -> Result<ValueRange, APIError> {
        // Build the request.
        let request = self.request(
            Method::GET,
            format!("spreadsheets/{}/values/{}", sheet_id.to_string(), range),
            (),
            Some(vec![
                ("valueRenderOption", "FORMATTED_VALUE".to_string()),
                ("dateTimeRenderOption", "FORMATTED_STRING".to_string()),
                ("majorDimension", "ROWS".to_string()),
            ]),
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
        Ok(resp.json().await.unwrap())
    }

    /// Update values.
    pub async fn update_values(&self, sheet_id: &str, range: &str, value: String) -> Result<UpdateValuesResponse, APIError> {
        // Build the request.
        let request = self.request(
            Method::PUT,
            format!("spreadsheets/{}/values/{}", sheet_id.to_string(), range.to_string()),
            ValueRange {
                range: Some(range.to_string()),
                values: Some(vec![vec![value]]),
                major_dimension: None,
            },
            Some(vec![
                ("valueInputOption", "USER_ENTERED".to_string()),
                ("responseValueRenderOption", "FORMATTED_VALUE".to_string()),
                ("responseDateTimeRenderOption", "FORMATTED_STRING".to_string()),
            ]),
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
        Ok(resp.json().await.unwrap())
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

/// A range of values.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct ValueRange {
    /// The range the values cover, in A1 notation.
    /// For output, this range indicates the entire requested range,
    /// even though the values will exclude trailing rows and columns.
    /// When appending values, this field represents the range to search for a
    /// table, after which values will be appended.
    pub range: Option<String>,
    /// The data that was read or to be written.  This is an array of arrays,
    /// the outer array representing all the data and each inner array
    /// representing a major dimension. Each item in the inner array
    /// corresponds with one cell.
    ///
    /// For output, empty trailing rows and columns will not be included.
    ///
    /// For input, supported value types are: bool, string, and double.
    /// Null values will be skipped.
    /// To set a cell to an empty value, set the string value to an empty string.
    pub values: Option<Vec<Vec<String>>>,
    /// The major dimension of the values.
    ///
    /// For output, if the spreadsheet data is: `A1=1,B1=2,A2=3,B2=4`,
    /// then requesting `range=A1:B2,majorDimension=ROWS` will return
    /// `[[1,2],[3,4]]`,
    /// whereas requesting `range=A1:B2,majorDimension=COLUMNS` will return
    /// `[[1,3],[2,4]]`.
    ///
    /// For input, with `range=A1:B2,majorDimension=ROWS` then `[[1,2],[3,4]]`
    /// will set `A1=1,B1=2,A2=3,B2=4`. With `range=A1:B2,majorDimension=COLUMNS`
    /// then `[[1,2],[3,4]]` will set `A1=1,B1=3,A2=2,B2=4`.
    ///
    /// When writing, if this field is not set, it defaults to ROWS.
    #[serde(rename = "majorDimension")]
    pub major_dimension: Option<String>,
}

/// The response returned from updating values.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct UpdateValuesResponse {
    /// The number of columns where at least one cell in the column was updated.
    #[serde(rename = "updatedColumns")]
    pub updated_columns: Option<i32>,
    /// The range (in A1 notation) that updates were applied to.
    #[serde(rename = "updatedRange")]
    pub updated_range: Option<String>,
    /// The number of rows where at least one cell in the row was updated.
    #[serde(rename = "updatedRows")]
    pub updated_rows: Option<i32>,
    /// The values of the cells after updates were applied.
    /// This is only included if the request's `includeValuesInResponse` field
    /// was `true`.
    #[serde(rename = "updatedData")]
    pub updated_data: Option<ValueRange>,
    /// The spreadsheet the updates were applied to.
    #[serde(rename = "spreadsheetId")]
    pub spreadsheet_id: Option<String>,
    /// The number of cells updated.
    #[serde(rename = "updatedCells")]
    pub updated_cells: Option<i32>,
}
