use anyhow::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{companies::Companys, db::Database};

#[derive(Serialize, Deserialize, Default, PartialEq, Debug, Clone, JsonSchema)]
pub struct NewPullRequest {
    #[serde(rename = "_links", default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub active_lock_reason: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub assignee: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assignees: Vec<String>,
    /**
     * How the author is associated with the repository.
     */
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub author_association: String,
    /**
     * The status of auto merging a pull request.
     */
    #[serde(default)]
    pub auto_merge: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub base: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub comments_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub commits_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub diff_url: String,
    #[serde(default)]
    pub draft: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub head: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub html_url: String,
    #[serde(default)]
    pub id: i64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub issue_url: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
    #[serde(default)]
    pub locked: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub merge_commit_sha: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merged_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub milestone: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub node_id: String,
    #[serde(default)]
    pub number: i64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub patch_url: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requested_reviewers: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requested_teams: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub review_comment_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub review_comments_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub state: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub statuses_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub user: String,
}

impl From<octorust::types::PullRequestSimple> for NewPullRequest {
    fn from(item: octorust::types::PullRequestSimple) -> Self {
        NewPullRequest {
            links: item.links.to_vec(),
            active_lock_reason: item.active_lock_reason.to_string(),
            assignee: item.assignee.to_string(),
            assignees: item.assignees.to_vec(),
            author_association: item.author_association.to_string(),
            auto_merge: item.auto_merge.is_some(),
            base: item.base.label.to_string(),
            body: item.body.to_string(),
            closed_at: item.closed_at,
            comments_url: item.comments_url.to_string(),
            commits_url: item.commits_url.to_string(),
            created_at: item.created_at,
            diff_url: item.diff_url.to_string(),
            draft: item.draft,
            head: item.head.label.to_string(),
            html_url: item.html_url.to_string(),
            id: item.id,
            issue_url: item.issue_url.to_string(),
            labels: item.labels.to_vec(),
            locked: item.locked,
            merge_commit_sha: item.merge_commit_sha.to_string(),
            merged_at: item.merged_at,
            milestone: item.milestone.to_string(),
            node_id: item.node_id.to_string(),
            number: item.number,
            patch_url: item.patch_url.to_string(),
            requested_reviewers: item.requested_reviewers.to_vec(),
            requested_teams: item.requested_teams.to_vec(),
            review_comment_url: item.review_comment_url.to_string(),
            review_comments_url: item.review_comments_url.to_string(),
            state: item.state.to_string(),
            statuses_url: item.statuses_url.to_string(),
            title: item.title.to_string(),
            updated_at: item.updated_at,
            url: item.url.to_string(),
            user: item.user.to_string(),
        }
    }
}

pub trait FromSimpleUser {
    fn to_string(&self) -> String;
}

impl FromSimpleUser for Option<octorust::types::SimpleUser> {
    fn to_string(&self) -> String {
        if let Some(u) = self {
            u.login.to_string()
        } else {
            String::new()
        }
    }
}

pub trait FromMilestone {
    fn to_string(&self) -> String;
}

impl FromMilestone for Option<octorust::types::Milestone> {
    fn to_string(&self) -> String {
        if let Some(u) = self {
            u.title.to_string()
        } else {
            String::new()
        }
    }
}

pub trait FromVecTeams {
    fn to_vec(&self) -> Vec<String>;
}

impl FromVecTeams for Vec<octorust::types::Team> {
    fn to_vec(&self) -> Vec<String> {
        let mut teams: Vec<String> = Default::default();

        for t in self {
            teams.push(t.slug.to_string());
        }

        teams
    }
}

pub trait FromVecSimpleUsers {
    fn to_vec(&self) -> Vec<String>;
}

impl FromVecSimpleUsers for Vec<octorust::types::SimpleUser> {
    fn to_vec(&self) -> Vec<String> {
        let mut users: Vec<String> = Default::default();

        for t in self {
            users.push(t.login.to_string());
        }

        users
    }
}

pub trait FromVecPullRequestSimpleLabels {
    fn to_vec(&self) -> Vec<String>;
}

impl FromVecPullRequestSimpleLabels for Vec<octorust::types::LabelsData> {
    fn to_vec(&self) -> Vec<String> {
        let mut labels: Vec<String> = Default::default();

        for t in self {
            labels.push(t.name.to_string());
        }

        labels
    }
}

pub trait FromVecPullRequestSimpleLinks {
    fn to_vec(&self) -> Vec<String>;
}

impl FromVecPullRequestSimpleLinks for octorust::types::PullRequestSimpleLinks {
    fn to_vec(&self) -> Vec<String> {
        vec![
            self.comments.href.to_string(),
            self.commits.href.to_string(),
            self.html.href.to_string(),
            self.issue.href.to_string(),
            self.review_comment.href.to_string(),
            self.review_comments.href.to_string(),
            self.self_.href.to_string(),
            self.statuses.href.to_string(),
        ]
    }
}

pub async fn refresh_pulls() -> Result<()> {
    let db = Database::new().await;

    let companies = Companys::get_from_db(&db, 1).await?;

    for company in companies {
        let github = company.authenticate_github()?;

        // List all the repos.
        let repos = crate::repos::list_all_github_repos(&github, &company).await?;

        // For each repo, get all the pull requests.
        for repo in repos {
            // Get all the pull requests.
            let pulls = github
                .pulls()
                .list_all(
                    &company.github_org,
                    &repo.name,
                    octorust::types::IssuesListState::All,
                    "", // head
                    "", // base
                    octorust::types::PullsListSort::Created,
                    octorust::types::Order::Asc,
                )
                .await?;

            for pull in pulls {
                println!("{:#?}", pull);
                let p: NewPullRequest = pull.into();
                println!("{:#?}", p);
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
    async fn test_pulls() {
        refresh_pulls().await.unwrap();
    }
}
