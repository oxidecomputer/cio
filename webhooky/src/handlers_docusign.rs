// use std::{str::FromStr, sync::Arc};

// use anyhow::Result;
// use async_trait::async_trait;
// use chrono::offset::Utc;
// use cio_api::{
//     companies::Company,
//     configs::{
//         get_configs_from_repo, sync_buildings, sync_certificates, sync_conference_rooms,
//         sync_github_outside_collaborators, sync_groups, sync_links, sync_users,
//     },
//     repos::NewRepo,
//     rfds::{is_image, NewRFD, RFD},
//     shorturls::{generate_shorturls_for_configs_links, generate_shorturls_for_repos, generate_shorturls_for_rfds},
//     utils::{create_or_update_file_in_github_repo, decode_base64_to_string, get_file_content_from_repo},
// };
// use dropshot::{RequestContext, ExtractorMetadata, UntypedBody, HttpError, ServerContext, Extractor};
// use google_drive::traits::{DriveOps, FileOps};
// use hmac::Hmac;
// use log::{info, warn};
// use sha2::Sha256;

// use std::borrow::Cow;

// use crate::{event_types::EventType, github_types::GitHubWebhook, repos::Repo, server::Context, sig::HmacSignatureVerifier, http::{unauthorized, Headers}};

// pub struct GitHubWebhookVerification;

// #[async_trait]
// impl HmacSignatureVerifier for GitHubWebhookVerification {
//     type Algo = Hmac<Sha256>;

//     async fn key<'a, Context: ServerContext>(rqctx: &'a Arc<RequestContext<Context>>) -> Result<Cow<'a, [u8]>> {
//         Ok(std::env::var("GITHUB_KEY").map(|key| Cow::Owned(key.into_bytes()))?)
//     }

//     async fn signature<'a, Context: ServerContext>(rqctx: &'a Arc<RequestContext<Context>>) -> Result<Cow<'a, [u8]>> {
//         let headers = Headers::from_request(rqctx.clone()).await?;
//         let signature = headers.0
//             .get("X-Hub-Signature-256")
//             .and_then(|header_value| {
//                 header_value.to_str().ok()
//             })
//             .and_then(|header| {
//                 hex::decode(header.trim_start_matches("sha256")).ok()
//             }).ok_or_else(unauthorized)?;

//         Ok(Cow::Owned(signature))
//     }
// }