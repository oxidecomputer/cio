use std::env;
use std::fmt::Debug;
use std::ops::Add;
use std::{thread, time};

use chrono::offset::Utc;
use chrono::{DateTime, Duration};
use futures_util::TryStreamExt;
use influxdb::InfluxDbWriteable;
use influxdb::{Client as InfluxClient, Query as InfluxQuery};

use crate::event_types::EventType;
use crate::utils::{authenticate_github_jwt, list_all_github_repos};

#[derive(Clone)]
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

    pub async fn check_exists(
        &self,
        table: &str,
        github_id: i64,
        action: &str,
        sha: &str,
        time: DateTime<Utc>,
    ) -> bool {
        let flux_date_format = "%Y-%m-%dT%H:%M:%SZ";

        let read_query = InfluxQuery::raw_read_query(&format!(
            r#"from(bucket:"github_webhooks")
                    |> range(start: {}, stop: {})
                    |> filter(fn: (r) => r._measurement == "{}")
                    |> filter(fn: (r) => r.github_id == {})
                    |> filter(fn: (r) => r.action == "{}")
                    |> filter(fn: (r) => r.sha == "{}")
                    "#,
            time.format(flux_date_format),
            // TODO: see how accurate the webhook server is.
            (time + Duration::minutes(60)).format(flux_date_format),
            table,
            github_id,
            action,
            sha
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
                    let mut closed_by = issue.closed_by.login.to_string();
                    if closed_by.is_empty() {
                        closed_by = issue.user.login.to_string();
                    }

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
                            sender: closed_by,
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
        let gh = authenticate_github_jwt();
        let repos = list_all_github_repos(&gh).await;

        let mut handles: Vec<tokio::task::JoinHandle<()>> = Default::default();

        // For each repo, get information on the pull requests.
        for repo in repos {
            if repo.fork {
                // Continue early, we don't care about the forks.
                continue;
            }

            // Skip the RFD repo for now.
            // TODO: remove this
            if repo.name.to_string() == "rfd" {
                continue;
            }

            let client = self.clone();
            let handle = tokio::task::spawn(async move {
                let github = authenticate_github_jwt();

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
                    let commit = match r.commits().get(&c.sha).await {
                        Ok(c) => c,
                        Err(e) => {
                            // Check if we were rate limited here.
                            // If so we should sleep until the rate limit is over.
                            match e {
                                hubcaps::errors::Error::RateLimit { reset } => {
                                    // We got a rate limit error.
                                    println!(
                                        "got rate limited, sleeping for {}s",
                                        reset.as_secs()
                                    );
                                    thread::sleep(
                                        reset.add(time::Duration::from_secs(5)),
                                    );
                                }
                                _ => panic!(
                                    "github getting commits failed: {}",
                                    e
                                ),
                            }

                            // Try to get the commit again.
                            r.commits().get(&c.sha).await.unwrap()
                        }
                    };

                    // Add events for each commit if it does not already exist.
                    // Check if this event already exists.
                    // Let's see if the data we wrote is there.
                    let time = commit.commit.author.date;
                    let exists = client
                        .commit_exists(
                            EventType::Push.name(),
                            &commit.sha,
                            &repo.name,
                            time,
                        )
                        .await;

                    let sender = commit.author.login.to_string();
                    if sender.is_empty() {
                        // Make sure we don't have an empty sender!
                        println!(
                            "[warn]: sender for commit {} on repo {} is empty",
                            commit.sha, repo.name
                        );
                        // Continue early, do not push the event.
                        continue;
                    }

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
                            sender: sender.to_string(),
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

                        client.query(push_event, EventType::Push.name()).await;
                    }

                    // Handle the check_suite events for each commit.
                    let check_suites = match r
                        .commits()
                        .list_check_suites(
                            &c.sha,
                            &hubcaps::checks::CheckSuiteListOptions::builder()
                                .per_page(100)
                                .build(),
                        )
                        .await
                    {
                        Ok(c) => c,
                        Err(e) => {
                            // Check if we were rate limited here.
                            // If so we should sleep until the rate limit is over.
                            match e {
                                hubcaps::errors::Error::RateLimit { reset } => {
                                    // We got a rate limit error.
                                    println!(
                                        "got rate limited, sleeping for {}s",
                                        reset.as_secs()
                                    );
                                    thread::sleep(
                                        reset.add(time::Duration::from_secs(5)),
                                    );
                                }
                                _ => panic!(
                                    "github getting check suites failed: {}",
                                    e
                                ),
                            }

                            // Try to get the check suites again.
                            r
                        .commits()
                        .list_check_suites(&c.sha,
                            &hubcaps::checks::CheckSuiteListOptions::builder()
                                .per_page(100)
                                .build(),
                        )
                        .await
                        .unwrap()
                        }
                    }
                    .check_suites;

                    for check_suite in check_suites {
                        let github_id =
                            check_suite.id.to_string().parse::<i64>().unwrap();

                        if check_suite.app.id <= 0 {
                            // Continue early.
                            println!("app id for check suite is 0 for https://github.com/{}/{}/commits/{}", repo.owner.login, repo.name,
                            c.sha);
                            continue;
                        }

                        // Add events for each check_suite if it does not already exist.
                        // Check if this event already exists.
                        // Let's see if the data we wrote is there.
                        let exists = client
                            .check_exists(
                                EventType::CheckSuite.name(),
                                github_id,
                                "created",
                                &commit.sha,
                                check_suite.created_at,
                            )
                            .await;

                        if !exists {
                            // Add the event.
                            let check_suite_event = CheckSuite {
                                time: check_suite.created_at,
                                repo_name: repo.name.to_string(),
                                sender: sender.to_string(),
                                // TODO: iterate over all the branches
                                // Do we need to do this??
                                reference: repo.default_branch.to_string(),
                                sha: commit.sha.to_string(),
                                action: "created".to_string(),
                                github_id,
                                status: "requested".to_string(),
                                conclusion: "null".to_string(),
                                head_branch: check_suite
                                    .head_branch
                                    .to_string(),
                                head_sha: check_suite.head_sha.to_string(),
                                name: check_suite.app.name.to_string(),
                                slug: check_suite.app.slug.to_string(),
                            };

                            client
                                .query(
                                    check_suite_event,
                                    EventType::CheckSuite.name(),
                                )
                                .await;
                        }

                        // Add the completed event if it is completed.
                        if check_suite.status.to_string() == "completed" {
                            // Check if this event already exists.
                            // Let's see if the data we wrote is there.
                            let exists = client
                                .check_exists(
                                    EventType::CheckSuite.name(),
                                    github_id,
                                    "completed",
                                    &commit.sha,
                                    check_suite.updated_at,
                                )
                                .await;

                            if !exists {
                                // Add the event.
                                let completed_check_suite_event = CheckSuite {
                                    time: check_suite.updated_at,
                                    repo_name: repo.name.to_string(),
                                    sender: sender.to_string(),
                                    // TODO: iterate over all the branches
                                    // Do we need to do this??
                                    reference: repo.default_branch.to_string(),
                                    sha: commit.sha.to_string(),
                                    action: "completed".to_string(),
                                    github_id,
                                    status: "completed".to_string(),
                                    conclusion: check_suite
                                        .conclusion
                                        .to_string(),
                                    head_branch: check_suite
                                        .head_branch
                                        .to_string(),
                                    head_sha: check_suite.head_sha.to_string(),
                                    name: check_suite.app.name.to_string(),
                                    slug: check_suite.app.slug.to_string(),
                                };

                                client
                                    .query(
                                        completed_check_suite_event,
                                        EventType::CheckSuite.name(),
                                    )
                                    .await;
                            }
                        }

                        // Get the check runs for this check suite.
                        let check_runs =
                            match r
                                .checkruns()
                                .list_for_suite(&github_id.to_string())
                                .await
                            {
                                Ok(c) => c,
                                Err(e) => {
                                    // Check if we were rate limited here.
                                    // If so we should sleep until the rate limit is over.
                                    match e {
                                        hubcaps::errors::Error::RateLimit {
                                            reset,
                                        } => {
                                            // We got a rate limit error.
                                            println!(
                                        "got rate limited, sleeping for {}s",
                                        reset.as_secs()
                                    );
                                            thread::sleep(reset.add(
                                                time::Duration::from_secs(5),
                                            ));
                                        }
                                        _ => panic!(
                                    "github getting check suites failed: {}",
                                    e
                                ),
                                    }

                                    // Try to get the check runs again.
                                    r.checkruns()
                                        .list_for_suite(&github_id.to_string())
                                        .await
                                        .unwrap()
                                }
                            };

                        // Iterate over the check runs.
                        for check_run in check_runs {
                            let check_run_github_id = check_suite
                                .id
                                .to_string()
                                .parse::<i64>()
                                .unwrap();

                            // Add events for each check_run if it does not already exist.
                            // Check if this event already exists.
                            // Let's see if the data we wrote is there.
                            let exists = client
                                .check_exists(
                                    EventType::CheckRun.name(),
                                    check_run_github_id,
                                    "created",
                                    &commit.sha,
                                    check_run.started_at,
                                )
                                .await;

                            if !exists {
                                // Add the event.
                                let check_run_event = CheckRun {
                                    time: check_run.started_at,
                                    repo_name: repo.name.to_string(),
                                    sender: sender.to_string(),
                                    // TODO: iterate over all the branches
                                    // Do we need to do this??
                                    reference: repo.default_branch.to_string(),
                                    sha: commit.sha.to_string(),
                                    action: "created".to_string(),
                                    github_id: check_run_github_id,
                                    status: "queued".to_string(),
                                    conclusion: "null".to_string(),
                                    name: check_run.name.to_string(),

                                    // Check suite details
                                    head_branch: check_suite
                                        .head_branch
                                        .to_string(),
                                    head_sha: check_suite.head_sha.to_string(),
                                    app_name: check_suite.app.name.to_string(),
                                    app_slug: check_suite.app.slug.to_string(),
                                    check_suite_id: github_id,
                                };

                                client
                                    .query(
                                        check_run_event,
                                        EventType::CheckRun.name(),
                                    )
                                    .await;
                            }
                        }
                    }
                }
            });

            handles.push(handle);
        }

        // Wait for all the handles.
        for handle in handles {
            handle.await.unwrap();
        }
    }

    pub async fn update_pull_request_events(&self) {
        let github = authenticate_github_jwt();
        let repos = list_all_github_repos(&github).await;

        // For each repo, get information on the pull requests.
        for repo in repos {
            let r = github
                .repo(repo.owner.login.to_string(), repo.name.to_string());
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
                    let mut closed_by = pull.closed_by.login.to_string();
                    if closed_by.is_empty() {
                        closed_by = pull.user.login.to_string();
                    }

                    if pull.merged_at.is_some() {
                        closed_at = pull.merged_at.unwrap();
                        if !pull.merged_by.login.is_empty() {
                            closed_by = pull.merged_by.login.to_string();
                        }
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
                            sender: closed_by,
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

/// FROM: https://docs.github.com/en/free-pro-team@latest/developers/webhooks-and-events/webhook-events-and-payloads#check_suite
#[derive(InfluxDbWriteable, Clone, Debug)]
pub struct CheckSuite {
    pub time: DateTime<Utc>,
    #[tag]
    pub repo_name: String,
    #[tag]
    pub sender: String,
    #[tag]
    pub action: String,
    #[tag]
    pub head_branch: String,
    #[tag]
    pub head_sha: String,
    #[tag]
    pub status: String,
    #[tag]
    pub conclusion: String,

    #[tag]
    pub slug: String,
    #[tag]
    pub name: String,

    #[tag]
    pub reference: String,
    #[tag]
    pub sha: String,

    pub github_id: i64,
}

/// FROM: https://docs.github.com/en/free-pro-team@latest/developers/webhooks-and-events/webhook-events-and-payloads#check_run
#[derive(InfluxDbWriteable, Clone, Debug)]
pub struct CheckRun {
    pub time: DateTime<Utc>,
    #[tag]
    pub repo_name: String,
    #[tag]
    pub sender: String,
    #[tag]
    pub action: String,
    #[tag]
    pub head_branch: String,
    #[tag]
    pub head_sha: String,
    #[tag]
    pub status: String,
    #[tag]
    pub conclusion: String,

    #[tag]
    pub name: String,
    #[tag]
    pub check_suite_id: i64,
    #[tag]
    pub app_slug: String,
    #[tag]
    pub app_name: String,

    #[tag]
    pub reference: String,
    #[tag]
    pub sha: String,

    pub github_id: i64,
}

#[cfg(test)]
mod tests {
    use crate::influx::Client;

    #[ignore]
    #[tokio::test(threaded_scheduler)]
    async fn test_cron_influx_push() {
        let influx = Client::new_from_env();
        influx.update_push_events().await;
    }

    #[tokio::test(threaded_scheduler)]
    async fn test_cron_influx_pulls() {
        let influx = Client::new_from_env();
        influx.update_pull_request_events().await;
    }

    #[tokio::test(threaded_scheduler)]
    async fn test_cron_influx_issues() {
        let influx = Client::new_from_env();
        influx.update_issues_events().await;
    }
}
