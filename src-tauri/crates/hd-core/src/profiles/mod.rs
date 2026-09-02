//! Profiles: the harness's plugin/layer composition unit and credential
//! isolation boundary.
//!
//! A profile is a directory under `~/.dsh/profiles/<name>` holding the
//! harness's pnpm manifest (`package.json` with `dsh.profile.bundles`), a user
//! patch layer (`cordis.patch.yml`) and a pnpm workspace file. The shipped
//! `web` profile is the one with a UI; `headless` is the scriptable one. Both
//! belong to the harness — the shell refuses to rename or delete them.
//!
//! Everything in this module is filesystem-domain logic. Installing packages
//! into a profile is the runtime layer's job; the operations here that need
//! installs (`duplicate`, `import`) return the spec list for it to run.

pub mod selection;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};
use crate::paths;
use crate::plugins as pkg;
use crate::plugins::switches;

/// The profile with a UI, and the selection fallback.
pub const DEFAULT: &str = "web";
/// Profiles the harness ships templates for and will re-create; the shell
/// never renames or removes them.
pub const SHIPPED: [&str; 2] = ["web", "headless"];
/// The harness keeps its shared module tree beside the profiles; not a profile.
const SHARED_MODULES: &str = "node_modules";

const MANIFEST: &str = "package.json";
const PATCH: &str = "cordis.patch.yml";
const WORKSPACE: &str = "pnpm-workspace.yaml";
const EMPTY_PATCH: &str = "[]\n";

const DECLARATION_KIND: &str = "harnesslite-profile";
const DECLARATION_VERSION: u32 = 2;
const MAX_DECLARATION_BYTES: usize = 2 * 1024 * 1024;

/// One profile, as the switcher and the manager show it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub name: String,
    pub dir: PathBuf,
    /// Whether the harness has written a manifest for it yet. A directory
    /// without one is a profile waiting to be initialized, not a broken one.
    pub initialized: bool,
    /// A name the harness ships a template for, and will re-create if it goes.
    pub shipped: bool,
    /// Whether it carries the bundles the shipped web profile carries, which is
    /// what makes a profile one this window can show. Reported, never enforced.
    pub serves_window: bool,
    /// Plugins installed into it, the bundles it came with excluded.
    pub plugins: usize,
    /// How many of those the user has switched off.
    pub disabled: usize,
}

/// Every profile on the machine, and the one this window is pointed at.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Roster {
    pub profiles: Vec<Profile>,
    pub selected: String,
    /// Shown in the manager, because a profile is a directory and the first
    /// thing anyone wants when something is wrong with one is its path.
    pub root: PathBuf,
}

/// How one package stands in one profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Standing {
    /// Not in this profile at all.
    Absent,
    /// Installed, and in the layer stack.
    Active,
    /// Installed, and taken out of the layer stack by the user.
    Disabled,
    /// Installed and never in the layer stack — a plain library.
    Library,
    /// Came with the profile rather than being installed into it.
    Builtin,
}

/// One package, and what the two profiles say about it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Difference {
    pub name: String,
    pub left: Standing,
    pub right: Standing,
    /// The range each side records, empty where there is none to record. Two
    /// profiles can both run a plugin and still not be running the same one.
    pub left_spec: String,
    pub right_spec: String,
    /// Whether the two sides agree about this package, in every respect above.
    pub same: bool,
}

/// Two profiles, side by side.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Comparison {
    pub left: String,
    pub right: String,
    pub rows: Vec<Difference>,
    /// Rows the two profiles disagree about, so the header can say how far apart
    /// they are without counting them again.
    pub differences: usize,
}

