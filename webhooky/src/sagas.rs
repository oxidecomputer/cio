use std::sync::Arc;

use anyhow::Result;
use cio_api::{
    db::Database,
    functions::{FnOutput, Function},
};
use serde::{Deserialize, Serialize};
use slog_scope_futures::FutureExt as _;

use crate::health::SelfMemory;

/// Define our saga for syncing repos.
#[derive(Debug)]
pub struct Saga;

#[derive(Debug, Deserialize, Serialize)]
pub struct Params {
    cmd_name: String,
    saga_id: uuid::Uuid,
}

#[derive(Debug)]
pub struct Context {
    db: Database,
}

impl steno::SagaType for Saga {
    // Type for the saga's parameters
    type SagaParamsType = Params;

    // Type for the application-specific context (see above)
    type ExecContextType = Arc<Context>;
}

async fn undo_action(_action_context: steno::ActionContext<Saga>) -> Result<()> {
    // This is a noop, we don't have to undo anything.
    Ok(())
}

/// Create a new saga with the given parameters and then execute it.
pub async fn do_saga(
    db: &Database,
    sec: &steno::SecClient,
    id: &uuid::Uuid,
    template: steno::SagaTemplate<Saga>,
    cmd_name: &str,
) -> Result<()> {
    let context = Arc::new(Context { db: db.clone() });
    let params = Params {
        cmd_name: cmd_name.to_string(),
        saga_id: *id,
    };

    let saga_template = Arc::new(template);

    let saga_id = steno::SagaId(*id);

    // Create the saga.
    sec.saga_create(saga_id, Arc::new(context), saga_template, cmd_name.to_string(), params)
        .await?;

    // Set it running.
    sec.saga_start(saga_id).await?;

    Ok(())
}

pub async fn run_cmd(db: &Database, sec: &steno::SecClient, id: &uuid::Uuid, cmd_name: &str) -> Result<()> {
    let mut builder = steno::SagaTemplateBuilder::new();
    builder.append(
        // name of this action's output (can be used in subsequent actions)
        cmd_name,
        // human-readable label for the action
        cmd_name,
        steno::ActionFunc::new_action(
            // action function
            action_run_cmd,
            // undo function
            undo_action,
        ),
    );

    do_saga(db, sec, id, builder.build(), cmd_name).await
}

async fn action_run_cmd(action_context: steno::ActionContext<Saga>) -> Result<FnOutput, steno::ActionError> {
    let db = &action_context.user_data().db;
    let cmd_name = &action_context.saga_params().cmd_name;
    let saga_id = &action_context.saga_params().saga_id;

    if let Ok(mem) = SelfMemory::new() {
        log::info!("Memory before running {}({}): {:?}", cmd_name, saga_id, mem);
    }

    let sub_cmd = crate::core::SubCommand::SyncZoho(crate::core::SyncZoho {});
    let logger = slog_scope::logger();
    let cmd_logger = logger
        .new(slog::o!("cmd" => cmd_name.to_string(), "saga_id" => saga_id.to_string()))
        .clone();
    let context = crate::context::Context::new(1).await.map_err(AsActionError)?;

    let result = crate::job::run_job_cmd(sub_cmd, context).with_logger(cmd_logger).await;

    if let Ok(mem) = SelfMemory::new() {
        log::info!("Memory after running {}({}): {:?}", cmd_name, saga_id, mem);
    }

    match result {
        Ok(_) => {
            Function::add_logs_with_conclusion(db, saga_id, "", &octorust::types::Conclusion::Success)
                .await
                .map_err(AsActionError)?;
            Ok(FnOutput(String::new()))
        }
        Err(err) => {
            let output = format!("{:?}", err);
            Function::add_logs_with_conclusion(db, saga_id, &output, &octorust::types::Conclusion::Failure)
                .await
                .map_err(AsActionError)?;
            Err(AsActionError(err).into())
        }
    }
}

struct AsActionError(anyhow::Error);

impl From<AsActionError> for steno::ActionError {
    fn from(err: AsActionError) -> Self {
        steno::ActionError::action_failed(format!("ERROR:\n\n{:?}", err.0))
    }
}
