use std::{env, fmt, fs, sync::Arc};

use anyhow::{bail, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use slog::Drain;

use crate::{companies::Company, db::Database, functions::Function};

/// Define our saga for syncing repos.
#[derive(Debug)]
pub struct Saga;

#[derive(Debug, Deserialize, Serialize)]
pub struct Params {
    pub company: Company,
}

#[derive(Debug)]
pub struct Context {
    pub db: Database,
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
    company: &Company,
    template: steno::SagaTemplate<Saga>,
    fns: Vec<String>,
) -> Result<()> {
    let context = Arc::new(Context { db: db.clone() });
    let params = Params {
        company: company.clone(),
    };

    let saga_template = Arc::new(template);

    let saga_id = steno::SagaId(*id);

    // Create the saga.
    let saga_future = sec
        .saga_create(
            saga_id,
            Arc::new(context),
            saga_template,
            "sync-repos".to_string(),
            params,
        )
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
            let mut logs = String::new();
            for func in fns {
                // Save the success output to the logs.
                // For each function.
                let log = s.lookup_output::<FnOutput>(&func).unwrap();
                logs = format!("{}\n\nOUTPUT `{}`:\n\n{}", logs, func, log);
            }

            f.logs = logs.trim().to_string();
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

async fn action_sync_all_repo_settings(
    action_context: steno::ActionContext<Saga>,
) -> Result<FnOutput, steno::ActionError> {
    let context = action_context.user_data();
    let company = &action_context.saga_params().company;
    let github = company.authenticate_github().unwrap();

    // Create a temp file for our logs.
    let mut log_path = env::temp_dir();
    log_path.push(format!("{}.log", uuid::Uuid::new_v4().to_string()));
    let file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&log_path)
        .unwrap();

    // Create the logger.
    let decorator = slog_term::PlainDecorator::new(file);
    let drain = slog_term::FullFormat::new(decorator).build().fuse();
    let drain = slog_async::Async::new(drain).build().fuse();
    // TODO consider adding sentry as a drain here?
    // Unsure if that will duplicate the logs tho.
    let logger = slog::Logger::root(drain, slog::slog_o!("version" => env!("CARGO_PKG_VERSION")));

    let get_logs = || {
        let s = fs::read_to_string(&log_path).unwrap();

        s
    };

    if let Err(e) = slog_scope::scope(&logger, || async {
        let _log_guard = slog_stdlog::init_with_level(log::Level::Info).unwrap();

        // Execute the function within the scope of the logger.
        // TODO: print the error and return an ActionError.
        match crate::repos::sync_all_repo_settings(&context.db, &github, company).await {
            Ok(_) => Ok(()),
            Err(err) => {
                let s = get_logs();

                // Return an action error but include the logs.
                // Format the anyhow error with a stack trace.
                return Err(steno::ActionError::action_failed(format!(
                    "ERROR:\n\n{:?}\n\n===========\n\nLOGS:\n\n{}",
                    err, s
                )));
            }
        }
    })
    .await
    {
        // Delete our temporary file.
        fs::remove_file(&log_path).unwrap();

        return Err(e);
    }

    let s = get_logs();

    // Delete our temporary file.
    fs::remove_file(&log_path).unwrap();

    Ok(FnOutput(s))
}

async fn action_refresh_db_github_repos(
    action_context: steno::ActionContext<Saga>,
) -> Result<FnOutput, steno::ActionError> {
    let context = action_context.user_data();
    let company = &action_context.saga_params().company;
    let github = company.authenticate_github().unwrap();

    crate::repos::refresh_db_github_repos(&context.db, &github, company)
        .await
        .unwrap();

    Ok(FnOutput(String::new()))
}

pub async fn sync_repos(db: &Database, sec: &steno::SecClient, id: &uuid::Uuid, company: &Company) -> Result<()> {
    let mut fns: Vec<String> = Default::default();

    let mut builder = steno::SagaTemplateBuilder::new();
    builder.append(
        // name of this action's output (can be used in subsequent actions)
        "sync_all_repo_settings",
        // human-readable label for the action
        "SyncAllRepoSettings",
        steno::ActionFunc::new_action(
            // action function
            action_sync_all_repo_settings,
            // undo function
            undo_action,
        ),
    );
    fns.push("sync_all_repo_settings".to_string());

    builder.append(
        // name of this action's output (can be used in subsequent actions)
        "refresh_db_github_repos",
        // human-readable label for the action
        "RefreshDBGitHubRepos",
        steno::ActionFunc::new_action(
            // action function
            action_refresh_db_github_repos,
            // undo function
            undo_action,
        ),
    );
    fns.push("refresh_db_github_repos".to_string());

    do_saga(db, sec, id, company, builder.build(), fns).await
}