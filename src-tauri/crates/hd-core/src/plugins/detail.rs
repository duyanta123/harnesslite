//! One package, as the npm registry describes it, judged for this shell.
//!
//! The detail dialog is where a person decides whether to let a package into
//! a profile, so this module's job is to surface what the registry knows —
//! licence, repository, lifecycle scripts, deprecation — and to say what it
//! can about compatibility with the pinned harness. Pure: the runtime layer
//! fetched the document, this reads it.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;

use crate::error::{Error, Result};

/// Whether a package manifest declares a profile patch — a plugin, not a library.
const BUNDLE_POINTER: &str = "/dsh/profile/bundles";

/// Lifecycle scripts the harness will run on install. Their presence is a
/// fact a person is owed before confirming, not an accusation.
const LIFECYCLE_SCRIPTS: [&str; 5] = ["preinstall", "install", "postinstall", "prepack", "prepare"];

/// What one registry version says, shaped for the detail dialog.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageDetail {
    pub name: String,
    pub version: String,
    pub description: String,
    pub license: String,
    pub homepage: Option<String>,
    pub repository: Option<String>,
    /// Whether the manifest declares a profile patch — a plugin, not a library.
    pub bundle: bool,
    pub dependencies: Vec<String>,
    /// The exact `name@version` a confirmation would install.
    pub install_spec: String,
    /// The source the user reached this through, echoed back for the dialog.
    pub source: String,
    pub compatibility: Compatibility,
    /// The registry's own integrity digest, when it publishes one.
    pub integrity: Option<String>,
    /// The manifest's profile patch, verbatim, for the dialog's evidence view.
    pub bundle_patch: Option<Value>,
    pub lifecycle_scripts: Vec<String>,
    pub deprecated: Option<String>,
    /// The repository URL is the package's own claim; `verified` only means it
    /// is a GitHub URL the dialog can link to.
    pub repository_verified: bool,
    pub integrity_verified: bool,
}

/// What the manifest's `engines.dsh` range says about the pinned harness.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum Compatibility {
    Compatible { requirement: String },
    /// No `engines.dsh` at all: the author said nothing, which is not the same
    /// as saying yes.
    Unknown,
    Incompatible { requirement: String, reason: String },
}

/// Read one version out of a registry document.
///
/// An empty `version` means the dist-tag the registry considers latest. A
/// version the registry does not list is an error rather than a dialog of
/// blanks: the detail view exists to describe something real.
pub fn parse(document: &str, name: &str, version: &str) -> Result<PackageDetail> {
    let value: Value = serde_json::from_str(document)
        .map_err(|cause| Error::Plugin(format!("the registry document is not valid JSON: {cause}")))?;

    let resolved = if version.is_empty() {
        value
            .pointer("/dist-tags/latest")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    } else {
        version.to_string()
    };
    if resolved.is_empty() {
        return Err(Error::Plugin(format!("{name} publishes no version to describe")));
    }
    let manifest = value
        .pointer(&format!("/versions/{resolved}"))
        .cloned()
        .ok_or_else(|| Error::Plugin(format!("the registry lists no {name}@{resolved}")))?;

    let description = manifest
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let license = manifest
        .get("license")
        .map(|value| match value {
            Value::String(text) => text.clone(),
            other => other.to_string(),
        })
        .unwrap_or_else(|| "UNLICENSED".into());
    let homepage = manifest
        .get("homepage")
        .and_then(Value::as_str)
        .filter(|url| !url.is_empty())
        .map(str::to_string);
    let repository = manifest
        .pointer("/repository/url")
        .and_then(Value::as_str)
        .map(str::to_string);
    let dependencies: Vec<String> = manifest
        .get("dependencies")
        .and_then(Value::as_object)
        .map(|map| map.keys().cloned().collect())
        .unwrap_or_default();
    let integrity = manifest
        .pointer("/dist/integrity")
        .and_then(Value::as_str)
        .map(str::to_string);
    let deprecated = manifest
        .get("deprecated")
        .and_then(Value::as_str)
        .map(str::to_string);

    let scripts = manifest
        .get("scripts")
        .and_then(Value::as_object)
        .map(|map| {
            map.keys()
                .filter(|key| LIFECYCLE_SCRIPTS.contains(&key.as_str()))
                .cloned()
                .collect()
        })
        .unwrap_or_default();

    let requirement = manifest
        .pointer("/engines/dsh")
        .and_then(Value::as_str)
        .map(str::to_string);
    // The judge is the pinned Harness, not the package's own version: an
    // `engines.dsh` range is a claim about which Harness the plugin runs on.
    let compatibility = match requirement.as_deref() {
        None => Compatibility::Unknown,
        Some(range) if satisfies(crate::contract::DSH_VERSION, range) => {
            Compatibility::Compatible { requirement: range.to_string() }
        }
        Some(range) => Compatibility::Incompatible {
            requirement: range.to_string(),
            reason: format!("the pinned Harness is {}", crate::contract::DSH_VERSION),
        },
    };

    let bundle = manifest.pointer(BUNDLE_POINTER).is_some();
    let bundle_patch = manifest.pointer("/dsh").cloned().filter(|_| bundle);

    Ok(PackageDetail {
        name: name.to_string(),
        version: resolved,
        description,
        license,
        homepage,
        repository: repository.clone(),
        bundle,
        dependencies,
        install_spec: format!("{name}@{}", version_of_spec(&manifest)),
        source: String::new(),
        compatibility,
        integrity,
        bundle_patch,
        lifecycle_scripts: scripts,
        deprecated,
        repository_verified: repository
            .as_deref()
            .is_some_and(|url| url.contains("github.com")),
        integrity_verified: manifest.pointer("/dist/integrity").is_some(),
    })
}

