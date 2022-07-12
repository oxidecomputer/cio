use anyhow::{anyhow, Result};
use checkr::WebhookEvent as CheckrWebhook;
use docusign::Envelope;
use dropshot::{
    endpoint, ApiDescription, ConfigDropshot, ConfigLogging, ConfigLoggingLevel, HttpError, HttpResponseAccepted,
    HttpServer, HttpServerStarter, RequestContext,
};
use dropshot_verify_request::{
    bearer::{Bearer, BearerAudit},
    query::{QueryToken, QueryTokenAudit},
    sig::{HmacVerifiedBody, HmacVerifiedBodyAudit},
};
use slack_chat_api::BotCommand;
use std::sync::Arc;

use webhooky::{auth::InternalToken, github_types::GitHubWebhook, handlers_slack::InteractiveEvent};

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
    _: Bearer<InternalToken>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    Ok(HttpResponseAccepted("ok".to_string()))
}

#[endpoint {
    method = POST,
    path = "/bearer/audit",
}]
async fn bearer_audit(
    _rqctx: Arc<RequestContext<()>>,
    _: BearerAudit<InternalToken>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    Ok(HttpResponseAccepted("ok".to_string()))
}

#[endpoint {
    method = POST,
    path = "/token/verify",
}]
async fn token_verification(
    _rqctx: Arc<RequestContext<()>>,
    _: QueryToken<InternalToken>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    Ok(HttpResponseAccepted("ok".to_string()))
}

