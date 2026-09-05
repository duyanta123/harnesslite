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
    // The `--patch` seam must resolve from the profile directory before the
    // launcher composes; an install that predates the shared fallback (or a
    // `$DSH_HOME` restored from elsewhere) is repaired here, idempotently.
    hd_runtime::harness::install::sync_integration_shared_fallback()?;
    let plan = runtime_env::launch_plan()?;
    // Compose the exact profile once through the launcher itself; a conflict
    // between patch layers is repaired here, before startup log noise.
    for notice in hd_runtime::composition::preflight(&plan).await? {
        state.supervisor.note(Stream::Stderr, notice);
    }
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
            for notice in hd_runtime::composition::preflight(&fallback).await? {
                state.supervisor.note(Stream::Stderr, notice);
            }
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
/// Resolves only once npm is done, which is a minute or more on a cold cache —
/// the progress a user sees in the meantime is npm's own output, relayed
/// through the same log everything else in the shell writes to.
#[tauri::command]
pub async fn harness_install(state: State<'_, AppState>) -> Result<()> {
    if state.installing.swap(true, Ordering::SeqCst) {
        return Err(Error::Harness("an install is already running".into()));
    }
    let outcome = perform_install(&state).await;
    state.installing.store(false, Ordering::SeqCst);

    match &outcome {
        Ok(()) => state.supervisor.note(
            Stream::Stdout,
            format!("{} is installed", hd_runtime::harness::install::PACKAGE),
        ),
        Err(failure) => state.supervisor.note(Stream::Stderr, failure.to_string()),
    }
    outcome
}

async fn perform_install(state: &AppState) -> Result<()> {
    use hd_runtime::harness::install;

    // Every shared fallback junction points into the live runtime. Leave no
    // supervised process resolving through those junctions while the verified
    // staging directory is promoted over the live directory.
    state.supervisor.stop().await;
    state.supervisor.wait_until_inactive().await?;

    // Installing needs a Node that carries npm. When nothing on this machine
    // qualifies, the managed store is provisioned first — the download the
    // Environment panel drives explicitly is the same path taken silently here.
    let environment = runtime_env::environment();
    let usable = environment
        .all_node_runtimes
        .iter()
        .any(|install| install.version >= node_runtime::MINIMUM_SUPPORTED && install::npm_cli(&install.path).is_some());
    if !usable {
        provision_managed(state).await?;
    }

    let plan = runtime_env::install_plan()?;
    let supervisor = Arc::clone(&state.supervisor);
    supervisor.note(
        Stream::Stdout,
        format!("installing {} into {}", plan.spec, plan.target.display()),
    );

    let reporter = Arc::clone(&supervisor);
    install::run_transactional(&plan, move |stream, line| reporter.note(stream, line)).await?;

    // npm can exit successfully having installed something other than what we
    // need — a scope typo, a package that moved. Believe the file, not the
    // exit code.
    if !hd_core::paths::harness_entry().is_file() {
        return Err(Error::Harness(
            "npm reported success but the harness entry point is missing".into(),
        ));
    }
    Ok(())
}

/// Download the newest supported Node LTS into the managed store.
///
/// The shell's own runtime store is scanned by the same detector that finds
/// nvm/fnm/volta installs, so a runtime provisioned here needs no selection
/// step afterwards — it simply shows up as the machine's best available Node.
#[tauri::command]
pub async fn node_provision(state: State<'_, AppState>) -> Result<String> {
    if state.provisioning.swap(true, Ordering::SeqCst) {
        return Err(Error::Node("a Node download is already running".into()));
    }
    let outcome = provision_managed(&state).await;
    state.provisioning.store(false, Ordering::SeqCst);
    outcome.map(|path| path.to_string_lossy().into_owned())
}

async fn provision_managed(state: &AppState) -> Result<std::path::PathBuf> {
    // npm installs and runtime downloads both write under the managed tree;
    // serialising them keeps a promote from racing a partially-written store.
    if state.installing.load(Ordering::SeqCst) {
        return Err(Error::Node(
            "the Harness is being installed; Node download postponed".into(),
        ));
    }

    let runtimes = hd_core::paths::managed_node_dir();
    let reporter = Arc::clone(&state.supervisor);
    let mut last_note = std::time::Instant::now() - std::time::Duration::from_secs(1);
    let path = hd_runtime::node::install_newest_lts(&runtimes, move |received, total| {
        // Byte ticks arrive every chunk; the log only wants a heartbeat.
        if last_note.elapsed() >= std::time::Duration::from_secs(1) {
            last_note = std::time::Instant::now();
            let line = match total {
                Some(total) => format!(
                    "downloading Node: {} / {} MiB",
                    received / 1_048_576,
                    total / 1_048_576
                ),
                None => format!("downloading Node: {} MiB", received / 1_048_576),
            };
            reporter.note(Stream::Stdout, line);
        }
    })
    .await?;

    state
        .supervisor
        .note(Stream::Stdout, format!("Node installed at {}", path.display()));
    Ok(path)
}

/// Pin one of the machine's discovered runtimes as the one the next Harness
/// start runs on, until it disappears or a newer one is picked.
#[tauri::command]
pub async fn node_select(path: String) -> Result<node_runtime::NodeInstallation> {
    // Discovery shells out to `node --version`; never on the main thread.
    tauri::async_runtime::spawn_blocking(move || runtime_env::select_node(&path))
        .await
        .map_err(|cause| Error::Node(format!("runtime selection failed: {cause}")))?
}

/// Admission verdict for a candidate workspace, without moving anything.
#[tauri::command]
pub fn workspace_inspect(path: String) -> hd_core::validate::Admission {
    hd_core::validate::inspect(std::path::Path::new(&path))
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
    _app: AppHandle,
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

#[cfg(test)]
mod smoke {
    use super::*;

    /// The whole chain against the real machine: install the locked Harness
    /// release (provisioning a Node first if none qualifies), start it through
    /// the managed lifecycle gate, and stop it again.
    ///
    /// Ignored because it spends real network and real minutes on npm; it is
    /// the alignment test for the supervisor/install/node wiring, and the
    /// pre-warm a first GUI run benefits from. Run explicitly:
    /// `cargo test -p harnesslite --lib -- --ignored --nocapture smoke_cold_start`
    #[tokio::test]
    #[ignore = "real npm install and a real harness boot; run with --ignored"]
    async fn smoke_cold_start_installs_reaches_ready_and_stops() {
        let supervisor = Arc::new(Supervisor::new().expect("supervisor"));
        let state = AppState::new(Arc::clone(&supervisor));

        let environment = runtime_env::environment();
        if !environment.harness_installed || !environment.harness_compatible {
            perform_install(&state).await.expect("managed install");
        }

        let origin = start_managed(&state).await.expect("managed start");
        println!("harness ready at {origin}");

        // Stopping must end in a clean Stopped state — the process-tree
        // reclamation itself is what proc-guard's own tests cover; here it is
        // the supervised harness that has to go away.
        supervisor.stop().await;
        assert!(matches!(supervisor.status(), Status::Stopped));
    }
}
