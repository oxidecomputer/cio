use std::fmt;
use std::str::FromStr;

/// GitHub events that are specified in the X-Github-Event header.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum EventType {
    /// (Special event.) Any time any event is triggered (Wildcard Event).
    Wildcard,

    /// (Special event.) Sent when a webhook is added.
    Ping,

    /// Triggered when a check run is `created`, `rerequested`, `completed`, or
    /// has a `requested_action`.
    CheckRun,

    /// Triggered when a check suite is `completed`, `requested`, or
    /// `rerequested`.
    CheckSuite,

    /// Any time a Commit is commented on.
    CommitComment,

    /// Triggered when the body or comment of an issue or pull request includes
    /// a URL that matches a configured content reference domain. Only GitHub
    /// Apps can receive this event.
    ContentReference,

    /// Any time a Branch or Tag is created.
    Create,

    /// Any time a Branch or Tag is deleted.
    Delete,

    /// Any time a Repository has a new deployment created from the API.
    Deployment,

    /// Any time a deployment for a Repository has a status update from the
    /// API.
    DeploymentStatus,

    /// Any time a Repository is forked.
    Fork,

    /// Triggered when someone revokes their authorization of a GitHub App.
    GitHubAppAuthorization,

    /// Any time a Wiki page is updated.
    Gollum,

    /// Any time a GitHub App is installed or uninstalled.
    Installation,

    /// Same as `Installation`, but deprecated. This event is sent alongside
    /// the `Installation` event, but can always be ignored.
    IntegrationInstallation,

    /// Any time a repository is added or removed from an installation.
    InstallationRepositories,

    /// Same as `InstallationRepositories`, but deprecated. This event is sent
    /// alongside the `InstallationRepositories` event, but can always be
    /// ignored.
    IntegrationInstallationRepositories,

    /// Any time a comment on an issue is created, edited, or deleted.
    IssueComment,

    /// Any time an Issue is assigned, unassigned, labeled, unlabeled,
    /// opened, edited, milestoned, demilestoned, closed, or reopened.
    Issues,

    /// Any time a Label is created, edited, or deleted.
    Label,

    /// Any time a user purchases, cancels, or changes their GitHub
    /// Marketplace plan.
    MarketplacePurchase,

    /// Any time a User is added or removed as a collaborator to a
    /// Repository, or has their permissions modified.
    Member,

    /// Any time a User is added or removed from a team. Organization hooks
    /// only.
    Membership,

    /// Any time a Milestone is created, closed, opened, edited, or deleted.
    Milestone,

    /// Any time a user is added, removed, or invited to an Organization.
    /// Organization hooks only.
    Organization,

    /// Any time an organization blocks or unblocks a user. Organization
    /// hooks only.
    OrgBlock,

    /// Any time a Pages site is built or results in a failed build.
    PageBuild,

    /// Any time a Project Card is created, edited, moved, converted to an
    /// issue, or deleted.
    ProjectCard,

    /// Any time a Project Column is created, edited, moved, or deleted.
    ProjectColumn,

    /// Any time a Project is created, edited, closed, reopened, or deleted.
    Project,

    /// Any time a Repository changes from private to public.
    Public,

    /// Any time a pull request is assigned, unassigned, labeled, unlabeled,
    /// opened, edited, closed, reopened, or synchronized (updated due to a
    /// new push in the branch that the pull request is tracking). Also any
    /// time a pull request review is requested, or a review request is
    /// removed.
    PullRequest,

    /// Any time a comment on a pull request's unified diff is created,
    /// edited, or deleted (in the Files Changed tab).
    PullRequestReviewComment,

    /// Any time a pull request review is submitted, edited, or dismissed.
    PullRequestReview,

    /// Any Git push to a Repository, including editing tags or branches.
    /// Commits via API actions that update references are also counted.
    /// This is the default event.
    Push,

    /// Any time a Release is published in a Repository.
    Release,

    /// Any time a Repository is created, deleted (organization hooks
    /// only), archived, unarchived, made public, or made private.
    Repository,

    /// Triggered when a successful, cancelled, or failed repository import
    /// finishes for a GitHub organization or a personal repository. To receive
    /// this event for a personal repository, you must create an empty
    /// repository prior to the import. This event can be triggered using
    /// either the GitHub Importer or the Source imports API.
    RepositoryImport,

    /// Triggered when a security alert is created, dismissed, or resolved.
    RepositoryVulnerabilityAlert,

    /// Triggered when a new security advisory is published, updated, or
    /// withdrawn. A security advisory provides information about
    /// security-related vulnerabilities in software on GitHub. Security
    /// Advisory webhooks are available to GitHub Apps only.
    SecurityAdvisory,

    /// Any time a Repository has a status update from the API.
    Status,

    /// Any time a team is created, deleted, modified, or added to or
    /// removed from a repository. Organization hooks only
    Team,

    /// Any time a team is added or modified on a Repository.
    TeamAdd,

    /// Any time a User stars a Repository.
    Watch,

    /// When a GitHub Actions workflow run is requested or completed.
    WorkflowRun,
}

