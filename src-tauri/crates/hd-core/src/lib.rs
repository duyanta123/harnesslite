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
pub mod paths;

/// Workspace version of the HarnessLite shell this crate belongs to.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn version_matches_workspace_baseline() {
        assert_eq!(super::VERSION, "0.1.0");
    }
}
