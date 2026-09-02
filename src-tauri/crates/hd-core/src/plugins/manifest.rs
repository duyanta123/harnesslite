//! Reading a profile's manifest: the plugins domain's read-only half.
//!
//! A profile's `package.json` is the manifest of record — its `dependencies`
//! are what was installed into the profile and its `dsh.profile.bundles` is
//! the layer stack. Every reader (roster, compare, duplicate, market receipts)
//! goes through here so one schema change cannot fork three opinions.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;
use serde_json::Value;

use crate::error::{Error, Result};

const MANIFEST: &str = "package.json";

/// One package as the manager lists it.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledPlugin {
    pub name: String,
    /// What an install would ask for: `name@range` for installed packages, the
    /// bare name for bundles the profile came with.
    pub spec: String,
    /// Recorded in the profile's disabled-plugin switches.
    pub disabled: bool,
    /// Came with the profile rather than being installed into it.
    pub builtin: bool,
}

/// Read a profile's manifest, or None when the profile has not been
/// initialized yet.
pub fn read_manifest(dir: &Path) -> Option<Value> {
    let body = std::fs::read(dir.join(MANIFEST)).ok()?;
    serde_json::from_slice(&body).ok()
}

/// The layer list a manifest records, in the order it records it.
pub fn bundles(manifest: &Value) -> Vec<String> {
    manifest
        .pointer("/dsh/profile/bundles")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// What a manifest says was installed into the profile, as name → range.
pub fn dependencies(manifest: &Value) -> BTreeMap<String, String> {
    manifest
        .get("dependencies")
        .and_then(Value::as_object)
        .map(|dependencies| {
            dependencies
                .iter()
                .map(|(name, range)| {
                    (name.clone(), range.as_str().unwrap_or_default().to_string())
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Every package a manifest knows about, bundles first in their recorded
/// order, then installed-only packages in name order — with the disabled
/// switches applied.
pub fn list(manifest: &Value, disabled: &[String]) -> Vec<InstalledPlugin> {
    let installed = dependencies(manifest);
    let mut disabled: std::collections::HashSet<&String> = disabled.iter().collect();

    let mut entries: Vec<InstalledPlugin> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for bundle in bundles(manifest) {
        seen.insert(bundle.clone());
        match installed.get(&bundle) {
            Some(range) => {
                let is_disabled = disabled.remove(&bundle);
                entries.push(InstalledPlugin {
                    spec: format!("{bundle}@{range}"),
                    disabled: is_disabled,
                    builtin: false,
                    name: bundle,
                });
            }
            None => entries.push(InstalledPlugin {
                spec: bundle.clone(),
                disabled: false,
                builtin: true,
                name: bundle,
            }),
        }
    }

    for (name, range) in &installed {
        if seen.contains(name) {
            continue;
        }
        let is_disabled = disabled.remove(name);
        entries.push(InstalledPlugin {
            name: name.clone(),
            spec: format!("{name}@{range}"),
            disabled: is_disabled,
            builtin: false,
        });
    }

    entries
}

/// Whether a spec is one an install may be asked for again.
///
/// `name@range` with an npm-legal name and a range that is a registry range —
/// not a path, not a URL, not a git ref. A path dependency is anchored against
/// the directory the install ran in, so re-asking for it from anywhere else
/// would fetch something else or nothing.
pub fn is_package_spec(spec: &str) -> bool {
    let Some((name, range)) = split_spec(spec) else {
        return false;
    };
    if !is_package_name(name) {
        return false;
    }
    let lowered = range.to_ascii_lowercase();
    !lowered.is_empty()
        && lowered.len() <= 200
        && !lowered.starts_with('/')
        && !lowered.starts_with("./")
        && !lowered.starts_with("../")
        && !lowered.starts_with('~')
        && !lowered.starts_with("file:")
        && !lowered.starts_with("link:")
        && !lowered.contains("://")
        && !lowered.starts_with("git")
}

fn split_spec(spec: &str) -> Option<(&str, &str)> {
    let (name, range) = spec.rsplit_once('@')?;
    if range.is_empty() {
        return None;
    }
    // A scoped package's last `@` is the range separator; what is left of it
    // still starts with its own `@`.
    if name.is_empty() {
        return None;
    }
    Some((name, range))
}

/// Whether a package name is npm-legal: lowercase, URL-safe segments, and a
/// scoped name shaped exactly `@scope/name`.
pub fn is_package_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 214 {
        return false;
    }
    let scoped = name.starts_with('@');
    if scoped && (name.matches('@').count() != 1 || name.matches('/').count() != 1) {
        return false;
    }
    if !scoped && (name.contains('@') || name.contains('/')) {
        // An unscoped name is a single segment; a second `@` is a range that
        // was never split.
        return false;
    }
    // The scope sigil is part of the name but not of its segments.
    let bare = name.strip_prefix('@').unwrap_or(name);
    bare.split('/').all(|segment| {
        !segment.is_empty()
            && !segment.starts_with(['.', '_', '-'])
            && !segment.ends_with(['.', '_', '-'])
            && segment
                .chars()
                .all(|character| character.is_ascii_lowercase() || character.is_ascii_digit() || matches!(character, '-' | '_' | '.'))
    })
}

/// Append to a manifest's dependencies without disturbing anything else.
pub fn add_dependency(manifest: &mut Value, name: &str, range: &str) -> Result<()> {
    let dependencies = manifest
        .as_object_mut()
        .ok_or_else(|| Error::Plugin("the manifest is not a JSON object".into()))?
        .entry("dependencies")
        .or_insert_with(|| Value::Object(Default::default()));
    dependencies
        .as_object_mut()
        .ok_or_else(|| Error::Plugin("the manifest dependencies are not an object".into()))?
        .insert(name.to_string(), Value::from(range));
    Ok(())
}

/// Remove one dependency entry; absent is already fine.
pub fn remove_dependency(manifest: &mut Value, name: &str) {
    if let Some(dependencies) = manifest
        .as_object_mut()
        .and_then(|object| object.get_mut("dependencies"))
        .and_then(Value::as_object_mut)
    {
        dependencies.remove(name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample() -> Value {
        json!({
            "name": "dsh-profile-web",
            "private": true,
            "dependencies": {
                "dsh-plugin-b": "^1.2.0"
            },
            "dsh": { "profile": { "bundles": [
                "@deepseek-ai/dsh-base",
                "dsh-plugin-b"
            ] }}
        })
    }

    #[test]
    fn bundles_are_read_in_recorded_order() {
        assert_eq!(
            bundles(&sample()),
            vec!["@deepseek-ai/dsh-base".to_string(), "dsh-plugin-b".to_string()]
        );
    }

    #[test]
    fn the_list_marks_builtins_and_disabled_switches() {
        let listed = list(&sample(), &["dsh-plugin-b".to_string()]);
        assert_eq!(listed.len(), 2);
        assert!(listed[0].builtin);
        assert!(!listed[0].disabled);
        assert_eq!(listed[0].spec, "@deepseek-ai/dsh-base");
        assert!(!listed[1].builtin);
        assert!(listed[1].disabled);
        assert_eq!(listed[1].spec, "dsh-plugin-b@^1.2.0");
    }

    #[test]
    fn package_specs_accept_registry_ranges_only() {
        assert!(is_package_spec("dsh-plugin@^1.2.0"));
        assert!(is_package_spec("@scope/plugin@1.2.3"));
        assert!(is_package_spec("dsh-plugin@latest"));
        assert!(!is_package_spec("dsh-plugin"));
        assert!(!is_package_spec("dsh-plugin@file:../local"));
        assert!(!is_package_spec("dsh-plugin@git+https://example.com/repo"));
        assert!(!is_package_spec("dsh-plugin@https://example.com/tgz"));
        assert!(!is_package_spec("dsh-plugin@/absolute/path"));
        assert!(!is_package_spec("@incomplete-scope"));
    }

    #[test]
    fn dependency_edits_are_surgical() {
        let mut manifest = sample();
        add_dependency(&mut manifest, "dsh-new", "^0.1.0").unwrap();
        assert_eq!(dependencies(&manifest)["dsh-new"], "^0.1.0");
        remove_dependency(&mut manifest, "dsh-plugin-b");
        assert!(!dependencies(&manifest).contains_key("dsh-plugin-b"));
    }
}
