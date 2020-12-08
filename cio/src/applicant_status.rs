use std::str::FromStr;

/// The various different statuses that an applicant can be in.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum Status {
    /// The applicant has been hired.
    Hired,

    /// The applicant has been deferred.
    Deferred,

    /// We are taking next steps with the applicant.
    NextSteps,

    /// The applicant has been declined.
    Declined,

    /// The applicant needs to be triaged.
    NeedsToBeTriaged,

    /// The applicant has been hired as a contractor.
    Contractor,

    /// We are keeping the applicant warm.
    KeepingWarm,
}

impl Default for Status {
    fn default() -> Self {
        Status::NeedsToBeTriaged
    }
}

impl FromStr for Status {
    type Err = &'static str;

    fn from_str(status: &str) -> Result<Self, Self::Err> {
        let s = status.to_lowercase();

        if s.contains("next steps") {
            Ok(Status::NextSteps)
        } else if s.contains("deferred") {
            Ok(Status::Deferred)
        } else if s.contains("declined") {
            Ok(Status::Declined)
        } else if s.contains("hired") {
            Ok(Status::Hired)
        } else if s.contains("contractor") || s.contains("consulting") {
            Ok(Status::Contractor)
        } else if s.contains("keeping warm") {
            Ok(Status::KeepingWarm)
        } else {
            Ok(Status::NeedsToBeTriaged)
        }
    }
}

impl ToString for Status {
    fn to_string(&self) -> String {
        match self {
            Status::NextSteps => "Next steps".to_string(),
            Status::Deferred => "Deferred".to_string(),
            Status::Declined => "Declined".to_string(),
            Status::Hired => "Hired".to_string(),
            Status::Contractor => "Contractor".to_string(),
            Status::KeepingWarm => "Keeping warm".to_string(),
            Status::NeedsToBeTriaged => "Needs to be triaged".to_string(),
        }
    }
}
