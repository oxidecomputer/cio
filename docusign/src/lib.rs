/*!
 * A rust library for interacting with the DocuSign API.
 *
 * For more information, you can check out their documentation at:
 * https://developers.docusign.com/docs/esign-rest-api/reference/
 *
 * Example:
 *
 * ```ignore
 * use docusign::DocuSign;
 * use serde::{Deserialize, Serialize};
 *
 * async fn get_envelope() {
 *     // Initialize the DocuSign client.
 *     let docusign = DocuSign::new_from_env("", "", "", "");
 *
 *     let envelope = docusign.get_envelope("some-envelope-id").await.unwrap();
 *
 *     println!("{:?}", envelope);
 * }
 * ```
 */
#![allow(clippy::field_reassign_with_default)]
use std::{env, error, fmt, sync::Arc};

use bytes::Bytes;
use chrono::{offset::Utc, DateTime};
use reqwest::{header, Client, Method, Request, StatusCode, Url};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Entrypoint for interacting with the DocuSign API.
#[derive(Debug, Clone)]
pub struct DocuSign {
    token: String,
    // This expires in 101 days. It is hardcoded in the GitHub Actions secrets,
    // We might want something a bit better like storing it in the database.
    refresh_token: String,
    client_id: String,
    client_secret: String,
    redirect_uri: String,
    account_id: String,
    endpoint: String,

    client: Arc<Client>,
}

impl DocuSign {
    /// Create a new DocuSign client struct. It takes a type that can convert into
    /// an &str (`String` or `Vec<u8>` for example). As long as the function is
    /// given a valid API key your requests will work.
    pub fn new<I, K, R, T, Q, A, E>(
        client_id: I,
        client_secret: K,
        redirect_uri: R,
        token: T,
        refresh_token: Q,
        account_id: A,
        endpoint: E,
    ) -> Self
    where
        I: ToString,
        K: ToString,
        R: ToString,
        T: ToString,
        Q: ToString,
        A: ToString,
        E: ToString,
    {
        let client = Client::builder().build();
        match client {
            Ok(c) => {
                let g = DocuSign {
                    client_id: client_id.to_string(),
                    client_secret: client_secret.to_string(),
                    redirect_uri: redirect_uri.to_string(),
                    token: token.to_string(),
                    refresh_token: refresh_token.to_string(),
                    account_id: account_id.to_string(),
                    endpoint: endpoint.to_string(),

                    client: Arc::new(c),
                };

                if g.token.is_empty() || g.refresh_token.is_empty() {
                    // This is super hacky and a work around since there is no way to
                    // auth without using the browser.
                    println!("docusign consent URL: {}", g.user_consent_url());
                }
                // We do not refresh the access token since we leave that up to the
                // user to do so they can re-save it to their database.

                g
            }
            Err(e) => panic!("creating client failed: {:?}", e),
        }
    }

    /// Create a new DocuSign client struct from environment variables. It
    /// takes a type that can convert into
    /// an &str (`String` or `Vec<u8>` for example). As long as the function is
    /// given a valid API key and your requests will work.
    pub fn new_from_env<T, R, A, E>(token: T, refresh_token: R, account_id: A, endpoint: E) -> Self
    where
        T: ToString,
        R: ToString,
        A: ToString,
        E: ToString,
    {
        let client_id = env::var("DOCUSIGN_INTEGRATION_KEY").unwrap();
        let client_secret = env::var("DOCUSIGN_CLIENT_SECRET").unwrap();
        let redirect_uri = env::var("DOCUSIGN_REDIRECT_URI").unwrap();

        DocuSign::new(
            client_id,
            client_secret,
            redirect_uri,
            token,
            refresh_token,
            account_id,
            endpoint,
        )
    }

    /// user_consent_url creates a url allowing a user to consent to impersonation
    /// https://developers.docusign.com/esign-rest-api/guides/authentication/obtaining-consent#individual-consent
    pub fn user_consent_url(&self) -> String {
        let scope = "signature impersonation";
        let endpoint = "https://account.docusign.com/oauth/auth";

        let state = uuid::Uuid::new_v4();

        // docusign insists upon %20 not + in scope definition
        format!(
            "{}?response_type=code&scope={}&client_id={}&redirect_uri={}&state={}",
            endpoint,
            scope.replace(' ', "%20"),
            self.client_id,
            self.redirect_uri,
            state,
        )
    }

