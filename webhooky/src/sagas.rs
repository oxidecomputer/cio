use std::io;
use std::sync::Arc;
use std::sync::Mutex;

use anyhow::Result;
use cio_api::{
    db::Database,
    functions::{FnOutput, Function},
};
use lazy_static::lazy_static;
use log::info;
use serde::{Deserialize, Serialize};
use slog::Drain;
use slog_scope_futures::FutureExt as _;

use crate::health::SelfMemory;

#[derive(Debug, Clone)]
struct SagaLogOutput {
    output: Arc<Mutex<Vec<u8>>>,
}

impl SagaLogOutput {
    pub fn new() -> Self {
        Self {
            output: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn handle(&self) -> Arc<Mutex<Vec<u8>>> {
        self.output.clone()
    }
}

impl io::Write for SagaLogOutput {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut out = self.output.lock().unwrap();
        out.extend(buf);

        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn create_saga_logger<W>(out: W, cmd_name: String, saga_id: String) -> slog::Logger
where
    W: io::Write + Send + Sync + 'static,
{
    let drain = slog_async::Async::new(
        slog::Duplicate::new(
            slog_json::Json::new(out).add_default_keys().build().fuse(),
            slog_json::Json::new(std::io::stdout())
                .add_default_keys()
                .build()
                .fuse(),
        )
        .fuse(),
    )
    .build()
    .fuse();

    slog::Logger::root(drain, slog::slog_o!("cmd" => cmd_name, "saga_id" => saga_id))
}

/// Define our saga for syncing repos.
#[derive(Debug)]
pub struct Saga;

#[derive(Clone, Debug, Deserialize, Serialize)]
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
    // type SagaParamsType = Params;

    // Type for the application-specific context (see above)
    type ExecContextType = Arc<Context>;
}

lazy_static! {
    static ref EXEC_CMD: Arc<dyn steno::Action<Saga>> = steno::new_action_noop_undo("exec", action_run_cmd);
}

pub fn create_registry() -> steno::ActionRegistry<Saga> {
    let mut registry = steno::ActionRegistry::<Saga>::new();
    registry.register(EXEC_CMD.clone());

    registry
}

pub async fn run_cmd(
    db: &Database,
    sec: &steno::SecClient,
    registry: Arc<steno::ActionRegistry<Saga>>,
    id: &uuid::Uuid,
    cmd_name: &str,
) -> Result<()> {
    let params = Params {
        cmd_name: cmd_name.to_string(),
        saga_id: *id,
    };

    let mut builder = steno::DagBuilder::new(steno::SagaName::new(cmd_name));
    builder.append(steno::Node::action(cmd_name, cmd_name, EXEC_CMD.as_ref()));

    let dag = Arc::new(steno::SagaDag::new(
        builder.build().expect("Failed to build DAG for execution saga"),
        serde_json::to_value(&params).unwrap(),
    ));

    let context = Arc::new(Context { db: db.clone() });
    let saga_id = steno::SagaId(params.saga_id);

    // Create the saga.
    let saga = sec.saga_create(saga_id, Arc::new(context), dag, registry).await?;

    // Set it running.
    sec.saga_start(saga_id).await?;

    // Listen for the saga to complete
    tokio::spawn(async {
        let result = saga.await;
        info!("Saga completed {:?}", result);
    });

    Ok(())
}

async fn action_run_cmd(action_context: steno::ActionContext<Saga>) -> Result<FnOutput, steno::ActionError> {
    let db = &action_context.user_data().db;
    let cmd_name = &action_context.saga_params::<Params>()?.cmd_name;
    let saga_id = &action_context.saga_params::<Params>()?.saga_id;

    if let Some(sub_cmd) = crate::core::into_job_command(cmd_name) {
        if let Ok(mem) = SelfMemory::new() {
            log::info!("Memory before running {}({}): {:?}", cmd_name, saga_id, mem);
        }

        let saga_log_output = SagaLogOutput::new();
        let output_handle = saga_log_output.handle();
        let logger = create_saga_logger(saga_log_output, cmd_name.to_string(), saga_id.to_string());

        let context = crate::context::Context::new(1).await.map_err(AsActionError)?;
        let result = crate::job::run_job_cmd(sub_cmd, context).with_logger(logger).await;

        if let Ok(mem) = SelfMemory::new() {
            log::info!("Memory after running {}({}): {:?}", cmd_name, saga_id, mem);
        }

        match result {
            Ok(_) => {
                let output = {
                    if let Ok(guard) = output_handle.lock() {
                        std::str::from_utf8(&guard).ok().map(|s| s.to_string())
                    } else {
                        None
                    }
                    .unwrap_or_default()
                };

                Function::add_logs_with_conclusion(db, saga_id, &output, &octorust::types::Conclusion::Success)
                    .await
                    .map_err(AsActionError)?;
                Ok(FnOutput(output))
            }
            Err(err) => {
                let output = format!("{:?}", err);
                Function::add_logs_with_conclusion(db, saga_id, &output, &octorust::types::Conclusion::Failure)
                    .await
                    .map_err(AsActionError)?;
                Err(AsActionError(err).into())
            }
        }
    } else {
        Err(steno::ActionError::action_failed(format!(
            "ERROR:\n\n Failed to determine job to run for {}",
            cmd_name
        )))
    }
}

struct AsActionError(anyhow::Error);

impl From<AsActionError> for steno::ActionError {
    fn from(err: AsActionError) -> Self {
        steno::ActionError::action_failed(format!("ERROR:\n\n{:?}", err.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_write_saga_output() {
        let mut output = SagaLogOutput::new();
        output.write(&[1, 2, 3]);

        assert_eq!(vec![1, 2, 3], output.handle().lock().unwrap().clone());
    }

    #[test]
    fn test_saga_logger_output() {
        let output = SagaLogOutput::new();
        let handle = output.handle();
        let logger = create_saga_logger(output.clone(), "test_cmd".to_string(), "not-a-real-uuid".to_string());
        slog::info!(&logger, "First message that should be available from the handle");
        slog::info!(&logger, "Second message that should be available from the handle");

        // Drop the logger so that records flush
        drop(logger);

        let output: Vec<u8> = handle.lock().unwrap().clone();
        let records = String::from_utf8(output).unwrap();

        #[derive(Deserialize)]
        struct Line {
            msg: String,
            cmd: String,
            saga_id: String,
        }

        let lines = records
            .split('\n')
            .into_iter()
            .filter(|s| s.len() != 0)
            .map(|s| serde_json::from_str::<Line>(s).unwrap())
            .collect::<Vec<_>>();

        assert_eq!(2, lines.len());

        assert_eq!("First message that should be available from the handle", lines[0].msg);
        assert_eq!("test_cmd", lines[0].cmd);
        assert_eq!("not-a-real-uuid", lines[0].saga_id);

        assert_eq!("Second message that should be available from the handle", lines[1].msg);
        assert_eq!("test_cmd", lines[1].cmd);
        assert_eq!("not-a-real-uuid", lines[1].saga_id);
    }
}
