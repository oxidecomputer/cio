use anyhow::{anyhow, Result};
use dropshot::{HttpServerStarter, HttpServer, ConfigLogging, ConfigLoggingLevel, HttpError, HttpResponseAccepted, RequestContext, ApiDescription, ConfigDropshot, endpoint};
use slog::Drain;
use std::sync::Arc;

use webhooky::auth::GlobalToken;
use webhooky::bearer::{Bearer, BearerAudit};
use webhooky::sig::{HmacVerifiedBody, HmacVerifiedBodyAudit};
use webhooky::token::{Token, TokenAudit};

#[endpoint {
    method = POST,
    path = "/hmac/github/verify",
}]
async fn hmac_github_verification(
    rqctx: Arc<RequestContext<()>>,
    body: HmacVerifiedBody<webhooky::handlers_github::GitHubWebhookVerification>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    Ok(HttpResponseAccepted("ok".to_string()))
}

#[endpoint {
    method = POST,
    path = "/hmac/github/audit",
}]
async fn hmac_github_audit(
    rqctx: Arc<RequestContext<()>>,
    body: HmacVerifiedBodyAudit<webhooky::handlers_github::GitHubWebhookVerification>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    Ok(HttpResponseAccepted("ok".to_string()))
}

#[endpoint {
    method = POST,
    path = "/hmac/checkr/verify",
}]
async fn hmac_checkr_verification(
    rqctx: Arc<RequestContext<()>>,
    body: HmacVerifiedBody<webhooky::handlers_checkr::CheckrWebhookVerification>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    Ok(HttpResponseAccepted("ok".to_string()))
}

#[endpoint {
    method = POST,
    path = "/hmac/checkr/audit",
}]
async fn hmac_checkr_audit(
    rqctx: Arc<RequestContext<()>>,
    body: HmacVerifiedBodyAudit<webhooky::handlers_checkr::CheckrWebhookVerification>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    Ok(HttpResponseAccepted("ok".to_string()))
}

#[endpoint {
    method = POST,
    path = "/hmac/docusign/verify",
}]
async fn hmac_docusign_verification(
    rqctx: Arc<RequestContext<()>>,
    body: HmacVerifiedBody<webhooky::handlers_docusign::DocusignWebhookVerification>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    Ok(HttpResponseAccepted("ok".to_string()))
}

#[endpoint {
    method = POST,
    path = "/hmac/docusign/audit",
}]
async fn hmac_docusign_audit(
    rqctx: Arc<RequestContext<()>>,
    body: HmacVerifiedBodyAudit<webhooky::handlers_docusign::DocusignWebhookVerification>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    Ok(HttpResponseAccepted("ok".to_string()))
}

#[endpoint {
    method = POST,
    path = "/bearer/verify",
}]
async fn bearer_verification(
    rqctx: Arc<RequestContext<()>>,
    _: Bearer<GlobalToken>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    Ok(HttpResponseAccepted("ok".to_string()))
}

#[endpoint {
    method = POST,
    path = "/bearer/audit",
}]
async fn bearer_audit(
    rqctx: Arc<RequestContext<()>>,
    _: BearerAudit<GlobalToken>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    Ok(HttpResponseAccepted("ok".to_string()))
}

#[endpoint {
    method = POST,
    path = "/token/verify",
}]
async fn token_verification(
    rqctx: Arc<RequestContext<()>>,
    _: Token<GlobalToken>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    Ok(HttpResponseAccepted("ok".to_string()))
}

#[endpoint {
    method = POST,
    path = "/token/audit",
}]
async fn token_audit(
    rqctx: Arc<RequestContext<()>>,
    _: TokenAudit<GlobalToken>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    Ok(HttpResponseAccepted("ok".to_string()))
}

async fn make_server() -> anyhow::Result<HttpServer<()>> {
    std::env::set_var("GITHUB_WH_KEY", "vkPkH4G2k8XNC5HWA6QgZd08v37P8KcVZMjaP4zgGWc=");
    std::env::set_var("DOCUSIGN_WH_KEY", "vkPkH4G2k8XNC5HWA6QgZd08v37P8KcVZMjaP4zgGWc=");

    let config_dropshot = ConfigDropshot {
        bind_address: "127.0.0.1:12345".parse()?,
        request_body_max_bytes: 107374182400, // 100 Gigiabytes.
        tls: None,
    };
    let config_logging = ConfigLogging::StderrTerminal { level: ConfigLoggingLevel::Error };
    let log = config_logging.to_logger("webhooky-server").unwrap();

    let mut api = ApiDescription::new();
    api.register(hmac_github_verification).unwrap();
    api.register(hmac_github_audit).unwrap();
    api.register(hmac_checkr_verification).unwrap();
    api.register(hmac_checkr_audit).unwrap();
    api.register(hmac_docusign_verification).unwrap();
    api.register(hmac_docusign_audit).unwrap();
    api.register(bearer_verification).unwrap();
    api.register(bearer_audit).unwrap();
    api.register(token_verification).unwrap();
    api.register(token_audit).unwrap();

    let api_context = ();
    let server = HttpServerStarter::new(&config_dropshot, api, api_context.clone(), &log)
        .map_err(|error| anyhow!("failed to create server: {}", error))?
        .start();

    Ok(server)
}

