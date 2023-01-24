use clap::Parser;

/// This doc string acts as a help message when the user runs '--help'
/// as do all doc strings on fields.
#[derive(Parser, Debug, Clone)]
#[clap(version = clap::crate_version!(), author = clap::crate_authors!("\n"))]
pub struct Opts {
    /// Print debug info
    #[clap(short, long)]
    pub debug: bool,

    /// Print logs as json
    #[clap(short, long)]
    pub json: bool,

    #[clap(subcommand)]
    pub subcmd: SubCommand,
}

#[derive(Parser, Debug, Clone)]
pub enum SubCommand {
    Server(Server),

    CreateServerSpec(SpecOut),
    SendRFDChangelog(SendRFDChangelog),
    SyncAnalytics(SyncAnalytics),
    #[clap(name = "sync-api-tokens")]
    SyncAPITokens(SyncAPITokens),
    SyncApplications(SyncApplications),
    SyncAssetInventory(SyncAssetInventory),
    SyncCompanies(SyncCompanies),
    SyncConfigs(SyncConfigs),
    SyncFinance(SyncFinance),
    SyncFunctions(SyncFunctions),
    SyncHuddles(SyncHuddles),
    SyncInterviews(SyncInterviews),
    SyncJournalClubs(SyncJournalClubs),
    SyncMailingLists(SyncMailingLists),
    SyncOther(SyncOther),
    SyncRecordedMeetings(SyncRecordedMeetings),
    SyncRepos(SyncRepos),
    #[clap(name = "sync-rfds")]
    SyncRFDs(SyncRFDs),
    SyncShipments(SyncShipments),
    SyncShorturls(SyncShorturls),
    SyncSwagInventory(SyncSwagInventory),
    SyncTravel(SyncTravel),
    SyncZoho(SyncZoho),
}

/// A subcommand for running the server.
#[derive(Parser, Clone, Debug)]
pub struct Server {
    /// IP address and port that the server should listen
    #[clap(short, long, default_value = "0.0.0.0:8080")]
    pub address: String,

    /// Sets if the server should run cron jobs in the background
    #[clap(long)]
    pub do_cron: bool,
}

/// A subcommand for outputting the Open API spec file for the server
#[derive(Parser, Clone, Debug)]
pub struct SpecOut {
    /// Sets an optional output file for the API spec
    #[clap(parse(from_os_str), value_hint = clap::ValueHint::FilePath)]
    pub spec_file: std::path::PathBuf,
}

/// A subcommand for sending the RFD changelog.
#[derive(Parser, Clone, Debug)]
pub struct SendRFDChangelog {}

/// A subcommand for running the background job of syncing analytics.
#[derive(Parser, Debug, Clone)]
pub struct SyncAnalytics {}

/// A subcommand for running the background job of syncing API tokens.
#[derive(Parser, Debug, Clone)]
pub struct SyncAPITokens {}

/// A subcommand for running the background job of syncing applications.
#[derive(Parser, Debug, Clone)]
pub struct SyncApplications {}

/// A subcommand for running the background job of syncing asset inventory.
#[derive(Parser, Debug, Clone)]
pub struct SyncAssetInventory {}

/// A subcommand for running the background job of syncing companies.
#[derive(Parser, Debug, Clone)]
pub struct SyncCompanies {}

/// A subcommand for running the background job of syncing configs.
#[derive(Parser, Debug, Clone)]
pub struct SyncConfigs {}

/// A subcommand for running the background job of syncing finance data.
#[derive(Parser, Debug, Clone)]
pub struct SyncFinance {}

/// A subcommand for running the background job of syncing functions.
#[derive(Parser, Debug, Clone)]
pub struct SyncFunctions {}

/// A subcommand for running the background job of syncing interviews.
#[derive(Parser, Debug, Clone)]
pub struct SyncInterviews {}

/// A subcommand for running the background job of syncing huddles.
#[derive(Parser, Debug, Clone)]
pub struct SyncHuddles {}

