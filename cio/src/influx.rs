use std::env;
use std::fmt::Debug;

use chrono::offset::Utc;
use chrono::{DateTime, Duration};
use futures_util::TryStreamExt;
use influxdb::InfluxDbWriteable;
use influxdb::{Client as InfluxClient, Query as InfluxQuery};

use crate::event_types::EventType;
use crate::utils::{authenticate_github_jwt, list_all_github_repos};

pub struct Client(pub InfluxClient);

impl Client {
    pub fn new_from_env() -> Self {
        Client(
            InfluxClient::new(
                env::var("INFLUX_DB_URL").unwrap(),
                "github_webhooks",
            )
            .with_auth(
                env::var("GADMIN_SUBJECT").unwrap(),
                env::var("INFLUX_DB_TOKEN").unwrap(),
            ),
        )
    }

    pub async fn commit_exists(
        &self,
        table: &str,
        sha: &str,
        repo_name: &str,
        time: DateTime<Utc>,
    ) -> bool {
        let flux_date_format = "%Y-%m-%dT%H:%M:%SZ";

        let read_query = InfluxQuery::raw_read_query(&format!(
            r#"from(bucket:"github_webhooks")
                    |> range(start: {}, stop: {})
                    |> filter(fn: (r) => r._measurement == "{}")
                    |> filter(fn: (r) => r.sha == "{}")
                    |> filter(fn: (r) => r.repo_name == "{}")
                    "#,
            time.format(flux_date_format),
            // TODO: see how accurate the webhook server is.
            (time + Duration::minutes(60)).format(flux_date_format),
            table,
            sha,
            repo_name
        ));
        let read_result = self.0.query(&read_query).await;

        if read_result.is_ok() {
            if read_result.unwrap().trim().is_empty() {
                return false;
            }
            return true;
        }
        false
    }

    pub async fn event_exists(
        &self,
        table: &str,
        github_id: i64,
        action: &str,
        time: DateTime<Utc>,
    ) -> bool {
        let flux_date_format = "%Y-%m-%dT%H:%M:%SZ";

        let read_query = InfluxQuery::raw_read_query(&format!(
            r#"from(bucket:"github_webhooks")
                    |> range(start: {}, stop: {})
                    |> filter(fn: (r) => r._measurement == "{}")
                    |> filter(fn: (r) => r.github_id == {})
                    |> filter(fn: (r) => r.action == "{}")
                    "#,
            time.format(flux_date_format),
            // TODO: see how accurate the webhook server is.
            (time + Duration::minutes(60)).format(flux_date_format),
            table,
            github_id,
            action
        ));
        let read_result = self.0.query(&read_query).await;

        if read_result.is_ok() {
            if read_result.unwrap().trim().is_empty() {
                return false;
            }
            return true;
        }
        false
    }

    pub async fn query<Q: InfluxDbWriteable + Clone + Debug>(
        &self,
        q: Q,
        table: &str,
    ) -> String {
        match self.0.query(&q.clone().into_query(table)).await {
            Ok(v) => {
                println!("successfully updated table `{}`: {:?}", table, q);
                return v;
            }
            Err(e) => {
                println!(
                    "[influxdb] table `{}` error: {}, event: {:?}",
                    table, e, q
                )
            }
        }

        "".to_string()
    }

