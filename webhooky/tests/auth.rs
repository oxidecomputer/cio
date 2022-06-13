use anyhow::{anyhow, Result};
use dropshot::{
    endpoint, ApiDescription, ConfigDropshot, ConfigLogging, ConfigLoggingLevel, HttpError, HttpResponseAccepted,
    HttpServer, HttpServerStarter, RequestContext,
};
use std::sync::Arc;

use webhooky::auth::{
    bearer::{Bearer, BearerAudit},
    global::GlobalToken,
    sig::{HmacVerifiedBody, HmacVerifiedBodyAudit},
    token::{QueryToken, QueryTokenAudit},
};

#[endpoint {
    method = POST,
    path = "/hmac/github/verify",
}]
async fn hmac_github_verification(
    _rqctx: Arc<RequestContext<()>>,
    _body: HmacVerifiedBody<webhooky::handlers_github::GitHubWebhookVerification>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    Ok(HttpResponseAccepted("ok".to_string()))
}

#[endpoint {
    method = POST,
    path = "/hmac/github/audit",
}]
async fn hmac_github_audit(
    _rqctx: Arc<RequestContext<()>>,
    _body: HmacVerifiedBodyAudit<webhooky::handlers_github::GitHubWebhookVerification>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    Ok(HttpResponseAccepted("ok".to_string()))
}

#[endpoint {
    method = POST,
    path = "/hmac/checkr/verify",
}]
async fn hmac_checkr_verification(
    _rqctx: Arc<RequestContext<()>>,
    _body: HmacVerifiedBody<webhooky::handlers_checkr::CheckrWebhookVerification>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    Ok(HttpResponseAccepted("ok".to_string()))
}

#[endpoint {
    method = POST,
    path = "/hmac/checkr/audit",
}]
async fn hmac_checkr_audit(
    _rqctx: Arc<RequestContext<()>>,
    _body: HmacVerifiedBodyAudit<webhooky::handlers_checkr::CheckrWebhookVerification>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    Ok(HttpResponseAccepted("ok".to_string()))
}

#[endpoint {
    method = POST,
    path = "/hmac/docusign/verify",
}]
async fn hmac_docusign_verification(
    _rqctx: Arc<RequestContext<()>>,
    _body: HmacVerifiedBody<webhooky::handlers_docusign::DocusignWebhookVerification>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    Ok(HttpResponseAccepted("ok".to_string()))
}

#[endpoint {
    method = POST,
    path = "/hmac/docusign/audit",
}]
async fn hmac_docusign_audit(
    _rqctx: Arc<RequestContext<()>>,
    _body: HmacVerifiedBodyAudit<webhooky::handlers_docusign::DocusignWebhookVerification>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    Ok(HttpResponseAccepted("ok".to_string()))
}

#[endpoint {
    method = POST,
    path = "/hmac/slack/verify",
}]
async fn hmac_slack_verification(
    _rqctx: Arc<RequestContext<()>>,
    _body: HmacVerifiedBody<webhooky::handlers_slack::SlackWebhookVerification>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    Ok(HttpResponseAccepted("ok".to_string()))
}

#[endpoint {
    method = POST,
    path = "/hmac/slack/audit",
}]
async fn hmac_slack_audit(
    _rqctx: Arc<RequestContext<()>>,
    _body: HmacVerifiedBodyAudit<webhooky::handlers_slack::SlackWebhookVerification>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    Ok(HttpResponseAccepted("ok".to_string()))
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

async fn make_server() -> anyhow::Result<HttpServer<()>> {
    std::env::set_var("AUTH_BEARER", "TEST_BEARER");
    std::env::set_var("DOCUSIGN_WH_KEY", "vkPkH4G2k8XNC5HWA6QgZd08v37P8KcVZMjaP4zgGWc=");
    std::env::set_var("GITHUB_WH_KEY", "vkPkH4G2k8XNC5HWA6QgZd08v37P8KcVZMjaP4zgGWc=");
    std::env::set_var("SLACK_WH_KEY", "vkPkH4G2k8XNC5HWA6QgZd08v37P8KcVZMjaP4zgGWc=");

    let config_dropshot = ConfigDropshot {
        bind_address: "127.0.0.1:12345".parse()?,
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

    server.close().await.unwrap();
}

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

    server.close().await.unwrap();
}

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

    server.close().await.unwrap();
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

    server.close().await.unwrap();
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

    server.close().await.unwrap();
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

    server.close().await.unwrap();
}

