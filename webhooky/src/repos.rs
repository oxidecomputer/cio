#![allow(clippy::upper_case_acronyms)]
use std::{fmt, str::FromStr};

/// GitHub repos.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum Repo {
    Configs,
    RFD,
    /// Any non-predefined repo
    Other(String)
}

impl Repo {
    /// Returns a string for the repo name.
    pub fn name(&self) -> &str {
        match self {
            Repo::Configs => "configs",
            Repo::RFD => "rfd",
            Repo::Other(repo_name) => repo_name.as_str()
        }
    }
}

impl FromStr for Repo {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "configs" => Ok(Repo::Configs),
            "rfd" => Ok(Repo::RFD),
            _ => {
                println!("invalid GitHub repo: `{}`", s);
                Ok(Repo::Other(s.to_string()))
            }
        }
    }
}

impl fmt::Display for Repo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}