/// A profile as a file, for carrying one to another machine.
///
/// A declaration and not an archive. What a profile *has* is packages from a
/// registry and layers from the installation, and both of those are already on
/// the machine reading this file or can be fetched by it — so the file records
/// what was asked for, and the import asks for it again.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Declaration {
    pub kind: String,
    pub version: u32,
    /// The profile it came from, offered as the name to import it under.
    pub name: String,
    /// Plugins as name → range, exactly as the profile recorded them.
    pub plugins: BTreeMap<String, String>,
    /// Which of those were switched off.
    pub disabled: Vec<String>,
    /// The profile's own patch layer, verbatim. The one part of a profile that is
    /// nobody else's copy of anything.
    pub patch: String,
    /// SHA-256 of the canonical declaration fields. This detects damaged or
    /// accidentally edited backups; it is integrity evidence, not a publisher
    /// signature.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub integrity: Option<String>,
    /// Computed while reading, never trusted from or written into the file.
    #[serde(skip)]
    pub verified: bool,
}

#[derive(Serialize)]
struct DeclarationPayload<'a> {
    kind: &'a str,
    version: u32,
    name: &'a str,
    plugins: &'a BTreeMap<String, String>,
    disabled: &'a [String],
    patch: &'a str,
}

/// Every profile on the machine. Cheap; safe to call on every render.
pub fn roster() -> Roster {
    let template = template_bundles();

    Roster {
        profiles: scan()
            .into_iter()
            .map(|name| describe(&name, &template))
            .collect(),
        selected: selected(),
        root: paths::profiles_dir(),
    }
}

/// The profile this window hosts.
///
/// Falls back to the shipped web profile whenever the recorded name is not a
/// profile any more: a directory deleted from a terminal, or a selection file
/// written by hand. The fallback is silent on purpose — the alternative is an
/// application that will not start until someone fixes a JSON file.
pub fn selected() -> String {
    selection::chosen()
}

/// Point this window at another profile.
///
/// Only records the choice. What is already running keeps running, because the
/// layer stack is composed at boot and a restart is the only thing that can
/// change it — and a shell that killed a live session to apply a menu click
/// would be deciding something that is the user's to decide.
pub fn select(name: &str) -> Result<()> {
    let name = expect_profile(name)?;
    selection::choose(&name)
}

/// Make a profile with the interface bundles in it and nothing else.
pub fn create(name: &str) -> Result<()> {
    let bundles = interface_bundles()?;
    build(name, |dir| {
        initialize(dir, &bundles, EMPTY_PATCH, workspace(DEFAULT).as_deref())
    })
}

/// Copy a profile, and say what the copy still has to install to be one.
///
/// The specs come back rather than being installed here so that the process
/// work — finding a package manager, streaming its output, guarding against a
/// second one — stays in the module that already does it for the market.
pub fn duplicate(source: &str, name: &str) -> Result<Vec<String>> {
    let source = expect_profile(source)?;
    let Some(manifest) = pkg::read_manifest(&paths::profile_dir(&source)) else {
        return Err(Error::Profile(format!(
            "{source} has not been initialized yet, so there is nothing to copy"
        )));
    };

    let installed = pkg::dependencies(&manifest);
    let mut specs = Vec::with_capacity(installed.len());
    for (package, range) in &installed {
        let spec = format!("{package}@{range}");
        if !pkg::is_package_spec(&spec) {
            return Err(Error::Profile(format!(
                "{source} installs {package} from {range}, which a copy cannot ask for again"
            )));
        }
        specs.push(spec);
    }

    // Only the bundles that came with the source. Everything it installed is in
    // `specs`, and the harness puts each one back into the layer list itself as
    // it installs it — listing them here first would mean a manifest naming
    // layers that are not on disk yet.
    let carried: Vec<String> = pkg::bundles(&manifest)
        .into_iter()
        .filter(|bundle| !installed.contains_key(bundle))
        .collect();

    build(name, |dir| {
        initialize(
            dir,
            &carried,
            &patch(&source).unwrap_or_else(|| EMPTY_PATCH.to_string()),
            workspace(&source).as_deref(),
        )?;
        switches::copy(&source, name)
    })?;
    Ok(specs)
}

