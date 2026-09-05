//! Agent presets: what new sessions start as.
//!
//! The harness keeps user-installed presets under `$DSH_HOME/.agent-presets/`,
//! one directory per preset carrying its metadata (`preset.yml`) and its patch
//! (`agent.cordis.yml`). The shell's whole job is the roster — names and what
//! they are — plus the one setting the harness reads back: a single
//! `agent-presets.default` key in `$DSH_HOME/settings.yaml`, edited in place so
//! every hand-written comment and key order in that file survives.

use std::path::PathBuf;

use serde::Serialize;

use crate::error::{Error, Result};
use crate::paths;

/// Upper bound on scanned presets, matching the integration's roster guard.
const MAX_PRESETS: usize = 128;

/// One preset, as much as a picker shows.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentPreset {
    /// The directory name, which is what `settings.yaml` records.
    pub id: String,
    /// What the preset calls itself, when it says.
    pub name: Option<String>,
    pub description: Option<String>,
    /// Everything this shell lists lives in the user's own preset directory,
    /// so nothing scanned here is one the harness ships.
    pub shipped: bool,
}

/// The presets on this machine, and what new sessions start as.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetRoster {
    pub presets: Vec<AgentPreset>,
    /// May name a preset that is no longer there; the picker treats that as
    /// "nothing chosen" rather than refusing to render.
    pub default: Option<String>,
}

/// Where the harness keeps user-installed presets.
fn presets_root() -> PathBuf {
    paths::dsh_home().join(".agent-presets")
}

/// Read the roster, in name order.
///
/// A preset directory without metadata is still a preset — the id is the
/// truth, the name is decoration. One unreadable directory must not hide the
/// rest, so scan errors degrade to "no metadata" rather than an empty list.
pub fn roster() -> PresetRoster {
    let mut presets = Vec::new();

    if let Ok(entries) = std::fs::read_dir(presets_root()) {
        for entry in entries.flatten() {
            if !entry.path().is_dir() || presets.len() >= MAX_PRESETS {
                continue;
            }
            let id = entry.file_name().to_string_lossy().into_owned();
            if !is_preset_id(&id) {
                continue;
            }
            let (name, description) = read_metadata(&entry.path());
            presets.push(AgentPreset {
                id,
                name,
                description,
                shipped: false,
            });
        }
    }
    presets.sort_by(|left, right| left.id.cmp(&right.id));

    PresetRoster {
        presets,
        default: selected(),
    }
}

/// Make `id` what new sessions start as.
pub fn choose(id: &str) -> Result<()> {
    if !is_preset_id(id) {
        return Err(Error::Profile(format!("{id} is not a preset id")));
    }
    if !presets_root().join(id).is_dir() {
        return Err(Error::Profile(format!("there is no preset called {id}")));
    }

    let path = paths::dsh_home().join("settings.yaml");
    let text = std::fs::read_to_string(&path)
        .map_err(|cause| Error::Profile(format!("{} could not be read: {cause}", path.display())))?;

    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    if !set_default(&mut lines, id) {
        if lines.last().map(|line| !line.is_empty()).unwrap_or(false) {
            lines.push(String::new());
        }
        lines.push("agent-presets:".into());
        lines.push(format!("  default: {id}"));
    }

    let mut body = lines.join("\n");
    if !body.ends_with('\n') {
        body.push('\n');
    }
    crate::atomic::write(&path, body.as_bytes()).map_err(|cause| {
        Error::Profile(format!("{} could not be written: {cause}", path.display()))
    })
}

/// The recorded default, when it names something this shell would list.
pub fn selected() -> Option<String> {
    let text = std::fs::read_to_string(paths::dsh_home().join("settings.yaml")).ok()?;
    let id = read_default(&text)?;
    is_preset_id(&id).then_some(id)
}

/// A directory name this shell will treat as a preset at all.
fn is_preset_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && !id.starts_with('.')
        && !id.contains(['/', '\\'])
        && id != "node_modules"
}

/// `(name, description)` out of a `preset.yml`, tolerating every absence:
/// metadata is two optional scalars at the document's top level, and the
/// harness itself treats the directory as the identity.
fn read_metadata(dir: &std::path::Path) -> (Option<String>, Option<String>) {
    let Ok(text) = std::fs::read_to_string(dir.join("preset.yml")) else {
        return (None, None);
    };
    (scalar(&text, "name"), scalar(&text, "description"))
}

