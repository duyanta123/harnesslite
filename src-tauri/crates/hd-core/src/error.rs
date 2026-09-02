//! The error vocabulary every domain shares.
//!
//! One enum rather than per-domain types: the shell layer reports these
//! verbatim to the frontend, and a sentence written where the failure happened
//! is worth more to the person reading it than a taxonomy is.

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_errors_render_as_their_sentence() {
        let error = Error::Profile("there is no profile called web2".into());
        assert_eq!(error.to_string(), "there is no profile called web2");
    }
}