/// Give a profile another name, keeping everything in it.
pub fn rename(from: &str, to: &str) -> Result<()> {
    let from = expect_profile(from)?;
    if SHIPPED.contains(&from.as_str()) {
        return Err(Error::Profile(format!(
            "{from} is one of the harness's own profiles; renaming it would only leave the harness to write a new one"
        )));
    }

    let source = paths::profile_dir(&from);
    let target = free_dir(to)?;
    std::fs::rename(&source, &target).map_err(|cause| {
        Error::Profile(format!(
            "{from} could not be renamed: {cause}. Close anything using it — the harness included — and try again"
        ))
    })?;

    // The manifest carries the name too. The harness only reads it when it makes
    // the profile, so a stale one changes nothing today and misleads whoever
    // opens the file next.
    let mut manifest_renamed = false;
    let mut switches_renamed = false;
    let mut selection_renamed = false;

    if let Err(error) = rename_in_manifest(&target, to) {
        return Err(rename_failure(
            error,
            rollback_profile_rename(
                &source,
                &target,
                &from,
                to,
                manifest_renamed,
                switches_renamed,
                selection_renamed,
            ),
        ));
    }
    manifest_renamed = true;

    if let Err(error) = switches::rename(&from, to) {
        return Err(rename_failure(
            error,
            rollback_profile_rename(
                &source,
                &target,
                &from,
                to,
                manifest_renamed,
                switches_renamed,
                selection_renamed,
            ),
        ));
    }
    switches_renamed = true;

    if let Err(error) = selection::rename(&from, to) {
        return Err(rename_failure(
            error,
            rollback_profile_rename(
                &source,
                &target,
                &from,
                to,
                manifest_renamed,
                switches_renamed,
                selection_renamed,
            ),
        ));
    }
    selection_renamed = true;

    if let Err(error) = crate::projects::profile_renamed(&from, to) {
        return Err(rename_failure(
            error,
            rollback_profile_rename(
                &source,
                &target,
                &from,
                to,
                manifest_renamed,
                switches_renamed,
                selection_renamed,
            ),
        ));
    }
    Ok(())
}

fn rename_failure(error: Error, rollback: Vec<String>) -> Error {
    if rollback.is_empty() {
        return error;
    }
    Error::Profile(format!(
        "{error}; the rename was rolled back with these follow-up errors: {}",
        rollback.join("; ")
    ))
}

fn rollback_profile_rename(
    source: &Path,
    target: &Path,
    from: &str,
    to: &str,
    manifest_renamed: bool,
    switches_renamed: bool,
    selection_renamed: bool,
) -> Vec<String> {
    let mut failures = Vec::new();
    if selection_renamed {
        if let Err(error) = selection::rename(to, from) {
            failures.push(format!("profile selection: {error}"));
        }
    }
    if switches_renamed {
        if let Err(error) = switches::rename(to, from) {
            failures.push(format!("disabled-plugin records: {error}"));
        }
    }
    if manifest_renamed {
        if let Err(error) = rename_in_manifest(target, from) {
            failures.push(format!("profile manifest: {error}"));
        }
    }
    if target.exists() {
        if let Err(error) = std::fs::rename(target, source) {
            failures.push(format!("profile directory: {error}"));
        }
    }
    failures
}

/// Take a profile away, with everything in it.
pub fn remove(name: &str) -> Result<()> {
    let name = expect_profile(name)?;
    if SHIPPED.contains(&name.as_str()) {
        return Err(Error::Profile(format!(
            "{name} is one of the harness's own profiles, and it would write a new one the next time it starts"
        )));
    }
    // Keep the selection cleanup inside the same registry lock as the directory
    // removal. Otherwise another window could create a new profile with this
    // now-free name before `selection::remove` runs, and that cleanup would
    // erase the new profile's selection state.
    crate::projects::remove_profile_if_unused(&name, || {
        discard(&name)?;
        // Never leave the window pointed at a profile that is not there. The
        // fallback in `selected` would cover it, but a selection file naming a
        // deleted profile is a lie the next reader has to work out for themselves.
        selection::remove(&name)
    })?;
    Ok(())
}

