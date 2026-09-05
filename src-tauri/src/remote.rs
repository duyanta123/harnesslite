//! Remote access commands: the door, the QR and the device list.
//!
//! Every reply is the pane's whole status, and every change is announced on
//! `remote://changed` so the counters stay live without polling. The gateway
//! itself lives in hd-runtime; this file is where it meets the supervisor —
//! remote access exists only while the harness it fronts is serving.

use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use hd_core::error::{Error, Result};
use hd_runtime::harness::supervisor::Supervisor;
use hd_runtime::remote::{self, Event, Remote};

use crate::state::AppState;

/// Channel the pane listens on for status changes.
const CHANGED_CHANNEL: &str = "remote://changed";

#[tauri::command]
pub fn remote_status(state: State<'_, AppState>) -> Status {
    status_of(&state)
}

/// Open the door. Requires the harness to be serving: a gateway in front of
/// nothing is a URL that lies.
#[tauri::command]
pub fn remote_open(app: AppHandle, state: State<'_, AppState>) -> Result<Status> {
    let port = harness_port(&state)?;
    let relay = relay_for(&app, &state.remote);
    let info = state.remote.open(port, relay)?;
    Ok(Status::of(&info))
}

#[tauri::command]
pub fn remote_close(app: AppHandle, state: State<'_, AppState>) -> Status {
    let relay = relay_for(&app, &state.remote);
    let info = state.remote.close(relay);
    Status::of(&info)
}

/// Put a fresh pairing code on screen. Paired devices are untouched.
#[tauri::command]
pub fn remote_renew(app: AppHandle, state: State<'_, AppState>) -> Result<Status> {
    let relay = relay_for(&app, &state.remote);
    let info = state.remote.renew(relay)?;
    Ok(Status::of(&info))
}

/// Forget one device, ending anything it has open.
#[tauri::command]
pub fn remote_forget(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<Status> {
    let relay = relay_for(&app, &state.remote);
    let info = state.remote.forget(&id, relay)?;
    Ok(Status::of(&info))
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub open: bool,
    pub addresses: Vec<String>,
    pub url: Option<String>,
    pub pairing_url: Option<String>,
    pub qr: Option<Qr>,
    pub code_seconds_left: Option<u64>,
    pub code_lifetime_seconds: u64,
    pub devices: Vec<Device>,
    pub active: u64,
    pub served: u64,
    pub refused: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Qr {
    pub size: usize,
    pub modules: Vec<bool>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub id: String,
    pub label: Option<String>,
    pub paired_seconds_ago: u64,
    pub last_seen_seconds_ago: u64,
}

impl Status {
    fn of(info: &remote::Info) -> Status {
        Status {
            open: info.open,
            addresses: info.addresses.clone(),
            url: info.url.clone(),
            pairing_url: info.pairing_url.clone(),
            qr: info.qr.as_ref().map(|matrix| Qr {
                size: matrix.size,
                modules: matrix.modules.clone(),
            }),
            code_seconds_left: info.code_seconds_left,
            code_lifetime_seconds: info.code_lifetime_seconds,
            devices: info
                .devices
                .iter()
                .map(|device| Device {
                    id: device.id.clone(),
                    label: device.label.clone(),
                    paired_seconds_ago: device.paired_seconds_ago,
                    last_seen_seconds_ago: device.last_seen_seconds_ago,
                })
                .collect(),
            active: info.active,
            served: info.served,
            refused: info.refused,
        }
    }
}

fn status_of(state: &AppState) -> Status {
    Status::of(&state.remote.info())
}

/// The loopback port the harness is serving on, or a refusal that says why.
fn harness_port(state: &AppState) -> Result<u16> {
    match state.supervisor.status() {
        hd_runtime::harness::supervisor::Status::Ready { origin, .. } => origin
            .rsplit(':')
            .next()
            .and_then(|port| port.parse().ok())
            .ok_or_else(|| Error::Harness(format!("the harness origin {origin} names no port"))),
        other => Err(Error::Harness(format!(
            "remote access needs a running Harness; the supervisor reports {other:?}"
        ))),
    }
}

/// Where gateway events go: the counters move, the pane hears about it.
fn relay_for(app: &AppHandle, remote: &Arc<Remote>) -> Arc<dyn Fn(Event) + Send + Sync> {
    let app = app.clone();
    let remote = Arc::clone(remote);
    Arc::new(move |event| {
        // The gateway's only event is "something changed"; the pane re-reads
        // the whole status rather than diffing counters.
        let Event::Changed = event;
        let _ = Emitter::emit(&app, CHANGED_CHANNEL, Status::of(&remote.info()));
    })
}

/// Close the gateway whenever the supervisor stops serving: a URL that fronts
/// a dead harness is worse than no URL. Subscribed once, next to the other
/// window-lifetime relays.
pub fn auto_close_when_harness_stops(app: AppHandle, remote: Arc<Remote>, supervisor: Arc<Supervisor>) {
    tauri::async_runtime::spawn(async move {
        let mut events = supervisor.subscribe();
        loop {
            let Ok(event) = events.recv().await else {
                break;
            };
            if let hd_runtime::harness::supervisor::Event::Status(status) = event {
                if !matches!(status, hd_runtime::harness::supervisor::Status::Ready { .. }) {
                    let info = remote.info();
                    if info.open {
                        let relay = relay_for(&app, &remote);
                        let _ = remote.close(relay);
                    }
                }
                let _ = status;
            }
        }
    });
}
