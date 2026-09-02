//! The market's source registry: where catalog listings may come from.
//!
//! A catalog source is a place plugin suggestions are read from — never a way
//! to run anything. Everything a source offers is reduced, in
//! [`crate::plugins::catalog`], to entries that at most suggest an exact npm
//! `name@version`, and installs still go through the official
//! `dsh plugin add` against the configured registry.
//!
//! Three sources are built in: the npm registry itself, the awesome-dsh
//! community index, and the reviewed DSH 1024Store snapshot. HarnessDeck v1
//! also shipped a `dshfind` source; HarnessLite drops it and ships
//! `awesome-dsh` in its place. On top of the built-ins a user may register at
//! most twelve custom catalogs, each of which has to pass the same admission
//! review before it is stored — and nothing a custom source says is ever
//! treated as verified.
//!
//! The registry is one small JSON file, `market-sources.json`, kept in the
//! exact shape HarnessDeck froze: `{"active": "npm", "custom": [...]}`.

use std::collections::BTreeSet;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// At most this many user-added catalogs may be registered.
pub const MAX_CUSTOM: usize = 12;

/// Built-in id of the npm registry source.
pub const NPM_ID: &str = "npm";
/// Built-in id of the awesome-dsh community index.
pub const AWESOME_ID: &str = "awesome-dsh";
/// Built-in id of the reviewed DSH 1024Store snapshot.
pub const REVIEWED_ID: &str = "dsh-1024store";

/// Where the awesome-dsh community index is served from.
pub const AWESOME_ENDPOINT: &str = "https://awesome-dsh-plugin.com/plugins.json";
/// Where the reviewed DSH 1024Store snapshot is served from.
pub const REVIEWED_ENDPOINT: &str = "https://deepseek1024.com/api/v1/plugins";

/// How much the shell trusts what one source says.
///
/// Trust attaches to the source, not the entry: a catalog can only suggest,
/// so the tiers decide how a suggestion is presented, never what may run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TrustTier {
    /// The npm registry itself — the authority on whether `name@version`
    /// exists, which is why its entries alone are verified by origin.
    BuiltinNpm,
    /// The awesome-dsh community index: curated, but it attests nothing.
    BuiltinAwesome,
    /// The reviewed DSH 1024Store snapshot, which re-verifies npm targets
    /// entry by entry.
    BuiltinReviewed,
    /// A user-added catalog. Nothing it says is verified.
    Custom,
}

impl TrustTier {
    /// Whether entries of this tier may claim the verified badge on their
    /// own word. Every custom-source entry is unverified, always.
    pub fn attests(self) -> bool {
        matches!(self, TrustTier::BuiltinNpm | TrustTier::BuiltinReviewed)
    }
}

/// Identity of one catalog source, built-in or user-added.
///
/// Serializes to the source's id string, so a custom source round-trips as
/// exactly the id stored in `market-sources.json`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SourceId {
    Npm,
    AwesomeDsh,
    Reviewed1024,
    Custom(String),
}

impl SourceId {
    /// The id string this source is stored and displayed under.
    pub fn as_str(&self) -> &str {
        match self {
            SourceId::Npm => NPM_ID,
            SourceId::AwesomeDsh => AWESOME_ID,
            SourceId::Reviewed1024 => REVIEWED_ID,
            SourceId::Custom(id) => id,
        }
    }

    /// Parse an id string, accepting built-ins and slug-shaped customs only.
    pub fn parse(id: &str) -> Result<Self> {
        match id {
            NPM_ID => Ok(SourceId::Npm),
            AWESOME_ID => Ok(SourceId::AwesomeDsh),
            REVIEWED_ID => Ok(SourceId::Reviewed1024),
            other if is_valid_id(other) => Ok(SourceId::Custom(other.to_string())),
            other => Err(Error::Plugin(format!("unknown catalog source {other}"))),
        }
    }

    /// The trust tier entries from this source carry.
    pub fn trust(&self) -> TrustTier {
        match self {
            SourceId::Npm => TrustTier::BuiltinNpm,
            SourceId::AwesomeDsh => TrustTier::BuiltinAwesome,
            SourceId::Reviewed1024 => TrustTier::BuiltinReviewed,
            SourceId::Custom(_) => TrustTier::Custom,
        }
    }