    fn request<B>(&self, method: Method, path: &str, body: B, query: Option<&[(&str, &str)]>) -> Request
    where
        B: Serialize,
    {
        let base = Url::parse(&format!("{}/restapi/v2.1/", self.endpoint)).unwrap();
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

    pub async fn get_user_info(&self) -> Result<UserInfo, APIError> {
        let mut headers = header::HeaderMap::new();
        headers.append(header::ACCEPT, header::HeaderValue::from_static("application/json"));
        let bt = format!("Bearer {}", self.token);
        let bearer = header::HeaderValue::from_str(&bt).unwrap();
        headers.append(header::AUTHORIZATION, bearer);

        let client = reqwest::Client::new();
        let resp = client
            .get("https://account.docusign.com/oauth/userinfo")
            .headers(headers)
            .send()
            .await
            .unwrap();

        // Unwrap the response.
        Ok(resp.json().await.unwrap())
    }

    pub async fn refresh_access_token(&mut self) -> Result<AccessToken, APIError> {
        let mut headers = header::HeaderMap::new();
        headers.append(header::ACCEPT, header::HeaderValue::from_static("application/json"));

        let params = [("grant_type", "refresh_token"), ("refresh_token", &self.refresh_token)];
        let client = reqwest::Client::new();
        let resp = client
            .post("https://account.docusign.com/oauth/token")
            .headers(headers)
            .form(&params)
            .basic_auth(&self.client_id, Some(&self.client_secret))
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

        let params = [("grant_type", "authorization_code"), ("code", code)];
        let client = reqwest::Client::new();
        let resp = client
            .post("https://account.docusign.com/oauth/token")
            .headers(headers)
            .form(&params)
            .basic_auth(&self.client_id, Some(&self.client_secret))
            .send()
            .await
            .unwrap();

        // Unwrap the response.
        let t: AccessToken = resp.json().await.unwrap();

        self.token = t.access_token.to_string();
        self.refresh_token = t.refresh_token.to_string();

        Ok(t)
    }

    /// List templates.
    pub async fn list_templates(&self) -> Result<Vec<Template>, APIError> {
        // Build the request.
        let request = self.request(
            Method::GET,
            &format!("accounts/{}/templates", self.account_id),
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

        let r: TemplatesResponse = resp.json().await.unwrap();
        Ok(r.envelope_templates)
    }

    /// Get an envelope.
    pub async fn get_envelope(&self, envelope_id: &str) -> Result<Envelope, APIError> {
        // Build the request.
        let request = self.request(
            Method::GET,
            &format!("accounts/{}/envelopes/{}", self.account_id, envelope_id),
            (),
            Some(&[("include", "documents")]),
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

        Ok(resp.json().await.unwrap())
    }

    /// List webhooks with "Connect".
    pub async fn list_webhooks(&self) -> Result<Vec<Webhook>, APIError> {
        // Build the request.
        let request = self.request(Method::GET, &format!("accounts/{}/connect", self.account_id), (), None);

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

        let r: WebhooksResponse = resp.json().await.unwrap();

        Ok(r.configurations)
    }

    /// Create a webhook with "Connect".
    pub async fn create_webhook(&self) -> Result<Webhook, APIError> {
        let mut connect: Webhook = Default::default();
        connect.url_to_publish_to = env::var("DOCUSIGN_WEBHOOK_ENDPOINT").unwrap();
        connect.allow_envelope_publish = "true".to_string();
        connect.envelope_events = vec![
            "Completed".to_string(),
            "Sent".to_string(),
            "Declined".to_string(),
            "Delivered".to_string(),
            "Signed".to_string(),
            "Voided".to_string(),
        ];
        connect.all_users = "true".to_string();
        connect.include_document_fields = "true".to_string();
        connect.name = "CIO Webhook".to_string();
        // This is the only valid choice.
        connect.configuration_type = "custom".to_string();
        connect.include_document_fields = "true".to_string();
        connect.include_time_zone_information = "true".to_string();
        connect.use_soap_interface = "false".to_string();
        connect.event_data = WebhookEventData {
            format: "json".to_string(),
            include_data: vec![
                "documents".to_string(),
                "attachments".to_string(),
                "custom_fields".to_string(),
            ],
            version: "restv2.1".to_string(),
        };

        // Get all the webhooks to check if we already have one.
        let webhooks = self.list_webhooks().await.unwrap();
        for webhook in webhooks {
            if webhook.name == connect.name {
                return Ok(webhook);
            }
        }

        // Build the request.
        let request = self.request(
            Method::POST,
            &format!("accounts/{}/connect", self.account_id),
            connect,
            None,
        );

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
            StatusCode::CREATED => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                })
            }
        };

