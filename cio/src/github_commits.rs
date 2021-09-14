use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{github_prs::FromSimpleUser, repos::FromUrl};

#[derive(Serialize, Deserialize, Default, PartialEq, Debug, Clone, JsonSchema)]
pub struct NewCommit {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub author: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub comments_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub commit_author: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_author_date: Option<DateTime<Utc>>,
    #[serde(default)]
    pub comment_count: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub commit_committer: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_committer_date: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub message: String,
    #[serde(default)]
    pub verified: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tree: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub committer: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub html_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub node_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parents: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sha: String,
    #[serde(default)]
    pub additions: i32,
    #[serde(default)]
    pub deletions: i32,
    #[serde(default)]
    pub total: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
}

impl From<octorust::types::CommitDataType> for NewCommit {
    fn from(item: octorust::types::CommitDataType) -> Self {
        let mut additions = 0;
        let mut deletions = 0;
        let mut total = 0;
        if let Some(stats) = item.stats {
            additions = stats.additions as i32;
            deletions = stats.deletions as i32;
            total = stats.total as i32;
        }

        NewCommit {
            author: item.author.to_string(),
            comments_url: item.comments_url.to_string(),
            commit_author: item.commit.author.to_string(),
            commit_author_date: None,
            comment_count: item.commit.comment_count as i32,
            commit_committer: item.commit.committer.to_string(),
            commit_committer_date: None,
            message: item.commit.message.to_string(),
            verified: item.commit.verification.is_some(),
            tree: item.commit.tree.sha.to_string(),
            committer: item.committer.to_string(),
            files: item.files.to_vec(),
            html_url: item.html_url.to_string(),
            node_id: item.node_id.to_string(),
            parents: item.parents.to_vec(),
            sha: item.sha.to_string(),
            additions,
            deletions,
            total,
            url: item.url.to_string(),
        }
    }
}

pub trait FromVecParents {
    fn to_vec(&self) -> Vec<String>;
}

impl FromVecParents for Vec<octorust::types::Parents> {
    fn to_vec(&self) -> Vec<String> {
        let mut parents: Vec<String> = Default::default();

        for t in self {
            parents.push(t.sha.to_string());
        }

        parents
    }
}

pub trait FromVecCommitFiles {
    fn to_vec(&self) -> Vec<String>;
}

impl FromVecCommitFiles for Vec<octorust::types::CommitFiles> {
    fn to_vec(&self) -> Vec<String> {
        let mut files: Vec<String> = Default::default();

        for t in self {
            files.push(t.filename.to_string());
        }

        files
    }
}

pub trait FromTagger {
    fn to_string(&self) -> String;
}

impl FromTagger for Option<octorust::types::Tagger> {
    fn to_string(&self) -> String {
        if let Some(u) = self {
            u.email.to_string()
        } else {
            String::new()
        }
    }
}
