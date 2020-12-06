use std::env;
use std::fmt::Debug;
use std::ops::Add;
use std::{thread, time};

use chrono::offset::Utc;
use chrono::{DateTime, Duration};
use cio_api::utils::{authenticate_github_jwt, list_all_github_repos};
use futures_util::stream::TryStreamExt;
use influxdb::InfluxDbWriteable;
use influxdb::{Client as InfluxClient, Query as InfluxQuery};

use crate::event_types::EventType;

#[derive(Clone)]
pub struct Client(pub InfluxClient);

pub static FLUX_DATE_FORMAT: &str = "%Y-%m-%dT%H:%M:%SZ";

impl Client {
    pub fn new_from_env() -> Self {
        Client(InfluxClient::new(env::var("INFLUX_DB_URL").unwrap(), "github_webhooks").with_auth(env::var("GADMIN_SUBJECT").unwrap(), env::var("INFLUX_DB_TOKEN").unwrap()))
    }

    async fn exists(&self, table: &str, time: DateTime<Utc>, filter: &str) -> bool {
        let read_query = InfluxQuery::raw_read_query(&format!(
            r#"import "influxdata/influxdb/schema"
from(bucket:"github_webhooks")
    |> range(start: {}, stop: {})
    |> filter(fn: (r) => r._measurement == "{}")
    {}"#,
            time.format(FLUX_DATE_FORMAT),
            // TODO: see how accurate the webhook server is.
            (time + Duration::minutes(60)).format(FLUX_DATE_FORMAT),
            table,
            filter,
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

    pub async fn commit_exists(&self, time: DateTime<Utc>, sha: &str, repo_name: &str) -> bool {
        let filter = format!(
            r#"|> filter(fn: (r) => r.repo_name == "{}")
    |> schema.fieldsAsCols()
    |> filter(fn: (r) => r.sha == "{}")"#,
            repo_name, sha
        );

        self.exists(EventType::Push.name(), time, &filter).await
    }

    pub async fn check_exists(&self, table: &str, time: DateTime<Utc>, github_id: i64, action: &str, sha: &str) -> bool {
        let filter = format!(
            r#"|> filter(fn: (r) => r.github_id == {})
    |> filter(fn: (r) => r.action == "{}")
    |> filter(fn: (r) => r.sha == "{}")"#,
            github_id, action, sha
        );

        self.exists(table, time, &filter).await
    }

    pub async fn event_exists(&self, table: &str, time: DateTime<Utc>, github_id: i64, action: &str) -> bool {
        let filter = format!(
            r#"|> filter(fn: (r) => r.github_id == {})
    |> filter(fn: (r) => r.action == "{}")"#,
            github_id, action
        );

        self.exists(table, time, &filter).await
    }

    pub async fn query<Q: InfluxDbWriteable + Clone + Debug>(&self, q: Q, table: &str) -> String {
        match self.0.query(&q.clone().into_query(table)).await {
            Ok(v) => {
                println!("successfully updated table `{}`: {:#?}", table, q);
                return v;
            }
            Err(e) => {
                println!("[influxdb] table `{}` error: {}, event: {:#?}", table, e, q)
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
                .iter(&hubcaps::issues::IssueListOptions::builder().state(hubcaps::issues::State::All).per_page(100).build())
                .try_collect::<Vec<hubcaps::issues::Issue>>()
                .await
                .unwrap();

            for issue in issues {
                // Add events for each issue if it does not already exist.
                let github_id = issue.id.to_string().parse::<i64>().unwrap();
                let repo_name = repo.name.to_string();
                let sender = issue.user.login.to_string();
                let number = issue.number.to_string().parse::<i64>().unwrap();
                let table = EventType::Issues.name();

                // Create the event.
                let mut i = Issue {
                    time: issue.created_at,
                    repo_name: repo_name.to_string(),
                    sender: sender.to_string(),
                    action: "opened".to_string(),
                    number,
                    github_id,
                };

                // Check if this event already exists.
                // Let's see if the data we wrote is there.
                let exists = self.event_exists(table, i.time, github_id, &i.action).await;

                if !exists {
                    // Add the event.
                    self.query(i.clone(), table).await;
                }

                if issue.closed_at.is_some() {
                    let closed_at = issue.closed_at.unwrap();
                    let mut closed_by = issue.closed_by.login.to_string();
                    if closed_by.is_empty() {
                        closed_by = issue.user.login.to_string();
                    }

                    // Modify the event with the new data.
                    i.time = closed_at;
                    i.sender = closed_by;
                    i.action = "closed".to_string();

                    // Check if we already have the event.
                    let exists = self.event_exists(table, i.time, github_id, &i.action).await;

                    if !exists {
                        // Add the event.
                        self.query(i, table).await;
                    }
                }

                // Get the comments for the issue.
                let issue_comments = r
                    .issue(issue.number)
                    .comments()
                    .iter(&hubcaps::comments::CommentListOptions::builder().per_page(100).build())
                    .try_collect::<Vec<hubcaps::comments::Comment>>()
                    .await
                    .unwrap();

                for issue_comment in issue_comments {
                    // Add events for each issue comment if it does not already exist.
                    // Check if this event already exists.
                    // Let's see if the data we wrote is there.
                    let github_id = issue_comment.id.to_string().parse::<i64>().unwrap();
                    let table = EventType::IssueComment.name();

                    // Create the event.
                    let ic = IssueComment {
                        time: issue_comment.created_at,
                        repo_name: repo_name.to_string(),
                        sender: issue_comment.user.login.to_string(),
                        action: "created".to_string(),
                        issue_number: number,
                        github_id,
                        comment: issue_comment.body.to_string(),
                    };

                    let exists = self.event_exists(table, ic.time, github_id, &ic.action).await;

                    if !exists {
                        // Add the event.
                        self.query(ic, table).await;
                    }
                }
            }
        }
    }

    pub async fn update_push_events(&self) {
        let github = authenticate_github_jwt();
        let repos = list_all_github_repos(&github).await;

        let mut handles: Vec<tokio::task::JoinHandle<()>> = Default::default();

        // For each repo, get information on the pull requests.
        for repo in repos {
            if repo.fork {
                // Continue early, we don't care about the forks.
                continue;
            }

            let repo_name = repo.name.to_string();

            let r = github.repo(repo.owner.login.to_string(), repo_name.to_string());

            // TODO: iterate over all the branches
            // Do we need to do this??
            let reference = repo.default_branch.to_string();

            let client = self.clone();
            let r = r.clone();
            let handle = tokio::task::spawn(async move {
                let mut inner_handles: Vec<tokio::task::JoinHandle<()>> = Default::default();
                let commits = r
                    .commits()
                    .iter()
                    .try_collect::<Vec<hubcaps::repo_commits::RepoCommit>>()
                    .await
                    .map_err(|e| println!("[warn]: iterating over commits in repo {} failed: {}", repo.name.to_string(), e))
                    .unwrap_or_default();

                for c in commits {
                    let commit_sha = c.sha.to_string();

                    // Get the verbose information for the commit.
                    let commit = match r.commits().get(&commit_sha).await {
                        Ok(c) => c,
                        Err(e) => {
                            // Check if we were rate limited here.
                            // If so we should sleep until the rate limit is over.
                            match e {
                                hubcaps::errors::Error::RateLimit { reset } => {
                                    // We got a rate limit error.
                                    println!("got rate limited, sleeping for {}s", reset.as_secs());
                                    thread::sleep(reset.add(time::Duration::from_secs(5)));
                                }
                                _ => panic!("[warn]: github getting commits failed: {}", e),
                            }

                            // Try to get the commit again.
                            r.commits().get(&commit_sha).await.unwrap()
                        }
                    };

                    // Add events for each commit if it does not already exist.
                    let time = commit.commit.author.date;

                    // Get the sender.
                    let sender = commit.author.login.to_string();
                    if sender.is_empty() {
                        // Make sure we don't have an empty sender!
                        println!("[warn]: sender for commit {} on repo {} is empty", commit_sha, repo_name);
                        // Continue early, do not push the event.
                        continue;
                    }

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

                    // Create the event.
                    let push = Push {
                        time,
                        repo_name: repo_name.to_string(),
                        sender: sender.to_string(),
                        reference: reference.to_string(),
                        sha: commit_sha.to_string(),

                        added: added.join(",").to_string(),
                        modified: modified.join(",").to_string(),
                        removed: removed.join(",").to_string(),

                        additions: commit.stats.additions,
                        deletions: commit.stats.deletions,
                        total: commit.stats.total,

                        message: commit.commit.message.to_string(),
                    };

                    // Check if this event already exists.
                    // Let's see if the data we wrote is there.
                    let exists = client.commit_exists(push.time, &push.sha, &push.repo_name).await;

                    if !exists {
                        // Add the event.
                        client.query(push, EventType::Push.name()).await;
                    }

                    let client = client.clone();
                    let repo = repo.clone();
                    let r = r.clone();
                    let repo_name = repo_name.clone();
                    let reference = reference.clone();
                    let inner_handle = tokio::task::spawn(async move {
                        // Handle the check_suite events for each commit.
                        let check_suite_list_options = hubcaps::checks::CheckSuiteListOptions::builder().per_page(100).build();
                        let check_suites = match r.commits().list_check_suites(&commit_sha, &check_suite_list_options).await {
                            Ok(c) => c,
                            Err(e) => {
                                // Check if we were rate limited here.
                                // If so we should sleep until the rate limit is over.
                                match e {
                                    hubcaps::errors::Error::RateLimit { reset } => {
                                        // We got a rate limit error.
                                        println!("got rate limited, sleeping for {}s", reset.as_secs());
                                        thread::sleep(reset.add(time::Duration::from_secs(5)));
                                    }
                                    _ => panic!("[warn]: github getting check suites failed: {}", e),
                                }

                                // Try to get the check suites again.
                                r.commits().list_check_suites(&commit_sha, &check_suite_list_options).await.unwrap()
                            }
                        }
                        .check_suites;

                        for check_suite in check_suites {
                            // Add events for each check_suite if it does not already exist.
                            let github_id = check_suite.id.to_string().parse::<i64>().unwrap();
                            let table = EventType::CheckSuite.name();

                            if check_suite.app.id == 0 {
                                // Continue early.
                                println!("[warn]: app id for check suite is 0 for https://github.com/{}/{}/commits/{}", repo.owner.login, repo.name, c.sha);
                                continue;
                            }

                            // Create the event.
                            let mut cs = CheckSuite {
                                time: check_suite.created_at,
                                repo_name: repo_name.to_string(),
                                sender: sender.to_string(),
                                reference: reference.to_string(),
                                sha: commit_sha.to_string(),

                                action: "created".to_string(),
                                status: "requested".to_string(),
                                conclusion: "null".to_string(),

                                head_branch: check_suite.head_branch.to_string(),
                                head_sha: check_suite.head_sha.to_string(),
                                name: check_suite.app.name.to_string(),
                                slug: check_suite.app.slug.to_string(),

                                github_id,
                            };

                            // Check if this event already exists.
                            // Let's see if the data we wrote is there.
                            let exists = client.check_exists(table, cs.time, cs.github_id, &cs.action, &cs.sha).await;

                            if !exists {
                                // Add the event.
                                client.query(cs.clone(), table).await;
                            }

                            // Add the completed event if it is completed.
                            if check_suite.status == "completed" {
                                // Modify the event.
                                cs.time = check_suite.updated_at;
                                cs.action = "completed".to_string();
                                cs.status = "completed".to_string();
                                cs.conclusion = check_suite.conclusion.to_string();

                                // Check if this event already exists.
                                // Let's see if the data we wrote is there.
                                let exists = client.check_exists(table, cs.time, cs.github_id, &cs.action, &cs.sha).await;

                                if !exists {
                                    // Add the event.
                                    client.query(cs.clone(), table).await;
                                }
                            }

                            // Get the check runs for this check suite.
                            let check_runs = match r.checkruns().list_for_suite(&github_id.to_string()).await {
                                Ok(c) => c,
                                Err(e) => {
                                    // Check if we were rate limited here.
                                    // If so we should sleep until the rate limit is over.
                                    match e {
                                        hubcaps::errors::Error::RateLimit { reset } => {
                                            // We got a rate limit error.
                                            println!("got rate limited, sleeping for {}s", reset.as_secs());
                                            thread::sleep(reset.add(time::Duration::from_secs(5)));
                                        }
                                        _ => {
                                            println!("[warn]: github getting check runs failed: {}, check_suite: {:#?}", e, check_suite);
                                            continue;
                                        }
                                    }

                                    // Try to get the check runs again.
                                    r.checkruns().list_for_suite(&github_id.to_string()).await.unwrap()
                                }
                            }
                            .check_runs;

                            // Iterate over the check runs.
                            for check_run in check_runs {
                                // Add events for each check_run if it does not already exist.
                                let github_id = check_suite.id.to_string().parse::<i64>().unwrap();
                                let table = EventType::CheckRun.name();

                                // Create the event.
                                let mut cr = CheckRun {
                                    time: check_run.started_at,
                                    repo_name: repo_name.to_string(),
                                    sender: sender.to_string(),
                                    reference: reference.to_string(),
                                    sha: commit_sha.to_string(),

                                    action: "created".to_string(),
                                    status: "queued".to_string(),
                                    conclusion: "null".to_string(),

                                    name: check_run.name.to_string(),

                                    // Check suite details
                                    head_branch: cs.head_branch.to_string(),
                                    head_sha: cs.head_sha.to_string(),
                                    app_name: cs.name.to_string(),
                                    app_slug: cs.slug.to_string(),
                                    check_suite_id: cs.github_id,

                                    github_id,
                                };

                                // Check if this event already exists.
                                // Let's see if the data we wrote is there.
                                let exists = client.check_exists(table, cr.time, cr.github_id, &cr.action, &cr.sha).await;

                                if !exists {
                                    // Add the event.
                                    client.query(cr.clone(), table).await;
                                }

                                // Get the status for the check run.
                                let mut check_run_status = &hubcaps::checks::CheckRunState::Queued;
                                if check_run.status.is_some() {
                                    check_run_status = check_run.status.as_ref().unwrap();
                                };

                                // Add the completed event if it is completed.
                                if *check_run_status == hubcaps::checks::CheckRunState::Completed {
                                    if check_run.completed_at.is_none() {
                                        println!("[warn]: check_run says it is completed but it does not have a completed_at time: {:#?}", check_run);
                                        continue;
                                    }

                                    // Modify the event.
                                    cr.time = check_run.completed_at.unwrap();
                                    cr.action = "completed".to_string();
                                    cr.status = "completed".to_string();
                                    cr.conclusion = json!(check_run.conclusion).to_string().trim_matches('"').to_string();

                                    // Check if this event already exists.
                                    // Let's see if the data we wrote is there.
                                    let exists = client.check_exists(table, cr.time, cr.github_id, &cr.action, &cr.sha).await;

                                    if !exists {
                                        // Add the event.
                                        client.query(cr, table).await;
                                    }
                                }
                            }
                        }
                    });

                    // Add this handle to our stack of handles.
                    inner_handles.push(inner_handle);
                }

                // Wait for all the handles.
                for inner_handle in inner_handles {
                    inner_handle.await.unwrap_or_else(|e| println!("[warn]: handle failed: {:#?}]", e));
                }
            });

            // Add this handle to our stack of handles.
            handles.push(handle);
        }

        // Wait for all the handles.
        for handle in handles {
            handle.await.unwrap_or_else(|e| println!("[warn]: handle failed: {:#?}]", e));
        }
    }

    pub async fn update_pull_request_events(&self) {
        let github = authenticate_github_jwt();
        let repos = list_all_github_repos(&github).await;

        // For each repo, get information on the pull requests.
        for repo in repos {
            let repo_name = repo.name.to_string();

            let r = github.repo(repo.owner.login.to_string(), repo.name.to_string());

            let pulls = r
                .pulls()
                .iter(&hubcaps::pulls::PullListOptions::builder().state(hubcaps::issues::State::All).per_page(100).build())
                .try_collect::<Vec<hubcaps::pulls::Pull>>()
                .await
                .unwrap();

            for pull in pulls {
                // Add events for each pull request if it does not already exist.
                let github_id = pull.id.to_string().parse::<i64>().unwrap();
                let table = EventType::PullRequest.name();
                let sender = pull.user.login.to_string();
                let number = pull.number.to_string().parse::<i64>().unwrap();

                // Create the event.
                let mut pr = PullRequest {
                    time: pull.created_at,
                    repo_name: repo_name.to_string(),
                    sender: sender.to_string(),

                    action: "opened".to_string(),
                    head_reference: pull.head.commit_ref.to_string(),
                    base_reference: pull.base.commit_ref.to_string(),

                    number,
                    github_id,
                    merged: false,
                };

                // Check if this event already exists.
                // Let's see if the data we wrote is there.
                let exists = self.event_exists(table, pr.time, pr.github_id, &pr.action).await;

                if !exists {
                    // Add the event.
                    self.query(pr.clone(), table).await;
                }

                if pull.closed_at.is_some() {
                    let mut closed_at = pull.closed_at.unwrap();
                    let mut closed_by = sender.to_string();
                    if closed_by.is_empty() {
                        closed_by = pull.closed_by.login.to_string();
                    }

                    if pull.merged_at.is_some() {
                        closed_at = pull.merged_at.unwrap();
                        if !pull.merged_by.login.is_empty() {
                            closed_by = pull.merged_by.login.to_string();
                        }
                    }

                    // Modify the event.
                    pr.time = closed_at;
                    pr.sender = closed_by;
                    pr.action = "closed".to_string();
                    pr.merged = pull.merged_at.is_some();

                    // Check if we already have the event.
                    let exists = self.event_exists(table, pr.time, pr.github_id, &pr.action).await;

                    if !exists {
                        // Add the event.
                        self.query(pr.clone(), table).await;
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
                    .map_err(|e| println!("[warn]: iterating over review comment in repo {} for pull {} failed: {}", repo.name.to_string(), pull.number, e))
                    .unwrap_or_default();

                for pull_comment in pull_comments {
                    // Add events for each pull comment if it does not already exist.
                    let github_id = pull_comment.id.to_string().parse::<i64>().unwrap();
                    let table = EventType::PullRequestReviewComment.name();

                    // Create the event.
                    let pc = PullRequestReviewComment {
                        time: pull_comment.created_at,
                        repo_name: repo_name.to_string(),
                        sender: pull_comment.user.login.to_string(),
                        action: "created".to_string(),
                        comment: pull_comment.body.to_string(),

                        pull_request_number: pr.number,
                        github_id,
                    };

                    // Check if this event already exists.
                    // Let's see if the data we wrote is there.
                    let exists = self.event_exists(table, pc.time, pc.github_id, &pc.action).await;

                    if !exists {
                        // Add the event.
                        self.query(pc, table).await;
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
