//! HarnessLite shell layer.
//!
//! Phase 3 baseline: single instance, one supervisor, the harness command
//! surface and the desktop bridge commands. Every Tauri command is glue only
//! (parameter conversion + one call into hd-core/hd-runtime + event reporting)
//! — domain logic lives in `hd-core`, process lifecycle in `hd-runtime`.

mod commands;
mod runtime_env;
mod state;
mod window_state;

use std::sync::Arc;

use tauri::Manager;

use hd_runtime::harness::supervisor::Supervisor;
use state::AppState;

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
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let supervisor = Supervisor::new().expect("supervisor");
            commands::relay_supervisor_events(app.handle().clone(), Arc::clone(&supervisor));
            app.manage(AppState::new(supervisor));

            // Restore the placement the user left the window in, if there was
            // one; the conf-file defaults cover the first run.
            if let (Some(window), Some(placement)) =
                (app.get_webview_window("main"), window_state::load())
            {
                let _ = window.set_position(tauri::LogicalPosition::new(
                    placement.x.max(0),
                    placement.y.max(0),
                ));
                if !placement.maximized {
                    let _ = window.set_size(tauri::LogicalSize::new(
                        placement.width.max(940),
                        placement.height.max(600),
                    ));
                }
            }

            // Remember the placement on the way out. The guard watches for
            // Windows' sharing-retry window; a failed write keeps last file.
            if let Some(window) = app.get_webview_window("main") {
                let handle = app.handle().clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { .. } = event {
                        if let Some(window) = handle.get_webview_window("main") {
                            let maximized = window.is_maximized().unwrap_or(false);
                            if let Ok(scale) = window.scale_factor() {
                                if let Ok(position) = window.outer_position() {
                                    if let Ok(size) = window.inner_size() {
                                        let _ = window_state::save(&window_state::Placement {
                                            x: (position.x as f64 / scale) as i32,
                                            y: (position.y as f64 / scale) as i32,
                                            width: (size.width as f64 / scale) as u32,
                                            height: (size.height as f64 / scale) as u32,
                                            maximized,
                                        });
                                    }
                                }
                            }
                        }
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::harness_status,
            commands::harness_log,
            commands::harness_environment,
            commands::harness_start,
            commands::harness_stop,
            commands::harness_install,
            commands::desktop_offer,
            commands::desktop_notify,
            commands::desktop_attention,
            commands::desktop_badge,
            commands::renderer_ready,
        ])
        .run(tauri::generate_context!())
        .expect("HarnessLite failed to start");
}
