use anyhow::{anyhow, Result};
use checkr::WebhookEvent as CheckrWebhook;
use docusign::Envelope;
use dropshot::{
    endpoint, ApiDescription, ConfigDropshot, ConfigLogging, ConfigLoggingLevel, HttpError, HttpResponseAccepted,
    HttpServer, HttpServerStarter, RequestContext,
};
use dropshot_auth::{
    bearer::{Bearer, BearerAudit},
    query::{QueryToken, QueryTokenAudit},
    sig::{HmacVerifiedBody, HmacVerifiedBodyAudit},
};
use slack_chat_api::BotCommand;
use std::sync::Arc;

use webhooky::{auth::GlobalToken, github_types::GitHubWebhook, handlers_slack::InteractiveEvent};

#[endpoint {
    method = POST,
    path = "/hmac/github/verify",
}]
async fn hmac_github_verification(
    _rqctx: Arc<RequestContext<()>>,
    body: HmacVerifiedBody<webhooky::handlers_github::GitHubWebhookVerification, GitHubWebhook>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    body.into_inner()?;
    Ok(HttpResponseAccepted("ok".to_string()))
}

#[endpoint {
    method = POST,
    path = "/hmac/github/audit",
}]
async fn hmac_github_audit(
    _rqctx: Arc<RequestContext<()>>,
    body: HmacVerifiedBodyAudit<webhooky::handlers_github::GitHubWebhookVerification, GitHubWebhook>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    body.into_inner()?;
    Ok(HttpResponseAccepted("ok".to_string()))
}

#[endpoint {
    method = POST,
    path = "/hmac/checkr/verify",
}]
async fn hmac_checkr_verification(
    _rqctx: Arc<RequestContext<()>>,
    body: HmacVerifiedBody<webhooky::handlers_checkr::CheckrWebhookVerification, CheckrWebhook>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    body.into_inner()?;
    Ok(HttpResponseAccepted("ok".to_string()))
}

#[endpoint {
    method = POST,
    path = "/hmac/checkr/audit",
}]
async fn hmac_checkr_audit(
    _rqctx: Arc<RequestContext<()>>,
    body: HmacVerifiedBodyAudit<webhooky::handlers_checkr::CheckrWebhookVerification, CheckrWebhook>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    body.into_inner()?;
    Ok(HttpResponseAccepted("ok".to_string()))
}

#[endpoint {
    method = POST,
    path = "/hmac/docusign/verify",
}]
async fn hmac_docusign_verification(
    _rqctx: Arc<RequestContext<()>>,
    body: HmacVerifiedBody<webhooky::handlers_docusign::DocusignWebhookVerification, Envelope>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    body.into_inner()?;
    Ok(HttpResponseAccepted("ok".to_string()))
}

#[endpoint {
    method = POST,
    path = "/hmac/docusign/audit",
}]
async fn hmac_docusign_audit(
    _rqctx: Arc<RequestContext<()>>,
    body: HmacVerifiedBodyAudit<webhooky::handlers_docusign::DocusignWebhookVerification, Envelope>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    body.into_inner()?;
    Ok(HttpResponseAccepted("ok".to_string()))
}

#[endpoint {
    method = POST,
    path = "/hmac/slack/verify",
    content_type = "application/x-www-form-urlencoded"
}]
async fn hmac_slack_verification(
    _rqctx: Arc<RequestContext<()>>,
    body: HmacVerifiedBody<webhooky::handlers_slack::SlackWebhookVerification, BotCommand>,
) -> Result<HttpResponseAccepted<BotCommand>, HttpError> {
    Ok(HttpResponseAccepted(body.into_inner()?))
}

#[endpoint {
    method = POST,
    path = "/hmac/slack/audit",
    content_type = "application/x-www-form-urlencoded"
}]
async fn hmac_slack_audit(
    _rqctx: Arc<RequestContext<()>>,
    body: HmacVerifiedBodyAudit<webhooky::handlers_slack::SlackWebhookVerification, BotCommand>,
) -> Result<HttpResponseAccepted<BotCommand>, HttpError> {
    Ok(HttpResponseAccepted(body.into_inner()?))
}

#[endpoint {
    method = POST,
    path = "/hmac/slack/interactive/verify",
    content_type = "application/x-www-form-urlencoded"
}]
async fn hmac_slack_interactive_verification(
    _rqctx: Arc<RequestContext<()>>,
    body: HmacVerifiedBody<webhooky::handlers_slack::SlackWebhookVerification, InteractiveEvent>,
) -> Result<HttpResponseAccepted<slack_chat_api::InteractivePayload>, HttpError> {
    let event = body.into_inner()?;
    let ser_payload = urlencoding::decode(&event.payload).unwrap();
    let payload: slack_chat_api::InteractivePayload = serde_json::from_str(&ser_payload).unwrap();
    Ok(HttpResponseAccepted(payload))
}

