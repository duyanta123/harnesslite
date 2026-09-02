//! HarnessLite shell layer.
//!
//! Phase 0 baseline: a single instance guard and an empty window. The thin
//! command layer, event channels, tray and bridge letterbox land in Phase 3;
//! every Tauri command stays glue-only (≤30 lines, no domain logic) — domain
//! logic lives in `hd-core`, process lifecycle in `hd-runtime`.

use tauri::Manager;

/// One instance of HarnessLite per user session: a second launch must focus
/// the existing window rather than start a second supervisor against the same
/// `~/.dsh` state.
fn focus_existing_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _arguments, _cwd| {
            focus_existing_window(app)
        }))
        .run(tauri::generate_context!())
        .expect("HarnessLite failed to start");
}
