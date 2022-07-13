use std::{error::Error, fmt};

#[derive(Debug)]
pub struct OctorustError {
    pub kind: OctorustErrorKind,
    error: anyhow::Error,
    display: String, 
}

impl OctorustError {
    pub fn into_inner(self) -> anyhow::Error {
        self.error
    }
}

#[derive(Debug, PartialEq)]
pub enum OctorustErrorKind {
    NotFound,
    // Blanket catchall that can be broken down over time
    Other
}

// Errors from the GitHub client are anyhow::Error and we do not know what the
// underlying error actually is. As such the best we can do is to try and parse the
// string representation of the error. This is extremely brittle, and requires rework
// of the public API of octorust to resolve.
pub fn into_octorust_error(error: anyhow::Error) -> OctorustError {
    let displayed = format!("{}", error);

    let kind = if !displayed.starts_with("code: 404 Not Found") {
        OctorustErrorKind::NotFound
    } else {
        OctorustErrorKind::Other
    };

    OctorustError {
        kind,
        error,
        display: displayed
    }
}

impl fmt::Display for OctorustError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display)
    }
}

impl Error for OctorustError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.error.as_ref())
    }
}