    pub async fn update_issues_events(&self) {
        let github = authenticate_github_jwt();
        let repos = list_all_github_repos(&github).await;

        // For each repo, get information on the pull requests.
        for repo in repos {
            let r = github.repo(repo.owner.login, repo.name.to_string());
            let issues = r
                .issues()
                .iter(
                    &hubcaps::issues::IssueListOptions::builder()
                        .state(hubcaps::issues::State::All)
                        .per_page(100)
                        .build(),
                )
                .try_collect::<Vec<hubcaps::issues::Issue>>()
                .await
                .unwrap();

            for issue in issues {
                // Add events for each issue if it does not already exist.
                // Check if this event already exists.
                // Let's see if the data we wrote is there.
                let github_id = issue.id.to_string().parse::<i64>().unwrap();
                let exists = self
                    .event_exists(
                        EventType::Issues.name(),
                        github_id,
                        "opened",
                        issue.created_at,
                    )
                    .await;

                if !exists {
                    // Add the event.
                    let issue_created = Issue {
                        time: issue.created_at,
                        repo_name: repo.name.to_string(),
                        sender: issue.user.login.to_string(),
                        action: "opened".to_string(),
                        number: issue
                            .number
                            .to_string()
                            .parse::<i64>()
                            .unwrap(),
                        github_id,
                    };
                    self.query(issue_created, EventType::Issues.name()).await;
                }

                if issue.closed_at.is_some() {
                    let closed_at = issue.closed_at.unwrap();

                    // Check if we already have the event.
                    let exists = self
                        .event_exists(
                            EventType::Issues.name(),
                            github_id,
                            "closed",
                            closed_at,
                        )
                        .await;

                    if !exists {
                        // Add the event.
                        let issue_closed = Issue {
                            time: closed_at,
                            repo_name: repo.name.to_string(),
                            sender: issue.user.login.to_string(),
                            action: "closed".to_string(),
                            number: issue
                                .number
                                .to_string()
                                .parse::<i64>()
                                .unwrap(),
                            github_id,
                        };
                        self.query(issue_closed, EventType::Issues.name())
                            .await;
                    }
                }

                // Get the comments for the issue.
                let issue_comments = r
                    .issue(issue.number)
                    .comments()
                    .iter(
                        &hubcaps::comments::CommentListOptions::builder()
                            .per_page(100)
                            .build(),
                    )
                    .try_collect::<Vec<hubcaps::comments::Comment>>()
                    .await
                    .unwrap();

                for issue_comment in issue_comments {
                    // Add events for each issue comment if it does not already exist.
                    // Check if this event already exists.
                    // Let's see if the data we wrote is there.
                    let github_id =
                        issue_comment.id.to_string().parse::<i64>().unwrap();
                    let exists = self
                        .event_exists(
                            EventType::IssueComment.name(),
                            github_id,
                            "created",
                            issue_comment.created_at,
                        )
                        .await;

                    if !exists {
                        // Add the event.
                        let issue_comment_created = IssueComment {
                            time: issue_comment.created_at,
                            repo_name: repo.name.to_string(),
                            sender: issue_comment.user.login.to_string(),
                            action: "created".to_string(),
                            issue_number: issue
                                .number
                                .to_string()
                                .parse::<i64>()
                                .unwrap(),
                            github_id,
                            comment: issue_comment.body.to_string(),
                        };
                        self.query(
                            issue_comment_created,
                            EventType::IssueComment.name(),
                        )
                        .await;
                    }
                }
            }
        }
    }

    pub async fn update_push_events(&self) {
        let github = authenticate_github_jwt();
        let repos = list_all_github_repos(&github).await;

        // For each repo, get information on the pull requests.
        for repo in repos {
            if repo.fork {
                // Continue early, we don't care about the forks.
                continue;
            }
            let r = github
                .repo(repo.owner.login.to_string(), repo.name.to_string());
            let commits = r
                .commits()
                .iter()
                .try_collect::<Vec<hubcaps::repo_commits::RepoCommit>>()
                .await
                .map_err(|e| {
                    println!(
                        "iterating over commits in repo {} failed: {}",
                        repo.name.to_string(),
                        e
                    )
                })
                .unwrap_or_default();

            for c in commits {
                // Get the verbose information for the commit.
                let commit = r.commits().get(&c.sha).await.unwrap();

                // Add events for each commit if it does not already exist.
                // Check if this event already exists.
                // Let's see if the data we wrote is there.
                let time = commit.commit.author.date;
                let exists = self
                    .commit_exists(
                        EventType::Push.name(),
                        &commit.sha,
                        &repo.name,
                        time,
                    )
                    .await;

                if !exists {
                    // Get the changed files.
                    let mut added: Vec<String> = Default::default();
                    let mut modified: Vec<String> = Default::default();
                    let mut removed: Vec<String> = Default::default();
                    for file in commit.files {
                        if file.status == "added" {
                            added.push(file.filename.to_string());
                        }
                        if file.status == "modified" {
                            modified.push(file.filename.to_string());
                        }
                        if file.status == "removed" {
                            removed.push(file.filename.to_string());
                        }
                    }

                    // Add the event.
                    let push_event = Push {
                        time,
                        repo_name: repo.name.to_string(),
                        sender: commit.author.login.to_string(),
                        // TODO: iterate over all the branches
                        // Do we need to do this??
                        reference: repo.default_branch.to_string(),
                        sha: commit.sha.to_string(),
                        added: added.join(",").to_string(),
                        modified: modified.join(",").to_string(),
                        removed: removed.join(",").to_string(),
                        additions: commit.stats.additions,
                        deletions: commit.stats.deletions,
                        total: commit.stats.total,
                        message: commit.commit.message.to_string(),
                    };

                    self.query(push_event, EventType::Push.name()).await;
                }
            }
        }
    }

