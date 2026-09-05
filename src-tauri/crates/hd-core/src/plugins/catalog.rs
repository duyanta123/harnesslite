//! One normalized catalog model over every source's JSON.
//!
//! Each source speaks its own dialect, so the market reads all of them
//! through one pure function per contract: fetched text goes in, normalized
//! [`CatalogEntry`] values come out. No entry can carry a command, a path, a
//! git URL or a lifecycle permission — the only installable thing in the
//! model is `npm_spec`, an exact `name@version` the runtime layer resolves
//! again through the configured registry before `dsh plugin add` runs. An
//! entry without one is display-only, honestly shown but never suggested.
//!
//! What each source can honestly offer differs, and the adapters keep that
//! honesty rather than inventing versions: npm is the authority on
//! `name@version` existence; the reviewed 1024Store attests per install
//! method; awesome-dsh has no per-plugin version at all; and a custom source
//! is never verified, whatever it claims.

use serde::Serialize;
use serde_json::Value;

use crate::error::{Error, Result};
use crate::plugins::{is_package_name, is_package_spec};
use crate::plugins::sources::{SourceId, TrustTier};

/// One plugin listing, normalized out of whatever JSON a source speaks.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEntry {
    /// Where this entry was read from.
    pub source: SourceId,
    /// Display name, as the source spells it.
    pub name: String,
    /// The npm package the entry is about, when it names one at all — kept
    /// even when no exact version is known, so the panel can still show and
    /// later suggest the package.
    pub npm_name: Option<String>,
    /// Exact `name@version` this entry suggests installing. Only set when the
    /// source pinned one precisely enough to be worth resolving.
    pub npm_spec: Option<String>,
    pub summary_en: Option<String>,
    pub summary_zh: Option<String>,
    /// The source's own category key or label, for its faceted filter.
    pub category: Option<String>,
    /// True only when the source itself attests verification of this entry.
    /// A custom source never does.
    pub verified: bool,
    /// How much the shell trusts this source's word overall.
    pub trust: TrustTier,
    /// Lifetime install count, when the source reports one.
    pub installs: Option<u64>,
    /// Repository stars, when the source reports them.
    pub stars: Option<u64>,
    /// Weekly downloads, when the source reports them (npm does).
    pub downloads: Option<u64>,
}

/// Parse one fetched catalog body into normalized entries.
///
/// Pure: the runtime layer fetched the text within its own network budget and
/// hands it over as a string. A body that does not satisfy the source's
/// contract is an error; individual malformed entries inside a valid body are
/// skipped, exactly as HarnessDeck v1 read its catalogs.
pub fn parse(source: &SourceId, body: &str) -> Result<Vec<CatalogEntry>> {
    let value: Value = serde_json::from_str(body)
        .map_err(|cause| Error::Plugin(format!("catalog response is not valid JSON: {cause}")))?;
    match source {
        SourceId::Npm => parse_npm(&value),
        SourceId::AwesomeDsh => parse_awesome(&value),
        SourceId::Reviewed1024 => parse_reviewed(&value),
        SourceId::Custom(_) => parse_standard(source, &value),
    }
}

/// Filter entries the way the market panel asks for them.
///
/// Case-insensitive substring match over the display name, npm name and both
/// summaries; every whitespace-separated term must match. The category filter
/// is an exact match against a source's own category key when given. An empty
/// query keeps everything.
pub fn search<'a>(
    entries: &'a [CatalogEntry],
    query: &str,
    category: Option<&str>,
) -> Vec<&'a CatalogEntry> {
    let terms: Vec<String> = query
        .split_whitespace()
        .map(|term| term.to_ascii_lowercase())
        .collect();
    entries
        .iter()
        .filter(|entry| {
            let haystack = format!(
                "{} {} {} {}",
                entry.name,
                entry.npm_name.as_deref().unwrap_or_default(),
                entry.summary_en.as_deref().unwrap_or_default(),
                entry.summary_zh.as_deref().unwrap_or_default(),
            )
            .to_ascii_lowercase();
            terms.iter().all(|term| haystack.contains(term))
                && category.is_none_or(|wanted| entry.category.as_deref() == Some(wanted))
        })
        .collect()
}

