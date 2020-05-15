#[macro_use]
extern crate clap;
use clap::App;
use simplelog::{CombinedLogger, Config as LogConfig, LevelFilter, TermLogger, TerminalMode};

use configs::airtable_cmd::cmd_airtable_run;
use configs::applications::cmd_applications_run;
use configs::gsuite::cmd_gsuite_run;
use configs::repos::cmd_repos_run;
use configs::shorturls::cmd_shorturls_run;
use configs::tables::cmd_tables_run;
use configs::teams::cmd_teams_run;
use configs::zoom_cmd::cmd_zoom_run;

fn main() {
    let _ = CombinedLogger::init(vec![
        TermLogger::new(LevelFilter::Info, LogConfig::default(), TerminalMode::Mixed).unwrap(),
        TermLogger::new(LevelFilter::Warn, LogConfig::default(), TerminalMode::Mixed).unwrap(),
    ]);

    // Initialize clap.
    // The YAML file is found relative to the current file, similar to how modules are found.
    let cli_yaml = load_yaml!("cli.yml");
    let cli_matches = App::from_yaml(cli_yaml).get_matches();

    if let Some(_) = cli_matches.subcommand_matches("airtable") {
        cmd_airtable_run();
    }

    if let Some(cli_matches) = cli_matches.subcommand_matches("applications") {
        cmd_applications_run(cli_matches);
    }

    if let Some(cli_matches) = cli_matches.subcommand_matches("gsuite") {
        cmd_gsuite_run(cli_matches);
    }

    if let Some(cli_matches) = cli_matches.subcommand_matches("repos") {
        cmd_repos_run(cli_matches);
    }

    if let Some(cli_matches) = cli_matches.subcommand_matches("shorturls") {
        cmd_shorturls_run(cli_matches);
    }

    if let Some(cli_matches) = cli_matches.subcommand_matches("tables") {
        cmd_tables_run(cli_matches);
    }

    if let Some(cli_matches) = cli_matches.subcommand_matches("teams") {
        cmd_teams_run(cli_matches);
    }

    if let Some(cli_matches) = cli_matches.subcommand_matches("zoom") {
        cmd_zoom_run(cli_matches);
    }
}