    pub async fn update_pull_request_events(&self) {
        let github = authenticate_github_jwt();
        let repos = list_all_github_repos(&github).await;

        // For each repo, get information on the pull requests.
        for repo in repos {
            let r = github.repo(repo.owner.login, repo.name.to_string());
            let pulls = r
                .pulls()
                .iter(
                    &hubcaps::pulls::PullListOptions::builder()
                        .state(hubcaps::issues::State::All)
                        .per_page(100)
                        .build(),
                )
                .try_collect::<Vec<hubcaps::pulls::Pull>>()
                .await
                .unwrap();

            for pull in pulls {
                // Add events for each pull request if it does not already exist.
                // Check if this event already exists.
                // Let's see if the data we wrote is there.
                let github_id = pull.id.to_string().parse::<i64>().unwrap();
                let exists = self
                    .event_exists(
                        EventType::PullRequest.name(),
                        github_id,
                        "opened",
                        pull.created_at,
                    )
                    .await;

                if !exists {
                    // Add the event.
                    let pull_request_created = PullRequest {
                        time: pull.created_at,
                        repo_name: repo.name.to_string(),
                        sender: pull.user.login.to_string(),
                        action: "opened".to_string(),
                        head_reference: pull.head.commit_ref.to_string(),
                        base_reference: pull.base.commit_ref.to_string(),
                        number: pull.number.to_string().parse::<i64>().unwrap(),
                        github_id,
                        merged: false,
                    };
                    self.query(
                        pull_request_created,
                        EventType::PullRequest.name(),
                    )
                    .await;
                }

                if pull.closed_at.is_some() {
                    let mut closed_at = pull.closed_at.unwrap();
                    if pull.merged_at.is_some() {
                        closed_at = pull.merged_at.unwrap();
                    }

                    // Check if we already have the event.
                    let exists = self
                        .event_exists(
                            EventType::PullRequest.name(),
                            github_id,
                            "closed",
                            closed_at,
                        )
                        .await;

                    if !exists {
                        // Add the event.
                        let pull_request_closed = PullRequest {
                            time: closed_at,
                            repo_name: repo.name.to_string(),
                            sender: pull.user.login.to_string(),
                            action: "closed".to_string(),
                            head_reference: pull.head.commit_ref.to_string(),
                            base_reference: pull.base.commit_ref.to_string(),
                            number: pull
                                .number
                                .to_string()
                                .parse::<i64>()
                                .unwrap(),
                            github_id,
                            merged: pull.merged_at.is_some(),
                        };
                        self.query(
                            pull_request_closed,
                            EventType::PullRequest.name(),
                        )
                        .await;
                    }
                }

                // Get the pull request review comments for the pull request.
                let pull_comments = r
                    .pulls()
                    .get(pull.number)
                    .review_comments()
                    .iter(&hubcaps::review_comments::ReviewCommentListOptions::builder().per_page(100).build())
                    .try_collect::<Vec<hubcaps::review_comments::ReviewComment>>()
                    .await
                    .map_err(|e| {
                        println!(
                            "iterating over review comment in repo {} for pull {} failed: {}",
                            repo.name.to_string(),
                            pull.number,
                            e
                        )
                    })
                    .unwrap_or_default();

                for pull_comment in pull_comments {
                    // Add events for each pull comment if it does not already exist.
                    // Check if this event already exists.
                    // Let's see if the data we wrote is there.
                    let github_id =
                        pull_comment.id.to_string().parse::<i64>().unwrap();
                    let exists = self
                        .event_exists(
                            EventType::PullRequestReviewComment.name(),
                            github_id,
                            "created",
                            pull_comment.created_at,
                        )
                        .await;

                    if !exists {
                        // Add the event.
                        let pull_comment_created = PullRequestReviewComment {
                            time: pull_comment.created_at,
                            repo_name: repo.name.to_string(),
                            sender: pull_comment.user.login.to_string(),
                            action: "created".to_string(),
                            pull_request_number: pull
                                .number
                                .to_string()
                                .parse::<i64>()
                                .unwrap(),
                            github_id,
                            comment: pull_comment.body.to_string(),
                        };
                        self.query(
                            pull_comment_created,
                            EventType::PullRequestReviewComment.name(),
                        )
                        .await;
                    }
                }
            }
        }
    }
}

