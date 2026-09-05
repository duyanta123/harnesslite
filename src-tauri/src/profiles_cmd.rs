//! Profile commands: the manager's thin face on the composition domain.
//!
//! Every mutation answers with the fresh roster, so the manager never has to
//! re-fetch after acting — and never holds a stale one. The operations that
//! need installs (`duplicate`, `import`) return specs for the runtime layer
//! through hd-core, which is where that decision lives.

use serde::Serialize;


use hd_core::error::Result;
use hd_core::profiles;
use hd_core::profiles::{Comparison, Declaration, Roster};

/// The durable notice, joined with the levers the recovery centre offers.
///
/// The notice itself is hd-core's; the plugin candidates are computed from the
/// failed profile's manifest at read time, because "what can safely be
/// switched off" changes with every install the profile sees.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryNotice {
    pub generation: String,
    pub failed_profile: String,
    pub recovered_profile: Option<String>,
    pub reason: String,
    /// Active third-party plugins in the failed profile that can be disabled safely.
    pub plugins: Vec<String>,
}

#[tauri::command]
pub fn profile_roster() -> Roster {
    profiles::roster()
}

#[tauri::command]
pub fn profile_recovery_notice() -> Option<RecoveryNotice> {
    profiles::recovery_notice().map(|notice| RecoveryNotice {
        generation: notice.generation,
        failed_profile: notice.failed_profile,
        recovered_profile: notice.recovered_profile,
        reason: notice.reason,
        plugins: profiles::recovery_plugins(),
    })
}

#[tauri::command]
pub fn profile_recovery_acknowledge() -> Result<()> {
    profiles::recovery_acknowledge()
}

#[tauri::command]
pub fn profile_recovery_disable_plugin(name: String, generation: String) -> Result<()> {
    profiles::recovery_disable_plugin(&name, &generation)
}

#[tauri::command]
pub fn profile_recovery_retry(generation: String) -> Result<Roster> {
    profiles::recovery_retry(&generation)?;
    Ok(profiles::roster())
}

/// Choose a profile. A click is a candidate: it is only the active one once
/// the harness reaches readiness — containment handles the case where it does
/// not.
#[tauri::command]
pub fn profile_select(name: String) -> Result<Roster> {
    profiles::select(&name)?;
    Ok(profiles::roster())
}

#[tauri::command]
pub fn profile_create(name: String) -> Result<Roster> {
    profiles::create(&name)?;
    Ok(profiles::roster())
}

/// Copy a profile. The specs the copy still needs are installed by the
/// runtime layer; the roster is answered immediately, with the copy marked by
/// what it is missing.
#[tauri::command]
pub fn profile_duplicate(source: String, name: String) -> Result<Roster> {
    profiles::duplicate(&source, &name)?;
    Ok(profiles::roster())
}

#[tauri::command]
pub fn profile_rename(from: String, to: String) -> Result<Roster> {
    profiles::rename(&from, &to)?;
    Ok(profiles::roster())
}

#[tauri::command]
pub fn profile_remove(name: String) -> Result<Roster> {
    profiles::remove(&name)?;
    Ok(profiles::roster())
}

#[tauri::command]
pub fn profile_compare(left: String, right: String) -> Result<Comparison> {
    profiles::compare(&left, &right)
}

/// Write a profile's declaration to a path the user has already picked.
#[tauri::command]
pub fn profile_export(name: String, path: String) -> Result<()> {
    let declaration = profiles::export(&name)?;
    profiles::save(&declaration, std::path::Path::new(&path))
}

/// Read an exported profile, so its name can be offered before writing.
#[tauri::command]
pub fn profile_declaration(path: String) -> Result<Declaration> {
    profiles::declaration(std::path::Path::new(&path))
}

#[tauri::command]
pub fn profile_import(path: String, name: String) -> Result<Roster> {
    let declaration = profiles::declaration(std::path::Path::new(&path))?;
    profiles::import(&declaration, &name)?;
    Ok(profiles::roster())
}