        Ok(resp.json().await.unwrap())
    }

    /// Create an envelope.
    pub async fn create_envelope(&self, envelope: Envelope) -> Result<Envelope, APIError> {
        // Build the request.
        let request = self.request(
            Method::POST,
            &format!("accounts/{}/envelopes", self.account_id),
            envelope,
            None,
        );

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
            StatusCode::CREATED => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                })
            }
        };

        Ok(resp.json().await.unwrap())
    }

    /// Get envelope form fields.
    pub async fn get_envelope_form_data(&self, envelope_id: &str) -> Result<Vec<FormDatum>, APIError> {
        // Build the request.
        let request = self.request(
            Method::GET,
            &format!("accounts/{}/envelopes/{}/form_data", self.account_id, envelope_id),
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

        let data: FormData = resp.json().await.unwrap();
        Ok(data.form_data)
    }

    /// Get document fields.
    pub async fn get_document_fields(
        &self,
        envelope_id: &str,
        document_id: &str,
    ) -> Result<Vec<DocumentField>, APIError> {
        // Build the request.
        let request = self.request(
            Method::GET,
            &format!(
                "accounts/{}/envelopes/{}/documents/{}/fields",
                self.account_id, envelope_id, document_id
            ),
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

        Ok(resp.json().await.unwrap())
    }

    /// Get document.
    pub async fn get_document(&self, envelope_id: &str, document_id: &str) -> Result<Bytes, APIError> {
        // Build the request.
        let request = self.request(
            Method::GET,
            &format!(
                "accounts/{}/envelopes/{}/documents/{}",
                self.account_id, envelope_id, document_id
            ),
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

        Ok(resp.bytes().await.unwrap())
    }

    /// Update document fields.
    pub async fn update_document_fields(
        &self,
        envelope_id: &str,
        document_id: &str,
        document_fields: Vec<DocumentField>,
    ) -> Result<(), APIError> {
        // Build the request.
        let request = self.request(
            Method::PUT,
            &format!(
                "accounts/{}/envelopes/{}/documents/{}/fields",
                self.account_id, envelope_id, document_id
            ),
            document_fields,
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

#[derive(Debug, JsonSchema, Clone, Default, Serialize, Deserialize)]
pub struct Envelope {
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "envelopeDocuments")]
    pub documents: Vec<Document>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "createdDateTime")]
    pub created_date_time: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "completedDateTime")]
    pub completed_date_time: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "declinedDateTime")]
    pub declined_date_time: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "deliveredDateTime")]
    pub delivered_date_time: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "transactionId")]
    pub transaction_id: String,
    /// Indicates the envelope status. Valid values are:
    ///
    /// * `completed`: The envelope has been completed and all tags have been signed.
    /// * `created`: The envelope is created as a draft. It can be modified and sent later.
    /// * `declined`: The envelope has been declined by the recipients.
    /// * `delivered`: The envelope has been delivered to the recipients.
    /// * `sent`: The envelope is sent to the recipients.
    /// * `signed`: The envelope has been signed by the recipients.
    /// * `voided`: The envelope is no longer valid and recipients cannot access or sign the envelope.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub status: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "documentsUri")]
    pub documents_uri: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "recipientsUri")]
    pub recipients_uri: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "attachmentsUri")]
    pub attachments_uri: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "envelopeUri")]
    pub envelope_uri: String,
    /// The subject line of the email message that is sent to all recipients.
    ///
    /// For information about adding merge field information to the email subject, see [Template Email Subject Merge Fields](https://developers.docusign.com/esign-rest-api/reference/Templates/Templates/create#template-email-subject-merge-fields).
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "emailSubject")]
    pub email_subject: String,
    /// This is the same as the email body. If specified it is included in the email body for all envelope recipients.
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "emailBlurb")]
    pub email_blurb: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "envelopeId")]
    pub envelope_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "signingLocation")]
    pub signing_location: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "customFieldsUri")]
    pub custom_fields_uri: String,
    #[serde(default, rename = "customFields")]
    pub custom_fields: CustomFields,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "brandLock")]
    pub brand_lock: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "brandId")]
    pub brand_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "useDisclosure")]
    pub use_disclosure: String,
    #[serde(default, rename = "emailSettings")]
    pub email_settings: EmailSettings,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "purgeState")]
    pub purge_state: String,
    #[serde(default, rename = "lockInformation")]
    pub lock_information: LockInformation,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "is21CFRPart11")]
    pub is21_cfr_part11: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "signerCanSignOnMobile")]
    pub signer_can_sign_on_mobile: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "isSignatureProviderEnvelope"
    )]
    pub is_signature_provider_envelope: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "allowViewHistory")]
    pub allow_view_history: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "allowComments")]
    pub allow_comments: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "allowMarkup")]
    pub allow_markup: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "allowReassign")]
    pub allow_reassign: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub asynchronous: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "disableResponsiveDocument"
    )]
    pub disable_responsive_document: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "copyRecipientData")]
    pub copy_recipient_data: String,
    /// The id of the template. If a value is not provided, DocuSign generates a value.
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "templateId")]
    pub template_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "templateRoles")]
    pub template_roles: Vec<TemplateRole>,
    #[serde(default)]
    pub recipients: Recipients,
}

