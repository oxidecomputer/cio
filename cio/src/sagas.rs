use std::sync::Arc;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::{companies::Company, db::Database};

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

async fn undo_action(_action_context: steno::ActionContext<Saga>) -> Result<()> {
    // This is a noop, we don't have to undo anything.
    Ok(())
}

/// Create a new saga with the given parameters and then execute it.
pub async fn sync_repos(db: Database, sec: steno::SecClient, id: uuid::Uuid, company: &Company) -> Result<()> {
    let context = Arc::new(Context { db });
    let params = Params {
        company: company.clone(),
    };

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
    let saga_template = Arc::new(builder.build());

    let saga_id = steno::SagaId(id);

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

    // Print the results.
    if let Err(error) = result.kind {
        println!("action failed: {}", error.error_node_name);
        println!("error: {}", error.error_source);
    }

    Ok(())
}

async fn action_sync_all_repo_settings(action_context: steno::ActionContext<Saga>) -> Result<(), steno::ActionError> {
    let context = action_context.user_data();
    let company = &action_context.saga_params().company;
    let github = company.authenticate_github().unwrap();
    crate::repos::sync_all_repo_settings(&context.db, &github, company)
        .await
        .unwrap();
    Ok(())
}

async fn action_refresh_db_github_repos(action_context: steno::ActionContext<Saga>) -> Result<(), steno::ActionError> {
    let context = action_context.user_data();
    let company = &action_context.saga_params().company;
    let github = company.authenticate_github().unwrap();
    crate::repos::refresh_db_github_repos(&context.db, &github, company)
        .await
        .unwrap();

    Ok(())
}
