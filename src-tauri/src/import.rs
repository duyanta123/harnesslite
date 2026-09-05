//! One-time import from a HarnessDeck install, when there is one to import.
//!
//! HarnessLite keeps its own data directory and never writes into
//! HarnessDeck's, and HarnessDeck has no reason to know this shell exists.
//! What carries over is the state a person had already expressed — their
//! projects, which profile each binds, what was selected, and any custom
//! market sources. Credentials and sessions live in `~/.dsh`, which both
//! shells read in the same way, so they carry over by existing.
//!
//! The import runs once, in setup, and only when this shell has no data of
//! its own yet: after the first write, HarnessLite's directory is the truth.

use std::path::PathBuf;

use hd_core::paths;
use serde_json::Value;

/// Where the previous shell kept its data, if it was installed here.
fn source_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("harnessdeck")
}

/// Run the one-time import. Silent by design: every step degrades to "skip".
pub fn import_once() {
    let target = paths::app_data_dir();
    let source = source_dir();
    if !source.is_dir() || target.join(".imported").exists() {
        return;
    }
    // Marker file: the import ran, so a later HarnessDeck install cannot
    // resurrect data this shell has since replaced. Gated on projects.json
    // too: a shell that already has its own projects has its own truth.
    if target.join("projects.json").exists() {
        return;
    }
    let _ = std::fs::create_dir_all(&target);
    let _ = std::fs::write(target.join(".imported"), "from harnessdeck\n");

    copy_if_absent(&source.join("projects.json"), &target.join("projects.json"));
    copy_if_absent(&source.join("profile.json"), &target.join("profile.json"));
    copy_if_absent(&source.join("window.json"), &target.join("window.json"));
    copy_custom_sources(&source, &target);

    // The profile directory the old selection names has to exist here for the
    // selection file to be usable. It usually does — `~/.dsh` is shared — but
    // a profile that was never opened by the harness itself is not portable.
    if let Ok(body) = std::fs::read_to_string(target.join("profile.json")) {
        if let Ok(value) = serde_json::from_slice::<Value>(body.as_bytes()) {
            for key in ["active", "lastKnownGood"] {
                if let Some(name) = value.get(key).and_then(Value::as_str) {
                    let dir = paths::profile_dir(name);
                    if !dir.is_dir() {
                        let _ = std::fs::create_dir_all(&dir);
                    }
                }
            }
        }
    }
}

/// Copy a file only when the destination has none: first import, not a sync.
fn copy_if_absent(from: &Path, to: &Path) {
    if !from.is_file() || to.exists() {
        return;
    }
    if let Some(parent) = to.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::copy(from, to);
}

/// Custom market sources survive; the active source does not, because the
/// built-in roster changed between the two shells and a stale `active` would
/// point at a source this shell does not ship.
fn copy_custom_sources(source: &std::path::Path, target: &std::path::Path) {
    let Ok(body) = std::fs::read_to_string(source.join("market-sources.json")) else {
        return;
    };
    let Ok(value) = serde_json::from_slice::<Value>(body.as_bytes()) else {
        return;
    };
    let Some(custom) = value.get("custom").cloned() else {
        return;
    };
    let migrated = serde_json::json!({ "active": "npm", "custom": custom });
    let _ = hd_core::atomic::write(
        &target.join("market-sources.json"),
        migrated.to_string().as_bytes(),
    );
}

use std::path::Path;