/// A profile as a file.
pub fn export(name: &str) -> Result<Declaration> {
    let name = expect_profile(name)?;
    let dir = paths::profile_dir(&name);
    let Some(manifest) = pkg::read_manifest(&dir) else {
        return Err(Error::Profile(format!(
            "{name} has not been initialized yet, so there is nothing to export"
        )));
    };
    let plugins = pkg::dependencies(&manifest);
    let disabled: Vec<String> = switches::switched_off(&name)
        .into_iter()
        .filter(|plugin| plugins.contains_key(plugin))
        .collect();
    let patch = patch(&name).unwrap_or_else(|| EMPTY_PATCH.to_string());
    let declaration = Declaration {
        kind: DECLARATION_KIND.to_string(),
        version: DECLARATION_VERSION,
        name,
        plugins,
        disabled,
        patch,
        integrity: None,
        verified: false,
    };
    verified_declaration(declaration)
}

/// Read a declaration file, verifying its recorded integrity.
pub fn declaration(path: &Path) -> Result<Declaration> {
    let body = std::fs::read(path).map_err(|cause| {
        Error::Profile(format!("{} could not be read: {cause}", path.display()))
    })?;
    if body.len() > MAX_DECLARATION_BYTES {
        return Err(Error::Profile(
            "the declaration file is implausibly large".into(),
        ));
    }
    let mut declaration: Declaration = serde_json::from_slice(&body)
        .map_err(|cause| Error::Profile(format!("the declaration is not valid: {cause}")))?;
    if declaration.kind != DECLARATION_KIND {
        return Err(Error::Profile(format!(
            "this file is a {}, not a {DECLARATION_KIND}",
            declaration.kind
        )));
    }
    if declaration.version != DECLARATION_VERSION {
        return Err(Error::Profile(format!(
            "declaration version {} is not supported; version {DECLARATION_VERSION} is",
            declaration.version
        )));
    }
    declaration.verified = declaration
        .integrity
        .as_deref()
        .is_some_and(|recorded| recorded == declaration_integrity(&declaration).unwrap_or_default());
    Ok(declaration)
}

fn verified_declaration(mut declaration: Declaration) -> Result<Declaration> {
    let integrity = declaration_integrity(&declaration)?;
    declaration.integrity = Some(integrity);
    declaration.verified = true;
    Ok(declaration)
}

fn declaration_integrity(declaration: &Declaration) -> Result<String> {
    let payload = DeclarationPayload {
        kind: &declaration.kind,
        version: declaration.version,
        name: &declaration.name,
        plugins: &declaration.plugins,
        disabled: &declaration.disabled,
        patch: &declaration.patch,
    };
    let canonical = serde_json::to_vec(&payload)
        .map_err(|cause| Error::Profile(format!("declaration could not be encoded: {cause}")))?;
    Ok(hex(&Sha256::digest(canonical)))
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    out
}

/// Recreate a profile from a declaration, and say what still has to install.
///
/// The plugins come back as specs for the runtime layer, the same contract
/// `duplicate` uses: this module never asks a registry for anything.
pub fn import(declaration: &Declaration, name: &str) -> Result<Vec<String>> {
    if !declaration.verified {
        return Err(Error::Profile(
            "the declaration's integrity does not match; the file was edited or damaged".into(),
        ));
    }
    let mut specs = Vec::with_capacity(declaration.plugins.len());
    for (package, range) in &declaration.plugins {
        let spec = format!("{package}@{range}");
        if !pkg::is_package_spec(&spec) {
            return Err(Error::Profile(format!(
                "the declaration installs {package} from {range}, which an import cannot ask for again"
            )));
        }
        specs.push(spec);
    }

    let bundles = interface_bundles()?;
    build(name, |dir| {
        initialize(dir, &bundles, &declaration.patch, workspace(DEFAULT).as_deref())?;
        for plugin in &declaration.disabled {
            switches::disable(name, plugin)?;
        }
        Ok(())
    })?;
    Ok(specs)
}

/// Write a declaration to a file.
pub fn save(declaration: &Declaration, path: &Path) -> Result<()> {
    let mut body = serde_json::to_vec_pretty(declaration)
        .map_err(|cause| Error::Profile(format!("declaration could not be encoded: {cause}")))?;
    body.push(b'\n');
    crate::atomic::write(path, body).map_err(|cause| {
        Error::Profile(format!("{} could not be written: {cause}", path.display()))
    })
}

