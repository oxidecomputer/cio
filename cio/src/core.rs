use airtable_api::User as AirtableUser;
use anyhow::Result;
use async_trait::async_trait;
use chrono::{naive::NaiveDate, DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Define the trait for doing logic in updating Airtable.
#[async_trait]
pub trait UpdateAirtableRecord<T> {
    async fn update_airtable_record(&mut self, _: T) -> Result<()>;
}

/// The data type for customer interactions.
/// This is inline with our Airtable workspace.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CustomerInteraction {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Company")]
    pub company: Vec<String>,
    #[serde(rename = "Date")]
    pub date: NaiveDate,
    #[serde(rename = "Type")]
    pub meeting_type: String,
    #[serde(rename = "Phase")]
    pub phase: String,
    #[serde(default, rename = "People")]
    pub people: Vec<String>,
    #[serde(default, rename = "Oxide Folks")]
    pub oxide_folks: Vec<AirtableUser>,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "Link to Notes")]
    pub notes_link: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "Notes")]
    pub notes: String,
}

/// The data type for discussion topics.
/// This is inline with our Airtable workspace.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DiscussionTopic {
    #[serde(rename = "Topic", default)]
    pub topic: String,
    #[serde(default, rename = "Submitter")]
    pub submitter: AirtableUser,
    #[serde(rename = "Priority", skip_serializing_if = "String::is_empty", default)]
    pub priority: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "Notes")]
    pub notes: String,
    // Never modify this, it is a linked record.
    #[serde(rename = "Associated meetings")]
    pub associated_meetings: Vec<String>,
}

/// The data type for a meeting.
/// This is inline with our Airtable workspace for product huddle meetings, hardware
/// huddle meetings, etc.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Meeting {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    pub date: NaiveDate,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub week: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub action_items: String,
    // Never modify this, it is a linked record.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proposed_discussion: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub recording: String,
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        serialize_with = "airtable_api::user_format_as_array_of_strings::serialize",
        deserialize_with = "airtable_api::user_format_as_array_of_strings::deserialize"
    )]
    pub attendees: Vec<String>,
    #[serde(default)]
    pub reminder_email_sent: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub calendar_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub calendar_event_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub calendar_event_link: String,
    #[serde(default)]
    pub cancelled: bool,
}

/// The data type for sending reminders for meetings.
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct MeetingReminderEmailData {
    pub date: String,
    pub topics: Vec<DiscussionTopic>,
    pub huddle_name: String,
    pub time: String,
    pub email: String,
}

/// A GitHub pull request.
/// FROM: https://docs.github.com/en/free-pro-team@latest/rest/reference/pulls#get-a-pull-request
#[derive(Debug, Default, Clone, PartialEq, JsonSchema, Deserialize, Serialize)]
pub struct GitHubPullRequest {
    #[serde(default)]
    pub id: i64,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "octorust::utils::deserialize_null_string::deserialize"
    )]
    pub url: String,
    /// The HTML location of this pull request.
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
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "octorust::utils::deserialize_null_string::deserialize"
    )]
    pub issue_url: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "octorust::utils::deserialize_null_string::deserialize"
    )]
    pub commits_url: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "octorust::utils::deserialize_null_string::deserialize"
    )]
    pub review_comments_url: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "octorust::utils::deserialize_null_string::deserialize"
    )]
    pub review_comment_url: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "octorust::utils::deserialize_null_string::deserialize"
    )]
    pub comments_url: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "octorust::utils::deserialize_null_string::deserialize"
    )]
    pub statuses_url: String,
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
    /*pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,*/
    #[serde(default)]
    pub closed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub merged_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub head: GitHubCommit,
    #[serde(default)]
    pub base: GitHubCommit,
    // links
    #[serde(default)]
    pub user: crate::repos::GitHubUser,
    #[serde(default)]
    pub merged: bool,
}

