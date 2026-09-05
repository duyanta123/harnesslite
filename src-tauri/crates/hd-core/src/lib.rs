//! hd-core — HarnessLite's core layer.
//!
//! Pure data-domain logic for the desktop shell: projects, profiles, sessions,
//! plugins, plus the security primitives (atomic writes, path validation,
//! redaction) and the single `contract` module that owns every cross-boundary
//! constant (env var names, protocol numbers, bridge methods).
//!
//! Layer rules, enforced by review and CI:
//! - no `tauri` dependency,
//! - no process spawning,
//! - no network access.
//!
//! Everything here is a pure function or filesystem-domain logic, which is
//! what makes the unit-test suite the specification of record.

pub mod atomic;
pub mod contract;
pub mod error;
pub mod paths;
pub mod presets;
pub mod plugins;
pub mod profiles;
pub mod projects;
pub mod sessions;
pub mod validate;

/// Workspace version of the HarnessLite shell this crate belongs to.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn version_matches_workspace_baseline() {
        // Read against the workspace manifest rather than a spelled-out
        // version: the constant must be whatever the workspace says it is,
        // and a bump should not have to visit every crate to stay true.
        let manifest = include_str!("../../../Cargo.toml");
        let version = manifest
            .lines()
            .find_map(|line| line.strip_prefix("version = \""))
            .and_then(|rest| rest.split('"').next())
            .expect("the workspace manifest declares a version");
        assert_eq!(super::VERSION, version);
    }
}