#[endpoint {
    method = POST,
    path = "/token/audit",
}]
async fn token_audit(
    _rqctx: Arc<RequestContext<()>>,
    _: QueryTokenAudit<InternalToken>,
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

    // Configure fake test keys for checking implementations. These keys are used to generate
    // the signatures used down below in each test case. The signatures are precomputed with
    // the following function, where `key` is the secret key and `content` is the content of
    // a test string. If the test keys or the test content are altered, then the signatures
    // in the respective tests will need to be regenerated.
    //
    // fn sign(key: &[u8], content: &[u8]) -> String {
    //     use hmac::{Hmac, Mac};
    //     use sha2::Sha256;
    //     type HmacSha256 = Hmac<Sha256>;

    //     let mut mac = HmacSha256::new_from_slice(key).unwrap();
    //     mac.update(content);

    //     let result = mac.finalize();
    //     hex::encode(result.into_bytes())
    // }
    std::env::set_var("INTERNAL_AUTH_BEARER", "TEST_BEARER");
    std::env::set_var("DOCUSIGN_WH_KEY", "vkPkH4G2k8XNC5HWA6QgZd08v37P8KcVZMjaP4zgGWc=");
    std::env::set_var("GH_WH_KEY", "vkPkH4G2k8XNC5HWA6QgZd08v37P8KcVZMjaP4zgGWc=");
    std::env::set_var("SLACK_WH_KEY", "vkPkH4G2k8XNC5HWA6QgZd08v37P8KcVZMjaP4zgGWc=");
    std::env::set_var("CHECKR_API_KEY", "vkPkH4G2k8XNC5HWA6QgZd08v37P8KcVZMjaP4zgGWc=");

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

/// General signature tests

const GITHUB_TEST_BODY: &str = r#"{"action":"created","starred_at":"2022-06-10T12:53:33Z","repository":{"id":378353157,"node_id":"MDEwOlJlcG9zaXRvcnkzNzgzNTMxNTc=","name":"progenitor","full_name":"oxidecomputer/progenitor","private":false,"owner":{"login":"oxidecomputer","id":54040662,"node_id":"MDEyOk9yZ2FuaXphdGlvbjU0MDQwNjYy","avatar_url":"https://avatars.githubusercontent.com/u/54040662?v=4","gravatar_id":"","url":"https://api.github.com/users/oxidecomputer","html_url":"https://github.com/oxidecomputer","followers_url":"https://api.github.com/users/oxidecomputer/followers","following_url":"https://api.github.com/users/oxidecomputer/following{/other_user}","gists_url":"https://api.github.com/users/oxidecomputer/gists{/gist_id}","starred_url":"https://api.github.com/users/oxidecomputer/starred{/owner}{/repo}","subscriptions_url":"https://api.github.com/users/oxidecomputer/subscriptions","organizations_url":"https://api.github.com/users/oxidecomputer/orgs","repos_url":"https://api.github.com/users/oxidecomputer/repos","events_url":"https://api.github.com/users/oxidecomputer/events{/privacy}","received_events_url":"https://api.github.com/users/oxidecomputer/received_events","type":"Organization","site_admin":false},"html_url":"https://github.com/oxidecomputer/progenitor","description":"An OpenAPI client generator","fork":false,"url":"https://api.github.com/repos/oxidecomputer/progenitor","forks_url":"https://api.github.com/repos/oxidecomputer/progenitor/forks","keys_url":"https://api.github.com/repos/oxidecomputer/progenitor/keys{/key_id}","collaborators_url":"https://api.github.com/repos/oxidecomputer/progenitor/collaborators{/collaborator}","teams_url":"https://api.github.com/repos/oxidecomputer/progenitor/teams","hooks_url":"https://api.github.com/repos/oxidecomputer/progenitor/hooks","issue_events_url":"https://api.github.com/repos/oxidecomputer/progenitor/issues/events{/number}","events_url":"https://api.github.com/repos/oxidecomputer/progenitor/events","assignees_url":"https://api.github.com/repos/oxidecomputer/progenitor/assignees{/user}","branches_url":"https://api.github.com/repos/oxidecomputer/progenitor/branches{/branch}","tags_url":"https://api.github.com/repos/oxidecomputer/progenitor/tags","blobs_url":"https://api.github.com/repos/oxidecomputer/progenitor/git/blobs{/sha}","git_tags_url":"https://api.github.com/repos/oxidecomputer/progenitor/git/tags{/sha}","git_refs_url":"https://api.github.com/repos/oxidecomputer/progenitor/git/refs{/sha}","trees_url":"https://api.github.com/repos/oxidecomputer/progenitor/git/trees{/sha}","statuses_url":"https://api.github.com/repos/oxidecomputer/progenitor/statuses/{sha}","languages_url":"https://api.github.com/repos/oxidecomputer/progenitor/languages","stargazers_url":"https://api.github.com/repos/oxidecomputer/progenitor/stargazers","contributors_url":"https://api.github.com/repos/oxidecomputer/progenitor/contributors","subscribers_url":"https://api.github.com/repos/oxidecomputer/progenitor/subscribers","subscription_url":"https://api.github.com/repos/oxidecomputer/progenitor/subscription","commits_url":"https://api.github.com/repos/oxidecomputer/progenitor/commits{/sha}","git_commits_url":"https://api.github.com/repos/oxidecomputer/progenitor/git/commits{/sha}","comments_url":"https://api.github.com/repos/oxidecomputer/progenitor/comments{/number}","issue_comment_url":"https://api.github.com/repos/oxidecomputer/progenitor/issues/comments{/number}","contents_url":"https://api.github.com/repos/oxidecomputer/progenitor/contents/{+path}","compare_url":"https://api.github.com/repos/oxidecomputer/progenitor/compare/{base}...{head}","merges_url":"https://api.github.com/repos/oxidecomputer/progenitor/merges","archive_url":"https://api.github.com/repos/oxidecomputer/progenitor/{archive_format}{/ref}","downloads_url":"https://api.github.com/repos/oxidecomputer/progenitor/downloads","issues_url":"https://api.github.com/repos/oxidecomputer/progenitor/issues{/number}","pulls_url":"https://api.github.com/repos/oxidecomputer/progenitor/pulls{/number}","milestones_url":"https://api.github.com/repos/oxidecomputer/progenitor/milestones{/number}","notifications_url":"https://api.github.com/repos/oxidecomputer/progenitor/notifications{?since,all,participating}","labels_url":"https://api.github.com/repos/oxidecomputer/progenitor/labels{/name}","releases_url":"https://api.github.com/repos/oxidecomputer/progenitor/releases{/id}","deployments_url":"https://api.github.com/repos/oxidecomputer/progenitor/deployments","created_at":"2021-06-19T07:37:00Z","updated_at":"2022-06-10T12:53:34Z","pushed_at":"2022-06-09T19:13:33Z","git_url":"git://github.com/oxidecomputer/progenitor.git","ssh_url":"git@github.com:oxidecomputer/progenitor.git","clone_url":"https://github.com/oxidecomputer/progenitor.git","svn_url":"https://github.com/oxidecomputer/progenitor","homepage":null,"size":652,"stargazers_count":64,"watchers_count":64,"language":"Rust","has_issues":true,"has_projects":true,"has_downloads":true,"has_wiki":true,"has_pages":false,"forks_count":10,"mirror_url":null,"archived":false,"disabled":false,"open_issues_count":11,"license":null,"allow_forking":true,"is_template":false,"topics":[],"visibility":"public","forks":10,"open_issues":11,"watchers":64,"default_branch":"main"},"organization":{"login":"oxidecomputer","id":54040662,"node_id":"MDEyOk9yZ2FuaXphdGlvbjU0MDQwNjYy","url":"https://api.github.com/orgs/oxidecomputer","repos_url":"https://api.github.com/orgs/oxidecomputer/repos","events_url":"https://api.github.com/orgs/oxidecomputer/events","hooks_url":"https://api.github.com/orgs/oxidecomputer/hooks","issues_url":"https://api.github.com/orgs/oxidecomputer/issues","members_url":"https://api.github.com/orgs/oxidecomputer/members{/member}","public_members_url":"https://api.github.com/orgs/oxidecomputer/public_members{/member}","avatar_url":"https://avatars.githubusercontent.com/u/54040662?v=4","description":"Servers as they should be."}}"#;

#[tokio::test]
async fn test_missing_signature_hmac_fails() {
    let (port, _server) = make_server();

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{}/hmac/github/verify", port))
        .body(GITHUB_TEST_BODY)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_missing_signature_hmac_audit_passes() {
    let (port, _server) = make_server();

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{}/hmac/github/audit", port))
        .body(GITHUB_TEST_BODY)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::ACCEPTED);
}

/// Test GitHub signatures

#[tokio::test]
async fn test_github_hmac_passes() {
    let (port, _server) = make_server();

    let test_signature = "sha256=8c7ac6b6a1ca30229207b4406d50b5c034d90f56009835bc7f32a16b2044d29d";

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{}/hmac/github/verify", port))
        .header("X-Hub-Signature-256", test_signature)
        .body(GITHUB_TEST_BODY)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::ACCEPTED);
}

