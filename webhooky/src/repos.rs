#![allow(clippy::upper_case_acronyms)]
use std::fmt;
use std::str::FromStr;

/// GitHub repos.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum Repo {
    /// (Special repo.) Any repo.
    Wildcard,

    Configs,
    RFD,
}

impl Repo {
    /// Returns a static string for the repo name.
    pub fn name(self) -> &'static str {
        match self {
            Repo::Wildcard => "*",
            Repo::Configs => "configs",
            Repo::RFD => "rfd",
        }
    }
}

impl FromStr for Repo {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "*" => Ok(Repo::Wildcard),
            "configs" => Ok(Repo::Configs),
            "rfd" => Ok(Repo::RFD),
            _ => {
                println!("invalid GitHub repo: `{}`", s);
                Ok(Repo::Wildcard)
            }
        }
    }
}

impl fmt::Display for Repo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}