/// Remove a profile directory without touching any records about it.
pub fn discard(name: &str) -> Result<()> {
    match std::fs::remove_dir_all(paths::profile_dir(name)) {
        Ok(()) => Ok(()),
        Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(cause) => Err(Error::Profile(format!(
            "{name} could not be removed: {cause}"
        ))),
    }
}

/// Two profiles, side by side.
pub fn compare(left: &str, right: &str) -> Result<Comparison> {
    let left = expect_profile(left)?;
    let right = expect_profile(right)?;
    let rows = differences(&inventory(&left), &inventory(&right));
    let differences = rows.iter().filter(|row| !row.same).count();
    Ok(Comparison {
        left,
        right,
        rows,
        differences,
    })
}

fn inventory(name: &str) -> Vec<(String, Standing, String)> {
    let dir = paths::profile_dir(name);
    let Some(manifest) = pkg::read_manifest(&dir) else {
        return Vec::new();
    };
    let disabled: BTreeSet<String> = switches::switched_off(name).into_iter().collect();
    let installed = pkg::dependencies(&manifest);
    let layered = pkg::bundles(&manifest);

    let mut names: BTreeSet<String> = installed.keys().cloned().collect();
    names.extend(layered.iter().cloned());

    names
        .into_iter()
        .map(|entry| {
            let installed = installed.get(&entry);
            let standing = match (installed, layered.contains(&entry)) {
                (None, true) => Standing::Builtin,
                (Some(_), true) if disabled.contains(&entry) => Standing::Disabled,
                (Some(_), true) => Standing::Active,
                (Some(_), false) => Standing::Library,
                (None, false) => Standing::Absent,
            };
            let spec = installed
                .map(|range| format!("{entry}@{range}"))
                .unwrap_or_else(|| entry.clone());
            (entry, standing, spec)
        })
        .collect()
}

fn differences(
    left: &[(String, Standing, String)],
    right: &[(String, Standing, String)],
) -> Vec<Difference> {
    let mut names: BTreeSet<String> = BTreeSet::new();
    for (name, _, _) in left.iter().chain(right.iter()) {
        names.insert(name.clone());
    }

    names
        .into_iter()
        .map(|name| {
            let find = |side: &[(String, Standing, String)]| {
                side.iter()
                    .find(|(entry, _, _)| *entry == name)
                    .map(|(_, standing, spec)| (*standing, spec.clone()))
            };
            let (left_standing, left_spec) = find(left).unwrap_or((Standing::Absent, String::new()));
            let (right_standing, right_spec) =
                find(right).unwrap_or((Standing::Absent, String::new()));
            Difference {
                same: left_standing == right_standing && left_spec == right_spec,
                name,
                left: left_standing,
                right: right_standing,
                left_spec,
                right_spec,
            }
        })
        .collect()
}

fn scan() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(paths::profiles_dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| is_name(name))
        .collect();
    names.sort();
    names
}

fn describe(name: &str, template: &[String]) -> Profile {
    let dir = paths::profile_dir(name);
    let manifest = pkg::read_manifest(&dir);
    let listed = manifest
        .as_ref()
        .map(|manifest| pkg::list(manifest, &switches::switched_off(name)))
        .unwrap_or_default();

    Profile {
        name: name.to_string(),
        initialized: manifest.is_some(),
        shipped: SHIPPED.contains(&name),
        serves_window: manifest.as_ref().is_some_and(|manifest| {
            let carried = pkg::bundles(manifest);
            template.iter().all(|bundle| carried.contains(bundle))
        }),
        plugins: listed.iter().filter(|plugin| !plugin.builtin).count(),
        disabled: listed.iter().filter(|plugin| plugin.disabled).count(),
        dir,
    }
}

/// The bundles the shipped web profile came with, in the order it lists them.
///
/// Order is kept because a layer stack is applied in order, and the base layer
/// being first is not an accident anybody should have to rediscover.
fn template_bundles() -> Vec<String> {
    let Some(manifest) = pkg::read_manifest(&paths::profile_dir(DEFAULT)) else {
        return Vec::new();
    };
    let installed = pkg::dependencies(&manifest);

    pkg::bundles(&manifest)
        .into_iter()
        .filter(|bundle| !installed.contains_key(bundle))
        .collect()
}

