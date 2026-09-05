//! The project registry commands.
//!
//! Deliberately thin: the roster is re-read from disk on every reply so every
//! window agrees on it, and the rules that make a project safe to host —
//! admission of the folder, profile binding, the first-run default — live in
//! `hd_core::projects`, where the tray and the launcher reach the same ones.

use std::path::PathBuf;

use hd_core::error::Result;
use hd_core::projects;

#[tauri::command]
pub fn projects_list() -> projects::Roster {
    projects::roster()
}

#[tauri::command]
pub fn projects_add(
    path: String,
    name: Option<String>,
    profile: Option<String>,
) -> Result<projects::Roster> {
    projects::add(name, PathBuf::from(path), profile)
}

#[tauri::command]
pub fn projects_select(id: String) -> Result<projects::Roster> {
    projects::select(&id)
}

#[tauri::command]
pub fn projects_remove(id: String) -> Result<projects::Roster> {
    projects::remove(&id)
}

#[tauri::command]
pub fn projects_rename(id: String, name: String) -> Result<projects::Roster> {
    projects::rename(&id, name)
}

#[tauri::command]
pub fn projects_bind_profile(id: String, profile: String) -> Result<projects::Roster> {
    projects::bind_profile(&id, profile)
}