/// Whether a version string is a concrete version a catalog may suggest.
///
/// [`is_package_spec`] accepts every registry range (`^1.2.3`, `1.x`,
/// `latest`); a suggestion must additionally be one exact version: a
/// `v`-prefixed or bare semver core, optionally with a prerelease tag. Build
/// metadata is refused, as v1 refused it — `1.2.3+rebuilt` is not an identity
/// anyone can pin.
pub fn is_exact_version(range: &str) -> bool {
    if range.contains('+') {
        return false;
    }
    let version = range.strip_prefix('v').unwrap_or(range);
    let core = version.split('-').next().unwrap_or_default();
    !core.is_empty()
        && core
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
        && is_package_spec(&format!("hd-catalog@{range}"))
}

/// npm registry search (`GET {registry}/-/v1/search`), field-mapped as v1
/// mapped it: the package object carries everything, the entry carries the
/// weekly downloads.
fn parse_npm(value: &Value) -> Result<Vec<CatalogEntry>> {
    let objects = value
        .get("objects")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Plugin("npm search response has no objects array".into()))?;
    let entries = objects
        .iter()
        .filter_map(|object| {
            let package = object.get("package")?;
            let name = package.get("name")?.as_str()?;
            let version = package
                .get("version")
                .and_then(Value::as_str)
                .unwrap_or_default();
            Some(CatalogEntry {
                source: SourceId::Npm,
                name: name.to_string(),
                npm_name: Some(name.to_string()),
                npm_spec: (!version.is_empty()).then(|| format!("{name}@{version}")),
                summary_en: plain_text(package.get("description").and_then(Value::as_str)),
                summary_zh: None,
                // npm has keywords rather than one category; the first
                // (stable-ordered) keyword stands in for the facet filter.
                category: keywords(package).into_iter().next(),
                verified: true,
                trust: TrustTier::BuiltinNpm,
                installs: None,
                stars: None,
                downloads: object.pointer("/downloads/weekly").and_then(Value::as_u64),
            })
        })
        .collect();
    Ok(entries)
}

/// One npm version manifest (`GET /<name>/latest`) as a catalog entry.
///
/// The exact-name lookup builds its answer here. The search endpoint ranks by
/// popularity, so this is the path that lets a person typing a full package
/// name find a package nobody else is using yet.
pub fn npm_manifest_entry(name: &str, body: &str) -> Result<CatalogEntry> {
    let value: Value = serde_json::from_str(body)
        .map_err(|cause| Error::Plugin(format!("latest manifest is not valid JSON: {cause}")))?;
    let version = value
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if version.is_empty() {
        return Err(Error::Plugin("latest manifest carries no version".into()));
    }
    Ok(CatalogEntry {
        source: SourceId::Npm,
        name: name.to_string(),
        npm_name: Some(name.to_string()),
        npm_spec: Some(format!("{name}@{version}")),
        summary_en: plain_text(value.get("description").and_then(Value::as_str)),
        summary_zh: None,
        category: keywords(&value).into_iter().next(),
        verified: true,
        trust: TrustTier::BuiltinNpm,
        installs: None,
        stars: None,
        downloads: None,
    })
}

/// npm keywords, read as v1 read them: trimmed, bounded, stable order.
fn keywords(package: &Value) -> Vec<String> {
    let values: Vec<&str> = match package.get("keywords") {
        Some(Value::Array(values)) => values.iter().filter_map(Value::as_str).collect(),
        Some(Value::String(value)) => value.split(',').collect(),
        _ => Vec::new(),
    };
    let mut categories: Vec<String> = values
        .into_iter()
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.len() <= 48)
        .take(20)
        .map(str::to_string)
        .collect();
    categories.sort_by_key(|value| value.to_ascii_lowercase());
    categories.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    categories
}

