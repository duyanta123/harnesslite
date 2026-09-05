//! Startup preferences: how the application enters the session.
//!
//! Three kinds of setting, one file. The login item and the global shortcut
//! are *operating-system* state — this module is where the checkbox meets the
//! OS, and the file is only the memory of what was chosen. Notification
//! preferences and the fixed harness port are purely ours; the port is read
//! back by the launch plan, so it lives with the rest of the shell's state
//! rather than in a domain crate.

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use hd_core::error::{Error, Result};
use hd_core::paths;

/// What the picker offers before anything is chosen.
const SUGGESTED_SHORTCUT: &str = "CmdOrCtrl+Shift+D";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Notifications {
    #[serde(default = "default_true")]
    pub turn_completed: bool,
    #[serde(default = "default_true")]
    pub turn_failed: bool,
    #[serde(default)]
    pub job_completed: bool,
    #[serde(default = "default_true")]
    pub job_failed: bool,
}

fn default_true() -> bool {
    true
}

impl Default for Notifications {
    fn default() -> Self {
        Self {
            turn_completed: true,
            turn_failed: true,
            job_completed: false,
            job_failed: true,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preferences {
    #[serde(default)]
    pub shortcut: Option<String>,
    #[serde(default)]
    pub notifications: Notifications,
    #[serde(default = "default_level")]
    pub log_level: String,
    /// Fixed loopback port, or `None` for an OS-assigned collision-free port.
    #[serde(default)]
    pub harness_port: Option<u16>,
}

fn default_level() -> String {
    "info".into()
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            shortcut: None,
            notifications: Notifications::default(),
            log_level: default_level(),
            harness_port: None,
        }
    }
}

/// Read the stored preferences. Missing, partial or unreadable files all mean
/// the same thing: defaults. A preference file that fails to parse must not
/// take the settings pane down with it.
fn load() -> Preferences {
    std::fs::read(paths::app_data_dir().join("startup.json"))
        .ok()
        .and_then(|body| serde_json::from_slice(&body).ok())
        .unwrap_or_default()
}

fn store(preferences: &Preferences) -> Result<()> {
    let path = paths::app_data_dir().join("startup.json");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|cause| {
            Error::Harness(format!("{} could not be created: {cause}", parent.display()))
        })?;
    }
    let mut body = serde_json::to_vec_pretty(preferences)
        .map_err(|cause| Error::Harness(format!("preferences could not be encoded: {cause}")))?;
    body.push(b'\n');
    hd_core::atomic::write(&path, body).map_err(|cause| {
        Error::Harness(format!("{} could not be written: {cause}", path.display()))
    })
}

/// What the settings pane renders.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Startup {
    pub autostart: bool,
    pub shortcut: Option<String>,
    /// Whether the accelerator is registered right now. `false` while one is
    /// set means another program on this machine got to the combination first.
    pub held: bool,
    pub suggested: String,
    pub notifications: Notifications,
    pub log_level: String,
    pub harness_port: Option<u16>,
}

fn snapshot(app: &AppHandle, preferences: &Preferences) -> Startup {
    use tauri_plugin_autostart::ManagerExt as _;

    Startup {
        autostart: app.autolaunch().is_enabled().unwrap_or(false),
        shortcut: preferences.shortcut.clone(),
        held: preferences
            .shortcut
            .as_deref()
            .map(|accelerator| is_registered(app, accelerator))
            .unwrap_or(false),
        suggested: SUGGESTED_SHORTCUT.into(),
        notifications: preferences.notifications.clone(),
        log_level: preferences.log_level.clone(),
        harness_port: preferences.harness_port,
    }
}

/// Every mutation answers with the pane's whole state, so the checkbox never
/// has to reconcile a partial reply.
fn after(app: &AppHandle, change: impl FnOnce(&mut Preferences) -> Result<()>) -> Result<Startup> {
    let mut preferences = load();
    change(&mut preferences)?;
    store(&preferences)?;
    Ok(snapshot(app, &preferences))
}

#[tauri::command]
pub fn startup_state(app: AppHandle) -> Startup {
    snapshot(&app, &load())
}

