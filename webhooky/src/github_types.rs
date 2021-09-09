use std::collections::BTreeMap;

use anyhow::Result;
use chrono::{offset::Utc, DateTime};
use cio_api::{
    repos::GitHubUser,
    rfds::{GitHubCommit, GitHubPullRequest},
};
use log::warn;
use octorust::Client as GitHub;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A GitHub organization.
#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct GitHubOrganization {
    pub login: String,
    pub id: u64,
    pub url: String,
    pub repos_url: String,
    pub events_url: String,
    pub hooks_url: String,
    pub issues_url: String,
    pub members_url: String,
    pub public_members_url: String,
    pub avatar_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

/// A GitHub app installation.
#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct GitHubInstallation {
    #[serde(default)]
    pub id: i64,
    // account: Account
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub access_tokens_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub repositories_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub html_url: String,
    #[serde(default)]
    pub app_id: i32,
    #[serde(default)]
    pub target_id: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub target_type: String,
    // permissions: Permissions
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<String>,
    // created_at, updated_at
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub single_file_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub repository_selection: String,
}

/// A GitHub webhook event.
/// FROM: https://docs.github.com/en/free-pro-team@latest/developers/webhooks-and-events/webhook-events-and-payloads
#[derive(Debug, Clone, JsonSchema, Deserialize, Serialize)]
pub struct GitHubWebhook {
    /// Most webhook payloads contain an action property that contains the
    /// specific activity that triggered the event.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub action: String,
    /// The user that triggered the event. This property is included in
    /// every webhook payload.
    #[serde(default)]
    pub sender: GitHubUser,
    /// The `repository` where the event occurred. Webhook payloads contain the
    /// `repository` property when the event occurs from activity in a repository.
    #[serde(default)]
    pub repository: GitHubRepo,
    /// Webhook payloads contain the `organization` object when the webhook is
    /// configured for an organization or the event occurs from activity in a
    /// repository owned by an organization.
    #[serde(default)]
    pub organization: GitHubOrganization,
    /// The GitHub App installation. Webhook payloads contain the `installation`
    /// property when the event is configured for and sent to a GitHub App.
    #[serde(default)]
    pub installation: GitHubInstallation,

    /// `push` event fields.
    /// FROM: https://docs.github.com/en/free-pro-team@latest/developers/webhooks-and-events/webhook-events-and-payloads#push
    ///
    /// The full `git ref` that was pushed. Example: `refs/heads/main`.
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "ref",
        deserialize_with = "octorust::utils::deserialize_null_string::deserialize"
    )]
    pub refv: String,
    /// The SHA of the most recent commit on `ref` before the push.
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "octorust::utils::deserialize_null_string::deserialize"
    )]
    pub before: String,
    /// The SHA of the most recent commit on `ref` after the push.
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "octorust::utils::deserialize_null_string::deserialize"
    )]
    pub after: String,
    /// An array of commit objects describing the pushed commits.
    /// The array includes a maximum of 20 commits. If necessary, you can use
    /// the Commits API to fetch additional commits. This limit is applied to
    /// timeline events only and isn't applied to webhook deliveries.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commits: Vec<GitHubCommit>,

    /// `pull_request` event fields.
    /// FROM: https://docs.github.com/en/free-pro-team@latest/developers/webhooks-and-events/webhook-events-and-payloads#pull_request
    ///
    /// The pull request number.
    #[serde(default)]
    pub number: i64,
    /// The pull request itself.
    #[serde(default)]
    pub pull_request: GitHubPullRequest,

    /// `issues` event fields.
    /// FROM: https://docs.github.com/en/free-pro-team@latest/developers/webhooks-and-events/webhook-events-and-payloads#issues
    ///
    /// The issue itself.
    #[serde(default)]
    pub issue: GitHubIssue,

    /// `issue_comment` event fields.
    /// FROM: https://docs.github.com/en/free-pro-team@latest/developers/webhooks-and-events/webhook-events-and-payloads#issue_comment
    ///
    /// The comment itself.
    #[serde(default)]
    pub comment: GitHubComment,

    /// `check_suite` event fields.
    /// FROM: https://docs.github.com/en/free-pro-team@latest/developers/webhooks-and-events/webhook-events-and-payloads#check_suite
    ///
    /// The check suite itself.
    #[serde(default)]
    pub check_suite: GitHubCheckSuite,

    /// `check_run` event fields.
    /// FROM: https://docs.github.com/en/free-pro-team@latest/developers/webhooks-and-events/webhook-events-and-payloads#check_run
    ///
    /// The check run itself.
    #[serde(default)]
    pub check_run: GitHubCheckRun,
}