/// The awesome-dsh community index (`plugins.json`): a curated, bilingual
/// snapshot with npm links — and, deliberately, no per-plugin version field.
fn parse_awesome(value: &Value) -> Result<Vec<CatalogEntry>> {
    let plugins = value
        .get("plugins")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Plugin("awesome-dsh catalog has no plugins array".into()))?;
    let entries = plugins
        .iter()
        .filter_map(|plugin| {
            let name = plugin.get("name")?.as_str()?;
            let npm = plugin
                .get("npm")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|npm| !npm.is_empty());
            // The tarball link is the only place a version can appear, and
            // only a `v`-prefixed spelling is unambiguous enough to suggest.
            // Everything else keeps its npm name and stays display-only.
            let version = npm.and_then(|_| {
                tarball_version(plugin.get("tarball").and_then(Value::as_str).unwrap_or_default())
            });
            let (summary_en, summary_zh) = localized(plugin.get("description"));
            Some(CatalogEntry {
                source: SourceId::AwesomeDsh,
                name: name.to_string(),
                npm_name: npm.map(str::to_string),
                npm_spec: npm.zip(version).map(|(npm, version)| format!("{npm}@{version}")),
                summary_en,
                summary_zh,
                category: plugin.get("category").and_then(Value::as_str).map(str::to_string),
                verified: false,
                trust: TrustTier::BuiltinAwesome,
                installs: None,
                stars: plugin.get("stars").and_then(Value::as_u64),
                downloads: plugin.get("downloads").and_then(Value::as_u64),
            })
        })
        .collect();
    Ok(entries)
}

/// A version read from a tarball URL, and only a `v`-prefixed one.
///
/// Registry tarballs spell versions without the `v` (`pkg-1.2.3.tgz`), which
/// is indistinguishable from a name suffix, so they yield nothing.
fn tarball_version(tarball: &str) -> Option<String> {
    let base = tarball.rsplit('/').next()?;
    let base = base.strip_suffix(".tar.gz").unwrap_or(base);
    let base = base.strip_suffix(".tgz").unwrap_or(base);
    let version = match base.rsplit_once("-v") {
        Some((_, rest)) if !rest.is_empty() => rest,
        // GitHub tag archives arrive as `.../refs/tags/v1.2.3.tar.gz`.
        _ if base.starts_with('v') => &base[1..],
        _ => return None,
    };
    is_exact_version(version).then(|| version.to_string())
}

/// The reviewed DSH 1024Store snapshot (`/api/v1/plugins`): GitHub telemetry
/// plus per-package install methods, the npm ones carrying the verification
/// word the store re-checked.
fn parse_reviewed(value: &Value) -> Result<Vec<CatalogEntry>> {
    let packages = value
        .get("packages")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Plugin("1024Store catalog has no packages array".into()))?;
    let entries = packages
        .iter()
        .filter_map(|package| {
            let name = package.get("name")?.as_str()?;
            // Prefer the first npm install method; github-only packages stay
            // in the list as display-only entries.
            let method = package
                .get("installMethods")
                .and_then(Value::as_array)
                .and_then(|methods| {
                    methods
                        .iter()
                        .find(|method| method.get("kind").and_then(Value::as_str) == Some("npm"))
                });
            let parts = method
                .and_then(|method| method.get("spec").and_then(Value::as_str).map(str::trim))
                .filter(|spec| !spec.is_empty() && is_package_spec(spec))
                .and_then(spec_parts);
            let verified = method
                .and_then(|method| method.get("verification").and_then(Value::as_str))
                == Some("verified");
            let (summary_en, summary_zh) = localized(package.get("description"));
            Some(CatalogEntry {
                source: SourceId::Reviewed1024,
                name: name.to_string(),
                npm_name: parts.map(|(name, _)| name.to_string()),
                npm_spec: parts
                    .filter(|(_, version)| is_exact_version(version))
                    .map(|(name, version)| format!("{name}@{version}")),
                summary_en,
                summary_zh,
                category: package.get("category").and_then(Value::as_str).map(str::to_string),
                verified,
                trust: TrustTier::BuiltinReviewed,
                installs: package.get("installCount").and_then(Value::as_u64),
                stars: package.get("stars").and_then(Value::as_u64),
                downloads: None,
            })
        })
        .collect();
    Ok(entries)
}

