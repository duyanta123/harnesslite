//! HarnessLite shell layer.
//!
//! Phase 3 baseline: single instance, one supervisor, the harness command
//! surface and the desktop bridge commands. Every Tauri command is glue only
//! (parameter conversion + one call into hd-core/hd-runtime + event reporting)
//! — domain logic lives in `hd-core`, process lifecycle in `hd-runtime`.

mod commands;
mod diagnostics;
mod plugins;
mod presets;
mod profiles_cmd;
mod remote;
mod runtime_env;
mod sessions;
mod state;
mod startup;
mod terminal;
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
    // The summon shortcut is registered once here rather than through the
    // plugin's dynamic API: one accelerator, one behaviour — bring the window
    // back, wherever the user is.
    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        focus_existing_window(app);
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_single_instance::init(|app, _arguments, _cwd| {
            focus_existing_window(app)
        }))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let supervisor = Supervisor::new().expect("supervisor");
            commands::relay_supervisor_events(app.handle().clone(), Arc::clone(&supervisor));
            let remote = hd_runtime::remote::Remote::new();

            app.manage(AppState::new(Arc::clone(&supervisor)));
            remote::auto_close_when_harness_stops(app.handle().clone(), remote, supervisor);

            // One shelf of sessions, shared by every window: read-only over
            // `~/.dsh/sessions`, cached and decoded here.
            app.manage(hd_core::sessions::Library::at(
                hd_core::paths::sessions_dir(),
            ));

            // The stored summon accelerator, if the user chose one.
            startup::register_saved(app.handle());

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
            commands::node_provision,
            terminal::terminal_open,
            terminal::terminal_write,
            terminal::terminal_resize,
            terminal::terminal_close,
            terminal::terminal_list,
            sessions::session_roster,
            sessions::session_search,
            sessions::session_read,
            sessions::session_export,
            sessions::session_save,
            profiles_cmd::profile_roster,
            profiles_cmd::profile_recovery_notice,
            profiles_cmd::profile_recovery_acknowledge,
            profiles_cmd::profile_recovery_disable_plugin,
            profiles_cmd::profile_recovery_retry,
            profiles_cmd::profile_select,
            profiles_cmd::profile_create,
            profiles_cmd::profile_duplicate,
            profiles_cmd::profile_rename,
            profiles_cmd::profile_remove,
            profiles_cmd::profile_compare,
            profiles_cmd::profile_export,
            profiles_cmd::profile_declaration,
            profiles_cmd::profile_import,
            plugins::plugin_state,
            plugins::plugin_recovery_notice,
            plugins::plugin_recovery_acknowledge,
            plugins::plugin_recovery_retry,
            plugins::plugin_search,
            plugins::plugin_detail,
            plugins::plugin_media,
            plugins::plugin_preview,
            plugins::plugin_sources,
            plugins::plugin_source_health,
            plugins::plugin_source_select,
            plugins::plugin_source_add,
            plugins::plugin_source_remove,
            plugins::plugin_add,
            plugins::plugin_remove,
            plugins::plugin_switch,
            plugins::plugin_archive,
            plugins::plugin_import,
            presets::preset_roster,
            presets::preset_choose,
            startup::startup_state,
            startup::startup_autostart,
            startup::startup_shortcut,
            startup::startup_notification,
            startup::startup_notification_test,
            startup::startup_log_level,
            startup::startup_harness_port,
            remote::remote_status,
            remote::remote_open,
            remote::remote_close,
            remote::remote_renew,
            remote::remote_forget,
            diagnostics::report_build,
            diagnostics::report_save,
            diagnostics::report_archive,
            diagnostics::report_frontend_crash,
            commands::desktop_offer,
            commands::desktop_notify,
            commands::desktop_attention,
            commands::desktop_badge,
            commands::renderer_ready,
        ])
        .run(tauri::generate_context!())
        .expect("HarnessLite failed to start");
}
