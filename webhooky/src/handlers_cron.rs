use anyhow::Result;
use async_bb8_diesel::AsyncRunQueryDsl;
use chrono::Utc;
use chrono_humanize::HumanTime;
use cio_api::{functions::Function, schema::functions};
use diesel::{ExpressionMethods, QueryDsl};
use log::info;

use crate::server::Context;

pub async fn handle_reexec_cmd(api_context: &Context, cmd_name: &str, background: bool) -> Result<uuid::Uuid> {
    let db = &api_context.db;

    // Check if we already have an in-progress run for this job.
    if let Ok(f) = functions::dsl::functions
        .filter(functions::dsl::name.eq(cmd_name.to_string()))
        .filter(functions::dsl::status.eq(octorust::types::JobStatus::InProgress.to_string()))
        .order_by(functions::dsl::created_at.desc()) // Get the most recent one first.
        .first_async::<Function>(db.pool())
        .await
    {
        let u = uuid::Uuid::parse_str(&f.saga_id)?;

        // If the server stopped and restarted, we might have a lingering job
        // that we want to ignore and instead start a new one.
        // Check if the duration it was started is longer than a few hours ago.
        let hours = -1;
        let duration_from_now = f.created_at.signed_duration_since(Utc::now());
        if (duration_from_now.num_hours()) > hours {
            info!(
                "existing job for `{}` was created `{}`, returning that job",
                cmd_name,
                HumanTime::from(duration_from_now),
            );
            // TODO: a better way to be to check if we know about the saga.
            // Return that uuid versus starting another.
            return Ok(u);
        }
    }

    let id = uuid::Uuid::new_v4();

    // Run the saga.
    crate::sagas::run_cmd(db, &api_context.sec, api_context.exec_registry.clone(), &id, cmd_name, background).await?;

    Ok(id)
}