/// The standard contract every custom catalog serves, as v1 defined it:
/// `schemaVersion` 1.0.0 with items that pin one npm target each. Nothing
/// here is verified — a custom source only ever suggests.
fn parse_standard(source: &SourceId, value: &Value) -> Result<Vec<CatalogEntry>> {
    if value.get("schemaVersion").and_then(Value::as_str) != Some("1.0.0") {
        return Err(Error::Plugin(
            "standard catalog schemaVersion must be 1.0.0".into(),
        ));
    }
    let items = value
        .get("items")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Plugin("standard catalog has no items array".into()))?;
    let entries = items
        .iter()
        .filter_map(|item| {
            let name = item.pointer("/package/name").and_then(Value::as_str)?;
            if !is_package_name(name) {
                return None;
            }
            // The contract pins `latestVersion`; a body that stretches it to a
            // range still lists, but only as a display-only entry.
            let version = item.get("latestVersion").and_then(Value::as_str);
            let npm_spec = version
                .filter(|version| is_exact_version(version))
                .map(|version| format!("{name}@{version}"));
            Some(CatalogEntry {
                source: source.clone(),
                name: name.to_string(),
                npm_name: Some(name.to_string()),
                npm_spec,
                summary_en: plain_text(item.get("summary").and_then(Value::as_str)),
                summary_zh: None,
                category: categories(item).into_iter().next(),
                verified: false,
                trust: TrustTier::Custom,
                installs: None,
                stars: None,
                downloads: None,
            })
        })
        .collect();
    Ok(entries)
}

/// A standard item's category tags, read as v1 read them.
fn categories(value: &Value) -> Vec<String> {
    let mut categories: Vec<String> = ["categories", "tags", "keywords"]
        .into_iter()
        .filter_map(|key| value.get(key))
        .flat_map(|value| match value {
            Value::Array(values) => values
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>(),
            Value::String(value) => value.split(',').collect(),
            _ => Vec::new(),
        })
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.len() <= 48)
        .take(20)
        .map(str::to_string)
        .collect();
    categories.sort_by_key(|value| value.to_ascii_lowercase());
    categories.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    categories
}

/// Split `name@range`, keeping a scoped name whole — the manifest's shape.
fn spec_parts(spec: &str) -> Option<(&str, &str)> {
    let (name, range) = spec.rsplit_once('@')?;
    (!name.is_empty()).then_some((name, range))
}

/// Read a source's `{en, zh}` blurb, or a plain string, into the two slots.
fn localized(value: Option<&Value>) -> (Option<String>, Option<String>) {
    match value {
        Some(Value::String(text)) => (plain_text(Some(text)), None),
        Some(value) => (
            plain_text(value.get("en").and_then(Value::as_str)),
            plain_text(value.get("zh").and_then(Value::as_str)),
        ),
        None => (None, None),
    }
}