    /// Whether this is one of the three built-in sources.
    pub fn built_in(&self) -> bool {
        !matches!(self, SourceId::Custom(_))
    }
}

impl std::fmt::Display for SourceId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Serialize for SourceId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for SourceId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let id = String::deserialize(deserializer)?;
        SourceId::parse(&id).map_err(serde::de::Error::custom)
    }
}

/// One catalog source as the market panel lists it.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub id: SourceId,
    pub label: String,
    /// The response contract the runtime layer speaks to this source:
    /// `npm`, `snapshot-http`, `reviewed-http` or `standard-http-v1`.
    pub kind: &'static str,
    pub endpoint: Option<String>,
    pub built_in: bool,
    pub active: bool,
}

/// One user-added catalog, exactly as `market-sources.json` stores it.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomSource {
    pub id: String,
    pub label: String,
    pub endpoint: String,
}

/// Whether a custom source id is slug-like: lowercase ASCII letters, digits
/// and hyphens, at most forty characters.
pub fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 40
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

/// Trimmed, control-character-free text of at most `max` bytes.
///
/// Directionality overrides have no business in a user-visible label: they
/// can make one source's name render as if the shell had vouched for it.
fn plain(value: &str, max: usize) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()
        && trimmed.len() <= max
        && !trimmed.chars().any(|character| {
            character.is_control()
                || matches!(character, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        }))
    .then(|| trimmed.to_string())
}

/// Whether an endpoint is an `https://` URL without credentials or a fragment.
///
/// Deliberately shallow — this is the pure admission check. The runtime layer
/// repeats the full URL, SSRF and redirect review when it actually fetches.
fn is_valid_endpoint(endpoint: &str) -> bool {
    if !endpoint.starts_with("https://") {
        return false;
    }
    if endpoint.chars().any(|character| character.is_whitespace() || character.is_control()) {
        return false;
    }
    if endpoint.contains('#') {
        return false;
    }
    let authority = endpoint["https://".len()..]
        .split(['/', '?'])
        .next()
        .unwrap_or_default();
    // Userinfo would smuggle credentials into the stored endpoint.
    !authority.is_empty() && !authority.contains('@')
}

/// Pure admission review for a user-added catalog source.
///
/// A source that does not pass this never reaches the store. The id must be
/// slug-shaped and cannot shadow a built-in source; the label has to be
/// readable text; the endpoint has to be plain HTTPS.
pub fn admit_custom(id: &str, label: &str, endpoint: &str) -> Result<CustomSource> {
    if !is_valid_id(id) {
        return Err(Error::Plugin(
            "catalog source id must be 1-40 lowercase ASCII letters, digits or hyphens".into(),
        ));
    }
    if id == NPM_ID || id == AWESOME_ID || id == REVIEWED_ID {
        return Err(Error::Plugin(format!(
            "{id} is a built-in catalog source and cannot be re-registered"
        )));
    }
    let label = plain(label, 80)
        .ok_or_else(|| Error::Plugin("catalog label must be 1-80 readable characters".into()))?;
    if !is_valid_endpoint(endpoint) {
        return Err(Error::Plugin(
            "catalog endpoint must be a credential-free https:// URL".into(),
        ));
    }
    Ok(CustomSource {
        id: id.to_string(),
        label,
        endpoint: endpoint.to_string(),
    })
}

/// What `market-sources.json` stores, in the schema HarnessDeck froze.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Settings {
    #[serde(default = "default_active")]
    active: String,
    #[serde(default)]
    custom: Vec<CustomSource>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            active: default_active(),
            custom: Vec::new(),
        }
    }
}

fn default_active() -> String {
    NPM_ID.to_string()
}

/// The `market-sources.json` store: the active source plus user-added catalogs.
pub struct Store {
    file: PathBuf,
}

impl Store {
    /// The store the shell manages, at the application data root.
    pub fn managed() -> Self {
        Self {
            file: crate::paths::market_sources_file(),
        }
    }