/// FROM: https://docs.github.com/en/free-pro-team@latest/developers/webhooks-and-events/webhook-events-and-payloads#push
#[derive(InfluxDbWriteable, Clone, Debug)]
pub struct Push {
    pub time: DateTime<Utc>,
    #[tag]
    pub repo_name: String,
    #[tag]
    pub sender: String,
    #[tag]
    pub reference: String,
    pub added: String,
    pub modified: String,
    pub removed: String,
    pub additions: i64,
    pub deletions: i64,
    pub total: i64,
    pub sha: String,
    pub message: String,
}

/// FROM: https://docs.github.com/en/free-pro-team@latest/developers/webhooks-and-events/webhook-events-and-payloads#pull_request
#[derive(InfluxDbWriteable, Clone, Debug)]
pub struct PullRequest {
    pub time: DateTime<Utc>,
    #[tag]
    pub repo_name: String,
    #[tag]
    pub sender: String,
    #[tag]
    pub action: String,
    #[tag]
    pub merged: bool,
    #[tag]
    pub head_reference: String,
    #[tag]
    pub base_reference: String,
    #[tag]
    pub number: i64,
    pub github_id: i64,
}

/// FROM: https://docs.github.com/en/free-pro-team@latest/developers/webhooks-and-events/webhook-events-and-payloads#issues
#[derive(InfluxDbWriteable, Clone, Debug)]
pub struct Issue {
    pub time: DateTime<Utc>,
    #[tag]
    pub repo_name: String,
    #[tag]
    pub sender: String,
    #[tag]
    pub action: String,
    #[tag]
    pub number: i64,
    pub github_id: i64,
}

/// FROM: https://docs.github.com/en/free-pro-team@latest/developers/webhooks-and-events/webhook-events-and-payloads#issue_comment
#[derive(InfluxDbWriteable, Clone, Debug)]
pub struct IssueComment {
    pub time: DateTime<Utc>,
    #[tag]
    pub repo_name: String,
    #[tag]
    pub sender: String,
    #[tag]
    pub action: String,
    #[tag]
    pub issue_number: i64,
    pub github_id: i64,
    pub comment: String,
}

/// FROM: https://docs.github.com/en/free-pro-team@latest/developers/webhooks-and-events/webhook-events-and-payloads#pull_request_review_comment
#[derive(InfluxDbWriteable, Clone, Debug)]
pub struct PullRequestReviewComment {
    pub time: DateTime<Utc>,
    #[tag]
    pub repo_name: String,
    #[tag]
    pub sender: String,
    #[tag]
    pub action: String,
    #[tag]
    pub pull_request_number: i64,
    pub github_id: i64,
    pub comment: String,
}

#[cfg(test)]
mod tests {
    use crate::influx::Client;

    #[tokio::test(threaded_scheduler)]
    async fn test_cron_influx() {
        let influx = Client::new_from_env();
        influx.update_push_events().await;
        influx.update_issues_events().await;
        influx.update_pull_request_events().await;
    }
}