/// Trimmed, control-character-free text, capped for one summary field.
fn plain_text(value: Option<&str>) -> Option<String> {
    let trimmed = value?.trim();
    if trimmed.is_empty()
        || trimmed.chars().any(|character| {
            character.is_control()
                || matches!(character, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        })
    {
        return None;
    }
    Some(trimmed.chars().take(2_000).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_latest_manifest_becomes_a_catalog_entry() {
        let body = r#"{
            "name": "dsh-local-telemetry",
            "version": "0.1.0",
            "description": "local-first telemetry",
            "keywords": ["dsh-plugin", "telemetry"]
        }"#;
        let entry = npm_manifest_entry("dsh-local-telemetry", body).expect("entry");
        assert_eq!(entry.name, "dsh-local-telemetry");
        assert_eq!(entry.source, SourceId::Npm);
        assert_eq!(entry.npm_spec.as_deref(), Some("dsh-local-telemetry@0.1.0"));
        assert_eq!(entry.summary_en.as_deref(), Some("local-first telemetry"));
        assert_eq!(entry.category.as_deref(), Some("dsh-plugin"));
    }

    #[test]
    fn a_latest_manifest_without_a_version_is_refused() {
        assert!(npm_manifest_entry("dsh-local-telemetry", "{}").is_err());
        assert!(npm_manifest_entry("dsh-local-telemetry", "not json").is_err());
    }

    const NPM_BODY: &str = r#"{
        "objects": [
            {
                "downloads": { "weekly": 421 },
                "package": {
                    "name": "@vendor/dsh-notes",
                    "version": "0.2.4",
                    "description": "notes as a profile layer",
                    "date": "2026-08-16T06:46:50.294Z",
                    "publisher": { "username": "vendor" },
                    "links": { "repository": "https://example.invalid/notes" },
                    "keywords": ["notes", "dsh"]
                }
            },
            { "package": { "name": "bare-plugin" } }
        ]
    }"#;

    const AWESOME_BODY: &str = r#"{
        "name": "awesome-dsh",
        "url": "https://awesome-dsh-plugin.com",
        "updated": "2026-09-01",
        "count": 4,
        "categories": {
            "productivity": { "en": "Productivity", "zh": "效率" },
            "theme": { "en": "Themes", "zh": "主题" }
        },
        "plugins": [
            {
                "name": "Note Sync",
                "owner": "example",
                "category": "productivity",
                "description": { "en": "Sync notes across profiles", "zh": "跨配置同步笔记" },
                "npm": "dsh-note-sync",
                "tarball": "https://registry.npmjs.org/dsh-note-sync/-/dsh-note-sync-v1.2.3.tgz",
                "stars": 120,
                "downloads": 900
            },
            {
                "name": "Tagger",
                "owner": "example",
                "category": "productivity",
                "description": { "en": "Tag sessions" },
                "npm": "dsh-tagger",
                "tarball": "https://registry.npmjs.org/dsh-tagger/-/dsh-tagger-1.0.0.tgz",
                "stars": 7,
                "downloads": 12
            },
            {
                "name": "Archiver",
                "owner": "example",
                "category": "productivity",
                "description": { "en": "Archive sessions locally" },
                "npm": "dsh-archiver",
                "tarball": "https://github.com/example/dsh-archiver/archive/refs/tags/v0.9.0.tar.gz",
                "stars": 3,
                "downloads": null
            },
            {
                "name": "Ocean Theme",
                "owner": "example",
                "category": "theme",
                "description": { "en": "A calm blue theme" },
                "npm": null,
                "stars": 40,
                "downloads": null
            }
        ]
    }"#;

    const REVIEWED_BODY: &str = r#"{
        "packages": [
            {
                "id": "note-sync",
                "name": "Note Sync",
                "owner": "example",
                "category": "productivity",
                "description": { "en": "Sync notes", "zh": "同步笔记" },
                "installCount": 4000,
                "installs24h": 30,
                "installs7d": 210,
                "installs30d": 900,
                "stars": 88,
                "updatedAt": "2026-08-31T10:00:00Z",
                "installMethods": [
                    { "kind": "github", "spec": "github:example/dsh-note-sync", "verification": "unverified" },
                    { "kind": "npm", "spec": "@example/dsh-note-sync@1.2.3", "verification": "verified",
                      "code": "repository_backlink", "requiresBuildAllowance": false, "revision": "v1.2.3" }
                ]
            },
            {
                "id": "ranged",
                "name": "Ranged Tool",
                "category": "tools",
                "description": { "en": "Verified but ranged" },
                "installCount": 30,
                "stars": 2,
                "installMethods": [
                    { "kind": "npm", "spec": "ranged-tool@^1.0.0", "verification": "verified" }
                ]
            },
            {
                "id": "unverified",
                "name": "Unverified Tool",
                "category": "tools",
                "description": { "en": "Unverified" },
                "installCount": 10,
                "stars": 1,
                "installMethods": [
                    { "kind": "npm", "spec": "unverified-tool@0.1.0", "verification": "unverified" }
                ]
            },
            {
                "id": "github-only",
                "name": "Github Only",
                "category": "tools",
                "description": { "en": "GitHub only" },
                "installCount": 3,
                "stars": 5,
                "installMethods": [
                    { "kind": "github", "spec": "github:other/repo", "verification": "unverified" }
                ]
            }
        ]
    }"#;

    #[test]
    fn npm_search_results_suggest_exact_registry_versions() {
        let entries = parse(&SourceId::Npm, NPM_BODY).expect("npm body");
        assert_eq!(entries.len(), 2);

        let notes = &entries[0];
        assert_eq!(notes.source, SourceId::Npm);
        assert_eq!(notes.npm_spec.as_deref(), Some("@vendor/dsh-notes@0.2.4"));
        assert_eq!(notes.npm_name.as_deref(), Some("@vendor/dsh-notes"));
        assert_eq!(notes.summary_en.as_deref(), Some("notes as a profile layer"));
        assert_eq!(notes.summary_zh, None);
        assert_eq!(notes.category.as_deref(), Some("dsh"));
        assert_eq!(notes.downloads, Some(421));
        assert_eq!(notes.installs, None);
        assert!(notes.verified);
        assert_eq!(notes.trust, TrustTier::BuiltinNpm);

        // The registry supplied no version for this one, so it cannot suggest.
        assert_eq!(entries[1].name, "bare-plugin");
        assert_eq!(entries[1].npm_spec, None);
        assert_eq!(entries[1].downloads, None);
    }

    #[test]
    fn awesome_dsh_entries_without_pinned_versions_stay_display_only() {
        let entries = parse(&SourceId::AwesomeDsh, AWESOME_BODY).expect("awesome body");
        assert_eq!(entries.len(), 4);

        // A `v`-prefixed tarball is the one honest version source here.
        let synced = &entries[0];
        assert_eq!(synced.npm_spec.as_deref(), Some("dsh-note-sync@1.2.3"));
        assert_eq!(synced.npm_name.as_deref(), Some("dsh-note-sync"));
        assert_eq!(synced.summary_zh.as_deref(), Some("跨配置同步笔记"));
        assert_eq!(synced.category.as_deref(), Some("productivity"));
        assert_eq!(synced.stars, Some(120));
        assert_eq!(synced.downloads, Some(900));
        assert!(!synced.verified);
        assert_eq!(synced.trust, TrustTier::BuiltinAwesome);

        // Registry tarballs spell versions without the `v`, so this one keeps
        // its npm name but makes no suggestion.
        let tagger = &entries[1];
        assert_eq!(tagger.npm_name.as_deref(), Some("dsh-tagger"));
        assert_eq!(tagger.npm_spec, None);

        // GitHub tag archives do carry a `v`-prefixed version.
        let archiver = &entries[2];
        assert_eq!(archiver.npm_spec.as_deref(), Some("dsh-archiver@0.9.0"));

        let theme = &entries[3];
        assert_eq!(theme.npm_name, None);
        assert_eq!(theme.npm_spec, None);
        assert_eq!(theme.downloads, None);
        assert_eq!(theme.category.as_deref(), Some("theme"));
    }

    #[test]
    fn reviewed_store_entries_carry_their_verification_word() {
        let entries = parse(&SourceId::Reviewed1024, REVIEWED_BODY).expect("reviewed body");
        assert_eq!(entries.len(), 4);

        let synced = &entries[0];
        assert_eq!(synced.source, SourceId::Reviewed1024);
        assert_eq!(synced.npm_spec.as_deref(), Some("@example/dsh-note-sync@1.2.3"));
        assert_eq!(synced.npm_name.as_deref(), Some("@example/dsh-note-sync"));
        assert!(synced.verified);
        assert_eq!(synced.trust, TrustTier::BuiltinReviewed);
        assert_eq!(synced.installs, Some(4000));
        assert_eq!(synced.stars, Some(88));
        assert_eq!(synced.summary_zh.as_deref(), Some("同步笔记"));

        // Verified by the store but pinned to a range: named, not suggested.
        let ranged = &entries[1];
        assert_eq!(ranged.npm_name.as_deref(), Some("ranged-tool"));
        assert_eq!(ranged.npm_spec, None);
        assert!(ranged.verified);

        // The store did not vouch for this one even though the spec is exact.
        let unverified = &entries[2];
        assert_eq!(unverified.npm_spec.as_deref(), Some("unverified-tool@0.1.0"));
        assert!(!unverified.verified);

        // No npm method at all: display-only, unverified.
        let github_only = &entries[3];
        assert_eq!(github_only.npm_name, None);
        assert_eq!(github_only.npm_spec, None);
        assert!(!github_only.verified);
        assert_eq!(github_only.installs, Some(3));
    }

    #[test]
    fn custom_sources_serve_the_standard_v1_contract() {
        let source = SourceId::Custom("my-catalog".into());
        let entries = parse(
            &source,
            r#"{
                "schemaVersion": "1.0.0",
                "items": [
                    { "package": { "name": "community-plugin" }, "latestVersion": "2.0.0",
                      "summary": "From a custom catalog", "categories": ["agent"] },
                    { "package": { "name": "ranged-plugin" }, "latestVersion": "^2.0.0" },
                    { "package": { "name": "no-version-plugin" } }
                ]
            }"#,
        )
        .expect("standard body");
        assert_eq!(entries.len(), 3);

        let pinned = &entries[0];
        assert_eq!(pinned.source, source);
        assert_eq!(pinned.npm_spec.as_deref(), Some("community-plugin@2.0.0"));
        assert_eq!(pinned.summary_en.as_deref(), Some("From a custom catalog"));
        assert_eq!(pinned.category.as_deref(), Some("agent"));
        assert!(!pinned.verified);
        assert_eq!(pinned.trust, TrustTier::Custom);

        // A stretched contract lists, but never suggests.
        assert_eq!(entries[1].npm_name.as_deref(), Some("ranged-plugin"));
        assert_eq!(entries[1].npm_spec, None);
        assert_eq!(entries[2].npm_spec, None);
    }

    #[test]
    fn exact_versions_are_concrete_not_ranges_or_tags() {
        assert!(is_exact_version("1.2.3"));
        assert!(is_exact_version("v1.2.3"));
        assert!(is_exact_version("0.0.1-rc.1"));
        assert!(is_exact_version("16"));
        assert!(!is_exact_version("^1.2.3"));
        assert!(!is_exact_version("~1.2"));
        assert!(!is_exact_version("1.x"));
        assert!(!is_exact_version("*"));
        assert!(!is_exact_version("latest"));
        assert!(!is_exact_version("1.2.3+rebuilt"));
        assert!(!is_exact_version(""));
        assert!(!is_exact_version("next"));
        assert!(!is_exact_version("1.2.3@next"));
    }

    #[test]
    fn search_matches_every_term_across_names_and_summaries() {
        let entries = parse(&SourceId::AwesomeDsh, AWESOME_BODY).expect("awesome body");
        // Multi-term AND, case-insensitive, over names and npm names.
        assert_eq!(search(&entries, "note sync", None).len(), 1);
        assert_eq!(search(&entries, "NOTE", None).len(), 1);
        assert_eq!(search(&entries, "dsh-tagger", None).len(), 1);
        assert_eq!(search(&entries, "note missing", None).len(), 0);
        assert_eq!(search(&entries, "", None).len(), 4);
        // The category filter is an exact match on the source's own key.
        assert_eq!(search(&entries, "", Some("theme")).len(), 1);
        assert_eq!(search(&entries, "", Some("Theme")).len(), 0);
        assert_eq!(search(&entries, "ocean", Some("theme")).len(), 1);
        assert_eq!(search(&entries, "ocean", Some("productivity")).len(), 0);

        let reviewed = parse(&SourceId::Reviewed1024, REVIEWED_BODY).expect("reviewed body");
        assert_eq!(search(&reviewed, "同步笔记", None).len(), 1);
        assert_eq!(search(&reviewed, "", None).len(), 4);
    }

    #[test]
    fn malformed_catalog_bodies_are_contract_errors() {
        assert!(parse(&SourceId::Npm, "not json").is_err());
        assert!(parse(&SourceId::Npm, "{}").is_err());
        assert!(parse(&SourceId::AwesomeDsh, "{}").is_err());
        assert!(parse(&SourceId::Reviewed1024, "{}").is_err());
        assert!(parse(
            &SourceId::Custom("my-catalog".into()),
            r#"{ "schemaVersion": "2.0.0", "items": [] }"#
        )
        .is_err());
        assert!(parse(
            &SourceId::Custom("my-catalog".into()),
            r#"{ "schemaVersion": "1.0.0" }"#
        )
        .is_err());
    }
}