/// The version the manifest itself claims, which is the one `name@version`
/// should carry: the registry's key and the package's own field agree, and
/// when they do not, the manifest is what the profile will record.
fn version_of_spec(manifest: &Value) -> String {
    manifest
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Whether `version` satisfies an npm range, for the ranges authors actually
/// declare: exact, `^`, `~`, and the comparison prefixes. Not a general
/// semver engine — a range this cannot read answers `false`, and the detail
/// dialog shows the requirement verbatim so a person can judge.
pub fn satisfies(version: &str, range: &str) -> bool {
    let trimmed = version.trim().trim_start_matches(['v', '=']);
    let Some(parsed) = parse_version(trimmed) else {
        return false;
    };

    for alternative in range.split("||") {
        let alternative = alternative.trim();
        if alternative.is_empty() || alternative == "*" || alternative == "latest" {
            return true;
        }
        if range_satisfied(parsed, alternative) {
            return true;
        }
    }
    false
}

/// One (major, minor, patch) triple, ordered the way versions compare.
type Version = (u64, u64, u64);

fn range_satisfied(version: Version, range: &str) -> bool {
    let Some(at) = range.find(|c: char| c.is_ascii_digit() || c == 'v') else {
        return false;
    };
    let operator = range[..at].trim();
    let Some(target) = parse_version(range[at..].trim()) else {
        return false;
    };

    match operator {
        "" | "=" | "==" => version == target,
        // Same major, and at least the version named: `^1.2.3` is [1.2.3, 2.0.0).
        "^" => version.0 == target.0 && version >= target,
        "~" => version.0 == target.0 && version.1 == target.1 && version >= target,
        ">" => version > target,
        ">=" => version >= target,
        "<" => version < target,
        "<=" => version <= target,
        _ => false,
    }
}

fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let text = text.trim().trim_start_matches('v');
    let mut parts = text.split(['.', '-', '+']);
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    Some((major, minor, patch))
}

/// A tiny map alias so the struct definition reads the way the JSON does.
#[allow(dead_code)]
type Fields = BTreeMap<String, Value>;

#[cfg(test)]
mod tests {
    use super::{parse, satisfies};

    const DOCUMENT: &str = r#"{
        "dist-tags": { "latest": "1.2.0" },
        "versions": {
            "1.2.0": {
                "name": "dsh-demo",
                "version": "1.2.0",
                "description": "A demo plugin",
                "license": "MIT",
                "homepage": "https://example.com",
                "repository": { "url": "git+https://github.com/someone/dsh-demo.git" },
                "engines": { "dsh": "^0.1.1" },
                "dependencies": { "left-pad": "^1.0.0" },
                "dist": { "integrity": "sha512-abc" },
                "scripts": { "postinstall": "echo hi", "test": "echo no" },
                "dsh": { "profile": { "bundles": ["dsh-demo"] } }
            },
            "0.9.0": {
                "name": "dsh-demo",
                "version": "0.9.0",
                "engines": { "dsh": ">=1.0.0" }
            }
        }
    }"#;

    #[test]
    fn the_latest_version_is_described_with_its_evidence() {
        let detail = parse(DOCUMENT, "dsh-demo", "").expect("latest");
        assert_eq!(detail.version, "1.2.0");
        assert!(detail.bundle, "the manifest declares a patch");
        assert!(detail.bundle_patch.is_some());
        assert_eq!(detail.lifecycle_scripts, vec!["postinstall".to_string()]);
        assert_eq!(detail.integrity.as_deref(), Some("sha512-abc"));
        assert!(detail.integrity_verified);
        assert!(detail.repository_verified);
        assert_eq!(detail.install_spec, "dsh-demo@1.2.0");
    }

    #[test]
    fn an_engine_range_against_the_pinned_harness_is_judged() {
        let detail = parse(DOCUMENT, "dsh-demo", "").expect("latest");
        assert!(matches!(
            detail.compatibility,
            super::Compatibility::Compatible { ref requirement } if requirement == "^0.1.1"
        ));

        let old = parse(DOCUMENT, "dsh-demo", "0.9.0").expect("0.9.0");
        assert!(matches!(
            old.compatibility,
            super::Compatibility::Incompatible { ref requirement, .. } if requirement == ">=1.0.0"
        ));
    }

    #[test]
    fn a_version_the_registry_does_not_list_is_an_error_not_a_blank_dialog() {
        assert!(parse(DOCUMENT, "dsh-demo", "9.9.9").is_err());
        assert!(parse("not json", "dsh-demo", "").is_err());
    }

    #[test]
    fn ranges_a_plugin_author_writes_are_read() {
        assert!(satisfies("0.1.1-rc.2", "^0.1.0"));
        assert!(satisfies("1.2.3", "^1.0.0"));
        assert!(!satisfies("2.0.0", "^1.0.0"));
        assert!(satisfies("1.3.0", "^1.2.3"), "^1.2.3 allows any 1.x ≥ 1.2.3");
        assert!(!satisfies("1.2.2", "^1.2.3"));
        assert!(satisfies("1.2.9", "~1.2.3"));
        assert!(!satisfies("1.3.0", "~1.2.3"));
        assert!(satisfies("1.0.0", ">=0.9.0"));
        assert!(satisfies("0.1.1-rc.2", "0.1.1-rc.2"));
        assert!(satisfies("3.2.1", "*"));
        assert!(satisfies("3.2.1", "^2.0.0 || ^3.0.0"));
        assert!(!satisfies("4.0.0", "^2.0.0 || ^3.0.0"));
    }
}