impl From<octorust::types::PullRequestSimple> for GitHubPullRequest {
    fn from(item: octorust::types::PullRequestSimple) -> Self {
        GitHubPullRequest {
            id: item.id,
            url: item.url.to_string(),
            diff_url: item.diff_url.to_string(),
            issue_url: item.issue_url.to_string(),
            patch_url: item.patch_url.to_string(),
            comments_url: item.comments_url.to_string(),
            html_url: item.html_url.to_string(),
            commits_url: item.commits_url.to_string(),
            review_comments_url: item.review_comments_url.to_string(),
            review_comment_url: item.review_comment_url.to_string(),
            statuses_url: item.statuses_url.to_string(),
            number: item.number,
            state: item.state.to_string(),
            title: item.title.to_string(),
            body: item.body.to_string(),
            closed_at: item.closed_at,
            merged_at: item.merged_at,
            head: GitHubCommit {
                id: item.head.sha.to_string(),
                timestamp: None,
                label: item.head.label.to_string(),
                author: Default::default(),
                url: "".to_string(),
                distinct: true,
                added: vec![],
                modified: vec![],
                removed: vec![],
                message: "".to_string(),
                commit_ref: item.head.ref_.to_string(),
                sha: item.head.sha.to_string(),
            },
            base: GitHubCommit {
                id: item.base.sha.to_string(),
                timestamp: None,
                label: item.base.label.to_string(),
                author: Default::default(),
                url: "".to_string(),
                distinct: true,
                added: vec![],
                modified: vec![],
                removed: vec![],
                message: "".to_string(),
                commit_ref: item.base.ref_.to_string(),
                sha: item.base.sha.to_string(),
            },
            // TODO: actually do these.
            user: Default::default(),
            merged: item.merged_at.is_some(),
        }
    }
}

/// A GitHub commit.
/// FROM: https://docs.github.com/en/free-pro-team@latest/developers/webhooks-and-events/webhook-events-and-payloads#push
#[derive(Debug, Clone, Default, PartialEq, JsonSchema, Deserialize, Serialize)]
pub struct GitHubCommit {
    /// The SHA of the commit.
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "octorust::utils::deserialize_null_string::deserialize"
    )]
    pub id: String,
    /// The ISO 8601 timestamp of the commit.
    pub timestamp: Option<DateTime<Utc>>,
    /// The commit message.
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "octorust::utils::deserialize_null_string::deserialize"
    )]
    pub message: String,
    /// The git author of the commit.
    #[serde(default, alias = "user")]
    pub author: crate::repos::GitHubUser,
    /// URL that points to the commit API resource.
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "octorust::utils::deserialize_null_string::deserialize"
    )]
    pub url: String,
    /// Whether this commit is distinct from any that have been pushed before.
    #[serde(default)]
    pub distinct: bool,
    /// An array of files added in the commit.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub added: Vec<String>,
    /// An array of files modified by the commit.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modified: Vec<String>,
    /// An array of files removed in the commit.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed: Vec<String>,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "octorust::utils::deserialize_null_string::deserialize"
    )]
    pub label: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        alias = "ref",
        deserialize_with = "octorust::utils::deserialize_null_string::deserialize"
    )]
    pub commit_ref: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "octorust::utils::deserialize_null_string::deserialize"
    )]
    pub sha: String,
}

impl GitHubCommit {
    /// Filter the files that were added, modified, or removed by their prefix
    /// including a specified directory or path.
    pub fn filter_files_by_path(&mut self, dir: &str) {
        self.added = filter(&self.added, dir);
        self.modified = filter(&self.modified, dir);
        self.removed = filter(&self.removed, dir);
    }

    /// Return if the commit has any files that were added, modified, or removed.
    pub fn has_changed_files(&self) -> bool {
        !self.added.is_empty() || !self.modified.is_empty() || !self.removed.is_empty()
    }

    /// Return if a specific file was added, modified, or removed in a commit.
    pub fn file_changed(&self, file: &str) -> bool {
        self.added.contains(&file.to_string())
            || self.modified.contains(&file.to_string())
            || self.removed.contains(&file.to_string())
    }
}

fn filter(files: &[String], dir: &str) -> Vec<String> {
    let mut in_dir: Vec<String> = Default::default();
    for file in files {
        if file.starts_with(dir) {
            in_dir.push(file.to_string());
        }
    }

    in_dir
}