/// One top-level `key: value` scalar from a small YAML document.
///
/// Deliberately not a YAML parser: the file has exactly two flat fields this
/// shell reads, both written by the same tool that reads them, and a second
/// YAML dependency for two lines of it would cost more than it bought.
fn scalar(text: &str, key: &str) -> Option<String> {
    let line = text
        .lines()
        .find(|line| line.starts_with(key) && line[key.len()..].strip_prefix(':').is_some())?;
    let value = line[key.len() + 1..].trim();
    let value = value.trim_matches(['"', '\'']);
    (!value.is_empty()).then(|| value.to_string())
}

/// The `default` value inside the `agent-presets:` block, if written.
fn read_default(text: &str) -> Option<String> {
    let mut inside = false;
    for line in text.lines() {
        if !line.starts_with(' ') && !line.starts_with('\t') {
            inside = line.trim_end_matches(':') == "agent-presets";
            continue;
        }
        if inside {
            if let Some(value) = line.trim().strip_prefix("default:") {
                let value = value.trim().trim_matches(['"', '\'']);
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
    }
    None
}

/// Point the `default` line of the `agent-presets:` block at `id`, rewriting
/// that one line in place. `false` when the document has no such line yet.
fn set_default(lines: &mut [String], id: &str) -> bool {
    let mut inside = false;
    for line in lines {
        if !line.starts_with(' ') && !line.starts_with('\t') {
            inside = line.trim_end_matches(':') == "agent-presets";
            continue;
        }
        if !inside {
            continue;
        }
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix("default:") else {
            continue;
        };
        let indent = &line[..line.len() - trimmed.len()];
        // A `# …` comment that shared the line with the old value stays.
        let comment = match rest.find('#') {
            Some(at) if at > 0 => format!(" {}", &rest[at..]),
            _ => String::new(),
        };
        *line = format!("{indent}default: {id}{comment}");
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::{read_default, set_default};

    fn edited(original: &str, id: &str) -> String {
        let mut lines: Vec<String> = original.lines().map(str::to_string).collect();
        if !set_default(&mut lines, id) {
            lines.push("agent-presets:".into());
            lines.push(format!("  default: {id}"));
        }
        lines.join("\n")
    }

    #[test]
    fn an_existing_default_is_rewritten_in_place() {
        let body = edited("a:\n  x: 1\nagent-presets:\n  default: minimal\nb: 2\n", "scaffold");
        assert!(body.contains("  default: scaffold"));
        assert!(body.contains("a:\n  x: 1\nagent-presets:"));
        assert!(body.ends_with("b: 2"));
        assert_eq!(read_default(&body).as_deref(), Some("scaffold"));
    }

    #[test]
    fn a_block_without_default_gains_one() {
        let body = edited("agent-presets:\n  other: keep\n", "scaffold");
        assert!(body.contains("  other: keep"), "siblings survive");
        assert!(body.contains("  default: scaffold"));
        assert_eq!(read_default(&body).as_deref(), Some("scaffold"));
    }

    #[test]
    fn a_file_without_the_block_appends_one() {
        let body = edited("llm:\n  providers: {}\n", "scaffold");
        assert!(body.starts_with("llm:"));
        assert!(body.contains("agent-presets:\n  default: scaffold"));
        assert_eq!(read_default(&body).as_deref(), Some("scaffold"));
    }

    #[test]
    fn a_line_comment_survives_the_rewrite() {
        let mut lines = vec![
            "agent-presets:".to_string(),
            "  default: minimal # chosen by hand".to_string(),
        ];
        assert!(set_default(&mut lines, "scaffold"));
        assert_eq!(lines[1], "  default: scaffold # chosen by hand");
    }

    #[test]
    fn the_default_is_only_read_from_its_own_block() {
        assert_eq!(
            read_default("other-presets:\n  default: no\nagent-presets:\n  default: yes\n").as_deref(),
            Some("yes")
        );
        assert_eq!(read_default("unrelated: 1\n").as_deref(), None);
    }
}
