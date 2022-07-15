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
pub struct ApplicantLogin {
    email: String,
    application: Option<ApplicationView>,
    upload_token: String,
}

pub async fn handle_applicant_login(rqctx: Arc<RequestContext<Context>>, email: String) -> Result<ApplicantLogin> {
    let ctx = rqctx.context();

    // Third argument is a Google Sheet id. This is a no-longer supported argument
    let applicant = Applicant::get_from_db(&ctx.db, email.clone(), "".to_string()).await;
    let token = ctx.upload_token_store.get(&email).await?;

    Ok(ApplicantLogin {
        email,
        application: applicant.map(|applicant| ApplicationView {
            submitted_at: applicant.submitted_time,
            status: applicant.status,
        }),
        upload_token: token.token,
    })
}