/// A subcommand for running the background job of syncing journal clubs.
#[derive(Parser, Debug, Clone)]
pub struct SyncJournalClubs {}

/// A subcommand for running the background job of syncing mailing lists.
#[derive(Parser, Debug, Clone)]
pub struct SyncMailingLists {}

/// A subcommand for running the background job of syncing other things.
#[derive(Parser, Debug, Clone)]
pub struct SyncOther {}

/// A subcommand for running the background job of syncing recorded_meetings.
#[derive(Parser, Debug, Clone)]
pub struct SyncRecordedMeetings {}

/// A subcommand for running the background job of syncing repos.
#[derive(Parser, Debug, Clone)]
pub struct SyncRepos {}

/// A subcommand for running the background job of syncing RFDs.
#[derive(Parser, Debug, Clone)]
pub struct SyncRFDs {}

/// A subcommand for running the background job of syncing shipments.
#[derive(Parser, Debug, Clone)]
pub struct SyncShipments {}

/// A subcommand for running the background job of syncing shorturls.
#[derive(Parser, Debug, Clone)]
pub struct SyncShorturls {}

/// A subcommand for running the background job of syncing swag inventory.
#[derive(Parser, Debug, Clone)]
pub struct SyncSwagInventory {}

/// A subcommand for running the background job of syncing travel data.
#[derive(Parser, Debug, Clone)]
pub struct SyncTravel {}

/// A subcommand for running the background job of syncing Zoho leads.
#[derive(Parser, Debug, Clone)]
pub struct SyncZoho {}

pub fn into_job_command(cmd: &str) -> Option<SubCommand> {
    match cmd {
        "send-rfd-changelog" => Some(SubCommand::SendRFDChangelog(SendRFDChangelog {})),
        "sync-analytics" => Some(SubCommand::SyncAnalytics(SyncAnalytics {})),
        "sync-api-tokens" => Some(SubCommand::SyncAPITokens(SyncAPITokens {})),
        "sync-applications" => Some(SubCommand::SyncApplications(SyncApplications {})),
        "sync-asset-inventory" => Some(SubCommand::SyncAssetInventory(SyncAssetInventory {})),
        "sync-companies" => Some(SubCommand::SyncCompanies(SyncCompanies {})),
        "sync-configs" => Some(SubCommand::SyncConfigs(SyncConfigs {})),
        "sync-finance" => Some(SubCommand::SyncFinance(SyncFinance {})),
        "sync-functions" => Some(SubCommand::SyncFunctions(SyncFunctions {})),
        "sync-huddles" => Some(SubCommand::SyncHuddles(SyncHuddles {})),
        "sync-interviews" => Some(SubCommand::SyncInterviews(SyncInterviews {})),
        "sync-journal-clubs" => Some(SubCommand::SyncJournalClubs(SyncJournalClubs {})),
        "sync-mailing-lists" => Some(SubCommand::SyncMailingLists(SyncMailingLists {})),
        "sync-other" => Some(SubCommand::SyncOther(SyncOther {})),
        "sync-recorded-meetings" => Some(SubCommand::SyncRecordedMeetings(SyncRecordedMeetings {})),
        "sync-repos" => Some(SubCommand::SyncRepos(SyncRepos {})),
        "sync-rfds" => Some(SubCommand::SyncRFDs(SyncRFDs {})),
        "sync-shipments" => Some(SubCommand::SyncShipments(SyncShipments {})),
        "sync-shorturls" => Some(SubCommand::SyncShorturls(SyncShorturls {})),
        "sync-swag-inventory" => Some(SubCommand::SyncSwagInventory(SyncSwagInventory {})),
        "sync-travel" => Some(SubCommand::SyncTravel(SyncTravel {})),
        "sync-zoho" => Some(SubCommand::SyncZoho(SyncZoho {})),
        _ => None,
    }
}
