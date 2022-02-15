use std::{
    env,
    io::{BufRead, BufReader},
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{bail, Result};
use chrono::Utc;
use cio_api::{
    db::Database,
    functions::{FnOutput, Function},
};
use serde::{Deserialize, Serialize};

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
    background: bool,
) -> Result<()> {
    let context = Arc::new(Context { db: db.clone() });
    let params = Params {
        cmd_name: cmd_name.to_string(),
        saga_id: *id,
    };

    let saga_template = Arc::new(template);

    let saga_id = steno::SagaId(*id);

    // Create the saga.
    let saga_future = sec
        .saga_create(saga_id, Arc::new(context), saga_template, cmd_name.to_string(), params)
        .await?;

    // Set it running.
    sec.saga_start(saga_id).await?;

    if !background {
        //
        // Wait for the saga to finish running.  This could take a while, depending
        // on what the saga does!  This traverses the DAG of actions, executing each
        // one.  If one fails, then it's all unwound: any actions that previously
        // completed will be undone.
        //
        // Note that the SEC will run all this regardless of whether you wait for it
        // here.  This is just a handle for you to know when the saga has finished.
        let result = saga_future.await;
        on_saga_complete(db, &saga_id, &result, cmd_name).await?;
    }

    Ok(())
}

pub async fn on_saga_complete(
    db: &Database,
    saga_id: &steno::SagaId,
    result: &steno::SagaResult,
    cmd_name: &str,
) -> Result<()> {
    // Get the function.
    let mut f = Function::get_from_db(db, saga_id.to_string()).unwrap();

    // Print the results.
    match result.kind.clone() {
        Ok(s) => {
            // Save the success output to the logs.
            // For each function.
            let log = s.lookup_output::<FnOutput>(cmd_name)?;

            f.logs = log.0.trim().to_string();
            f.conclusion = octorust::types::Conclusion::Success.to_string();
            f.completed_at = Some(Utc::now());
        }
        Err(e) => {
            // Save the error to the logs.
            f.logs = format!("{}\n\n{:?}", f.logs, e).trim().to_string();
            f.conclusion = octorust::types::Conclusion::Failure.to_string();
            f.completed_at = Some(Utc::now());

            bail!("action failed: {:#?}", e);
        }
    }

    f.update(db).await?;

    Ok(())
}
pub async fn run_cmd(
    db: &Database,
    sec: &steno::SecClient,
    id: &uuid::Uuid,
    cmd_name: &str,
    background: bool,
) -> Result<()> {
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

    do_saga(db, sec, id, builder.build(), cmd_name, background).await
}
async fn action_run_cmd(action_context: steno::ActionContext<Saga>) -> Result<FnOutput, steno::ActionError> {
    let db = &action_context.user_data().db;
    let cmd_name = &action_context.saga_params().cmd_name;
    let saga_id = &action_context.saga_params().saga_id;

    // Scope a new logger for this command.
    let result = slog_scope::scope(
        &slog_scope::logger().new(slog::slog_o!("cmd" => cmd_name.to_string(), "saga_id" => saga_id.to_string())),
        || async {
            // Execute the function within the scope of the logger.
            // Print the error and return an ActionError.
            match reexec(db, cmd_name, saga_id).await {
                Ok(s) => Ok(FnOutput(s)),
                Err(err) => {
                    // Return an action error but include the logs.
                    // Format the anyhow error with a stack trace.
                    Err(steno::ActionError::action_failed(format!("ERROR:\n\n{:?}", err)))
                }
            }
        },
    )
    .await;

    result
}

// We re-exec our current binary so we can get the best log output.
// The only downside is we are creating more connections to the database.
async fn reexec(db: &Database, cmd: &str, saga_id: &uuid::Uuid) -> Result<String> {
    let exe = env::current_exe()?;

    let child = duct::cmd!(exe, cmd);
    let reader = child.stderr_to_stdout().reader()?;

    let mut output = String::new();

    let out = BufReader::new(reader);

    let mut start = Instant::now();

    for line in out.lines() {
        match line {
            Ok(l) => {
                output.push_str(&l);
                output.push('\n');

                slog::info!(slog_scope::logger(), "{}", l);

                // Only save the logs when we have time, just do it async and don't
                // wait on it, else we will be waiting forever.
                // Update our start time after saving.
                if start.elapsed() > Duration::from_secs(15) {
                    // Save the logs.
                    Function::add_logs(db, saga_id, &output).await?;

                    // Reset our start time to now.
                    start = Instant::now();
                }
            }
            Err(e) => {
                // Save the logs.
                Function::add_logs_with_conclusion(db, saga_id, &output, &octorust::types::Conclusion::Failure).await?;

                bail!(e);
            }
        }
    }

    // We do this here because sometimes the saga fails to update.
    Function::add_logs_with_conclusion(db, saga_id, &output, &octorust::types::Conclusion::Success).await?;
    Ok(output)
}
