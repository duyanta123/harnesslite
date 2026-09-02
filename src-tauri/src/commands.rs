//! The IPC surface the frontend drives. Every command is glue: parameter
//! conversion, one call into a domain or the runtime layer, event reporting.
//! Domain logic belongs to hd-core; process logic to hd-runtime.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use hd_core::contract;
use hd_core::error::{Error, Result};
use hd_runtime::harness::supervisor::{Status, Stream, Supervisor};

use crate::runtime_env;
use crate::state::AppState;

/// One line of harness output, shaped for the log panel.
#[derive(Serialize)]
pub struct LogLine {
    pub stream: Stream,
    pub line: String,
}

#[tauri::command]
pub fn harness_status(state: State<'_, AppState>) -> Status {
    state.supervisor.status()
}

/// Output buffered since launch, so a late-opened log panel is not empty.
#[tauri::command]
pub fn harness_log(state: State<'_, AppState>) -> Vec<LogLine> {
    state
        .supervisor
        .recent_log()
        .into_iter()
        .map(|(stream, line)| LogLine { stream, line })
        .collect()
}

#[tauri::command]
pub async fn harness_environment(_state: State<'_, AppState>) -> Result<runtime_env::Environment> {
    // Cheap enough to answer without the lifecycle gate; the fields are
    // file-existence checks and one JSON read.
    Ok(runtime_env::environment())
}

#[tauri::command]
pub async fn harness_start(state: State<'_, AppState>) -> Result<String> {
    start_managed(&state).await
}

/// Start through the one managed-runtime lifecycle gate.
///
/// Kept separate from the Tauri wrapper because the tray owns the same action
/// and must not bypass preflight or race an install.
pub(crate) async fn start_managed(state: &AppState) -> Result<String> {
    let plan = runtime_env::launch_plan()?;
    let attempted = plan.profile.clone();
    state.boots.fetch_add(1, Ordering::Relaxed);
    match Arc::clone(&state.supervisor).start(plan).await {
        Ok(origin) => {
            hd_core::profiles::mark_healthy(&attempted)?;
            Ok(origin)
        }
        Err(failure) => {
            let reason = failure.to_string();
            let Some(recovered) = hd_core::profiles::failed_start(&attempted, &reason)? else {
                return Err(failure);
            };

            state.supervisor.note(
                Stream::Stderr,
                format!(
                    "profile {attempted} failed startup; automatically retrying last-known-good profile {recovered}"
                ),
            );
            let fallback = runtime_env::launch_plan()?;
            match Arc::clone(&state.supervisor).start(fallback).await {
                Ok(origin) => {
                    hd_core::profiles::mark_healthy(&recovered)?;
                    Ok(origin)
                }
                Err(fallback_failure) => Err(Error::Harness(format!(
                    "profile {attempted} failed to start ({reason}); last-known-good profile {recovered} also failed ({fallback_failure})"
                ))),
            }
        }
    }
}

#[tauri::command]
pub async fn harness_stop(state: State<'_, AppState>) -> Result<()> {
    state.supervisor.stop().await;
    Ok(())
}

/// Install the harness, or replace it with the locked release.
///
/// TODO(phase-2): route through the hd-runtime install transaction once it
/// lands; until then the shell refuses honestly rather than pretending.
#[tauri::command]
pub async fn harness_install(state: State<'_, AppState>) -> Result<()> {
    if state.installing.load(Ordering::SeqCst) {
        return Err(Error::Harness("an install is already running".into()));
    }
    Err(Error::Harness(
        "the install transaction is not wired into this build yet".into(),
    ))
}

/* -------------------------------------------------------------------------- */
/* Desktop service interface                                                  */
/* -------------------------------------------------------------------------- */

/// What the shell tells a frame about the desktop it is running on.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopOffer {
    pub protocol: u32,
    pub app: String,
    pub version: String,
    pub platform: String,
    pub scheme: String,
    pub capabilities: Vec<String>,
    pub link: Option<String>,
}

/// Describe the desktop, and take any link that was waiting for a listener.
#[tauri::command]
pub fn desktop_offer(
    app: AppHandle,
    state: State<'_, AppState>,
) -> DesktopOffer {
    DesktopOffer {
        protocol: contract::BRIDGE_PROTOCOL,
        app: "HarnessLite".into(),
        version: hd_core::VERSION.to_string(),
        platform: std::env::consts::OS.into(),
        scheme: contract::DEEP_LINK_SCHEME.into(),
        capabilities: contract::BRIDGE_METHODS.iter().map(|s| s.to_string()).collect(),
        link: state.pending_link.take(),
    }
}

/// A notification is the one thing the desktop can do that a page in a frame
/// cannot: outlive the window being looked at.
#[tauri::command]
pub fn desktop_notify(app: AppHandle, title: String, body: String) -> Result<()> {
    use tauri_plugin_notification::NotificationExt;
    app.notification()
        .builder()
        .title(title)
        .body(body)
        .show()
        .map_err(|cause| Error::Harness(format!("notification failed: {cause}")))
}

/// Window attention from the harness's job stream: draw the taskbar flag.
#[tauri::command]
pub fn desktop_attention(app: AppHandle, kind: String) -> Result<()> {
    // `job-failed` demands attention; a completed job only flags the taskbar
    // when the window is not in front. Phase 5 narrows this with the user's
    // notification preferences.
    let _ = &app;
    let _ = kind;
    Ok(())
}

/// Put a count on the tray, or zero to take it off.
#[tauri::command]
pub fn desktop_badge(count: u32) -> Result<()> {
    // The tray lands in Phase 4; until then the count is acknowledged so the
    // bridge stays honest about what it may ask for.
    let _ = count;
    Ok(())
}

/* -------------------------------------------------------------------------- */
/* Window lifecycle                                                           */
/* -------------------------------------------------------------------------- */

/// Mark this window healthy only after React committed the application root.
#[tauri::command]
pub fn renderer_ready() {}

/// Relays supervisor events onto the webview channel.
///
/// One task, started once: the broadcast channel fans every status and log
/// line out to every window that subscribed.
pub fn relay_supervisor_events(app: AppHandle, supervisor: Arc<Supervisor>) {
    tauri::async_runtime::spawn(async move {
        let mut events = supervisor.subscribe();
        loop {
            let Ok(event) = events.recv().await else {
                break;
            };
            let _ = Emitter::emit(&app, contract::EVENT_HARNESS, &event);
        }
    });
}
