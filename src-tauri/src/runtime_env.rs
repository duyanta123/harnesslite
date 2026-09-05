//! Shell-side assembly of the runtime picture.
//!
//! The glue between the pure domains (hd-core), the process layer (hd-runtime)
//! and the Tauri commands: what this machine can run, and the launch plan that
//! turns it into a supervised harness.

use std::path::PathBuf;

use hd_runtime::harness::supervisor::LaunchPlan;
use node_runtime::{NodeInstallation, Version};
use serde::Serialize;

use hd_core::contract;
use hd_core::error::{Error, Result};
use hd_core::paths;

/// Loopback only. Binding anywhere else would expose an agent that can run
/// shell commands to the local network, so it is not a setting.
const BIND_HOST: &str = "127.0.0.1";

/// Where the runtime-owned integration patch lives once the harness is
/// installed. Supplied per process with `--patch`, never persisted into the
/// user's bundle stack.
pub fn integration_patch() -> Option<PathBuf> {
    let patch = paths::harness_dir()
        .join("node_modules")
        .join("@duyanta123")
        .join("harnesslite-integration")
        .join("cordis.patch.yml");
    patch.is_file().then_some(patch)
}

/// The installed harness version, read from the package the shell manages.
pub fn runtime_version() -> Option<String> {
    let manifest = paths::harness_dir()
        .join("node_modules")
        .join("@deepseek-ai")
        .join("dsh")
        .join("package.json");
    let body = std::fs::read(manifest).ok()?;
    let value: serde_json::Value = serde_json::from_slice(&body).ok()?;
    value
        .get("version")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

pub fn harness_compatible() -> bool {
    runtime_version().as_deref() == Some(contract::DSH_VERSION)
}

/// Whether this machine can currently run the harness, and what is missing.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Environment {
    /// Best usable runtime, or `None` when nothing qualifies.
    pub node: Option<NodeInstallation>,
    /// Every runtime found, so the UI can explain why one was rejected.
    pub all_node_runtimes: Vec<NodeInstallation>,
    pub minimum_node: Version,
    pub harness_installed: bool,
    pub harness_compatible: bool,
    pub harness_version: Option<String>,
    pub expected_harness_version: String,
    /// An interrupted install that could not be recovered automatically.
    pub harness_problem: Option<String>,
    pub harness_entry: PathBuf,
    /// The project the next Harness start serves.
    pub project: String,
    pub workspace: PathBuf,
    pub workspace_admission: hd_core::validate::Admission,
}

/// Inspect the machine. Cheap enough to call whenever the UI needs it.
pub fn environment() -> Environment {
    // An install interrupted by a crash is repaired by the next probe that
    // sees its journal — here — so no UI action is ever needed to clear one.
    // A recovery that itself fails is the only "problem" this surface reports.
    let harness_problem = hd_runtime::harness::install::recover_managed_install()
        .err()
        .map(|failure| failure.to_string());

    // The shell's own store is searched alongside the version managers, so a
    // runtime it installed is chosen by exactly the same rule as one the user
    // installed — and shows up in the same list, labelled for what it is.
    let all_node_runtimes = node_runtime::discover_in(Some(&paths::managed_node_dir()));
    let node = all_node_runtimes
        .iter()
        .find(|install| install.version >= node_runtime::MINIMUM_SUPPORTED)
        .cloned();
    let harness_entry = paths::harness_entry();
    let harness_version = runtime_version();
    let harness_installed = harness_entry.is_file() && harness_version.is_some();

    let project = hd_core::projects::active()
        .map(|project| project.name)
        .unwrap_or_else(|| "Default project".to_string());
    let workspace = hd_core::projects::active_workspace()
        .unwrap_or_else(paths::default_workspace_dir);
    let workspace_admission = hd_core::validate::inspect(&workspace);

    Environment {
        node,
        all_node_runtimes,
        minimum_node: node_runtime::MINIMUM_SUPPORTED,
        harness_installed,
        harness_compatible: harness_compatible(),
        harness_version,
        expected_harness_version: contract::DSH_VERSION.to_string(),
        harness_problem,
        harness_entry,
        project,
        workspace,
        workspace_admission,
    }
}

