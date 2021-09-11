use std::sync::Arc;

use anyhow::{bail, Result};
use cio_api::{functions::Function, schema::functions};
use diesel::{ExpressionMethods, QueryDsl, RunQueryDsl};
use dropshot::{Path, RequestContext};
use log::info;

use crate::server::{Context, FunctionPathParams};

pub async fn handle_get_function_by_uuid(
    rqctx: Arc<RequestContext<Context>>,
    path_params: Path<FunctionPathParams>,
) -> Result<Function> {
    let uuid = path_params.into_inner().uuid;
    info!("getting info for function uuid `{}`", uuid);

    let api_context = rqctx.context();
    let db = &api_context.db;

    let result = Function::get_from_db(db, uuid.to_string());
    if result.is_none() {
        // Return early, we couldn't find a function.
        bail!("no function was found with uuid `{}`", uuid);
    }

    Ok(result.unwrap())
}

pub async fn handle_get_function_logs_by_uuid(
    rqctx: Arc<RequestContext<Context>>,
    path_params: Path<FunctionPathParams>,
) -> Result<String> {
    let f = handle_get_function_by_uuid(rqctx, path_params).await?;

    Ok(f.logs)
}

pub async fn handle_reexec_cmd(rqctx: Arc<RequestContext<Context>>, cmd_name: &str) -> Result<uuid::Uuid> {
    let api_context = rqctx.context();
    let db = &api_context.db;
    let sec = &api_context.sec;

    // Check if we already have an in-progress run for this job.
    if let Ok(f) = functions::dsl::functions
        .filter(functions::dsl::name.eq(cmd_name.to_string()))
        .filter(functions::dsl::status.eq(octorust::types::JobStatus::InProgress.to_string()))
        .first::<Function>(&db.conn())
    {
        let u = uuid::Uuid::parse_str(&f.saga_id)?;

        // If the server stopped and restarted, we might have a lingering job
        // that we want to ignore and instead start a new one.
        // Check if our steno client knows of this saga id.
        // if let Ok(_saga) = &sec.saga_get(steno::SagaId(u.clone())).await {
        // TODO: Make sure our saga is not "Done".
        // Our saga _should_ be marked as completed by the database, if it is done.
        // Return that uuid versus starting another.
        return Ok(u);
        //}
    }

    let id = uuid::Uuid::new_v4();

    // Run the saga.
    crate::sagas::run_cmd(db, sec, &id, cmd_name).await?;

    Ok(id)
}