#[derive(Debug, JsonSchema, Clone, Default, Serialize, Deserialize)]
pub struct Document {
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "documentId")]
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// These appear to be base64 encoded.
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "PDFBytes")]
    pub pdf_bytes: String,
}

#[derive(Debug, JsonSchema, Clone, Default, Serialize, Deserialize)]
pub struct Recipients {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agents: Vec<Recipient>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub signers: Vec<Recipient>,
}

#[derive(Debug, JsonSchema, Clone, Default, Serialize, Deserialize)]
pub struct Recipient {
    /// Email of the recipient. Notification will be sent to this email id.
    /// Maximum Length: 100 characters.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email: String,
    /// Full legal name of the recipient.
    /// Maximum Length: 100 characters.
    ///
    /// Note: If you are creating an envelope with DocuSign EU advanced signature enabled, ensure that recipient names do not contain any of the following characters: ^ : \ @ , + <
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Optional element. Specifies the role name associated with the recipient.
    /// This is required when working with template recipients.
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "roleName")]
    pub role_name: String,
    /// Required element with recipient type In Person Signers.
    /// Maximum Length: 100 characters.
    ///
    /// The full legal name of a signer for the envelope.
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "signerName")]
    pub signer_name: String,
    /// Unique for the recipient. It is used by the tab element to indicate which recipient is to sign the Document.
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "recipientId")]
    pub recipient_id: String,
}

#[derive(Debug, JsonSchema, Clone, Default, Serialize, Deserialize)]
pub struct TemplateRole {
    /// Email of the recipient. Notification will be sent to this email id.
    /// Maximum Length: 100 characters.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email: String,
    /// Full legal name of the recipient.
    /// Maximum Length: 100 characters.
    ///
    /// Note: If you are creating an envelope with DocuSign EU advanced signature enabled, ensure that recipient names do not contain any of the following characters: ^ : \ @ , + <
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Optional element. Specifies the role name associated with the recipient.
    /// This is required when working with template recipients.
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "roleName")]
    pub role_name: String,
    /// Required element with recipient type In Person Signers.
    /// Maximum Length: 100 characters.
    ///
    /// The full legal name of a signer for the envelope.
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "signerName")]
    pub signer_name: String,
    /// This specifies the routing order of the recipient in the envelope.
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "routingOrder")]
    pub routing_order: String,
    #[serde(default, rename = "emailNotification")]
    pub email_notification: EmailNotification,
}