#[endpoint {
    method = POST,
    path = "/hmac/slack/interactive/audit",
    content_type = "application/x-www-form-urlencoded"
}]
async fn hmac_slack_interactive_audit(
    _rqctx: Arc<RequestContext<()>>,
    body: HmacVerifiedBodyAudit<webhooky::handlers_slack::SlackWebhookVerification, InteractiveEvent>,
) -> Result<HttpResponseAccepted<slack_chat_api::InteractivePayload>, HttpError> {
    let event = body.into_inner()?;
    let ser_payload = urlencoding::decode(&event.payload).unwrap();
    let payload: slack_chat_api::InteractivePayload = serde_json::from_str(&ser_payload).unwrap();
    Ok(HttpResponseAccepted(payload))
}

#[endpoint {
    method = POST,
    path = "/bearer/verify",
}]
async fn bearer_verification(
    _rqctx: Arc<RequestContext<()>>,
    _: Bearer<GlobalToken>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    Ok(HttpResponseAccepted("ok".to_string()))
}

#[endpoint {
    method = POST,
    path = "/bearer/audit",
}]
async fn bearer_audit(
    _rqctx: Arc<RequestContext<()>>,
    _: BearerAudit<GlobalToken>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    Ok(HttpResponseAccepted("ok".to_string()))
}

#[endpoint {
    method = POST,
    path = "/token/verify",
}]
async fn token_verification(
    _rqctx: Arc<RequestContext<()>>,
    _: QueryToken<GlobalToken>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    Ok(HttpResponseAccepted("ok".to_string()))
}

#[endpoint {
    method = POST,
    path = "/token/audit",
}]
async fn token_audit(
    _rqctx: Arc<RequestContext<()>>,
    _: QueryTokenAudit<GlobalToken>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    Ok(HttpResponseAccepted("ok".to_string()))
}

static INIT: std::sync::Once = std::sync::Once::new();

/// Setup function that is only run once, even if called multiple times.
fn setup_logger() {
    INIT.call_once(|| {
        pretty_env_logger::init();
    });
}

fn make_server() -> (u16, HttpServer<()>) {
    setup_logger();

    // Configure fake test keys for checking implementations
    std::env::set_var("GLOBAL_AUTH_BEARER", "TEST_BEARER");
    std::env::set_var("DOCUSIGN_WH_KEY", "vkPkH4G2k8XNC5HWA6QgZd08v37P8KcVZMjaP4zgGWc=");
    std::env::set_var("GH_WH_KEY", "vkPkH4G2k8XNC5HWA6QgZd08v37P8KcVZMjaP4zgGWc=");
    std::env::set_var("SLACK_WH_KEY", "vkPkH4G2k8XNC5HWA6QgZd08v37P8KcVZMjaP4zgGWc=");

    let config_dropshot = ConfigDropshot {
        bind_address: "127.0.0.1:0".parse().unwrap(),
        request_body_max_bytes: 107374182400, // 100 Gigiabytes.
        tls: None,
    };
    let config_logging = ConfigLogging::StderrTerminal {
        level: ConfigLoggingLevel::Error,
    };
    let log = config_logging.to_logger("webhooky-server").unwrap();

    let mut api = ApiDescription::new();
    api.register(hmac_github_verification).unwrap();
    api.register(hmac_github_audit).unwrap();
    api.register(hmac_checkr_verification).unwrap();
    api.register(hmac_checkr_audit).unwrap();
    api.register(hmac_docusign_verification).unwrap();
    api.register(hmac_docusign_audit).unwrap();
    api.register(hmac_slack_verification).unwrap();
    api.register(hmac_slack_audit).unwrap();
    api.register(hmac_slack_interactive_verification).unwrap();
    api.register(hmac_slack_interactive_audit).unwrap();
    api.register(bearer_verification).unwrap();
    api.register(bearer_audit).unwrap();
    api.register(token_verification).unwrap();
    api.register(token_audit).unwrap();

    let api_context = ();
    let server = HttpServerStarter::new(&config_dropshot, api, api_context.clone(), &log)
        .map_err(|error| anyhow!("failed to create server: {}", error))
        .unwrap()
        .start();

    (server.local_addr().port(), server)
}

/// Test GitHub signatures

#[tokio::test]
async fn test_github_hmac_passes() {
    let (port, _server) = make_server();

    let test_signature = "sha256=8c7ac6b6a1ca30229207b4406d50b5c034d90f56009835bc7f32a16b2044d29d";
    let test_body = include_str!("../tests/github_webhook_sig_test.json").trim();

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{}/hmac/github/verify", port))
        .header("X-Hub-Signature-256", test_signature)
        .body(test_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::ACCEPTED);
}