#[tokio::test]
async fn test_github_hmac_fails() {
    let (port, _server) = make_server();

    let test_signature = "sha256=1111111111111111111111111111111111111111111111111111111111111";

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{}/hmac/github/verify", port))
        .header("X-Hub-Signature-256", test_signature)
        .body(GITHUB_TEST_BODY)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_github_hmac_audit_passes_with_invalid_signature() {
    let (port, _server) = make_server();

    let test_signature = "sha256=1111111111111111111111111111111111111111111111111111111111111";

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{}/hmac/github/audit", port))
        .header("X-Hub-Signature-256", test_signature)
        .body(GITHUB_TEST_BODY)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::ACCEPTED);
}

/// Test Checkr signatures

const CHECKR_TEST_BODY: &str = r#"{"scene":false,"dry":{"face":false,"fox":["accurate",1795857417,false]},"created_at":"2022-01-01T00:00:00Z"}"#;

#[tokio::test]
async fn test_checkr_hmac_passes() {
    let (port, _server) = make_server();

    let test_signature = "f40af2270bc2ef0528e626b267dfc7e3e4c2f3bff1b94f6153f85d2e047ee710";

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{}/hmac/checkr/verify", port))
        .header("X-Checkr-Signature", test_signature)
        .body(CHECKR_TEST_BODY)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::ACCEPTED);
}

#[tokio::test]
async fn test_checkr_hmac_fails() {
    let (port, _server) = make_server();

    let test_signature = "1111111111111111111111111111111111111111111111111111111111111";

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{}/hmac/checkr/verify", port))
        .header("X-Checkr-Signature", test_signature)
        .body(CHECKR_TEST_BODY)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_checkr_hmac_audit_passes_with_invalid_signature() {
    let (port, _server) = make_server();

    let test_signature = "1111111111111111111111111111111111111111111111111111111111111";

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{}/hmac/checkr/audit", port))
        .header("X-Checkr-Signature", test_signature)
        .body(CHECKR_TEST_BODY)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::ACCEPTED);
}

/// Test DocuSign signatures

const DOCUSIGN_TEST_BODY: &str = r#"{"fairly":false,"wait":2046690168,"influence":-1922644534.9610949,"measure":{"powder":"temperature","name":"barn","fire":"color","keep":false,"five":["second",false,"automobile",true,"object",298950147.7510176],"thirty":false},"rocky":true,"scientific":"white"}"#;

