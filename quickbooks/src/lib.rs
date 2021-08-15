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
 * async fn list_purchases() {
 *     // Initialize the QuickBooks client.
 *     let quickbooks = QuickBooks::new_from_env("", "", "");
 *
 *     let purchases = quickbooks.list_purchases().await.unwrap();
 *
 *     println!("{:?}", purchases);
 * }
 * ```
 */
use std::{env, error, fmt, sync::Arc};

use chrono::{DateTime, NaiveDate, Utc};
use reqwest::{header, Client, Method, Request, StatusCode, Url};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Endpoint for the QuickBooks API.
const ENDPOINT: &str = "https://quickbooks.api.intuit.com/v3/";

const QUERY_PAGE_SIZE: i64 = 1000;

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
    pub fn new<I, K, B, R, T, Q>(
        client_id: I,
        client_secret: K,
        company_id: B,
        redirect_uri: R,
        token: T,
        refresh_token: Q,
    ) -> Self
    where
        I: ToString,
        K: ToString,
        B: ToString,
        R: ToString,
        T: ToString,
        Q: ToString,
    {
        let client = Client::builder().build();
        match client {
            Ok(c) => {
                let qb = QuickBooks {
                    client_id: client_id.to_string(),
                    client_secret: client_secret.to_string(),
                    company_id: company_id.to_string(),
                    redirect_uri: redirect_uri.to_string(),
                    token: token.to_string(),
                    refresh_token: refresh_token.to_string(),

                    client: Arc::new(c),
                };

                if qb.token.is_empty() || qb.refresh_token.is_empty() {
                    // This is super hacky and a work around since there is no way to
                    // auth without using the browser.
                    println!("quickbooks consent URL: {}", qb.user_consent_url());
                }
                // We do not refresh the access token since we leave that up to the
                // user to do so they can re-save it to their database.

                qb
            }
            Err(e) => panic!("creating client failed: {:?}", e),
        }
    }

    /// Create a new QuickBooks client struct from environment variables. It
    /// takes a type that can convert into
    /// an &str (`String` or `Vec<u8>` for example). As long as the function is
    /// given a valid API key and your requests will work.
    /// We pass in the token and refresh token to the client so if you are storing
    /// it in a database, you can get it first.
    pub fn new_from_env<C, T, R>(company_id: C, token: T, refresh_token: R) -> Self
    where
        C: ToString,
        T: ToString,
        R: ToString,
    {
        let client_id = env::var("QUICKBOOKS_CLIENT_ID").unwrap();
        let client_secret = env::var("QUICKBOOKS_CLIENT_SECRET").unwrap();
        let redirect_uri = env::var("QUICKBOOKS_REDIRECT_URI").unwrap();

        QuickBooks::new(client_id, client_secret, company_id, redirect_uri, token, refresh_token)
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
        headers.append(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/json"),
        );
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
        let state = uuid::Uuid::new_v4();
        format!(
            "https://appcenter.intuit.com/connect/oauth2?client_id={}&response_type=code&scope=com.intuit.quickbooks.accounting&redirect_uri={}&state={}",
            self.client_id, self.redirect_uri, state
        )
    }

    pub async fn refresh_access_token(&mut self) -> Result<AccessToken, APIError> {
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
        self.refresh_token = t.refresh_token.to_string();

        Ok(t)
    }

    pub async fn get_access_token(&mut self, code: &str) -> Result<AccessToken, APIError> {
        let mut headers = header::HeaderMap::new();
        headers.append(header::ACCEPT, header::HeaderValue::from_static("application/json"));

        let params = [
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", &self.redirect_uri),
        ];
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
        self.refresh_token = t.refresh_token.to_string();

        Ok(t)
    }

    pub async fn company_info(&self, company_id: &str) -> Result<CompanyInfo, APIError> {
        // Build the request.
        let request = self.request(
            Method::GET,
            &format!("company/{}/query", company_id),
            (),
            Some(&[("query", "select * from CompanyInfo")]),
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

        let r: CompanyInfoResponse = resp.json().await.unwrap();

        Ok(r.query_response.company_info.get(0).unwrap().clone())
    }

    pub async fn list_attachments_for_purchase(&self, purchase_id: &str) -> Result<Vec<Attachment>, APIError> {
        // Build the request.
        let request = self.request(
            Method::GET,
            &format!("company/{}/query", self.company_id),
            (),
            Some(&[(
                "query",
                &format!(
                    "select * from attachable where AttachableRef.EntityRef.Type = 'purchase' and \
                     AttachableRef.EntityRef.value = '{}' MAXRESULTS {}",
                    purchase_id, QUERY_PAGE_SIZE
                ),
            )]),
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

        let r: AttachmentResponse = resp.json().await.unwrap();

        Ok(r.query_response.attachable)
    }

    pub async fn list_attachments_for_bill(&self, bill_id: &str) -> Result<Vec<Attachment>, APIError> {
        // Build the request.
        let request = self.request(
            Method::GET,
            &format!("company/{}/query", self.company_id),
            (),
            Some(&[(
                "query",
                &format!(
                    "select * from attachable where AttachableRef.EntityRef.Type = 'bill' and \
                     AttachableRef.EntityRef.value = '{}' MAXRESULTS {}",
                    bill_id, QUERY_PAGE_SIZE
                ),
            )]),
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

        let r: AttachmentResponse = resp.json().await.unwrap();

        Ok(r.query_response.attachable)
    }

    pub async fn list_attachments_for_bill_payment(&self, bill_payment_id: &str) -> Result<Vec<Attachment>, APIError> {
        // Build the request.
        let request = self.request(
            Method::GET,
            &format!("company/{}/query", self.company_id),
            (),
            Some(&[(
                "query",
                &format!(
                    "select * from attachable where AttachableRef.EntityRef.Type = 'billpayment' \
                     and AttachableRef.EntityRef.value = '{}' MAXRESULTS {}",
                    bill_payment_id, QUERY_PAGE_SIZE
                ),
            )]),
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

        let r: AttachmentResponse = resp.json().await.unwrap();

        Ok(r.query_response.attachable)
    }

    pub async fn get_bill(&self, bill_id: &str) -> Result<Bill, APIError> {
        // Build the request.
        let request = self.request(
            Method::GET,
            &format!("company/{}/bill/{}", self.company_id, bill_id),
            (),
            None,
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

        let r: BillResponse = resp.json().await.unwrap();

        Ok(r.bill)
    }

    pub async fn fetch_bill_payment_page(&self, start_position: i64) -> Result<Vec<BillPayment>, APIError> {
        // Build the request.
        let request = self.request(
            Method::GET,
            &format!("company/{}/query", self.company_id),
            (),
            Some(&[(
                "query",
                &format!(
                    "SELECT * FROM BillPayment ORDERBY Id STARTPOSITION {} MAXRESULTS {}",
                    start_position, QUERY_PAGE_SIZE
                ),
            )]),
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

        let r: BillPaymentResponse = resp.json().await.unwrap();

        Ok(r.query_response.bill_payment)
    }

    pub async fn list_bill_payments(&self) -> Result<Vec<BillPayment>, APIError> {
        // Build the request.
        let request = self.request(
            Method::GET,
            &format!("company/{}/query", self.company_id),
            (),
            Some(&[("query", "SELECT COUNT(*) FROM BillPayment")]),
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

        let r: CountResponse = resp.json().await.unwrap();
        let mut bill_payments: Vec<BillPayment> = Vec::new();

        let mut i = 0;
        while i < r.query_response.total_count {
            let mut page = self.fetch_bill_payment_page(i + 1).await.unwrap();

            // Add our page to our array.
            bill_payments.append(&mut page);

            i += QUERY_PAGE_SIZE;
        }

        Ok(bill_payments)
    }

    pub async fn fetch_purchase_page(&self, start_position: i64) -> Result<Vec<Purchase>, APIError> {
        // Build the request.
        let request = self.request(
            Method::GET,
            &format!("company/{}/query", self.company_id),
            (),
            Some(&[(
                "query",
                &format!(
                    "SELECT * FROM Purchase ORDERBY Id STARTPOSITION {} MAXRESULTS {}",
                    start_position, QUERY_PAGE_SIZE
                ),
            )]),
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

        let r: PurchaseResponse = resp.json().await.unwrap();

        Ok(r.query_response.purchase)
    }

    pub async fn list_purchases(&self) -> Result<Vec<Purchase>, APIError> {
        // Build the request.
        let request = self.request(
            Method::GET,
            &format!("company/{}/query", self.company_id),
            (),
            Some(&[("query", "SELECT COUNT(*) FROM Purchase")]),
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

        let r: CountResponse = resp.json().await.unwrap();
        let mut purchases: Vec<Purchase> = Vec::new();

        let mut i = 0;
        while i < r.query_response.total_count {
            let mut page = self.fetch_purchase_page(i + 1).await.unwrap();

            // Add our page to our array.
            purchases.append(&mut page);

            i += QUERY_PAGE_SIZE;
        }

        Ok(purchases)
    }

    pub async fn list_items(&self) -> Result<Vec<Item>, APIError> {
        // Build the request.
        let request = self.request(
            Method::GET,
            &format!("company/{}/query", self.company_id),
            (),
            Some(&[("query", &format!("SELECT * FROM Item MAXRESULTS {}", QUERY_PAGE_SIZE))]),
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

        let items: ItemsResponse = resp.json().await.unwrap();

        Ok(items.query_response.item)
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
pub struct CompanyInfoResponse {
    #[serde(default, rename = "QueryResponse")]
    pub query_response: QueryResponse,
}

#[derive(Debug, JsonSchema, Clone, Default, Serialize, Deserialize)]
pub struct QueryResponse {
    #[serde(default, rename = "totalCount")]
    pub total_count: i64,
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "Item")]
    pub item: Vec<Item>,
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "Purchase")]
    pub purchase: Vec<Purchase>,
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "Attachable")]
    pub attachable: Vec<Attachment>,
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "BillPayment")]
    pub bill_payment: Vec<BillPayment>,
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "CompanyInfo")]
    pub company_info: Vec<CompanyInfo>,
    #[serde(default, rename = "startPosition")]
    pub start_position: i64,
    #[serde(default, rename = "maxResults")]
    pub max_results: i64,
}

#[derive(Debug, JsonSchema, Clone, Serialize, Deserialize)]
pub struct ItemsResponse {
    #[serde(default, rename = "QueryResponse")]
    pub query_response: QueryResponse,
    pub time: DateTime<Utc>,
}

#[derive(Debug, JsonSchema, Clone, Serialize, Deserialize)]
pub struct Item {
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "Name")]
    pub name: String,
    #[serde(default, rename = "Active")]
    pub active: bool,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "FullyQualifiedName")]
    pub fully_qualified_name: String,
    #[serde(default, rename = "Taxable")]
    pub taxable: bool,
    #[serde(default, rename = "UnitPrice")]
    pub unit_price: f32,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "Type")]
    pub item_type: String,
    #[serde(default, rename = "PurchaseCost")]
    pub purchase_cost: f32,
    #[serde(default, rename = "ExpenseAccountRef")]
    pub expense_account_ref: NtRef,
    #[serde(default, rename = "TrackQtyOnHand")]
    pub track_qty_on_hand: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub domain: String,
    #[serde(default)]
    pub sparse: bool,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "Id")]
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "SyncToken")]
    pub sync_token: String,
    #[serde(rename = "MetaData")]
    pub meta_data: MetaData,
    #[serde(default, rename = "SubItem")]
    pub sub_item: bool,
    #[serde(default, rename = "ParentRef")]
    pub parent_ref: NtRef,
    #[serde(default, rename = "Level")]
    pub level: i64,
    #[serde(default, rename = "IncomeAccountRef")]
    pub income_account_ref: NtRef,
    #[serde(default, rename = "AssetAccountRef")]
    pub asset_account_ref: NtRef,
    #[serde(default, rename = "QtyOnHand")]
    pub qty_on_hand: i64,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "InvStartDate")]
    pub inv_start_date: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "Description")]
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "PurchaseDesc")]
    pub purchase_desc: String,
}

#[derive(Debug, JsonSchema, Clone, Default, Serialize, Deserialize)]
pub struct NtRef {
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "Value")]
    pub value: String,
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "Name")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "type")]
    pub entity_ref_type: String,
}

#[derive(Debug, JsonSchema, Clone, Serialize, Deserialize)]
pub struct MetaData {
    #[serde(rename = "CreateTime")]
    pub create_time: DateTime<Utc>,
    #[serde(rename = "LastUpdatedTime")]
    pub last_updated_time: DateTime<Utc>,
}

#[derive(Debug, JsonSchema, Clone, Serialize, Deserialize)]
pub struct PurchaseResponse {
    #[serde(default, rename = "QueryResponse")]
    pub query_response: QueryResponse,
    pub time: String,
}

#[derive(Debug, JsonSchema, Clone, Serialize, Deserialize)]
pub struct BillPaymentResponse {
    #[serde(default, rename = "QueryResponse")]
    pub query_response: QueryResponse,
    pub time: String,
}

#[derive(Debug, JsonSchema, Clone, Serialize, Deserialize)]
pub struct AttachmentResponse {
    #[serde(default, rename = "QueryResponse")]
    pub query_response: QueryResponse,
    pub time: String,
}

#[derive(Debug, JsonSchema, Clone, Serialize, Deserialize)]
pub struct Purchase {
    #[serde(default, rename = "AccountRef")]
    pub account_ref: NtRef,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "PaymentType")]
    pub payment_type: String,
    #[serde(default, rename = "EntityRef")]
    pub entity_ref: NtRef,
    #[serde(default, rename = "TotalAmt")]
    pub total_amt: f32,
    #[serde(default, rename = "PurchaseEx")]
    pub purchase_ex: PurchaseEx,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub domain: String,
    pub sparse: bool,
    #[serde(rename = "Id")]
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "SyncToken")]
    pub sync_token: String,
    #[serde(rename = "MetaData")]
    pub meta_data: MetaData,
    #[serde(rename = "TxnDate")]
    pub txn_date: NaiveDate,
    #[serde(default, rename = "CurrencyRef")]
    pub currency_ref: NtRef,
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "Line")]
    pub line: Vec<Line>,
    #[serde(default, rename = "Credit")]
    pub credit: bool,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "DocNumber")]
    pub doc_number: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "PrivateNote")]
    pub private_note: String,
}

#[derive(Debug, JsonSchema, Default, Clone, Serialize, Deserialize)]
pub struct Line {
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "Id")]
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "Description")]
    pub description: String,
    #[serde(default, rename = "Amount")]
    pub amount: f32,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "DetailType")]
    pub detail_type: String,
    #[serde(default, rename = "AccountBasedExpenseLineDetail")]
    pub account_based_expense_line_detail: AccountBasedExpenseLineDetail,
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "LinkedTxn")]
    pub linked_txn: Vec<LinkedTxn>,
}

#[derive(Debug, JsonSchema, Default, Clone, Serialize, Deserialize)]
pub struct LinkedTxn {
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "TxnId")]
    pub txn_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "TxnType")]
    pub txn_type: String,
}

#[derive(Debug, JsonSchema, Default, Clone, Serialize, Deserialize)]
pub struct AccountBasedExpenseLineDetail {
    #[serde(default, rename = "AccountRef")]
    pub account_ref: NtRef,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "BillableStatus")]
    pub billable_status: String,
    #[serde(default, rename = "TaxCodeRef")]
    pub tax_code_ref: NtRef,
}

#[derive(Debug, JsonSchema, Default, Clone, Serialize, Deserialize)]
pub struct PurchaseEx {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub any: Vec<Any>,
}

#[derive(Debug, JsonSchema, Default, Clone, Serialize, Deserialize)]
pub struct Any {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "declaredType")]
    pub declared_type: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub scope: String,
    #[serde(default)]
    pub value: NtRef,
    #[serde(default)]
    pub nil: bool,
    #[serde(default, rename = "globalScope")]
    pub global_scope: bool,
    #[serde(default, rename = "typeSubstituted")]
    pub type_substituted: bool,
}

#[derive(Debug, JsonSchema, Clone, Serialize, Deserialize)]
pub struct Attachment {
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "FileName")]
    pub file_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "FileAccessUri")]
    pub file_access_uri: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "TempDownloadUri")]
    pub temp_download_uri: String,
    #[serde(default, rename = "Size")]
    pub size: i64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub domain: String,
    #[serde(default)]
    pub sparse: bool,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "Id")]
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "SyncToken")]
    pub sync_token: String,
    #[serde(rename = "MetaData")]
    pub meta_data: MetaData,
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "AttachableRef")]
    pub attachable_ref: Vec<AttachableRef>,
}

#[derive(Debug, JsonSchema, Default, Clone, Serialize, Deserialize)]
pub struct AttachableRef {
    #[serde(default, rename = "EntityRef")]
    pub entity_ref: NtRef,
    #[serde(default, rename = "IncludeOnSend")]
    pub include_on_send: bool,
}

#[derive(Debug, JsonSchema, Clone, Serialize, Deserialize)]
pub struct BillPayment {
    #[serde(default, rename = "VendorRef")]
    pub vendor_ref: NtRef,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "PayType")]
    pub pay_type: String,
    #[serde(default, rename = "CheckPayment")]
    pub check_payment: Payment,
    #[serde(default, rename = "TotalAmt")]
    pub total_amt: f32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub domain: String,
    #[serde(default)]
    pub sparse: bool,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "Id")]
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "SyncToken")]
    pub sync_token: String,
    #[serde(rename = "MetaData")]
    pub meta_data: MetaData,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "DocNumber")]
    pub doc_number: String,
    #[serde(rename = "TxnDate")]
    pub txn_date: NaiveDate,
    #[serde(default, rename = "CurrencyRef")]
    pub currency_ref: NtRef,
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "Line")]
    pub line: Vec<Line>,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "PrivateNote")]
    pub private_note: String,
    #[serde(default, rename = "CreditCardPayment")]
    pub credit_card_payment: Payment,
}

#[derive(Debug, JsonSchema, Default, Clone, Serialize, Deserialize)]
pub struct Payment {
    #[serde(default, rename = "CCAccountRef")]
    pub cc_account_ref: NtRef,
    #[serde(default, rename = "BankAccountRef")]
    pub bank_account_ref: NtRef,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "PrintStatus")]
    pub print_status: String,
}

#[derive(Debug, JsonSchema, Clone, Serialize, Deserialize)]
pub struct BillResponse {
    #[serde(rename = "Bill")]
    pub bill: Bill,
    pub time: DateTime<Utc>,
}

#[derive(Debug, JsonSchema, Clone, Serialize, Deserialize)]
pub struct Bill {
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "SyncToken")]
    pub sync_token: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub domain: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "Id")]
    pub id: String,
    #[serde(default, rename = "APAccountRef")]
    pub ap_account_ref: NtRef,
    #[serde(default, rename = "VendorRef")]
    pub vendor_ref: NtRef,
    #[serde(rename = "TxnDate")]
    pub txn_date: NaiveDate,
    #[serde(default, rename = "TotalAmt")]
    pub total_amt: f64,
    #[serde(default, rename = "CurrencyRef")]
    pub currency_ref: NtRef,
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "LinkedTxn")]
    pub linked_txn: Vec<LinkedTxn>,
    #[serde(default, rename = "SalesTermRef")]
    pub sales_term_ref: NtRef,
    #[serde(rename = "DueDate")]
    pub due_date: NaiveDate,
    #[serde(default)]
    pub sparse: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "Line")]
    pub line: Vec<Line>,
    #[serde(default, rename = "Balance")]
    pub balance: i64,
    #[serde(rename = "MetaData")]
    pub meta_data: MetaData,
}

#[derive(Debug, JsonSchema, Clone, Serialize, Deserialize)]
pub struct CompanyInfo {
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "SyncToken")]
    pub sync_token: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub domain: String,
    #[serde(default, rename = "LegalAddr")]
    pub legal_addr: Addr,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "SupportedLanguages")]
    pub supported_languages: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "CompanyName")]
    pub company_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "Country")]
    pub country: String,
    #[serde(default, rename = "CompanyAddr")]
    pub company_addr: Addr,
    #[serde(default)]
    pub sparse: bool,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "Id")]
    pub id: String,
    #[serde(default, rename = "WebAddr")]
    pub web_addr: WebAddr,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "FiscalYearStartMonth")]
    pub fiscal_year_start_month: String,
    #[serde(default, rename = "CustomerCommunicationAddr")]
    pub customer_communication_addr: Addr,
    #[serde(default, rename = "PrimaryPhone")]
    pub primary_phone: PrimaryPhone,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "LegalName")]
    pub legal_name: String,
    #[serde(rename = "CompanyStartDate")]
    pub company_start_date: NaiveDate,
    #[serde(default, rename = "Email")]
    pub email: Email,
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "NameValue")]
    pub name_value: Vec<NtRef>,
    #[serde(rename = "MetaData")]
    pub meta_data: MetaData,
}

#[derive(Debug, Default, JsonSchema, Clone, Serialize, Deserialize)]
pub struct Addr {
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "City")]
    pub city: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "Country")]
    pub country: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "Line1")]
    pub line1: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "PostalCode")]
    pub postal_code: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "CountrySubDivisionCode")]
    pub country_sub_division_code: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "Id")]
    pub id: String,
}

#[derive(Debug, Default, JsonSchema, Clone, Serialize, Deserialize)]
pub struct Email {
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "Address")]
    pub address: String,
}

#[derive(Debug, Default, JsonSchema, Clone, Serialize, Deserialize)]
pub struct PrimaryPhone {
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "FreeFormNumber")]
    pub free_form_number: String,
}

#[derive(Debug, Default, JsonSchema, Clone, Serialize, Deserialize)]
pub struct WebAddr {}