/// Write a profile, and leave nothing behind if the writing fails.
///
/// A directory that exists but is not a profile is worse than no directory: the
/// next attempt at the same name is refused for a reason that has nothing to do
/// with what actually went wrong. The failure that gets reported is the first
/// one — a rollback that also fails has nothing more to tell anybody.
fn build<T>(name: &str, work: impl FnOnce(&Path) -> Result<T>) -> Result<T> {
    let dir = free_dir(name)?;
    match work(&dir) {
        Ok(made) => Ok(made),
        Err(failure) => {
            let _ = discard(name);
            Err(failure)
        }
    }
}

/// The bundles a profile needs to be one this window can show, or a sentence
/// saying where they would have come from.
fn interface_bundles() -> Result<Vec<String>> {
    let bundles = template_bundles();
    if bundles.is_empty() {
        return Err(Error::Profile(format!(
            "there is no {DEFAULT} profile to take the interface from yet; start the harness once and it will write one"
        )));
    }
    Ok(bundles)
}

/// Write a profile the harness can boot, and nothing more.
///
/// Three files, because that is what the harness's own initializer writes and
/// what its loader reads: the manifest with the layer list in it, a patch layer
/// for the user's own overrides, and the workspace file that decides how pnpm
/// lays out anything installed later. The workspace file is copied from a
/// profile the harness wrote rather than composed here — it is the installation's
/// file, and a copy of it cannot fall out of step with the installation.
fn initialize(dir: &Path, bundles: &[String], patch: &str, workspace: Option<&str>) -> Result<()> {
    std::fs::create_dir_all(dir)
        .map_err(|cause| Error::Profile(format!("{} could not be made: {cause}", dir.display())))?;

    // The harness names a profile's package after the directory it is in, so
    // this reads the name off the path for the same reason it does: they are the
    // same fact, and deriving it twice is how they end up disagreeing.
    let name = dir.file_name().unwrap_or_default().to_string_lossy();
    write_manifest(dir, &manifest(&name, bundles))?;
    write(&dir.join(PATCH), patch)?;
    if let Some(workspace) = workspace {
        write(&dir.join(WORKSPACE), workspace)?;
    }
    Ok(())
}

/// The manifest a new profile starts with.
fn manifest(name: &str, bundles: &[String]) -> Value {
    serde_json::json!({
        "name": format!("dsh-profile-{name}"),
        "private": true,
        "dependencies": {},
        "dsh": { "profile": { "bundles": bundles } }
    })
}

fn rename_in_manifest(dir: &Path, name: &str) -> Result<()> {
    let Some(mut manifest) = pkg::read_manifest(dir) else {
        return Ok(());
    };
    if let Some(slot) = manifest.get_mut("name") {
        *slot = Value::from(format!("dsh-profile-{name}"));
    }
    write_manifest(dir, &manifest)
}

/// Two-space JSON with a trailing newline, which is how every other writer of
/// this file leaves it — the harness's own included. A profile whose manifest
/// reformats itself depending on who touched it last is a diff nobody can read.
fn write_manifest(dir: &Path, manifest: &Value) -> Result<()> {
    let mut json = serde_json::to_string_pretty(manifest)
        .map_err(|cause| Error::Profile(format!("the manifest could not be written: {cause}")))?;
    json.push('\n');
    write(&dir.join(MANIFEST), &json)
}

fn write(path: &Path, contents: &str) -> Result<()> {
    crate::atomic::write(path, contents).map_err(|cause| {
        Error::Profile(format!("{} could not be written: {cause}", path.display()))
    })
}

/// A profile's patch layer, if it has one.
fn patch(name: &str) -> Option<String> {
    std::fs::read_to_string(paths::profile_dir(name).join(PATCH)).ok()
}

/// A profile's workspace file, if it has one.
fn workspace(name: &str) -> Option<String> {
    std::fs::read_to_string(paths::profile_dir(name).join(WORKSPACE)).ok()
}