#[tokio::test]
async fn test_github_hmac_fails() {
    let (port, _server) = make_server();

    let test_signature = "sha256=8c7ac6b6a1ca30229207b4406d50b5c034d90f56009835bc7f32a16b2044d29c";
    let test_body = include_str!("../tests/github_webhook_sig_test.json").trim();

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{}/hmac/github/verify", port))
        .header("X-Hub-Signature-256", test_signature)
        .body(test_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_github_hmac_audit_passes_with_invalid_signature() {
    let (port, _server) = make_server();

    let test_signature = "sha256=8c7ac6b6a1ca30229207b4406d50b5c034d90f56009835bc7f32a16b2044d29c";
    let test_body = include_str!("../tests/github_webhook_sig_test.json").trim();

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{}/hmac/github/audit", port))
        .header("X-Hub-Signature-256", test_signature)
        .body(test_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::ACCEPTED);
}

/// Test Checkr signatures

#[ignore]
#[tokio::test]
async fn test_checkr_hmac_passes() {
    let (port, _server) = make_server();

    let test_signature = "66781e800f7d2934506890f5546af7736f1f84c46be507a7042f0be4e92259a0";
    let test_body = include_str!("../tests/checkr_webhook_sig_test.json").trim();

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{}/hmac/checkr/verify", port))
        .header("X-Checkr-Signature", test_signature)
        .body(test_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::ACCEPTED);
}

#[ignore]
#[tokio::test]
async fn test_checkr_hmac_fails() {
    let (port, _server) = make_server();

    let test_signature = "66781e800f7d2934506890f5546af7736f1f84c46be507a7042f0be4e92259b0";
    let test_body = include_str!("../tests/checkr_webhook_sig_test.json").trim();

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{}/hmac/checkr/verify", port))
        .header("X-Checkr-Signature", test_signature)
        .body(test_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
}

#[ignore]
#[tokio::test]
async fn test_checkr_hmac_audit_passes_with_invalid_signature() {
    let (port, _server) = make_server();

    let test_signature = "66781e800f7d2934506890f5546af7736f1f84c46be507a7042f0be4e92259b0";
    let test_body = include_str!("../tests/checkr_webhook_sig_test.json").trim();

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{}/hmac/checkr/audit", port))
        .header("X-Checkr-Signature", test_signature)
        .body(test_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::ACCEPTED);
}

/// Test DocuSign signatures

#[tokio::test]
async fn test_docusign_hmac_passes() {
    let (port, _server) = make_server();

    let test_signature = "048a1564644f631795724ec078399d672b09a254b3adaf84e4b20100e0564216";
    let test_body = include_str!("../tests/docusign_webhook_sig_test.json").trim();

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{}/hmac/docusign/verify", port))
        .header("X-DocuSign-Signature-1", test_signature)
        .body(test_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::ACCEPTED);
}

#[tokio::test]
async fn test_docusign_hmac_fails() {
    let (port, _server) = make_server();

    let test_signature = "048a1564644f631795724ec078399d672b09a254b3adaf84e4b20100e0564217";
    let test_body = include_str!("../tests/docusign_webhook_sig_test.json").trim();

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{}/hmac/docusign/verify", port))
        .header("X-DocuSign-Signature-1", test_signature)
        .body(test_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_docusign_hmac_audit_passes_with_invalid_signature() {
    let (port, _server) = make_server();

    let test_signature = "048a1564644f631795724ec078399d672b09a254b3adaf84e4b20100e0564216";
    let test_body = include_str!("../tests/docusign_webhook_sig_test.json").trim();

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{}/hmac/docusign/audit", port))
        .header("X-DocuSign-Signature-1", test_signature)
        .body(test_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::ACCEPTED);
}

/// Test Slack signatures

#[tokio::test]
async fn test_slack_hmac_passes() {
    let (port, _server) = make_server();

    let test_signature = "v0=a421125f1e5572b0f7b2116a1df1f5083fc5eb742d4f54ccb19b8c986bd0bc74";
    let test_timestamp = "1531420618";
    let test_body = include_str!("../tests/slack_command_webhook_sig_test.txt").trim();

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{}/hmac/slack/verify", port))
        .header("X-Slack-Signature", test_signature)
        .header("X-Slack-Request-Timestamp", test_timestamp)
        .body(test_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::ACCEPTED);

    let body: BotCommand = response.json().await.unwrap();

    assert_eq!("test", body.user_name.as_str());
    assert_eq!("fakecommand", body.command.as_str());
}

#[tokio::test]
async fn test_slack_hmac_fails() {
    let (port, _server) = make_server();

    let test_signature = "v0=a421125f1e5572b0f7b2116a1df1f5083fc5eb742d4f54ccb19b8c986bd0bc75";
    let test_timestamp = "1531420618";
    let test_body = include_str!("../tests/slack_command_webhook_sig_test.txt").trim();

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{}/hmac/slack/verify", port))
        .header("X-Slack-Signature", test_signature)
        .header("X-Slack-Request-Timestamp", test_timestamp)
        .body(test_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_slack_hmac_audit_passes_with_invalid_signature() {
    let (port, _server) = make_server();

    let test_signature = "v0=a421125f1e5572b0f7b2116a1df1f5083fc5eb742d4f54ccb19b8c986bd0bc75";
    let test_timestamp = "1531420618";
    let test_body = include_str!("../tests/slack_command_webhook_sig_test.txt").trim();

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{}/hmac/slack/audit", port))
        .header("X-Slack-Signature", test_signature)
        .header("X-Slack-Request-Timestamp", test_timestamp)
        .body(test_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::ACCEPTED);

    let body: BotCommand = response.json().await.unwrap();

    assert_eq!("test", body.user_name.as_str());
    assert_eq!("fakecommand", body.command.as_str());
}

#[tokio::test]
async fn test_slack_interactive_hmac_passes() {
    let (port, _server) = make_server();

    let test_signature = "v0=f52abb78d323d299c4cc76c8809d18ede86b325761afdf8daa13f7a23f30a538";
    let test_timestamp = "1531420618";
    let test_body = include_str!("../tests/slack_interactive_webhook_sig_test.txt").trim();

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{}/hmac/slack/interactive/verify", port))
        .header("X-Slack-Signature", test_signature)
        .header("X-Slack-Request-Timestamp", test_timestamp)
        .body(test_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::ACCEPTED);

    let body: slack_chat_api::InteractivePayload = response.json().await.unwrap();

    assert_eq!("test_type", body.interactive_slack_payload_type.as_str());
    assert_eq!("test", body.api_app_id.as_str());
}

#[tokio::test]
async fn test_slack_interactive_hmac_fails() {
    let (port, _server) = make_server();

    let test_signature = "v0=f52abb78d323d299c4cc76c8809d18ede86b325761afdf8daa13f7a23f30a539";
    let test_timestamp = "1531420618";
    let test_body = include_str!("../tests/slack_interactive_webhook_sig_test.txt").trim();

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{}/hmac/slack/interactive/verify", port))
        .header("X-Slack-Signature", test_signature)
        .header("X-Slack-Request-Timestamp", test_timestamp)
        .body(test_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_slack_interactive_hmac_audit_passes_with_invalid_signature() {
    let (port, _server) = make_server();

    let test_signature = "v0=f52abb78d323d299c4cc76c8809d18ede86b325761afdf8daa13f7a23f30a539";
    let test_timestamp = "1531420618";
    let test_body = include_str!("../tests/slack_interactive_webhook_sig_test.txt").trim();

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{}/hmac/slack/interactive/audit", port))
        .header("X-Slack-Signature", test_signature)
        .header("X-Slack-Request-Timestamp", test_timestamp)
        .body(test_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::ACCEPTED);

    let body: slack_chat_api::InteractivePayload = response.json().await.unwrap();

    assert_eq!("test_type", body.interactive_slack_payload_type.as_str());
    assert_eq!("test", body.api_app_id.as_str());
}

/// Test global Bearer token

#[tokio::test]
async fn test_bearer_passes() {
    let (port, _server) = make_server();

    let test_token = "TEST_BEARER";
    let test_body = "";

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{}/bearer/verify", port))
        .header("Authorization", &format!("Bearer {}", test_token))
        .body(test_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::ACCEPTED);
}

#[tokio::test]
async fn test_bearer_fails() {
    let (port, _server) = make_server();

    let test_token = "TEST_BEARER_2";
    let test_body = "";

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{}/bearer/verify", port))
        .header("Authorization", &format!("Bearer {}", test_token))
        .body(test_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_bearer_audit_pass_with_invalid_token() {
    let (port, _server) = make_server();

    let test_token = "TEST_BEARER_2";
    let test_body = "";

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{}/bearer/audit", port))
        .header("Authorization", &format!("Bearer {}", test_token))
        .body(test_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::ACCEPTED);
}

/// Test global Query token

#[tokio::test]
async fn test_query_token_passes() {
    let (port, _server) = make_server();

    let test_token = "TEST_BEARER";
    let test_body = "";

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{}/token/verify?token={}", port, test_token))
        .body(test_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::ACCEPTED);
}

#[tokio::test]
async fn test_query_token_fails() {
    let (port, _server) = make_server();

    let test_token = "TEST_BEARER_2";
    let test_body = "";

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{}/token/verify?token={}", port, test_token))
        .body(test_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_query_token_audit_pass_with_invalid_token() {
    let (port, _server) = make_server();

    let test_token = "TEST_BEARER_2";
    let test_body = "";

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{}/token/audit?token={}", port, test_token))
        .body(test_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::ACCEPTED);
}
