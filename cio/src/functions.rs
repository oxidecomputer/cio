use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use chrono_humanize::HumanTime;
use macros::db;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use slack_chat_api::{
    FormattedMessage, MessageAttachment, MessageBlock, MessageBlockAccessory, MessageBlockText, MessageBlockType,
    MessageType,
};

use crate::{
    airtable::AIRTABLE_FUNCTIONS_TABLE, companies::Company, core::UpdateAirtableRecord, db::Database,
    schema::functions, utils::truncate,
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

fn get_color_based_from_status_and_conclusion(status: &str, conclusion: &str) -> String {
    if status == octorust::types::JobStatus::InProgress.to_string() {
        return crate::colors::Colors::Blue.to_string();
    }

    if status == octorust::types::JobStatus::Completed.to_string() && conclusion.is_empty() {
        return crate::colors::Colors::Yellow.to_string();
    }

    if status == octorust::types::JobStatus::Completed.to_string()
        && conclusion == octorust::types::Conclusion::Success.to_string()
    {
        return crate::colors::Colors::Green.to_string();
    }

    if status == octorust::types::JobStatus::Completed.to_string()
        && (conclusion == octorust::types::Conclusion::TimedOut.to_string()
            || conclusion == octorust::types::Conclusion::Failure.to_string())
    {
        return crate::colors::Colors::Red.to_string();
    }

    crate::colors::Colors::Yellow.to_string()
}

/// Convert the function into a Slack message.
impl From<NewFunction> for FormattedMessage {
    fn from(item: NewFunction) -> Self {
        let dur = Utc::now() - item.created_at;
        let human_date = HumanTime::from(dur);

        let text = format!("Function | `{}`", item.name);

        let mut context = format!("*{}*", item.status);
        if !item.conclusion.is_empty() {
            context += &format!(" | *{}*", item.conclusion);
        }
        context += &format!(" | _created {}_", human_date);
        if let Some(c) = item.completed_at {
            let dur = Utc::now() - c;
            let human_date = HumanTime::from(dur);

            context += &format!(" | _completed {}_", human_date);
        }

        let mut blocks = vec![
            MessageBlock {
                block_type: MessageBlockType::Section,
                text: Some(MessageBlockText {
                    text_type: MessageType::Markdown,
                    text,
                }),
                elements: Default::default(),
                accessory: Default::default(),
                block_id: Default::default(),
                fields: Default::default(),
            },
            MessageBlock {
                block_type: MessageBlockType::Context,
                elements: vec![slack_chat_api::BlockOption::MessageBlockText(MessageBlockText {
                    text_type: MessageType::Markdown,
                    text: context,
                })],
                text: Default::default(),
                accessory: Default::default(),
                block_id: Default::default(),
                fields: Default::default(),
            },
        ];

        if item.status == octorust::types::JobStatus::Completed.to_string()
            && item.conclusion != octorust::types::Conclusion::Success.to_string()
        {
            // Add a button to rerun the function.
            let button = MessageBlockAccessory {
                accessory_type: MessageType::Button,
                text: Some(MessageBlockText {
                    text_type: MessageType::PlainText,
                    text: format!("Re-run {}", item.name),
                }),
                action_id: "function".to_string(),
                value: item.name.to_string(),
                image_url: Default::default(),
                alt_text: Default::default(),
            };

            blocks[0].accessory = Some(button);

            let logs = MessageBlock {
                block_type: MessageBlockType::Context,
                elements: vec![slack_chat_api::BlockOption::MessageBlockText(MessageBlockText {
                    text_type: MessageType::PlainText,
                    // We can only send max 3000 chars.
                    text: crate::utils::tail(&item.logs, 3000),
                })],
                text: Default::default(),
                accessory: Default::default(),
                block_id: Default::default(),
                fields: Default::default(),
            };

            blocks.push(logs);
        }

        FormattedMessage {
            channel: Default::default(),
            blocks: Default::default(),
            attachments: vec![MessageAttachment {
                color: get_color_based_from_status_and_conclusion(&item.status, &item.conclusion),
                author_icon: Default::default(),
                author_link: Default::default(),
                author_name: Default::default(),
                fallback: Default::default(),
                fields: Default::default(),
                footer: Default::default(),
                footer_icon: Default::default(),
                image_url: Default::default(),
                pretext: Default::default(),
                text: Default::default(),
                thumb_url: Default::default(),
                title: Default::default(),
                title_link: Default::default(),
                ts: Default::default(),
                blocks,
            }],
        }
    }
}

impl From<Function> for FormattedMessage {
    fn from(item: Function) -> Self {
        let new: NewFunction = item.into();
        new.into()
    }
}

impl NewFunction {
    // Send a slack notification to the channels in the object.
    pub async fn send_slack_notification(&self, db: &Database, company: &Company) -> Result<()> {
        let mut msg: FormattedMessage = self.clone().into();

        // Set the channel.
        msg.channel = company.slack_channel_debug.to_string();

        // Post the message.
        company.post_to_slack_channel(db, &msg).await?;

        Ok(())
    }
}

impl Function {
    pub async fn send_slack_notification(&self, db: &Database, company: &Company) -> Result<()> {
        let n: NewFunction = self.into();
        n.send_slack_notification(db, company).await
    }

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

        let company = nf.company(db)?;
        nf.send_slack_notification(db, &company).await?;

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

        let mut send_notification = false;
        if status == octorust::types::JobStatus::Completed && nf.completed_at.is_none() {
            send_notification = true;
            nf.completed_at = Some(Utc::now());
        }

        // Update the status.
        nf.status = status.to_string();

        let new = nf.update(db).await?;

        if send_notification {
            let company = new.company(db)?;
            new.send_slack_notification(db, &company).await?;
        }

        Ok(new)
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
    let company = Company::get_by_id(&db, 1)?;

    let hours_ago = Utc::now().checked_sub_signed(chrono::Duration::days(1)).unwrap();

    // List all functions that are not "Completed".
    let fns = functions::dsl::functions
        .filter(functions::dsl::status.eq(octorust::types::JobStatus::InProgress.to_string()))
        .filter(functions::dsl::created_at.lt(hours_ago))
        .load::<Function>(&db.conn())?;

    for mut f in fns {
        // Set the function as TimedOut.
        f.status = octorust::types::JobStatus::Completed.to_string();
        f.conclusion = octorust::types::Conclusion::TimedOut.to_string();

        let new = f.update(&db).await?;

        new.send_slack_notification(&db, &company).await?;
    }

    // List all functions that are "Completed", but have no conclusion.
    let fns = functions::dsl::functions
        .filter(functions::dsl::status.eq(octorust::types::JobStatus::Completed.to_string()))
        .filter(functions::dsl::conclusion.eq("".to_string()))
        .load::<Function>(&db.conn())?;

    for mut f in fns {
        // Set the function as Neutral.
        f.conclusion = octorust::types::Conclusion::Neutral.to_string();

        let new = f.update(&db).await?;

        new.send_slack_notification(&db, &company).await?;
    }

    Functions::get_from_db(&db, 1)?.update_airtable(&db).await?;

    Ok(())
}
