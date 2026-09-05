//! Terminal commands: the pty layer's thin Tauri face.
//!
//! Everything that makes a terminal *useful* — the harness's own Node and
//! `dsh` on `PATH`, the active project as the working directory, the identity
//! markers plugins look for — is assembled here per shell, and the process
//! mechanics live in `hd-runtime::terminal`. The banner is the one thing this
//! side writes: it is app copy, not process plumbing.

use std::collections::HashMap;
use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};

use hd_core::contract;
use hd_core::error::{Error, Result};
use hd_core::paths;
use hd_runtime::terminal::{Event, OpenSpec, Session};

use crate::state::AppState;

/// Channels the terminal pane listens on.
const OUTPUT_CHANNEL: &str = "terminal://output";
const EXIT_CHANNEL: &str = "terminal://exit";

#[tauri::command]
pub fn terminal_open(
    app: AppHandle,
    state: State<'_, AppState>,
    rows: u16,
    cols: u16,
) -> Result<Session> {
    let spec = open_spec(rows, cols)?;
    let relay = move |event: Event| match event {
        Event::Output { id, data } => {
            let _ = app.emit(OUTPUT_CHANNEL, Output { id, data });
        }
        Event::Exit { id, code } => {
            let _ = app.emit(EXIT_CHANNEL, Exit { id, code });
        }
    };
    state.terminals.open(spec, Arc::new(relay))
}

#[tauri::command]
pub fn terminal_write(state: State<'_, AppState>, id: String, data: String) -> Result<()> {
    state.terminals.write(&id, &data)
}

#[tauri::command]
pub fn terminal_resize(state: State<'_, AppState>, id: String, rows: u16, cols: u16) -> Result<()> {
    state.terminals.resize(&id, rows, cols)
}

#[tauri::command]
pub fn terminal_close(state: State<'_, AppState>, id: String) -> Result<()> {
    state.terminals.close(&id)
}

#[tauri::command]
pub fn terminal_list(state: State<'_, AppState>) -> Vec<Session> {
    state.terminals.list()
}

/// One line of terminal output, already decoded on the Rust side.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Output {
    id: String,
    data: String,
}

/// A terminal whose shell has finished.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Exit {
    id: String,
    /// `None` when the shell was ended by a signal rather than by exiting.
    code: Option<u32>,
}

/// Resolve everything a new shell needs from the machine.
fn open_spec(rows: u16, cols: u16) -> Result<OpenSpec> {
    let environment_state = crate::runtime_env::environment();
    let profile = hd_core::projects::active_profile().unwrap_or_else(hd_core::profiles::selected);

    // `PATH` for a terminal: the harness's own tools, then everything already
    // there. `dsh` is installed inside application data, which is deliberately
    // not on anybody's `PATH` — without this, the one command the pane exists
    // to make available would be the one command that does not work.
    let mut directories = Vec::new();
    if let Some(node) = &environment_state.node {
        if let Some(directory) = node.path.parent() {
            directories.push(directory.to_path_buf());
        }
    }
    let bin = paths::harness_dir().join("node_modules").join(".bin");
    if bin.is_dir() {
        directories.push(bin);
    }
    let tools = paths::tools_dir().join("node_modules").join(".bin");
    if tools.is_dir() {
        directories.push(tools);
    }

    let inherited = std::env::var_os("PATH").unwrap_or_default();
    let mut search = std::env::join_paths(directories)
        .map(|joined| {
            let mut parts = std::env::split_paths(&joined).collect::<Vec<_>>();
            parts.extend(std::env::split_paths(&inherited));
            std::env::join_paths(parts)
        })
        .unwrap_or_else(|_| Ok(inherited.clone()))
        .map_err(|cause| Error::Terminal(format!("the search path could not be built: {cause}")))?;

    // An empty directory entry can slip in when the harness's tool folders do
    // not exist yet; `join_paths` keeps going, but a trailing separator would
    // point the shell at the current directory, so strip the empties.
    if search.is_empty() {
        search = inherited;
    }

    let mut vars = HashMap::new();
    // Same markers the supervised harness gets, so a plugin that behaves
    // differently under the desktop shell behaves the same way in here.
    vars.insert("DSH_DESKTOP".into(), "1".into());
    vars.insert("DSH_PROFILE".into(), profile.clone());
    vars.insert("DSH_PROFILE_DIR".into(), paths::profile_dir(&profile).to_string_lossy().into_owned());
    vars.insert("DSH_HOME".into(), paths::dsh_home().to_string_lossy().into_owned());
    vars.insert(contract::ENV_VERSION.into(), hd_core::VERSION.into());
    vars.insert(contract::ENV_RUNTIME_VERSION.into(), contract::DSH_VERSION.into());
    vars.insert(contract::ENV_PROFILE.into(), profile.clone());
    vars.insert(
        contract::ENV_PROFILE_DIR.into(),
        paths::profile_dir(&profile).to_string_lossy().into_owned(),
    );
    // What every terminal emulator declares, and what curses programs read to
    // decide whether they may use colour at all. Not set on Windows: ConPTY
    // already tells programs what they need.
    #[cfg(not(windows))]
    {
        vars.insert("TERM".into(), "xterm-256color".into());
        vars.insert("COLORTERM".into(), "truecolor".into());
    }

    // A terminal is a view onto the active project just like the Harness
    // itself. Resolved now, so switching projects later does not move an
    // already-open shell underneath the user's feet.
    let cwd = hd_core::projects::active_workspace().unwrap_or_else(paths::default_workspace_dir);

    // Whether `dsh` answers depends on the install, and the banner says so
    // rather than promising a command that is not there yet.
    let tools_line = if environment_state.harness_compatible {
        "dsh · pnpm · node"
    } else {
        "node (install the Harness from the console to enable dsh and pnpm)"
    };
    let banner = format!(
        "\r\n\x1b[1;36mHarnessLite {}\x1b[0m\r\nProfile: {}\r\nWorkspace: {}\r\nTools: {}\r\n\r\n",
        hd_core::VERSION,
        profile,
        cwd.display(),
        tools_line,
    );

    Ok(OpenSpec {
        rows,
        cols,
        cwd,
        banner,
        path: search,
        environment: vars,
    })
}
