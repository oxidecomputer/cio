/*!
 * configs: A multi-purpose tool made for automating the role of the Chief
 * Infrastructure Officer. See README for details.
 */
pub mod airtable;
pub mod applications;
pub mod core;
pub mod gsuite;
pub mod product_huddle;
pub mod repos;
pub mod shorturls;
pub mod tables;
pub mod teams;
pub mod utils;
pub mod zoom;

#[macro_use]
extern crate clap;
use clap::App;
use simplelog::{
    CombinedLogger, Config as LogConfig, LevelFilter, SharedLogger,
    SimpleLogger, TermLogger, TerminalMode,
};

use crate::airtable::cmd_airtable_run;
use crate::applications::cmd_applications_run;
use crate::gsuite::cmd_gsuite_run;
use crate::product_huddle::cmd_product_huddle_run;
use crate::repos::cmd_repos_run;
use crate::shorturls::cmd_shorturls_run;
use crate::tables::cmd_tables_run;
use crate::teams::cmd_teams_run;
use crate::zoom::cmd_zoom_run;

fn main() {
    // Set up the logger, we can't use TermLogger in GitHub actions.
    let mut loggers: Vec<Box<dyn SharedLogger>> = vec![
        SimpleLogger::new(LevelFilter::Info, LogConfig::default()),
        SimpleLogger::new(LevelFilter::Warn, LogConfig::default()),
    ];
    match TermLogger::new(
        LevelFilter::Info,
        LogConfig::default(),
        TerminalMode::Mixed,
    ) {
        Some(term_logger) => {
            loggers = vec![
                term_logger,
                TermLogger::new(
                    LevelFilter::Warn,
                    LogConfig::default(),
                    TerminalMode::Mixed,
                )
                .unwrap(),
            ];
        }
        None => (),
    };
    let _ = CombinedLogger::init(loggers);

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

    if let Some(_) = cli_matches.subcommand_matches("product-huddle") {
        cmd_product_huddle_run();
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
