//! The plugins data domain.
//!
//! Plugins are ordinary npm packages that declare a dsh/cordis profile patch.
//! This crate owns the *records*: manifest reading, the disabled-plugin
//! switches, install receipts, and the catalog sources. The npm work itself —
//! preflight installs, `dsh plugin add/remove` — belongs to the runtime layer;
//! this module never spawns a process or touches the network.

pub mod catalog;
pub mod detail;
pub mod preflight;
pub mod manifest;
pub mod sources;
pub mod switches;

pub use catalog::{is_exact_version, CatalogEntry};
pub use detail::PackageDetail;
pub use manifest::{add_dependency, bundles, dependencies, is_package_name, is_package_spec, list, read_manifest, remove_dependency, InstalledPlugin};
pub use sources::{admit_custom, CustomSource, Source, SourceId, Store as SourceStore, TrustTier};
pub use switches::Store as SwitchStore;