pub use selection::RecoveryNotice as StartupRecoveryNotice;

/// Promote a candidate only after the Harness has announced readiness.
pub fn mark_healthy(name: &str) -> Result<()> {
    selection::mark_healthy(name)
}

/// Contain a failed candidate and return the protected profile to retry.
pub fn failed_start(name: &str, reason: &str) -> Result<Option<String>> {
    selection::failed(name, reason)
}

pub fn recovery_notice() -> Option<StartupRecoveryNotice> {
    selection::notice()
}

pub fn recovery_acknowledge() -> Result<()> {
    selection::acknowledge()
}

/// A name that is a profile on this machine, or a sentence saying it is not.
fn expect_profile(name: &str) -> Result<String> {
    if !is_name(name) {
        return Err(Error::Profile(format!("{name} is not a profile name")));
    }
    if !paths::profile_dir(name).is_dir() {
        return Err(Error::Profile(format!("there is no profile called {name}")));
    }
    Ok(name.to_string())
}

/// The directory a new profile will be written into, or a sentence saying why it
/// cannot be. Checked before anything is written, never after.
fn free_dir(name: &str) -> Result<PathBuf> {
    if !is_new_name(name) {
        return Err(Error::Profile(format!(
            "{name} cannot be a profile name; use lowercase letters, digits, - and _"
        )));
    }

    let dir = paths::profile_dir(name);
    if dir.exists() {
        return Err(Error::Profile(format!(
            "there is already a profile called {name}"
        )));
    }
    Ok(dir)
}

/// Whether a name is one this shell will treat as a profile at all.
///
/// The harness's own rule, and it is about the name being a directory under
/// `profiles/`: not empty, no path separator in it, not `.` or `..`, and not the
/// `node_modules` the harness keeps beside the profiles. A name that fails this
/// is not a profile however it got here, a hand-edited selection file included.
pub fn is_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && !name.starts_with('.')
        && !name.contains(['/', '\\'])
        && name != SHARED_MODULES
}

/// Whether a name is one this shell will *make* a profile under.
///
/// Stricter, and about the manifest rather than the directory: the name goes
/// into `dsh-profile-<name>`, which is an npm package name, so what is allowed
/// here is what npm allows in one. Existing profiles are held to the looser rule
/// above, because a profile somebody made from a terminal is still theirs.
pub fn is_new_name(name: &str) -> bool {
    is_name(name)
        && !name.starts_with(['-', '_'])
        && name
            .chars()
            .all(|character| character.is_ascii_lowercase() || character.is_ascii_digit() || matches!(character, '-' | '_'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sandbox(label: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "harnesslite-profiles-{label}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("profiles/web")).expect("sandbox");
        base
    }

    #[test]
    fn profile_names_follow_the_harness_own_rules() {
        assert!(is_name("web"));
        assert!(is_name("proj-my-work"));
        assert!(!is_name(""));
        assert!(!is_name("node_modules"));
        assert!(!is_name(".hidden"));
        assert!(!is_name("a/b"));
        assert!(!is_name("a\\b"));
        assert!(!is_new_name("Web"));
        assert!(!is_new_name("-lead"));
        assert!(is_new_name("work-2"));
    }

    #[test]
    fn standings_cover_every_combination() {
        // (installed, layered, disabled) → Standing
        assert_eq!(standing_of(false, true, false), Standing::Builtin);
        assert_eq!(standing_of(true, true, false), Standing::Active);
        assert_eq!(standing_of(true, true, true), Standing::Disabled);
        assert_eq!(standing_of(true, false, false), Standing::Library);
        assert_eq!(standing_of(false, false, false), Standing::Absent);
    }

    fn standing_of(installed: bool, layered: bool, disabled: bool) -> Standing {
        match (installed, layered) {
            (false, true) => Standing::Builtin,
            (true, true) if disabled => Standing::Disabled,
            (true, true) => Standing::Active,
            (true, false) => Standing::Library,
            _ => Standing::Absent,
        }
    }
}