/// Test DocuSign signatures

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

    server.close().await.unwrap();
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

    server.close().await.unwrap();
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

    server.close().await.unwrap();
}

/// Test Slack signatures

#[ignore]
#[tokio::test]
async fn test_slack_hmac_passes() {
    let server = make_server().await.unwrap();

    let test_signature = "318376db08607eb984726533b1d53430e31c4825fd0d9b14e8ed38e2a88ada19";
    let test_timestamp = "1531420618";
    let test_body = include_str!("../tests/slack_webhook_sig_test.json").trim();

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post("http://127.0.0.1:12345/hmac/slack/verify")
        .header("X-Slack-Signature", test_signature)
        .header("X-Slack-Request-Timestamp", test_timestamp)
        .body(test_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::ACCEPTED);

    server.close().await.unwrap();
}

#[ignore]
#[tokio::test]
async fn test_slack_hmac_fails() {
    let server = make_server().await.unwrap();

    let test_signature = "318376db08607eb984726533b1d53430e31c4825fd0d9b14e8ed38e2a88ada18";
    let test_timestamp = "1531420618";
    let test_body = include_str!("../tests/slack_webhook_sig_test.json").trim();

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post("http://127.0.0.1:12345/hmac/slack/verify")
        .header("X-Slack-Signature", test_signature)
        .header("X-Slack-Request-Timestamp", test_timestamp)
        .body(test_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);

    server.close().await.unwrap();
}

#[tokio::test]
async fn test_slack_hmac_audit_passes_with_invalid_signature() {
    let server = make_server().await.unwrap();

    let test_signature = "318376db08607eb984726533b1d53430e31c4825fd0d9b14e8ed38e2a88ada18";
    let test_timestamp = "1531420618";
    let test_body = include_str!("../tests/slack_webhook_sig_test.json").trim();

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post("http://127.0.0.1:12345/hmac/slack/audit")
        .header("X-Slack-Signature", test_signature)
        .header("X-Slack-Request-Timestamp", test_timestamp)
        .body(test_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::ACCEPTED);

    server.close().await.unwrap();
}

/// Test global Bearer token

#[tokio::test]
async fn test_bearer_passes() {
    let server = make_server().await.unwrap();

    let test_token = "TEST_BEARER";
    let test_body = "";

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post("http://127.0.0.1:12345/bearer/verify")
        .header("Authorization", &format!("Bearer {}", test_token))
        .body(test_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::ACCEPTED);

    server.close().await.unwrap();
}

#[tokio::test]
async fn test_bearer_fails() {
    let server = make_server().await.unwrap();

    let test_token = "TEST_BEARER_2";
    let test_body = "";

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post("http://127.0.0.1:12345/bearer/verify")
        .header("Authorization", &format!("Bearer {}", test_token))
        .body(test_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);

    server.close().await.unwrap();
}

#[tokio::test]
async fn test_bearer_audit_pass_with_invalid_token() {
    let server = make_server().await.unwrap();

    let test_token = "TEST_BEARER_2";
    let test_body = "";

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post("http://127.0.0.1:12345/bearer/audit")
        .header("Authorization", &format!("Bearer {}", test_token))
        .body(test_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::ACCEPTED);

    server.close().await.unwrap();
}

/// Test global Query token

#[tokio::test]
async fn test_query_token_passes() {
    let server = make_server().await.unwrap();

    let test_token = "TEST_BEARER";
    let test_body = "";

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:12345/token/verify?token={}", test_token))
        .body(test_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::ACCEPTED);

    server.close().await.unwrap();
}

#[tokio::test]
async fn test_query_token_fails() {
    let server = make_server().await.unwrap();

    let test_token = "TEST_BEARER_2";
    let test_body = "";

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:12345/token/verify?token={}", test_token))
        .body(test_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);

    server.close().await.unwrap();
}

#[tokio::test]
async fn test_query_token_audit_pass_with_invalid_token() {
    let server = make_server().await.unwrap();

    let test_token = "TEST_BEARER_2";
    let test_body = "";

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:12345/token/audit?token={}", test_token))
        .body(test_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::ACCEPTED);

    server.close().await.unwrap();
}
