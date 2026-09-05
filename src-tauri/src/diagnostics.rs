//! Diagnostics: what this shell knows about itself, safe to paste in public.
//!
//! One Markdown document built from the live environment and the supervisor's
//! own log, with every absolute path into the user's home or the app's data
//! directory redacted before it leaves the process. The archive variant is the
//! same evidence plus the recent crash log, zipped — a bounded attachment an
//! issue template can take.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::State;

use hd_core::error::{Error, Result};
use hd_core::paths;

use crate::state::AppState;

/// The document, plus the two names the save dialog and the archive button offer.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub name: String,
    pub archive_name: String,
    pub text: String,
}

/// Build the report now, from what this process can see.
#[tauri::command]
pub fn report_build(state: State<'_, AppState>) -> Result<Report> {
    let environment = crate::runtime_env::environment();
    let status = state.supervisor.status();

    let mut text = String::new();
    text.push_str("# HarnessLite diagnostics\n\n");
    text.push_str(&format!("- HarnessLite: {}\n", hd_core::VERSION));
    text.push_str(&format!("- Harness runtime: {} (pinned {})\n",
        environment.harness_version.as_deref().unwrap_or("not installed"),
        environment.expected_harness_version));
    text.push_str(&format!("- Supervisor: {status:?}\n"));
    text.push_str(&format!(
        "- Node: {}\n",
        environment
            .node
            .as_ref()
            .map(|node| format!("{} ({:?})", node.version, node.source))
            .unwrap_or_else(|| "none found".into())));
    text.push_str(&format!("- Project: {} — {}\n", environment.project, redact(&environment.workspace.to_string_lossy())));
    text.push_str(&format!("- Workspace admission: {:?}\n", environment.workspace_admission.state));
    if let Some(problem) = &environment.harness_problem {
        text.push_str(&format!("- Install problem: {problem}\n"));
    }

    text.push_str("\n## Harness log (last 60 lines)\n\n```\n");
    for (stream, line) in state.supervisor.recent_log().into_iter().rev().take(60).rev() {
        text.push_str(&format!("{stream:?}: {}\n", redact(&line)));
    }
    text.push_str("```\n");

    // Crash evidence, if any was captured this window's lifetime.
    let crashes = crash_log_path();
    if let Ok(body) = std::fs::read_to_string(&crashes) {
        text.push_str("\n## Frontend crashes\n\n```\n");
        text.push_str(&redact(&body));
        text.push_str("\n```\n");
    }

        let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    Ok(Report {
        name: format!("harnesslite-diagnostics-{stamp}.md"),
        archive_name: format!("harnesslite-diagnostics-{stamp}.zip"),
        text,
    })
}

/// Put the report where the save dialog pointed.
#[tauri::command]
pub fn report_save(path: String, text: String) -> Result<()> {
    write_out(Path::new(&path), text.as_bytes())
}

/// The same evidence as a bounded ZIP: the report, the recent log file and the
/// crash evidence, nothing else — and nothing that names a home directory.
#[tauri::command]
pub fn report_archive(state: State<'_, AppState>, path: String, text: String) -> Result<()> {
    use std::io::{Read as _, Write as _};
    use zip::write::SimpleFileOptions;

    let file = std::fs::File::create(&path)
        .map_err(|cause| Error::Harness(format!("{} could not be created: {cause}", path)))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default();

    zip.start_file("report.md", options)
        .map_err(|cause| Error::Harness(format!("the archive could not be written: {cause}")))?;
    zip.write_all(text.as_bytes())
        .map_err(|cause| Error::Harness(format!("the archive could not be written: {cause}")))?;

    // The supervisor's persistent log, tail only — bounded the way the report
    // is, because an archive that grows with the log it carries is not one an
    // issue attachment can take.
    let mut log_tail = String::new();
    for (stream, line) in state.supervisor.recent_log().into_iter().rev().take(400).rev() {
        log_tail.push_str(&format!("{stream:?}: {}\n", redact(&line)));
    }
    zip.start_file("harness.log", options)
        .map_err(|cause| Error::Harness(format!("the archive could not be written: {cause}")))?;
    zip.write_all(log_tail.as_bytes())
        .map_err(|cause| Error::Harness(format!("the archive could not be written: {cause}")))?;

    if let Ok(mut crashes) = std::fs::File::open(crash_log_path()) {
        let mut body = String::new();
        let _ = crashes.read_to_string(&mut body);
        let _ = zip.start_file("frontend-crashes.log", options);
        let _ = zip.write_all(redact(&body).as_bytes());
    }

    zip.finish()
        .map_err(|cause| Error::Harness(format!("the archive could not be finished: {cause}")))?;
    Ok(())
}

/// Record one renderer crash. Bounded: the newest report replaces the oldest
/// once the file holds a run's worth, so a crash loop cannot grow without end.
#[tauri::command]
pub fn report_frontend_crash(message: String, stack: String, url: String) -> Result<()> {
    let entry = format!(
        "{}\t{}\t{}\t{}\n",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        redact(&url),
        redact(&message.replace(['\n', '\t'], " ")),
        redact(&stack.replace(['\r', '\n'], " ")).chars().take(2_000).collect::<String>(),
    );
    let path = crash_log_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let mut lines: Vec<&str> = existing.lines().collect();
    lines.push(entry.trim_end());
    if lines.len() > 32 {
        let drop = lines.len() - 32;
        lines.drain(..drop);
    }
    write_out(&path, lines.join("\n").as_bytes())
}

fn crash_log_path() -> PathBuf {
    paths::app_data_dir().join("logs").join("frontend-crashes.log")
}

fn write_out(path: &Path, body: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|cause| Error::Harness(format!("{} could not be created: {cause}", parent.display())))?;
    }
    std::fs::write(path, body)
        .map_err(|cause| Error::Harness(format!("{} could not be written: {cause}", path.display())))
}

/// Take the user out of the document before it can leave the process.
///
/// The home directory, the app's data directory and `$DSH_HOME` are the three
/// places private paths live; each becomes a placeholder that says as much.
fn redact(text: &str) -> String {
    let home = dirs::home_dir().unwrap_or_default();
    let app_data = paths::app_data_dir();
    let dsh = paths::dsh_home();
    text.replace(app_data.to_string_lossy().as_ref(), "%APPDATA%")
        .replace(dsh.to_string_lossy().as_ref(), "%DSH_HOME%")
        .replace(home.to_string_lossy().as_ref(), "~")
}