/// Turn the current environment into a runnable launch, or say what is missing.
pub fn launch_plan() -> Result<LaunchPlan> {    let environment = environment();

    let node = environment.node.clone().ok_or_else(|| {
        Error::Node(format!(
            "no usable Node runtime found; HarnessLite needs at least {}.{}.{}",
            node_runtime::MINIMUM_SUPPORTED.major,
            node_runtime::MINIMUM_SUPPORTED.minor,
            node_runtime::MINIMUM_SUPPORTED.patch,
        ))
    })?;
    if !environment.harness_installed {
        return Err(Error::Harness(
            "the Harness runtime is not installed yet; install it first".into(),
        ));
    }
    if !environment.harness_compatible {
        return Err(Error::Harness(format!(
            "the installed Harness runtime is {}, but HarnessLite requires {}; reinstall it from the Environment panel",
            environment.harness_version.as_deref().unwrap_or("unknown"),
            contract::DSH_VERSION,
        )));
    }
    if environment.workspace_admission.blocked() {
        return Err(Error::Harness(
            environment
                .workspace_admission
                .reason
                .unwrap_or_else(|| "the workspace is not safe to use".into()),
        ));
    }
    let profile = hd_core::projects::active_profile()
        .unwrap_or_else(hd_core::profiles::selected);

    // Two runtime-owned patch layers, in order: the integration seam, then the
    // user's plugin switches. Neither is persisted into the profile's bundle
    // stack; both are supplied per process and re-derived on every launch.
    let mut patches: Vec<PathBuf> = integration_patch().into_iter().collect();
    if let Some(disabled) = crate::plugins::disabled_patch() {
        patches.push(disabled);
    }

    Ok(LaunchPlan {
        node: node.path,
        entry: environment.harness_entry,
        profile,
        patches,
        workspace: environment.workspace,
        host: BIND_HOST.to_string(),
        // A port the user fixed in settings wins; otherwise an OS-assigned one
        // that cannot collide and cannot be guessed from outside. Either way
        // the readiness line decides where the webview points.
        port: crate::startup::harness_port().unwrap_or(0),
        environment: Default::default(),
    })
}

/// Work out how to install — or reinstall at the locked release — the harness.
///
/// Launching only needs Node, but installing needs that exact runtime's npm.
/// A newer Node-only package must not hide an older complete installation.
pub fn install_plan() -> Result<hd_runtime::harness::install::InstallPlan> {
    use hd_runtime::harness::install;

    let environment = environment();
    let supported = environment
        .all_node_runtimes
        .iter()
        .filter(|install| install.version >= node_runtime::MINIMUM_SUPPORTED)
        .collect::<Vec<_>>();
    if supported.is_empty() {
        return Err(Error::Node(format!(
            "no usable Node runtime found; HarnessLite needs at least {}.{}.{}",
            node_runtime::MINIMUM_SUPPORTED.major,
            node_runtime::MINIMUM_SUPPORTED.minor,
            node_runtime::MINIMUM_SUPPORTED.patch,
        )));
    }
    let selected = environment.node.as_ref().map(|node| node.path.as_path());
    let node = selected
        .and_then(|path| {
            supported
                .iter()
                .copied()
                .find(|install| install.path == path && install::npm_cli(&install.path).is_some())
        })
        .or_else(|| {
            supported
                .into_iter()
                .find(|install| install::npm_cli(&install.path).is_some())
        })
        .ok_or_else(|| {
            Error::Node(
                "this Node.js install has no npm next to it, so the harness cannot be installed"
                    .into(),
            )
        })?;

    install::plan(&node.path, paths::harness_dir(), install::SPEC.to_string())
}
