//! Preset commands: the picker over the harness's agent presets.

use tauri::State;

use hd_core::error::Result;
use hd_core::presets::{self, PresetRoster};

#[tauri::command]
pub fn preset_roster(_state: State<'_, crate::state::AppState>) -> PresetRoster {
    presets::roster()
}

/// Make one preset what new sessions start as. One key in the harness's own
/// `settings.yaml`, edited in place.
#[tauri::command]
pub fn preset_choose(_state: State<'_, crate::state::AppState>, id: String) -> Result<PresetRoster> {
    presets::choose(&id)?;
    Ok(presets::roster())
}
