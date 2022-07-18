use anyhow::Result;
use chrono::{DateTime, Utc};
use cio_api::applicants::Applicant;
use dropshot::RequestContext;
use schemars::JsonSchema;
use serde::Serialize;
use std::sync::Arc;

use crate::server::Context;

#[derive(Debug, Serialize, JsonSchema)]
pub struct ApplicationView {
    submitted_at: DateTime<Utc>,
    status: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ApplicantInfo {
    email: String,
    application: Option<ApplicationView>
}

pub async fn handle_applicant_info(rqctx: Arc<RequestContext<Context>>, email: String) -> Result<ApplicantInfo> {
    let ctx = rqctx.context();

    // Third argument is a Google Sheet id. This is a no-longer supported argument
    let applicant = Applicant::get_from_db(&ctx.db, email.clone(), "".to_string()).await;

    Ok(ApplicantInfo {
        email,
        application: applicant.map(|applicant| ApplicationView {
            submitted_at: applicant.submitted_time,
            status: applicant.status,
        })
    })
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ApplicantUploadToken {
    email: String,
    token: String,
}

pub async fn handle_applicant_upload_token(rqctx: Arc<RequestContext<Context>>, email: String) -> Result<ApplicantUploadToken> {
    let ctx = rqctx.context();

    // Third argument is a Google Sheet id. This is a no-longer supported argument
    let token = ctx.upload_token_store.get(&email).await?;

    Ok(ApplicantUploadToken {
        email,
        token: token.token,
    })
}
