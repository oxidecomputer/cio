use std::sync::Arc;

use anyhow::{bail, Result};
use cio_api::{companies::Companys, functions::Function};
use dropshot::{Path, RequestContext};
use log::info;

use crate::{Context, FunctionPathParams};

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

pub async fn handle_sync_repos_create(rqctx: Arc<RequestContext<Context>>) -> Result<Vec<uuid::Uuid>> {
    let api_context = rqctx.context();
    let db = &api_context.db;

    let mut fns: Vec<uuid::Uuid> = Vec::new();
    let companies = Companys::get_from_db(db, 1)?;
    // Iterate over the companies and update.
    for company in companies {
        let id = uuid::Uuid::new_v4();

        // Run the saga.
        cio_api::sagas::sync_repos(db, &api_context.sec, &id, &company).await?;

        fns.push(id);
    }

    Ok(fns)
}