#[tokio::test]
async fn test_docusign_hmac_passes() {
    let (port, _server) = make_server();

    let test_signature = "45f3fa45a13a02bc3331ad1fe034c9ba2a0fe2d186f1bca5c1e56319bf626469";

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{}/hmac/docusign/verify", port))
        .header("X-DocuSign-Signature-1", test_signature)
        .body(DOCUSIGN_TEST_BODY)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::ACCEPTED);
}

#[tokio::test]
async fn test_docusign_hmac_fails() {
    let (port, _server) = make_server();

    let test_signature = "1111111111111111111111111111111111111111111111111111111111111";

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{}/hmac/docusign/verify", port))
        .header("X-DocuSign-Signature-1", test_signature)
        .body(DOCUSIGN_TEST_BODY)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_docusign_hmac_audit_passes_with_invalid_signature() {
    let (port, _server) = make_server();

    let test_signature = "1111111111111111111111111111111111111111111111111111111111111";

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{}/hmac/docusign/audit", port))
        .header("X-DocuSign-Signature-1", test_signature)
        .body(DOCUSIGN_TEST_BODY)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::ACCEPTED);
}

/// Test Slack signatures

const SLACK_TEST_BODY: &str = r#"user_name=test&command=fakecommand"#;

#[tokio::test]
async fn test_slack_hmac_passes() {
    let (port, _server) = make_server();

    let test_signature = "v0=a421125f1e5572b0f7b2116a1df1f5083fc5eb742d4f54ccb19b8c986bd0bc74";
    let test_timestamp = "1531420618";

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{}/hmac/slack/verify", port))
        .header("X-Slack-Signature", test_signature)
        .header("X-Slack-Request-Timestamp", test_timestamp)
        .body(SLACK_TEST_BODY)
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

    let test_signature = "v0=1111111111111111111111111111111111111111111111111111111111111";
    let test_timestamp = "1531420618";

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{}/hmac/slack/verify", port))
        .header("X-Slack-Signature", test_signature)
        .header("X-Slack-Request-Timestamp", test_timestamp)
        .body(SLACK_TEST_BODY)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_slack_hmac_audit_passes_with_invalid_signature() {
    let (port, _server) = make_server();

    let test_signature = "v0=1111111111111111111111111111111111111111111111111111111111111";
    let test_timestamp = "1531420618";

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{}/hmac/slack/audit", port))
        .header("X-Slack-Signature", test_signature)
        .header("X-Slack-Request-Timestamp", test_timestamp)
        .body(SLACK_TEST_BODY)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::ACCEPTED);

    let body: BotCommand = response.json().await.unwrap();

    assert_eq!("test", body.user_name.as_str());
    assert_eq!("fakecommand", body.command.as_str());
}

const SLACK_INTERACTIVE_TEST_BODY: &str = r#"payload=%7B%22type%22%3A%22test_type%22%2C%22api_app_id%22%3A%20%22test%22%7D"#;

#[tokio::test]
async fn test_slack_interactive_hmac_passes() {
    let (port, _server) = make_server();

    let test_signature = "v0=f52abb78d323d299c4cc76c8809d18ede86b325761afdf8daa13f7a23f30a538";
    let test_timestamp = "1531420618";

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{}/hmac/slack/interactive/verify", port))
        .header("X-Slack-Signature", test_signature)
        .header("X-Slack-Request-Timestamp", test_timestamp)
        .body(SLACK_INTERACTIVE_TEST_BODY)
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

    let test_signature = "v0=1111111111111111111111111111111111111111111111111111111111111";
    let test_timestamp = "1531420618";

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{}/hmac/slack/interactive/verify", port))
        .header("X-Slack-Signature", test_signature)
        .header("X-Slack-Request-Timestamp", test_timestamp)
        .body(SLACK_INTERACTIVE_TEST_BODY)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_slack_interactive_hmac_audit_passes_with_invalid_signature() {
    let (port, _server) = make_server();

    let test_signature = "v0=1111111111111111111111111111111111111111111111111111111111111";
    let test_timestamp = "1531420618";

    // Make the post API call.
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{}/hmac/slack/interactive/audit", port))
        .header("X-Slack-Signature", test_signature)
        .header("X-Slack-Request-Timestamp", test_timestamp)
        .body(SLACK_INTERACTIVE_TEST_BODY)
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
