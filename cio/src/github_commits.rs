use anyhow::Result;
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{companies::Companys, db::Database, github_prs::FromSimpleUser, repos::FromUrl};

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
            commit_author_date: item.commit.author.to_date(),
            comment_count: item.commit.comment_count as i32,
            commit_committer: item.commit.committer.to_string(),
            commit_committer_date: item.commit.committer.to_date(),
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
    fn to_date(&self) -> Option<DateTime<Utc>>;
}

impl FromTagger for Option<octorust::types::Tagger> {
    fn to_string(&self) -> String {
        if let Some(u) = self {
            u.email.to_string()
        } else {
            String::new()
        }
    }

    fn to_date(&self) -> Option<DateTime<Utc>> {
        if let Some(u) = self {
            if let Ok(d) = DateTime::parse_from_str(&u.date, "%+") {
                Some(d.with_timezone(&Utc))
            } else {
                None
            }
        } else {
            None
        }
    }
}

pub async fn refresh_commits() -> Result<()> {
    let db = Database::new().await;

    let companies = Companys::get_from_db(&db, 1).await?;

    for company in companies {
        let github = company.authenticate_github()?;

        // List all the repos.
        let repos = crate::repos::list_all_github_repos(&github, &company).await?;

        // For each repo, get all the commits.
        for repo in repos {
            // Get all the commits.
            let commits = github
                .repos()
                .list_all_commits(
                    &company.github_org,
                    &repo.name,
                    "",   // sha
                    "",   // path
                    "",   // author
                    None, // since
                    None, // until
                )
                .await?;

            for commit in commits {
                println!("{:#?}", commit);
                let c: NewCommit = commit.into();
                println!("{:#?}", c);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[ignore]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_commits() {
        refresh_commits().await.unwrap();
    }
}
