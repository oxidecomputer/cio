/*!
 * A rust library for interacting with the DocuSign API.
 *
 * For more information, you can check out their documentation at:
 * https://developers.docusign.com/docs/esign-rest-api/reference/
 *
 * Example:
 *
 * ```
 * use docusign::DocuSign;
 * use serde::{Deserialize, Serialize};
 *
 * async fn geocode() {
 *     // Initialize the DocuSign client.
 *     let docusign = DocuSign::new_from_env();
 *
 *     let envelope = docusign.get_envelope("some-envelope-id").await.unwrap();
 *
 *     println!("{}", envelope);
 * }
 * ```
 */
#![allow(clippy::field_reassign_with_default)]
use std::env;
use std::error;
use std::fmt;
use std::sync::Arc;

use chrono::offset::Utc;
use chrono::DateTime;
use reqwest::multipart::Form;
use reqwest::{header, Client, Method, Request, StatusCode, Url};
use serde::{Deserialize, Serialize};

/// Endpoint for the DocuSign API.
const ENDPOINT: &str = "https://n2.docusign.net/restapi/v2.1/";

/// Entrypoint for interacting with the DocuSign API.
pub struct DocuSign {
    account_id: String,
    key: String,

    client: Arc<Client>,
}

impl DocuSign {
    /// Create a new DocuSign client struct. It takes a type that can convert into
    /// an &str (`String` or `Vec<u8>` for example). As long as the function is
    /// given a valid API key your requests will work.
    pub fn new<I, K>(account_id: I, key: K) -> Self
    where
        I: ToString,
        K: ToString,
    {
        let client = Client::builder().build();
        match client {
            Ok(c) => Self {
                account_id: account_id.to_string(),
                key: key.to_string(),

                client: Arc::new(c),
            },
            Err(e) => panic!("creating client failed: {:?}", e),
        }
    }

    /// Create a new DocuSign client struct from environment variables. It
    /// takes a type that can convert into
    /// an &str (`String` or `Vec<u8>` for example). As long as the function is
    /// given a valid API key and your requests will work.
    pub fn new_from_env() -> Self {
        let account_id = env::var("DOCUSIGN_ACCOUNT_ID").unwrap();
        let key = env::var("DOCUSIGN_API_KEY").unwrap();

        DocuSign::new(account_id, key)
    }

    fn request(&self, method: Method, path: &str, form: Option<Form>, query: Option<Vec<(&str, String)>>) -> Request {
        let base = Url::parse(ENDPOINT).unwrap();
        let url = base.join(path).unwrap();

        // Set the default headers.
        let mut headers = header::HeaderMap::new();
        if method != Method::POST {
            headers.append(header::CONTENT_TYPE, header::HeaderValue::from_static("application/json"));
        }
        if path.ends_with("/transcript") {
            // Get the plain text transcript
            headers.append(header::ACCEPT, header::HeaderValue::from_static("text/plain"));
        } else {
            headers.append(header::ACCEPT, header::HeaderValue::from_static("application/json"));
        }

        let mut rb = self.client.request(method, url).headers(headers).bearer_auth(&self.key);

        match query {
            None => (),
            Some(val) => {
                rb = rb.query(&val);
            }
        }

        // Add the body, this is to ensure our GET and DELETE calls succeed.
        if let Some(f) = form {
            rb = rb.multipart(f);
        }

        // Build the request.
        rb.build().unwrap()
    }

    /// Get an envelope.
    pub async fn get_envelope(&self, envelope_id: &str) -> Result<Envelope, APIError> {
        // Build the request.
        let request = self.request(Method::GET, &format!("accounts/{}/envelopes/{}", self.account_id, envelope_id), None, None);

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

    /// Create an envelope.
    pub async fn create_envelope(&self, envelope: Envelope) -> Result<Envelope, APIError> {
        // Build the request.
        let request = self.request(Method::POST, &format!("accounts/{}/envelopes", self.account_id), envelope, None);

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
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
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "isSignatureProviderEnvelope")]
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
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "disableResponsiveDocument")]
    pub disable_responsive_document: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "copyRecipientData")]
    pub copy_recipient_data: String,
    /// The id of the template. If a value is not provided, DocuSign generates a value.
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "templateId")]
    pub template_id: String,
    #[serde(default)]
    pub recipients: Recipients,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Recipients {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agents: Vec<Recipient>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CustomFields {
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "textCustomFields")]
    pub text_custom_fields: Vec<TextCustomField>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ErrorDetails {
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "errorCode")]
    pub error_code: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EmailSettings {
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "replyEmailAddressOverride")]
    pub reply_email_address_override: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "replyEmailNameOverride")]
    pub reply_email_name_override: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "bccEmailAddresses")]
    pub bcc_email_addresses: Vec<BccEmailAddress>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BccEmailAddress {
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "bccEmailAddressId")]
    pub bcc_email_address_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LockedByUser {}
