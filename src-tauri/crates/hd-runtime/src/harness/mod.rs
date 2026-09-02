//! The harness runtime layer: keeping one `dsh web` process alive and
//! observable.
//!
//! Everything here owns process lifecycle — install transactions, bounded
//! startup, streamed output, crash detection, backoff restart, process-tree
//! reclamation. Pure data logic stays in `hd-core`; anything that spawns or
//! listens lives here.

pub mod health;
pub mod logging;
pub mod readiness;
pub mod supervisor;

/// Version of the managed runtime this shell installs and supervises.
pub const VERSION: &str = hd_core::contract::DSH_VERSION;
