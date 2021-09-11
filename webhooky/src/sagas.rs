use std::{
    env, fmt,
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
    sync::Arc,
};

use anyhow::{bail, Result};
use chrono::Utc;
use cio_api::{db::Database, functions::Function};
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

#[derive(Debug, Deserialize, Serialize)]
pub struct FnOutput(String);

impl fmt::Display for FnOutput {
    // This trait requires `fmt` with this exact signature.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // Write strictly the first element into the supplied output
        // stream: `f`. Returns `fmt::Result` which indicates whether the
        // operation succeeded or failed. Note that `write!` uses syntax which
        // is very similar to `println!`.
        write!(f, "{}", self.0)
    }
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
    let saga_future = sec
        .saga_create(saga_id, Arc::new(context), saga_template, cmd_name.to_string(), params)
        .await?;

    // Set it running.
    sec.saga_start(saga_id).await?;

    //
    // Wait for the saga to finish running.  This could take a while, depending
    // on what the saga does!  This traverses the DAG of actions, executing each
    // one.  If one fails, then it's all unwound: any actions that previously
    // completed will be undone.
    //
    // Note that the SEC will run all this regardless of whether you wait for it
    // here.  This is just a handle for you to know when the saga has finished.
    //
    let result = saga_future.await;

    // Get the function.
    let mut f = Function::get_from_db(db, saga_id.to_string()).unwrap();

    // Print the results.
    match result.kind {
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
            f.logs = format!("{:?}", e);
            f.conclusion = octorust::types::Conclusion::Failure.to_string();
            f.completed_at = Some(Utc::now());

            bail!("action failed: {:#?}", e);
        }
    }

    f.update(db).await?;

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
}

// We re-exec our current binary so we can get the best log output.
// The only downside is we are creating more connections to the database.
async fn reexec(db: &Database, cmd: &str, saga_id: &uuid::Uuid) -> Result<String> {
    let exe = env::current_exe()?;

    // TODO, also pipe the logs to our logger but somehow nest them
    // or make it apparent its a child.
    let mut child = Command::new(exe)
        .args([cmd])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let mut output = Vec::new();

    // TODO: Should be moved to a function that accepts something implementing `Write`.
    async {
        let stdout = child.stdout.as_mut().expect("wasn't stdout");
        let stderr = child.stderr.as_mut().expect("wasn't stderr");

        let mut stdout = BufReader::new(stdout);
        let mut stderr = BufReader::new(stderr);

        loop {
            let (stdout_bytes, stderr_bytes) = match (stdout.fill_buf(), stderr.fill_buf()) {
                (Ok(stdout), Ok(stderr)) => {
                    output.write_all(stdout)?;
                    output.write_all(stderr)?;

                    // Save the new output to the database.
                    // TODO: we might want to buffer this so it's not so many requests.
                    Function::add_logs(db, saga_id, &output).await?;

                    (stdout.len(), stderr.len())
                }
                other => bail!("got some other pipe than stdout and stderr {:?}", other),
            };

            if stdout_bytes == 0 && stderr_bytes == 0 {
                // TODO: Seems less-than-ideal; should be some way of
                // telling if the child has actually exited vs just
                // not outputting anything.
                return Ok(());
            }

            stdout.consume(stdout_bytes);
            stderr.consume(stderr_bytes);
        }
    }
    .await?;

    let status = child.wait()?;

    // Format the output as a string.
    let s = String::from_utf8(output)?.trim().to_string();

    if status.success() {
        Ok(s)
    } else {
        bail!(s)
    }
}
