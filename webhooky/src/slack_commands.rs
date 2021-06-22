use std::fmt;
use std::str::FromStr;

/// Slack commands.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum SlackCommand {
    RFD,

    Meet,

    Applicants,

    Applicant,

    Papers,

    Paper,
}

impl SlackCommand {
    /// Returns a static string for the command.
    pub fn name(self) -> &'static str {
        match self {
            SlackCommand::RFD => "/rfd",
            SlackCommand::Meet => "/meet",
            SlackCommand::Applicants => "/applicants",
            SlackCommand::Applicant => "/applicant",
            SlackCommand::Papers => "/papers",
            SlackCommand::Paper => "/paper",
        }
    }
}

impl FromStr for SlackCommand {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "/rfd" => Ok(SlackCommand::RFD),
            "/meet" => Ok(SlackCommand::Meet),
            "/applicants" => Ok(SlackCommand::Applicants),
            "/applicant" => Ok(SlackCommand::Applicant),
            "/papers" => Ok(SlackCommand::Papers),
            "/paper" => Ok(SlackCommand::Paper),
            _ => Err(format!("invalid Slack command: `{}`", s)),
        }
    }
}

impl fmt::Display for SlackCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}