    /// A store rooted at an explicit file, for tests.
    #[cfg(test)]
    fn at(path: PathBuf) -> Self {
        Self { file: path }
    }

    /// Read the store, silently dropping anything that would no longer pass
    /// admission: a hand-edited or downgraded file must never resurrect an
    /// insecure endpoint or a shadowed built-in id.
    fn read(&self) -> Settings {
        let mut settings: Settings = std::fs::read(&self.file)
            .ok()
            .and_then(|body| serde_json::from_slice(&body).ok())
            .unwrap_or_default();
        settings.custom.truncate(MAX_CUSTOM);
        let mut seen_ids = BTreeSet::new();
        settings.custom.retain(|source| {
            source.id != NPM_ID
                && source.id != AWESOME_ID
                && source.id != REVIEWED_ID
                && is_valid_id(&source.id)
                && plain(&source.label, 80).is_some()
                && is_valid_endpoint(&source.endpoint)
                && seen_ids.insert(source.id.clone())
        });
        let known = settings.active == NPM_ID
            || settings.active == AWESOME_ID
            || settings.active == REVIEWED_ID
            || settings
                .custom
                .iter()
                .any(|source| source.id == settings.active);
        if !known {
            settings.active = default_active();
        }
        settings
    }

    fn write(&self, settings: &Settings) -> Result<()> {
        if settings.custom.len() > MAX_CUSTOM {
            return Err(Error::Plugin("too many catalog sources".into()));
        }
        if let Some(parent) = self.file.parent() {
            std::fs::create_dir_all(parent).map_err(|cause| {
                Error::Store(format!("{} could not be created: {cause}", parent.display()))
            })?;
        }
        let mut body = serde_json::to_vec_pretty(settings).map_err(|cause| {
            Error::Store(format!("catalog sources could not be encoded: {cause}"))
        })?;
        body.push(b'\n');
        crate::atomic::write(&self.file, body).map_err(|cause| {
            Error::Store(format!(
                "{} could not be committed: {cause}",
                self.file.display()
            ))
        })
    }

    /// The full roster: built-ins first, then custom sources in stored order,
    /// with the active flag applied and npm as the fallback.
    pub fn sources(&self) -> Vec<Source> {
        let settings = self.read();
        let mut sources = vec![
            Source {
                id: SourceId::Npm,
                label: NPM_ID.to_string(),
                kind: "npm",
                endpoint: None,
                built_in: true,
                active: settings.active == NPM_ID,
            },
            Source {
                id: SourceId::AwesomeDsh,
                label: AWESOME_ID.to_string(),
                kind: "snapshot-http",
                endpoint: Some(AWESOME_ENDPOINT.to_string()),
                built_in: true,
                active: settings.active == AWESOME_ID,
            },
            Source {
                id: SourceId::Reviewed1024,
                label: "DSH 1024Store".to_string(),
                kind: "reviewed-http",
                endpoint: Some(REVIEWED_ENDPOINT.to_string()),
                built_in: true,
                active: settings.active == REVIEWED_ID,
            },
        ];
        sources.extend(settings.custom.iter().map(|custom| Source {
            id: SourceId::Custom(custom.id.clone()),
            label: custom.label.clone(),
            kind: "standard-http-v1",
            endpoint: Some(custom.endpoint.clone()),
            built_in: false,
            active: settings.active == custom.id,
        }));
        if !sources.iter().any(|source| source.active) {
            sources[0].active = true;
        }
        sources
    }

    /// The source listings are currently fetched from.
    pub fn active(&self) -> SourceId {
        self.sources()
            .into_iter()
            .find(|source| source.active)
            .map(|source| source.id)
            .unwrap_or(SourceId::Npm)
    }

