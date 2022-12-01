use chrono::{DateTime, Utc};
use oauth2::{
    basic::{BasicClient, BasicErrorResponseType},
    reqwest::async_http_client,
    AuthUrl, ClientId, ClientSecret, Scope, StandardErrorResponse, TokenResponse, TokenUrl,
};
use reqwest::{header::HeaderValue, Client, Method, RequestBuilder, Response, StatusCode};
use serde::{Deserialize, Serialize};

use std::{
    sync::{Arc, RwLock},
    time::Instant,
};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Receipt {
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    pub id: String,
    pub receipt_url: String,
    pub transaction_id: String,
    pub user_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Reimbursement {
    pub amount: f64,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    pub currency: String,
    pub id: String,
    pub merchant: Option<String>,
    pub receipts: Vec<String>,
    pub transaction_date: Option<chrono::NaiveDate>,
    pub user_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct AccountingCategories {
    pub category_id: Option<String>,
    pub category_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct CardHolder {
    pub department_id: String,
    pub department_name: String,
    pub first_name: String,
    pub last_name: String,
    pub location_id: String,
    pub location_name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Disputes {
    pub created_at: Option<DateTime<Utc>>,
    pub id: String,
    pub memo: String,
    #[serde(skip_serializing_if = "Option::is_none", rename = "type")]
    pub type_: Option<GetTransactionResponseDataDisputesType>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub enum GetTransactionResponseDataDisputesType {
    #[serde(rename = "DISPUTE_CANCELLED")]
    DisputeCancelled,
    #[serde(rename = "MERCHANT_ERROR")]
    MerchantError,
    #[serde(rename = "UNKNOWN")]
    Unknown,
    #[serde(rename = "UNRECOGNIZED_CHARGE")]
    UnrecognizedCharge,
    #[serde(rename = "")]
    Noop,
    #[serde(other)]
    FallthroughString,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct PolicyViolations {
    pub created_at: Option<DateTime<Utc>>,
    pub id: String,
    pub memo: String,
    #[serde(skip_serializing_if = "Option::is_none", rename = "type")]
    pub type_: Option<PolicyViolationType>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub enum PolicyViolationType {
    #[serde(rename = "POLICY_VIOLATION_FROM_ADMIN")]
    PolicyViolationFromAdmin,
    #[serde(rename = "POLICY_VIOLATION_FROM_USER")]
    PolicyViolationFromUser,
    #[serde(rename = "")]
    Noop,
    #[serde(other)]
    FallthroughString,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Transaction {
    pub accounting_categories: Vec<AccountingCategories>,
    pub amount: f64,
    pub card_holder: CardHolder,
    pub card_id: String,
    pub disputes: Vec<Disputes>,
    pub id: String,
    pub memo: Option<String>,
    pub merchant_id: String,
    pub merchant_name: String,
    pub policy_violations: Vec<PolicyViolations>,
    pub receipts: Vec<String>,
    pub sk_category_id: f64,
    pub sk_category_name: String,
    pub state: String,
    pub user_transaction_time: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Department {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Location {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub enum Role {
    #[serde(rename = "BUSINESS_ADMIN")]
    Admin,
    #[serde(rename = "BUSINESS_BOOKKEEPER")]
    Bookkeeper,
    #[serde(rename = "BUSINESS_OWNER")]
    Owner,
    #[serde(rename = "BUSINESS_USER")]
    User,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum UserStatus {
    #[serde(rename = "INVITE_DELETED")]
    InviteDeleted,
    #[serde(rename = "INVITE_EXPIRED")]
    InviteExpired,
    #[serde(rename = "INVITE_PENDING")]
    InvitePending,
    #[serde(rename = "USER_ACTIVE")]
    Active,
    #[serde(rename = "USER_ONBOARDING")]
    Onboarding,
    #[serde(rename = "USER_SUSPENDED")]
    Suspended,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct User {
    pub id: String,
    pub business_id: String,
    pub department_id: String,
    pub location_id: String,
    pub manager_id: Option<String>,
    pub email: String,
    pub first_name: String,
    pub last_name: String,
    pub phone: Option<String>,
    pub status: UserStatus,
    pub role: Role,
    pub is_manager: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum WriteableRole {
    #[serde(rename = "BUSINESS_ADMIN")]
    Admin,
    #[serde(rename = "BUSINESS_BOOKKEEPER")]
    Bookkeeper,
    #[serde(rename = "BUSINESS_USER")]
    User,
}

impl From<WriteableRole> for Role {
    fn from(role: WriteableRole) -> Self {
        match role {
            WriteableRole::Admin => Self::Admin,
            WriteableRole::Bookkeeper => Self::Bookkeeper,
            WriteableRole::User => Self::User,
        }
    }
}

#[derive(Debug)]
pub struct OwnerRoleNotWriteable;

impl std::fmt::Display for OwnerRoleNotWriteable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Owner role is not allowed during write requests")
    }
}

impl std::error::Error for OwnerRoleNotWriteable {}

impl TryFrom<Role> for WriteableRole {
    type Error = OwnerRoleNotWriteable;

    fn try_from(role: Role) -> Result<WriteableRole, Self::Error> {
        match role {
            Role::Admin => Ok(WriteableRole::Admin),
            Role::Bookkeeper => Ok(WriteableRole::Bookkeeper),
            Role::Owner => Err(OwnerRoleNotWriteable),
            Role::User => Ok(WriteableRole::User),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateUserDeferred {
    pub idempotency_key: String,
    pub first_name: String,
    pub last_name: String,
    pub email: String,
    pub phone: String,
    pub role: WriteableRole,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub department_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direct_manager_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UpdateUser {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub department_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direct_manager_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<WriteableRole>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ResponseList<T> {
    pub data: Vec<T>,
    pub page: ResponsePagination,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ResponsePagination {
    pub next: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DeferredTaskId {
    pub id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ApiError {
    error_v2: ApiErrorDetails,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ApiErrorDetails {
    additional_info: serde_json::Value,
    error_code: String,
    message: String,
    notes: String,
}

struct AccessToken {
    secret: String,
    expires_at: Instant,
}

pub struct RampClient {
    access_token: Arc<RwLock<Option<AccessToken>>>,
    client: Client,
    oauth_client: BasicClient,
    scopes: Vec<Scope>,
}

impl RampClient {
    pub fn new(client_id: String, client_secret: String, scopes: Vec<String>) -> Self {
        Self {
            access_token: Arc::new(RwLock::new(None)),
            client: Client::new(),
            oauth_client: BasicClient::new(
                ClientId::new(client_id),
                Some(ClientSecret::new(client_secret)),
                AuthUrl::new("https://app.ramp.com/v1/authorize".to_string()).unwrap(),
                Some(TokenUrl::new("https://api.ramp.com/developer/v1/token".to_string()).unwrap()),
            ),
            scopes: scopes.into_iter().map(Scope::new).collect::<Vec<_>>(),
        }
    }

    async fn fetch_token(&self) -> Result<(), Error> {
        // Snapshot the time before the token request. Given that we only receive back an
        // "expires_in" duration we want to be conservative about determining when our access
        // will expire
        let now = Instant::now();

        let mut req = self.oauth_client.exchange_client_credentials();

        for scope in &self.scopes {
            req = req.add_scope(scope.clone());
        }

        let token = req.request_async(async_http_client).await?;
        let expires_at = token
            .expires_in()
            .as_ref()
            .and_then(|duration| now.checked_add(*duration))
            .ok_or(Error::ExpirationOutOfBounds)?;

        *self.access_token.write().unwrap() = Some(AccessToken {
            secret: token.access_token().secret().to_string(),
            expires_at,
        });

        Ok(())
    }

    fn token_is_expired(&self) -> bool {
        if let Ok(guard) = self.access_token.read() {
            guard
                .as_ref()
                .map(|token| token.expires_at <= Instant::now())
                .unwrap_or(true)
        } else {
            // If we do not have an access token then we consider it to be expired
            true
        }
    }

    pub async fn execute(&self, mut builder: reqwest::RequestBuilder) -> Result<Response, Error> {
        if self.token_is_expired() {
            self.fetch_token().await?;
        }

        if let Ok(guard) = self.access_token.read() {
            if let Some(token) = &*guard {
                builder = builder.bearer_auth(&token.secret);
            }
        }

        builder = builder.header(
            reqwest::header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );

        let request = builder.build()?;
        let response = self.client.execute(request).await?;

        if response.status().is_informational() || response.status().is_success() || response.status().is_redirection()
        {
            Ok(response)
        } else {
            let status = response.status();
            let error: Option<ApiError> = response.json().await.ok();
            Err(Error::RequestFailed { status, error })
        }
    }

    fn request(&self, method: Method, path: &str) -> RequestBuilder {
        self.client
            .request(method, format!("https://api.ramp.com/developer/v1/{}", path))
    }

    pub fn departments(&self) -> DepartmentClient {
        DepartmentClient { client: self }
    }

    pub fn receipts(&self) -> ReceiptClient {
        ReceiptClient { client: self }
    }

    pub fn reimbursements(&self) -> ReimbursementClient {
        ReimbursementClient { client: self }
    }

    pub fn transactions(&self) -> TransactionClient {
        TransactionClient { client: self }
    }

    pub fn users(&self) -> UserClient {
        UserClient { client: self }
    }
}

pub struct DepartmentClient<'a> {
    client: &'a RampClient,
}

impl<'a> DepartmentClient<'a> {
    pub async fn list(&self) -> Result<ResponseList<Department>, Error> {
        let req = self.client.request(Method::GET, "departments/");
        Ok(self.client.execute(req).await?.json().await?)
    }
}

pub struct ReceiptClient<'a> {
    client: &'a RampClient,
}

impl<'a> ReceiptClient<'a> {
    pub async fn get(&self, receipt_id: &str) -> Result<Receipt, Error> {
        let req = self.client.request(Method::GET, &format!("receipts/{}", receipt_id));
        Ok(self.client.execute(req).await?.json().await?)
    }
}

pub struct ReimbursementClient<'a> {
    client: &'a RampClient,
}

impl<'a> ReimbursementClient<'a> {
    pub async fn list(&self) -> Result<ResponseList<Reimbursement>, Error> {
        let req = self.client.request(Method::GET, "reimbursements");
        Ok(self.client.execute(req).await?.json().await?)
    }
}

pub struct TransactionClient<'a> {
    client: &'a RampClient,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct ListTransactionsQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    department_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    location_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    merchant_id: Option<String>,
    #[serde(rename = "sk_category_id", skip_serializing_if = "Option::is_none")]
    category_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    from_date: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    to_date: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "is_false")]
    order_by_date_asc: bool,
    #[serde(skip_serializing_if = "is_false")]
    order_by_date_desc: bool,
    #[serde(skip_serializing_if = "is_false")]
    order_by_amount_asc: bool,
    #[serde(skip_serializing_if = "is_false")]
    order_by_amount_desc: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    state: Option<String>,
    min_amount: f32,
    max_amount: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    page_size: Option<u32>,
}

impl<'a> TransactionClient<'a> {
    pub async fn get(&self, transaction_id: &str) -> Result<Transaction, Error> {
        let req = self
            .client
            .request(Method::GET, &format!("transactions/{}", transaction_id));
        Ok(self.client.execute(req).await?.json().await?)
    }

    pub async fn list(&self, query: &ListTransactionsQuery) -> Result<ResponseList<Transaction>, Error> {
        let req = self.client.request(Method::GET, "transactions/").query(query);
        Ok(self.client.execute(req).await?.json().await?)
    }
}

pub struct UserClient<'a> {
    client: &'a RampClient,
}

impl<'a> UserClient<'a> {
    pub async fn get(&self, user_id: &str) -> Result<User, Error> {
        let req = self.client.request(Method::GET, &format!("users/{}", user_id));
        Ok(self.client.execute(req).await?.json().await?)
    }

    pub async fn list(&self) -> Result<ResponseList<User>, Error> {
        let req = self.client.request(Method::GET, "users/");
        Ok(self.client.execute(req).await?.json().await?)
    }

    pub async fn deferred_create(&self, payload: &CreateUserDeferred) -> Result<DeferredTaskId, Error> {
        let req = self.client.request(Method::POST, "users/deferred").json(payload);
        Ok(self.client.execute(req).await?.json().await?)
    }

    pub async fn update(&self, user_id: &str, payload: &UpdateUser) -> Result<(), Error> {
        let req = self
            .client
            .request(Method::PATCH, &format!("users/{}", user_id))
            .json(payload);
        Ok(self.client.execute(req).await?.json().await?)
    }
}

#[derive(Debug)]
pub enum Error {
    Client(reqwest::Error),
    ExpirationOutOfBounds,
    RequestFailed {
        status: StatusCode,
        error: Option<ApiError>,
    },
    Token(
        oauth2::RequestTokenError<
            oauth2::reqwest::Error<reqwest::Error>,
            StandardErrorResponse<BasicErrorResponseType>,
        >,
    ),
}

impl From<reqwest::Error> for Error {
    fn from(err: reqwest::Error) -> Self {
        Self::Client(err)
    }
}

impl
    From<
        oauth2::RequestTokenError<
            oauth2::reqwest::Error<reqwest::Error>,
            StandardErrorResponse<BasicErrorResponseType>,
        >,
    > for Error
{
    fn from(
        err: oauth2::RequestTokenError<
            oauth2::reqwest::Error<reqwest::Error>,
            StandardErrorResponse<BasicErrorResponseType>,
        >,
    ) -> Self {
        Self::Token(err)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Client(inner) => write!(f, "Client error: {}", inner),
            Error::ExpirationOutOfBounds => write!(f, "Access token contains an invalid expiration duration"),
            Error::RequestFailed { status, .. } => write!(
                f,
                "Request failed to return a successful response. Instead a {} was returned",
                status
            ),
            Error::Token(inner) => write!(f, "Failure to retrieve access token: {}", inner),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Client(inner) => Some(inner),
            Error::Token(inner) => Some(inner),
            _ => None,
        }
    }
}

fn is_false(value: &bool) -> bool {
    !(*value)
}