impl EventType {
    /// Returns a static string for the event name.
    pub fn name(self) -> &'static str {
        match self {
            EventType::Wildcard => "*",
            EventType::Ping => "ping",
            EventType::CheckRun => "check_run",
            EventType::CheckSuite => "check_suite",
            EventType::CommitComment => "commit_comment",
            EventType::ContentReference => "content_reference",
            EventType::Create => "create",
            EventType::Delete => "delete",
            EventType::Deployment => "deployment",
            EventType::DeploymentStatus => "deployment_status",
            EventType::Fork => "fork",
            EventType::GitHubAppAuthorization => "github_app_authorization",
            EventType::Gollum => "gollum",
            EventType::Installation => "installation",
            EventType::IntegrationInstallation => "integration_installation",
            EventType::InstallationRepositories => "installation_repositories",
            EventType::IntegrationInstallationRepositories => {
                "integration_installation_repositories"
            }
            EventType::IssueComment => "issue_comment",
            EventType::Issues => "issues",
            EventType::Label => "label",
            EventType::MarketplacePurchase => "marketplace_purchase",
            EventType::Member => "member",
            EventType::Membership => "membership",
            EventType::Milestone => "milestone",
            EventType::Organization => "organization",
            EventType::OrgBlock => "org_block",
            EventType::PageBuild => "page_build",
            EventType::ProjectCard => "project_card",
            EventType::ProjectColumn => "project_column",
            EventType::Project => "project",
            EventType::Public => "public",
            EventType::PullRequest => "pull_request",
            EventType::PullRequestReview => "pull_request_review",
            EventType::PullRequestReviewComment => {
                "pull_request_review_comment"
            }
            EventType::Push => "push",
            EventType::Release => "release",
            EventType::Repository => "repository",
            EventType::RepositoryImport => "repository_import",
            EventType::RepositoryVulnerabilityAlert => {
                "repository_vulnerability_alert"
            }
            EventType::SecurityAdvisory => "security_advisory",
            EventType::Status => "status",
            EventType::Team => "team",
            EventType::TeamAdd => "team_add",
            EventType::Watch => "watch",
            EventType::WorkflowRun => "workflow_run",
        }
    }
}

impl FromStr for EventType {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "*" => Ok(EventType::Wildcard),
            "ping" => Ok(EventType::Ping),
            "check_run" => Ok(EventType::CheckRun),
            "check_suite" => Ok(EventType::CheckSuite),
            "commit_comment" => Ok(EventType::CommitComment),
            "content_reference" => Ok(EventType::ContentReference),
            "create" => Ok(EventType::Create),
            "delete" => Ok(EventType::Delete),
            "deployment" => Ok(EventType::Deployment),
            "deployment_status" => Ok(EventType::DeploymentStatus),
            "fork" => Ok(EventType::Fork),
            "github_app_authorization" => Ok(EventType::GitHubAppAuthorization),
            "gollum" => Ok(EventType::Gollum),
            "installation" => Ok(EventType::Installation),
            "integration_installation" => {
                Ok(EventType::IntegrationInstallation)
            }
            "installation_repositories" => {
                Ok(EventType::InstallationRepositories)
            }
            "integration_installation_repositories" => {
                Ok(EventType::IntegrationInstallationRepositories)
            }
            "issue_comment" => Ok(EventType::IssueComment),
            "issues" => Ok(EventType::Issues),
            "label" => Ok(EventType::Label),
            "marketplace_purchase" => Ok(EventType::MarketplacePurchase),
            "member" => Ok(EventType::Member),
            "membership" => Ok(EventType::Membership),
            "milestone" => Ok(EventType::Milestone),
            "organization" => Ok(EventType::Organization),
            "org_block" => Ok(EventType::OrgBlock),
            "page_build" => Ok(EventType::PageBuild),
            "project_card" => Ok(EventType::ProjectCard),
            "project_column" => Ok(EventType::ProjectColumn),
            "project" => Ok(EventType::Project),
            "public" => Ok(EventType::Public),
            "pull_request" => Ok(EventType::PullRequest),
            "pull_request_review_comment" => {
                Ok(EventType::PullRequestReviewComment)
            }
            "pull_request_review" => Ok(EventType::PullRequestReview),
            "push" => Ok(EventType::Push),
            "release" => Ok(EventType::Release),
            "repository" => Ok(EventType::Repository),
            "repository_import" => Ok(EventType::RepositoryImport),
            "repository_vulnerability_alert" => {
                Ok(EventType::RepositoryVulnerabilityAlert)
            }
            "security_advisory" => Ok(EventType::SecurityAdvisory),
            "status" => Ok(EventType::Status),
            "team" => Ok(EventType::Team),
            "team_add" => Ok(EventType::TeamAdd),
            "watch" => Ok(EventType::Watch),
            "workflow_run" => Ok(EventType::WorkflowRun),
            _ => {
                println!("invalid GitHub event: `{}`", s);
                Ok(EventType::Wildcard)
            }
        }
    }
}

impl fmt::Display for EventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}