#[derive(Debug, JsonSchema, Clone, Default, Serialize, Deserialize)]
pub struct EmailNotification {
    /// The subject line of the email message that is sent to all recipients.
    ///
    /// For information about adding merge field information to the email subject, see [Template Email Subject Merge Fields](https://developers.docusign.com/esign-rest-api/reference/Templates/Templates/create#template-email-subject-merge-fields).
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "emailSubject")]
    pub email_subject: String,
    /// This is the same as the email body. If specified it is included in the email body for all envelope recipients.
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "emailBody")]
    pub email_body: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub language: String,
}

#[derive(Debug, JsonSchema, Clone, Default, Serialize, Deserialize)]
pub struct CustomFields {
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "textCustomFields")]
    pub text_custom_fields: Vec<TextCustomField>,
}

#[derive(Debug, JsonSchema, Clone, Default, Serialize, Deserialize)]
pub struct TextCustomField {
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "fieldId")]
    pub field_id: String,
    pub name: String,
    pub show: String,
    pub required: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "configurationType")]
    pub configuration_type: String,
    #[serde(default, rename = "errorDetails")]
    pub error_details: ErrorDetails,
}

#[derive(Debug, JsonSchema, Clone, Default, Serialize, Deserialize)]
pub struct ErrorDetails {
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "errorCode")]
    pub error_code: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub message: String,
}

#[derive(Debug, JsonSchema, Clone, Default, Serialize, Deserialize)]
pub struct EmailSettings {
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "replyEmailAddressOverride"
    )]
    pub reply_email_address_override: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "replyEmailNameOverride")]
    pub reply_email_name_override: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "bccEmailAddresses")]
    pub bcc_email_addresses: Vec<BccEmailAddress>,
}

#[derive(Debug, JsonSchema, Clone, Default, Serialize, Deserialize)]
pub struct BccEmailAddress {
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "bccEmailAddressId")]
    pub bcc_email_address_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email: String,
}

#[derive(Debug, JsonSchema, Clone, Default, Serialize, Deserialize)]
pub struct LockInformation {
    #[serde(default, rename = "lockedByUser")]
    pub locked_by_user: LockedByUser,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "lockedByApp")]
    pub locked_by_app: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "lockedUntilDateTime")]
    pub locked_until_date_time: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "lockDurationInSeconds")]
    pub lock_duration_in_seconds: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "lockType")]
    pub lock_type: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "useScratchPad")]
    pub use_scratch_pad: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "lockToken")]
    pub lock_token: String,
    #[serde(default, rename = "errorDetails")]
    pub error_details: ErrorDetails,
}

#[derive(Debug, JsonSchema, Clone, Default, Serialize, Deserialize)]
pub struct LockedByUser {}

#[derive(Debug, JsonSchema, Clone, Default, Serialize, Deserialize)]
pub struct TemplatesResponse {
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "envelopeTemplates")]
    pub envelope_templates: Vec<Template>,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "resultSetSize")]
    pub result_set_size: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "startPosition")]
    pub start_position: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "endPosition")]
    pub end_position: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "totalSetSize")]
    pub total_set_size: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "nextUri")]
    pub next_uri: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "previousUri")]
    pub previous_uri: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub folders: Vec<Folder>,
}

#[derive(Debug, JsonSchema, Default, Clone, Serialize, Deserialize)]
pub struct Folder {
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "ownerUserName")]
    pub owner_user_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "ownerEmail")]
    pub owner_email: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "ownerUserId")]
    pub owner_user_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "type")]
    pub folder_type: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub uri: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "parentFolderId")]
    pub parent_folder_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "parentFolderUri")]
    pub parent_folder_uri: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "folderId")]
    pub folder_id: String,
    #[serde(default, rename = "errorDetails")]
    pub error_details: ErrorDetails,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub folders: Vec<LockedByUser>,
    #[serde(default)]
    pub filter: Filter,
}

#[derive(Debug, JsonSchema, Default, Clone, Serialize, Deserialize)]
pub struct Filter {
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "actionRequired")]
    pub action_required: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub expires: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "isTemplate")]
    pub is_template: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub status: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "fromDateTime")]
    pub from_date_time: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "toDateTime")]
    pub to_date_time: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "searchTarget")]
    pub search_target: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "searchText")]
    pub search_text: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "folderIds")]
    pub folder_ids: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "orderBy")]
    pub order_by: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub order: String,
}