impl GitHubWebhook {
    // Returns the check_run id so we can update it later.
    pub async fn create_check_run(&self, github: &octorust::Client) -> Result<i64> {
        let sha = if self.pull_request.head.sha.is_empty() {
            self.pull_request.head.id.to_string()
        } else {
            self.pull_request.head.sha.to_string()
        };

        if sha.is_empty() {
            // Return early.
            return Ok(0);
        }

        match github
            .checks()
            .create(
                &self.repository.owner.login,
                &self.repository.name,
                &octorust::types::ChecksCreateRequest {
                    actions: vec![],
                    completed_at: None, // We have not completed the run yet, we are merely putting it as in-progress.
                    conclusion: None,   // We don't have a conclusion yet.
                    details_url: "".to_string(), // TODO: maybe let's provide one? with running logs?
                    external_id: "".to_string(), // We don't have these, but we should maybe?
                    head_sha: sha.to_string(), // Sha of the commit.
                    name: format!("CIO bot: {}", self.repository.name), // Name of the check.
                    output: None,       // We don't have any output yet.
                    started_at: Some(Utc::now()),
                    status: Some(octorust::types::JobStatus::InProgress),
                },
            )
            .await
        {
            Ok(check) => return Ok(check.id),
            Err(e) => {
                warn!("unable to create check run on pull request event: {}", e,);
            }
        }

        Ok(0)
    }

    pub fn get_error_string(&self, msg: &str, e: anyhow::Error) -> String {
        let err = format!(
            r#"{} failed:

```
{:?}
```

<details>
<summary>event:</summary>

```
{:#?}
```

</details>

cc @jessfraz"#,
            msg,
            e, // We use the {:?} debug output for the error so we get the stack as well.
            self,
        );

        // Send the error to sentry.
        sentry_anyhow::capture_anyhow(&e);

        err
    }

    // Updates the check run after it has completed.
    pub async fn update_check_run(
        &self,
        github: &octorust::Client,
        id: i64,
        message: &str,
        conclusion: octorust::types::ChecksCreateRequestConclusion,
    ) -> Result<()> {
        if id <= 0 {
            // Return early.
            return Ok(());
        }

        let sha = if self.pull_request.head.sha.is_empty() {
            self.pull_request.head.id.to_string()
        } else {
            self.pull_request.head.sha.to_string()
        };

        if sha.is_empty() {
            // Return early.
            return Ok(());
        }

        if let Err(e) = github
            .checks()
            .update(
                &self.repository.owner.login,
                &self.repository.name,
                id,
                &octorust::types::ChecksUpdateRequest {
                    actions: vec![],
                    completed_at: Some(Utc::now()),
                    conclusion: Some(conclusion),
                    details_url: "".to_string(), // TODO: maybe let's provide one? with running logs?
                    external_id: "".to_string(), // We don't have these, but we should maybe?
                    name: format!("CIO bot: {}", self.repository.name), // Name of the check.
                    output: Some(octorust::types::ChecksUpdateRequestOutput {
                        annotations: vec![],
                        images: vec![],
                        summary: message.to_string(),
                        text: String::new(),
                        title: format!("CIO bot: {}", self.repository.name),
                    }),
                    started_at: None, // Keep the original start time.
                    status: Some(octorust::types::JobStatus::Completed),
                },
            )
            .await
        {
            warn!("unable to update check run {} on pull request event: {}", id, e);
        }

        Ok(())
    }

    pub async fn create_comment(&self, github: &GitHub, comment: &str) -> Result<()> {
        if comment.is_empty() {
            // Return early.
            return Ok(());
        }

        if !self.commits.is_empty() {
            if let Some(commit) = self.commits.get(0) {
                let sha = if commit.sha.is_empty() {
                    commit.id.to_string()
                } else {
                    commit.sha.to_string()
                };

                if sha.is_empty() {
                    // Return early.
                    return Ok(());
                }

                if let Err(e) = cio_api::utils::add_comment_to_commit(
                    github,
                    &self.repository.owner.login,
                    &self.repository.name,
                    &sha,
                    comment,
                )
                .await
                {
                    warn!("unable to create comment `{}` on commit event: {}", comment, e);
                }
            }
        }

        // TODO: comment on pull request instead, etc.

        Ok(())
    }
}

impl From<GitHubWebhook> for BTreeMap<String, serde_json::Value> {
    fn from(from: GitHubWebhook) -> Self {
        let mut map: BTreeMap<String, serde_json::Value> = Default::default();
        map.insert("action".to_string(), json!(from.action));
        map.insert("sender".to_string(), json!(from.sender));
        map.insert("repository".to_string(), json!(from.repository));
        map.insert("organization".to_string(), json!(from.organization));
        map.insert("installation".to_string(), json!(from.installation));
        map.insert("ref".to_string(), json!(from.refv));
        map.insert("before".to_string(), json!(from.before));
        map.insert("after".to_string(), json!(from.after));
        map.insert("commits".to_string(), json!(from.commits));
        map.insert("number".to_string(), json!(from.number));
        map.insert("pull_request".to_string(), json!(from.pull_request));
        map.insert("issue".to_string(), json!(from.issue));
        map.insert("comment".to_string(), json!(from.comment));
        map.insert("check_suite".to_string(), json!(from.check_suite));
        map.insert("check_run".to_string(), json!(from.check_run));

        map
    }
}

