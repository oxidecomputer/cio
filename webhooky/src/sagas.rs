use std::sync::Arc;
use std::io;
use std::sync::Mutex;

use anyhow::Result;
use cio_api::{
    db::Database,
    functions::{FnOutput, Function},
};
use serde::{Deserialize, Serialize};
use slog_scope_futures::FutureExt as _;
use slog::Drain;

use crate::health::SelfMemory;

#[derive(Debug, Clone)]
struct SagaLogOutput {
    output: Arc<Mutex<Vec<u8>>>,
}

impl SagaLogOutput {
    pub fn new() -> Self {
        Self { output: Arc::new(Mutex::new(Vec::new())) }
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

fn create_saga_logger<W>(out: W, cmd_name: String, saga_id: String) -> slog::Logger where W: io::Write + Send + Sync + 'static {
    let drain = slog_async::Async::new(
        slog::Duplicate::new(
            slog_json::Json::new(out).add_default_keys().build().fuse(),
            slog_json::Json::new(std::io::stdout()).add_default_keys().build().fuse(),
        ).fuse()
    ).build().fuse();

    let drain = sentry::integrations::slog::SentryDrain::new(drain);
    slog::Logger::root(drain, slog::slog_o!("cmd" => cmd_name, "saga_id" => saga_id))
}

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
                }.unwrap_or_else(String::new)
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
}

struct AsActionError(anyhow::Error);

impl From<AsActionError> for steno::ActionError {
    fn from(err: AsActionError) -> Self {
        steno::ActionError::action_failed(format!("ERROR:\n\n{:?}", err.0))
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use super::*;

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

        let lines = records.split('\n').into_iter().filter(|s| s.len() != 0).map(|s| {
            serde_json::from_str::<Line>(s).unwrap()
        }).collect::<Vec<_>>();

        assert_eq!(2, lines.len());

        assert_eq!("First message that should be available from the handle", lines[0].msg);
        assert_eq!("test_cmd", lines[0].cmd);
        assert_eq!("not-a-real-uuid", lines[0].saga_id);

        assert_eq!("Second message that should be available from the handle", lines[1].msg);
        assert_eq!("test_cmd", lines[1].cmd);
        assert_eq!("not-a-real-uuid", lines[1].saga_id);
    }
}