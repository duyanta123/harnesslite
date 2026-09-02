//! The error vocabulary every domain shares.
//!
//! One enum rather than per-domain types: the shell layer reports these
//! verbatim to the frontend, and a sentence written where the failure happened
//! is worth more to the person reading it than a taxonomy is.

use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Project(String),
    #[error("{0}")]
    Profile(String),
    #[error("{0}")]
    Plugin(String),
    #[error("{0}")]
    Session(String),
    #[error("{0}")]
    Node(String),
    #[error("{0}")]
    Harness(String),
    #[error("{0}")]
    Contract(String),
    #[error("{0}")]
    Store(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    /// The path whose access failed, when the error carries one.
    pub fn path(&self) -> Option<&Path> {
        match self {
            Error::Io(cause) => cause
                .raw_os_error()
                .and_then(|_| Some(Path::new(cause.to_string().as_str()))),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_errors_render_as_their_sentence() {
        let error = Error::Profile("there is no profile called web2".into());
        assert_eq!(error.to_string(), "there is no profile called web2");
    }
}