/// Test GitHub signatures

// #[ignore]
#[tokio::test]
async fn test_github_hmac_passes() {
    pretty_env_logger::init();

    let server = make_server().await.unwrap();

    let test_signature = "sha256=318376db08607eb984726533b1d53430e31c4825fd0d9b14e8ed38e2a88ada19";
    let test_body = include_str!("../tests/github_webhook_sig_test.json").trim();

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post("http://127.0.0.1:12345/hmac/github/verify")
        .header("X-Hub-Signature-256", test_signature)
        .body(test_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::ACCEPTED);

    server.close().await;
}

#[ignore]
#[tokio::test]
async fn test_github_hmac_fails() {
    let server = make_server().await.unwrap();

    let test_signature = "sha256=318376db08607eb984726533b1d53430e31c4825fd0d9b14e8ed38e2a88ada18";
    let test_body = include_str!("../tests/github_webhook_sig_test.json").trim();

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post("http://127.0.0.1:12345/hmac/github/verify")
        .header("X-Hub-Signature-256", test_signature)
        .body(test_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);

    server.close().await;
}

#[ignore]
#[tokio::test]
async fn test_github_hmac_audit_passes_with_invalid_signature() {
    let server = make_server().await.unwrap();

    let test_signature = "sha256=318376db08607eb984726533b1d53430e31c4825fd0d9b14e8ed38e2a88ada18";
    let test_body = include_str!("../tests/github_webhook_sig_test.json").trim();

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post("http://127.0.0.1:12345/hmac/github/audit")
        .header("X-Hub-Signature-256", test_signature)
        .body(test_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::ACCEPTED);

    server.close().await;
}

/// Test Checkr signatures

#[ignore]
#[tokio::test]
async fn test_checkr_hmac_passes() {
    let server = make_server().await.unwrap();

    let test_signature = "sha256=318376db08607eb984726533b1d53430e31c4825fd0d9b14e8ed38e2a88ada19";
    let test_body = include_str!("../tests/checkr_webhook_sig_test.json").trim();

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post("http://127.0.0.1:12345/hmac/checkr/verify")
        .header("X-Checkr-Signature", test_signature)
        .body(test_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::ACCEPTED);

    server.close().await;
}

#[ignore]
#[tokio::test]
async fn test_checkr_hmac_fails() {
    let server = make_server().await.unwrap();

    let test_signature = "sha256=318376db08607eb984726533b1d53430e31c4825fd0d9b14e8ed38e2a88ada18";
    let test_body = include_str!("../tests/checkr_webhook_sig_test.json").trim();

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post("http://127.0.0.1:12345/hmac/checkr/verify")
        .header("X-Checkr-Signature", test_signature)
        .body(test_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);

    server.close().await;
}

#[tokio::test]
async fn test_checkr_hmac_audit_passes_with_invalid_signature() {
    let server = make_server().await.unwrap();

    let test_signature = "sha256=318376db08607eb984726533b1d53430e31c4825fd0d9b14e8ed38e2a88ada18";
    let test_body = include_str!("../tests/checkr_webhook_sig_test.json").trim();

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post("http://127.0.0.1:12345/hmac/checkr/audit")
        .header("X-Checkr-Signature", test_signature)
        .body(test_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::ACCEPTED);

    server.close().await;
}

/// Test Docusign signatures

#[ignore]
#[tokio::test]
async fn test_docusign_hmac_passes() {
    let server = make_server().await.unwrap();

    let test_signature = "sha256=318376db08607eb984726533b1d53430e31c4825fd0d9b14e8ed38e2a88ada19";
    let test_body = include_str!("../tests/docusign_webhook_sig_test.json").trim();

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post("http://127.0.0.1:12345/hmac/docusign/verify")
        .header("X-DocuSign-Signature-1", test_signature)
        .body(test_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::ACCEPTED);

    server.close().await;
}

#[ignore]
#[tokio::test]
async fn test_docusign_hmac_fails() {
    let server = make_server().await.unwrap();

    let test_signature = "sha256=318376db08607eb984726533b1d53430e31c4825fd0d9b14e8ed38e2a88ada18";
    let test_body = include_str!("../tests/docusign_webhook_sig_test.json").trim();

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post("http://127.0.0.1:12345/hmac/docusign/verify")
        .header("X-DocuSign-Signature-1", test_signature)
        .body(test_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);

    server.close().await;
}

#[tokio::test]
async fn test_docusign_hmac_audit_passes_with_invalid_signature() {
    let server = make_server().await.unwrap();

    let test_signature = "sha256=318376db08607eb984726533b1d53430e31c4825fd0d9b14e8ed38e2a88ada18";
    let test_body = include_str!("../tests/docusign_webhook_sig_test.json").trim();

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post("http://127.0.0.1:12345/hmac/docusign/audit")
        .header("X-DocuSign-Signature-1", test_signature)
        .body(test_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::ACCEPTED);

    server.close().await;
}