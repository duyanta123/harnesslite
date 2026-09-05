//! Session history commands: a read-only window onto `~/.dsh/sessions`.
//!
//! The library owns the caches and the Zstd decoding; these commands are the
//! door. Nothing here ever writes a session file — the one write in the pane,
//! "save the export", lands wherever the user's own save dialog pointed.

use serde::Serialize;
use tauri::State;

use hd_core::error::{Error, Result};
use hd_core::sessions::export::Format;
use hd_core::sessions::{export, Library};

#[tauri::command]
pub fn session_roster(state: State<'_, Library>) -> hd_core::sessions::Shelved {
    state.roster()
}

/// The sessions a query describes, best answer first.
///
/// A session answers when every term appears somewhere in it, not necessarily
/// in one line — the file you named and the error you got are usually messages
/// apart.
#[tauri::command]
pub fn session_search(
    state: State<'_, Library>,
    query: String,
    project: Option<String>,
) -> Vec<hd_core::sessions::find::Hit> {
    state.search(&query, project.as_deref())
}

#[tauri::command]
pub fn session_read(
    state: State<'_, Library>,
    id: String,
) -> Result<hd_core::sessions::Transcript> {
    state
        .transcript(&id)
        .ok_or_else(|| Error::Session(format!("there is no session {id}")))
}

/// Render one session in the shape its destination wants.
///
/// Markdown for an issue or a weekly note, HTML for a file somebody opens
/// without tooling, JSON for whatever reads it next.
#[tauri::command]
pub fn session_export(state: State<'_, Library>, id: String, format: String) -> Result<Exported> {
    let format = parse_format(&format)?;
    let transcript = state
        .transcript(&id)
        .ok_or_else(|| Error::Session(format!("there is no session {id}")))?;
    Ok(Exported {
        name: export::suggest(&transcript.card, format),
        text: export::render(&transcript, format),
    })
}

/// Put a rendered session where the save dialog pointed.
///
/// Rust does the writing because the webview holds no filesystem permission at
/// all — the only path it can name is the one the system's own dialog returned.
#[tauri::command]
pub fn session_save(path: String, text: String) -> Result<()> {
    let path = std::path::PathBuf::from(&path);
    let parent = path
        .parent()
        .ok_or_else(|| Error::Session(format!("{} is not a writable path", path.display())))?;
    std::fs::create_dir_all(parent)
        .and_then(|()| std::fs::write(&path, text))
        .map_err(|cause| Error::Session(format!("{} could not be written: {cause}", path.display())))
}

/// One rendered session, ready to be put somewhere.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Exported {
    /// The name to offer the save dialog, which it may well override.
    pub name: String,
    pub text: String,
}

/// The three spellings the frontend sends, as the export layer spells them.
fn parse_format(text: &str) -> Result<Format> {
    match text {
        "markdown" => Ok(Format::Markdown),
        "html" => Ok(Format::Html),
        "json" => Ok(Format::Json),
        other => Err(Error::Session(format!(
            "{other} is not a format sessions render to"
        ))),
    }
}
