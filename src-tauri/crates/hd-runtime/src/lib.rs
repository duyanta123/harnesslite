//! hd-runtime — HarnessLite's runtime layer.
//!
//! All Node/DSH process-lifecycle complexity concentrates here: Node runtime
//! download and unpacking, the DSH install transaction (staging → promote →
//! backup), the supervisor state machine (readiness parse, health probe,
//! backoff restarts), composition preflight, and offline payloads.
//!
//! Depends on `hd-core` for contracts and domain types; owns every spawned
//! process through `proc-guard`, so a whole tree dies with the shell.

pub mod composition;
pub mod harness;
pub mod market;
pub mod node;
pub mod remote;
pub mod terminal;

/// Workspace version of the HarnessLite shell this crate belongs to.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn version_matches_workspace_baseline() {
        assert_eq!(super::VERSION, "0.1.0");
    }
}