/// A GitHub repository.
/// FROM: https://docs.github.com/en/free-pro-team@latest/developers/webhooks-and-events/webhook-events-and-payloads#push
#[derive(Debug, Clone, Default, PartialEq, JsonSchema, Deserialize, Serialize)]
pub struct GitHubRepo {
    #[serde(default)]
    pub owner: GitHubUser,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "octorust::utils::deserialize_null_string::deserialize"
    )]
    pub name: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "octorust::utils::deserialize_null_string::deserialize"
    )]
    pub full_name: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "octorust::utils::deserialize_null_string::deserialize"
    )]
    pub default_branch: String,
}

/// A octorust::Client issue.
/// FROM: https://docs.github.com/en/free-pro-team@latest/rest/reference/issues
#[derive(Debug, Default, Clone, PartialEq, JsonSchema, Deserialize, Serialize)]
pub struct GitHubIssue {
    #[serde(default)]
    pub id: i64,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "octorust::utils::deserialize_null_string::deserialize"
    )]
    pub url: String,
    pub labels_url: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "octorust::utils::deserialize_null_string::deserialize"
    )]
    pub comments_url: String,
    pub events_url: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "octorust::utils::deserialize_null_string::deserialize"
    )]
    pub html_url: String,
    #[serde(default)]
    pub number: i64,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "octorust::utils::deserialize_null_string::deserialize"
    )]
    pub state: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "octorust::utils::deserialize_null_string::deserialize"
    )]
    pub title: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "octorust::utils::deserialize_null_string::deserialize"
    )]
    pub body: String,
    #[serde(default)]
    pub user: GitHubUser,
    //#[serde(default, skip_serializing_if = "Vec::is_empty")]
    //pub labels: Vec<GitHubLabel>,
    #[serde(default)]
    pub assignee: GitHubUser,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub comments: i64,
    #[serde(default)]
    pub pull_request: GitHubPullRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<DateTime<Utc>>,
    /* pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,*/
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assignees: Vec<GitHubUser>,
}

/// A reference to a pull request.
#[derive(Debug, Default, Clone, PartialEq, JsonSchema, Deserialize, Serialize)]
pub struct GitHubPullRef {
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "octorust::utils::deserialize_null_string::deserialize"
    )]
    pub url: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "octorust::utils::deserialize_null_string::deserialize"
    )]
    pub html_url: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "octorust::utils::deserialize_null_string::deserialize"
    )]
    pub diff_url: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "octorust::utils::deserialize_null_string::deserialize"
    )]
    pub patch_url: String,
}

/// A octorust::Client comment.
/// FROM: https://docs.github.com/en/free-pro-team@latest/rest/reference/issues#comments
#[derive(Debug, Default, Clone, PartialEq, JsonSchema, Deserialize, Serialize)]
pub struct GitHubComment {
    #[serde(default)]
    pub id: i64,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "octorust::utils::deserialize_null_string::deserialize"
    )]
    pub url: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "octorust::utils::deserialize_null_string::deserialize"
    )]
    pub html_url: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "octorust::utils::deserialize_null_string::deserialize"
    )]
    pub body: String,
    #[serde(default)]
    pub user: GitHubUser,
    /* pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,*/
}

/// A GitHub check suite.
/// FROM: https://docs.github.com/en/free-pro-team@latest/rest/reference/checks#suites
#[derive(Debug, Default, Clone, PartialEq, JsonSchema, Deserialize, Serialize)]
pub struct GitHubCheckSuite {
    #[serde(default)]
    pub id: i64,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "octorust::utils::deserialize_null_string::deserialize"
    )]
    pub head_branch: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "octorust::utils::deserialize_null_string::deserialize"
    )]
    pub head_sha: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "octorust::utils::deserialize_null_string::deserialize"
    )]
    pub status: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "octorust::utils::deserialize_null_string::deserialize"
    )]
    pub conclusion: String,
    #[serde(default)]
    pub app: GitHubApp,
}

/// A GitHub app.
#[derive(Debug, Default, Clone, PartialEq, JsonSchema, Deserialize, Serialize)]
pub struct GitHubApp {
    pub id: i64,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "octorust::utils::deserialize_null_string::deserialize"
    )]
    pub name: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "octorust::utils::deserialize_null_string::deserialize"
    )]
    pub slug: String,
}

/// A GitHub check run.
/// FROM: https://docs.github.com/en/free-pro-team@latest/rest/reference/checks#get-a-check-run
#[derive(Debug, Default, Clone, PartialEq, JsonSchema, Deserialize, Serialize)]
pub struct GitHubCheckRun {
    #[serde(default)]
    pub id: i64,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "octorust::utils::deserialize_null_string::deserialize"
    )]
    pub head_sha: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "octorust::utils::deserialize_null_string::deserialize"
    )]
    pub status: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "octorust::utils::deserialize_null_string::deserialize"
    )]
    pub conclusion: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "octorust::utils::deserialize_null_string::deserialize"
    )]
    pub name: String,
    #[serde(default)]
    pub check_suite: GitHubCheckSuite,
    #[serde(default)]
    pub app: GitHubApp,
}
