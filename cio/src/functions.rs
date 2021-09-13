use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use macros::db;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    airtable::{AIRTABLE_FUNCTIONS_TABLE, AIRTABLE_GRID_VIEW},
    companies::Company,
    core::UpdateAirtableRecord,
    db::Database,
    schema::functions,
    utils::truncate,
};

#[db {
    new_struct_name = "Function",
    airtable_base = "cio",
    airtable_table = "AIRTABLE_FUNCTIONS_TABLE",
    match_on = {
        "saga_id" = "String",
    },
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[table_name = "functions"]
pub struct NewFunction {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub status: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub conclusion: String,
    pub created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub logs: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub saga_id: String,

    /// The CIO company ID.
    #[serde(default)]
    pub cio_company_id: i32,
}

/// Implement updating the Airtable record for a Function.
#[async_trait]
impl UpdateAirtableRecord<Function> for Function {
    async fn update_airtable_record(&mut self, _record: Function) -> Result<()> {
        self.logs = truncate(&self.logs, 100000);
        Ok(())
    }
}

impl Function {
    /// Add logs to a running saga.
    pub async fn add_logs(db: &Database, saga_id: &uuid::Uuid, logs: &str) -> Result<()> {
        if logs.is_empty() {
            // Return early.
            return Ok(());
        }

        // Get the saga from it's id.
        let mut nf = Function::get_from_db(db, saga_id.to_string()).unwrap();
        nf.logs = logs.to_string();
        nf.update(db).await?;

        Ok(())
    }

    /// Add logs with a conclusion saga.
    pub async fn add_logs_with_conclusion(
        db: &Database,
        saga_id: &uuid::Uuid,
        logs: &str,
        conclusion: &octorust::types::Conclusion,
    ) -> Result<()> {
        if logs.is_empty() {
            // Return early.
            return Ok(());
        }

        // Get the saga from it's id.
        let mut nf = Function::get_from_db(db, saga_id.to_string()).unwrap();
        nf.logs = logs.to_string();
        nf.conclusion = conclusion.to_string();
        nf.update(db).await?;

        Ok(())
    }

    /// Update a job from SagaCreateParams.
    pub async fn from_saga_create_params(db: &Database, saga: &steno::SagaCreateParams) -> Result<Self> {
        let status = match saga.state {
            steno::SagaCachedState::Running => octorust::types::JobStatus::InProgress,
            steno::SagaCachedState::Unwinding => octorust::types::JobStatus::InProgress,
            steno::SagaCachedState::Done => octorust::types::JobStatus::Completed,
        };

        let nf = NewFunction {
            name: saga.template_name.to_string(),
            status: status.to_string(),
            conclusion: octorust::types::Conclusion::Noop.to_string(),
            created_at: Utc::now(),
            completed_at: None,
            logs: "".to_string(),
            saga_id: saga.id.to_string(),
            cio_company_id: 1, // This is always 1 because these are meta and tied to Oxide.
        };

        nf.upsert(db).await
    }

    /// Update a job from SagaCachedState.
    pub async fn from_saga_cached_state(
        db: &Database,
        saga_id: &steno::SagaId,
        state: &steno::SagaCachedState,
    ) -> Result<Self> {
        // Get the saga from it's id.
        let mut nf = Function::get_from_db(db, saga_id.to_string()).unwrap();

        let status = match state {
            steno::SagaCachedState::Running => octorust::types::JobStatus::InProgress,
            steno::SagaCachedState::Unwinding => octorust::types::JobStatus::InProgress,
            steno::SagaCachedState::Done => octorust::types::JobStatus::Completed,
        };

        if status == octorust::types::JobStatus::Completed && nf.completed_at.is_none() {
            nf.completed_at = Some(Utc::now());
        }

        // Update the status.
        nf.status = status.to_string();

        nf.update(db).await
    }

    /// Update a job from SagaNodeEvent.
    pub async fn from_saga_node_event(db: &Database, event: &steno::SagaNodeEvent) -> Result<Self> {
        // Get the saga from it's id.
        let nf = Function::get_from_db(db, event.saga_id.to_string()).unwrap();

        match &event.event_type {
            steno::SagaNodeEventType::Started => {}
            steno::SagaNodeEventType::Succeeded(_s) => {}
            steno::SagaNodeEventType::Failed(_err) => {}
            steno::SagaNodeEventType::UndoStarted => (),
            steno::SagaNodeEventType::UndoFinished => (),
        }

        //nf.update(db).await
        Ok(nf)
    }
}

pub async fn refresh_functions() -> Result<()> {
    let db = Database::new();

    // This should forever only be Oxide.
    let oxide = Company::get_from_db(&db, "Oxide".to_string()).unwrap();

    let hours_ago = Utc::now().checked_sub_signed(chrono::Duration::days(1)).unwrap();

    // List all functions that are not "Completed".
    let fns = functions::dsl::functions
        .filter(functions::dsl::status.eq(octorust::types::JobStatus::InProgress.to_string()))
        .filter(functions::dsl::created_at.lt(hours_ago))
        .load::<Function>(&db.conn())?;

    for (mut f) in fns {
        // Set the function as TimedOut.
        f.status = octorust::types::JobStatus::Completed.to_string();
        f.conclusion = octorust::types::Conclusion::TimedOut.to_string();

        f.update(&db).await?;
    }

    Ok(())
}