#[derive(Debug, JsonSchema, Clone, Default, Serialize, Deserialize)]
pub struct Template {
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "templateId")]
    pub template_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub shared: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub password: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "lastModified")]
    pub last_modified: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub created: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "pageCount")]
    pub page_count: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub uri: String,
}

#[derive(Debug, JsonSchema, Clone, Default, Serialize, Deserialize)]
pub struct DocumentField {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub value: String,
}

#[derive(Debug, JsonSchema, Clone, Default, Serialize, Deserialize)]
pub struct FormData {
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "formData")]
    pub form_data: Vec<FormDatum>,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "envelopeId")]
    pub envelope_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub status: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "sentDateTime")]
    pub sent_date_time: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "recipientFormData")]
    pub recipient_form_data: Vec<RecipientFormDatum>,
}

#[derive(Debug, JsonSchema, Clone, Default, Serialize, Deserialize)]
pub struct FormDatum {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "originalValue")]
    pub original_value: Option<String>,
}

#[derive(Debug, JsonSchema, Clone, Default, Serialize, Deserialize)]
pub struct RecipientFormDatum {
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "formData")]
    pub form_data: Vec<FormDatum>,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "recipientId")]
    pub recipient_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email: String,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "SignedTime")]
    pub signed_time: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "DeliveredTime")]
    pub delivered_time: Option<DateTime<Utc>>,
}

#[derive(Debug, JsonSchema, Clone, Default, Serialize, Deserialize)]
pub struct Webhook {
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "connectId")]
    pub connect_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "configurationType")]
    pub configuration_type: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "urlToPublishTo")]
    pub url_to_publish_to: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "allowEnvelopePublish")]
    pub allow_envelope_publish: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "enableLog")]
    pub enable_log: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "includeDocuments")]
    pub include_documents: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "includeCertificateOfCompletion"
    )]
    pub include_certificate_of_completion: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "requiresAcknowledgement"
    )]
    pub requires_acknowledgement: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "signMessageWithX509Certificate"
    )]
    pub sign_message_with_x509_certificate: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "useSoapInterface")]
    pub use_soap_interface: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "includeTimeZoneInformation"
    )]
    pub include_time_zone_information: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "includeHMAC")]
    pub include_hmac: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "includeEnvelopeVoidReason"
    )]
    pub include_envelope_void_reason: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "includeSenderAccountasCustomField"
    )]
    pub include_sender_accountas_custom_field: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "envelopeEvents")]
    pub envelope_events: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "recipientEvents")]
    pub recipient_events: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "userIds")]
    pub user_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "soapNamespace")]
    pub soap_namespace: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "allUsers")]
    pub all_users: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "includeCertSoapHeader")]
    pub include_cert_soap_header: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "includeDocumentFields")]
    pub include_document_fields: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "salesforceAPIVersion")]
    pub salesforce_api_version: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "salesforceDocumentsAsContentFiles"
    )]
    pub salesforce_documents_as_content_files: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "salesforceAuthcode")]
    pub salesforce_auth_code: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "salesforceCallBackUrl")]
    pub salesforce_callback_url: String,
    #[serde(default, rename = "eventData")]
    pub event_data: WebhookEventData,
}

#[derive(Debug, JsonSchema, Clone, Default, Serialize, Deserialize)]
pub struct WebhookEventData {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub format: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "includeData")]
    pub include_data: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
}

#[derive(Debug, JsonSchema, Clone, Default, Serialize, Deserialize)]
pub struct WebhooksResponse {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub configurations: Vec<Webhook>,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "totalRecords")]
    pub total_records: String,
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
pub struct UserInfo {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sub: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub given_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub family_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub created: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accounts: Vec<Account>,
}

#[derive(Debug, JsonSchema, Clone, Default, Serialize, Deserialize)]
pub struct Account {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub account_id: String,
    #[serde(default)]
    pub is_default: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub account_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub base_uri: String,
}