#[tauri::command]
pub fn startup_autostart(app: AppHandle, enabled: bool) -> Result<Startup> {
    use tauri_plugin_autostart::ManagerExt as _;
    let autolaunch = app.autolaunch();
    let outcome = if enabled {
        autolaunch.enable()
    } else {
        autolaunch.disable()
    };
    outcome.map_err(|cause| Error::Harness(format!("the login item could not be changed: {cause}")))?;
    Ok(snapshot(&app, &load()))
}

/// Set the global shortcut. `None` gives the key up.
///
/// Registered before it is saved: a preference the machine could not honour
/// must not survive a restart pretending it did.
#[tauri::command]
pub fn startup_shortcut(app: AppHandle, accelerator: Option<String>) -> Result<Startup> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt as _;

    let previous = load().shortcut;
    if let Some(previous) = previous.as_deref() {
        let _ = app.global_shortcut().unregister(previous);
    }
    if let Some(accelerator) = accelerator.as_deref() {
        app.global_shortcut()
            .register(accelerator)
            .map_err(|cause| {
                // The old key was already given up above; put it back so a
                // rejected change leaves the machine as it was found.
                if let Some(previous) = previous.as_deref() {
                    let _ = app.global_shortcut().register(previous);
                }
                Error::Harness(format!("{accelerator} could not be registered: {cause}"))
            })?;
    }

    after(&app, |preferences| {
        preferences.shortcut = accelerator.clone();
        Ok(())
    })
}

#[tauri::command]
pub fn startup_notification(app: AppHandle, kind: String, enabled: bool) -> Result<Startup> {
    after(&app, |preferences| {
        match kind.as_str() {
            "turn-completed" => preferences.notifications.turn_completed = enabled,
            "turn-failed" => preferences.notifications.turn_failed = enabled,
            "job-completed" => preferences.notifications.job_completed = enabled,
            "job-failed" => preferences.notifications.job_failed = enabled,
            other => {
                return Err(Error::Harness(format!(
                    "{other} is not a notification preference"
                )))
            }
        }
        Ok(())
    })
}

/// Deliver one explicit test message through the same channel the real
/// notices use, so the user sees what they just agreed to.
#[tauri::command]
pub fn startup_notification_test(app: AppHandle) -> Result<Startup> {
    use tauri_plugin_notification::NotificationExt as _;
    app.notification()
        .builder()
        .title("HarnessLite")
        .body("This is what a Harness notice looks like.")
        .show()
        .map_err(|cause| Error::Harness(format!("the test notice failed: {cause}")))?;
    Ok(snapshot(&app, &load()))
}

#[tauri::command]
pub fn startup_log_level(app: AppHandle, level: String) -> Result<Startup> {
    if !matches!(level.as_str(), "debug" | "info" | "warn" | "error") {
        return Err(Error::Harness(format!("{level} is not a log level")));
    }
    after(&app, |preferences| {
        preferences.log_level = level.clone();
        Ok(())
    })
}

/// Fix the loopback port the harness serves on, or `None` for an OS-assigned
/// one. Read back by the launch plan on every start.
#[tauri::command]
pub fn startup_harness_port(app: AppHandle, port: Option<u16>) -> Result<Startup> {
    after(&app, |preferences| {
        preferences.harness_port = port;
        Ok(())
    })
}

fn is_registered(app: &AppHandle, accelerator: &str) -> bool {
    use tauri_plugin_global_shortcut::GlobalShortcutExt as _;
    app.global_shortcut().is_registered(accelerator)
}

/// Re-register the stored accelerator at launch, before any window can care.
pub fn register_saved(app: &AppHandle) {
    use tauri_plugin_global_shortcut::GlobalShortcutExt as _;

    let Some(accelerator) = load().shortcut else {
        return;
    };
    let shortcuts = app.global_shortcut();
    if shortcuts.is_registered(accelerator.as_str()) {
        return;
    }
    // A key another program took is a preference, not a promise: the settings
    // pane reports it as unheld rather than this failing loudly at boot.
    let _ = shortcuts.register(accelerator.as_str());
}

/// The fixed harness port, when one is set. Called by the launch plan.
pub fn harness_port() -> Option<u16> {
    load().harness_port
}