    /// Validate and record one custom catalog, making it the active source.
    pub fn add(&self, id: &str, label: &str, endpoint: &str) -> Result<Vec<Source>> {
        let custom = admit_custom(id, label, endpoint)?;
        let mut settings = self.read();
        if settings.custom.len() >= MAX_CUSTOM {
            return Err(Error::Plugin(format!(
                "at most {MAX_CUSTOM} custom catalog sources are allowed"
            )));
        }
        if settings.custom.iter().any(|source| source.id == custom.id) {
            return Err(Error::Plugin(format!(
                "a custom catalog source with id {} already exists",
                custom.id
            )));
        }
        settings.active = custom.id.clone();
        settings.custom.push(custom);
        self.write(&settings)?;
        Ok(self.sources())
    }

    /// Make a known source — built-in or custom — the active one.
    pub fn select(&self, id: &str) -> Result<Vec<Source>> {
        if !self.sources().iter().any(|source| source.id.as_str() == id) {
            return Err(Error::Plugin(format!("unknown catalog source {id}")));
        }
        let mut settings = self.read();
        settings.active = id.to_string();
        self.write(&settings)?;
        Ok(self.sources())
    }

    /// Remove a custom source; built-in and unknown ids are refused. Removing
    /// the active source falls back to npm.
    pub fn remove(&self, id: &str) -> Result<Vec<Source>> {
        let mut settings = self.read();
        let before = settings.custom.len();
        settings.custom.retain(|source| source.id != id);
        if settings.custom.len() == before {
            return Err(Error::Plugin(
                "built-in or unknown catalog sources cannot be removed".into(),
            ));
        }
        if settings.active == id {
            settings.active = default_active();
        }
        self.write(&settings)?;
        Ok(self.sources())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The v1 store HarnessDeck froze, exactly as it shipped. The fixture
    /// lives in the repository root's `tests/`, five levels up from here.
    const FIXTURE: &str =
        include_str!("../../../../../tests/fixtures/harnessdeck/market-sources.json");

    fn store(label: &str) -> Store {
        let root = std::env::temp_dir().join(format!(
            "harnesslite-sources-{label}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("sandbox root");
        Store::at(root.join("market-sources.json"))
    }

    #[test]
    fn the_frozen_v1_store_fixture_parses_and_activates_npm() {
        let settings: Settings = serde_json::from_str(FIXTURE).expect("fixture parses");
        assert_eq!(settings.active, "npm");
        assert!(settings.custom.is_empty());

        // And the store reads that same document without rewriting it.
        let store = store("fixture");
        std::fs::write(&store.file, FIXTURE).expect("fixture copy");
        let roster = store.sources();
        assert_eq!(roster.len(), 3);
        assert_eq!(roster[0].id, SourceId::Npm);
        assert!(roster[0].active);
        assert_eq!(roster[1].label, "awesome-dsh");
        assert_eq!(
            roster[1].endpoint.as_deref(),
            Some("https://awesome-dsh-plugin.com/plugins.json")
        );
        assert_eq!(roster[2].label, "DSH 1024Store");
        assert_eq!(roster[2].endpoint.as_deref(), Some(REVIEWED_ENDPOINT));
        assert!(roster.iter().all(|source| source.built_in));
        assert_eq!(store.active(), SourceId::Npm);
    }

    #[test]
    fn a_custom_source_is_admitted_recorded_selected_and_removed() {
        let store = store("cycle");
        let roster = store
            .add(
                "team-catalog",
                "Team Catalog",
                "https://catalog.example.invalid/plugins.json",
            )
            .expect("admitted");
        assert!(roster
            .iter()
            .any(|source| source.id.as_str() == "team-catalog" && source.active));
        assert_eq!(store.active(), SourceId::Custom("team-catalog".into()));

        // The selection survives a re-opened store.
        store.select("npm").expect("select npm");
        assert_eq!(Store::at(store.file.clone()).active(), SourceId::Npm);

        store.select("team-catalog").expect("select custom");
        assert_eq!(store.active(), SourceId::Custom("team-catalog".into()));

        store.remove("team-catalog").expect("removed");
        assert_eq!(store.active(), SourceId::Npm);
        assert!(!store
            .sources()
            .iter()
            .any(|source| source.id.as_str() == "team-catalog"));

        // Built-ins are neither selectable ghosts nor removable.
        assert!(store.select("ghost").is_err());
        assert!(store.remove("npm").is_err());
        assert!(store.remove("ghost").is_err());
    }

    #[test]
    fn admission_rejects_insecure_endpoints_and_bad_slugs() {
        let store = store("admission");
        let good = "https://catalog.example.invalid/plugins.json";
        assert!(admit_custom("ok", "Ok", "http://catalog.example.invalid/plugins.json").is_err());
        assert!(admit_custom("ok", "Ok", "https://user:pass@catalog.example.invalid/x").is_err());
        assert!(admit_custom("ok", "Ok", "https://catalog.example.invalid/page#frag").is_err());
        assert!(admit_custom("ok", "Ok", "notaurl").is_err());
        assert!(admit_custom("ok", "", good).is_err());
        assert!(admit_custom("ok", "   ", good).is_err());
        assert!(admit_custom("ok", &"x".repeat(81), good).is_err());
        assert!(admit_custom("Ok", "Ok", good).is_err());
        assert!(admit_custom("has space", "Ok", good).is_err());
        assert!(admit_custom(&"a".repeat(41), "Ok", good).is_err());
        for built_in in [NPM_ID, AWESOME_ID, REVIEWED_ID] {
            assert!(admit_custom(built_in, "Ok", good).is_err());
        }
        // Nothing rejected above may have reached the store.
        assert_eq!(store.sources().len(), 3);
        assert!(store.add("bad id", "Ok", good).is_err());
        assert_eq!(store.sources().len(), 3);

        // The happy path trims the label and keeps the frozen shape.
        let custom = admit_custom("ok", "  Spaced  ", good).expect("admitted");
        assert_eq!(custom.id, "ok");
        assert_eq!(custom.label, "Spaced");
        assert_eq!(custom.endpoint, good);
    }

    #[test]
    fn twelve_custom_sources_fit_and_a_thirteenth_does_not() {
        let store = store("capacity");
        for number in 0..MAX_CUSTOM {
            store.add(
                &format!("catalog-{number:02}"),
                &format!("Catalog {number:02}"),
                &format!("https://catalog{number:02}.example.invalid/plugins.json"),
            ).unwrap_or_else(|_| panic!("custom {number:02} should be admitted"));
        }
        assert_eq!(store.sources().len(), 3 + MAX_CUSTOM);
        assert!(store
            .add(
                "one-too-many",
                "One Too Many",
                "https://toomany.example.invalid/plugins.json"
            )
            .is_err());
        assert_eq!(store.sources().len(), 3 + MAX_CUSTOM);
    }

    #[test]
    fn custom_ids_are_unique_among_customs() {
        let store = store("duplicate");
        store
            .add("twice", "Twice", "https://one.example.invalid/plugins.json")
            .expect("first");
        assert!(store
            .add("twice", "Twice Again", "https://two.example.invalid/plugins.json")
            .is_err());
        assert_eq!(store.sources().len(), 4);
    }

    #[test]
    fn a_hand_edited_store_drops_what_would_not_pass_admission() {
        let store = store("sanitize");
        std::fs::write(
            &store.file,
            r#"{
                "active": "gone",
                "custom": [
                    { "id": "insecure", "label": "Insecure", "endpoint": "http://x.example.invalid" },
                    { "id": "npm", "label": "Shadow", "endpoint": "https://x.example.invalid" },
                    { "id": "ok", "label": "Ok", "endpoint": "https://ok.example.invalid" },
                    { "id": "ok", "label": "Duplicate", "endpoint": "https://again.example.invalid" }
                ]
            }"#,
        )
        .expect("hand-edited store");
        let roster = store.sources();
        assert_eq!(roster.len(), 4);
        // Only the custom "ok" survives: the insecure endpoint and the
        // built-in-shadowing id are dropped, and so is the duplicate.
        let custom: Vec<&Source> = roster.iter().filter(|source| !source.built_in).collect();
        assert_eq!(custom.len(), 1);
        assert_eq!(custom[0].id.as_str(), "ok");
        // The stale active pointer falls back to npm.
        assert_eq!(store.active(), SourceId::Npm);
    }
}
