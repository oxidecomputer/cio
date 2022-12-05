use anyhow::Result;
use async_bb8_diesel::AsyncRunQueryDsl;
use chrono::{DateTime, Utc};
use cio_api::{applicants::Applicant, schema::applicants};
use diesel::{ExpressionMethods, QueryDsl};
use dropshot::RequestContext;
use schemars::JsonSchema;
use serde::Serialize;
use std::sync::Arc;

use crate::context::Context;

#[derive(Debug, Serialize, JsonSchema)]
pub struct ApplicationView {
    role: String,
    submitted_at: DateTime<Utc>,
    status: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ApplicantInfo {
    email: String,
    application: Option<ApplicationView>,
}

pub async fn handle_applicant_info(rqctx: Arc<RequestContext<Context>>, email: String) -> Result<ApplicantInfo> {
    let ctx = rqctx.context();

    // Applicants is unfortunately not unique on the email column. We need to return the newest
    // record to work around a few cases where ignoring the Google sheet id resulted in multiple
    // records being created
    let applicants = applicants::dsl::applicants
        .filter(applicants::dsl::email.eq(email.clone()))
        .order_by(applicants::dsl::id.desc())
        .load_async::<Applicant>(ctx.db.pool())
        .await?;

    Ok(ApplicantInfo {
        email,
        application: applicants.into_iter().next().map(|applicant| ApplicationView {
            role: applicant.role,
            submitted_at: applicant.submitted_time,
            status: applicant.status,
        }),
    })
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ApplicantUploadToken {
    email: String,
    token: String,
}

pub async fn handle_applicant_upload_token(
    rqctx: Arc<RequestContext<Context>>,
    email: String,
) -> Result<ApplicantUploadToken> {
    let ctx = rqctx.context();

    let token = ctx.upload_token_store.get(&email).await?;

    Ok(ApplicantUploadToken {
        email,
        token: token.token,
    })
}
