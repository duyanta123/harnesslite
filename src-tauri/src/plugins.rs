//! Plugin market commands: the dialog's face on the registry and the harness.
//!
//! The division of labour the module comment in hd-core states is the design:
//! hd-core owns the records and the contracts, hd-runtime's market module does
//! the fetching and driving, and this file is the state machine between them —
//! one intent token at a time, one market operation at a time, and a journal
//! that can prove what a crashed operation was about to do.

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, State};

use hd_core::error::{Error, Result};
use hd_core::plugins as pkg;
use hd_core::plugins::catalog::CatalogEntry;

use crate::state::AppState;

/// The page size the frontend's grid was designed around.
const PAGE_SIZE: usize = 25;

/// How long a confirmation token answers for.
const INTENT_TTL: Duration = Duration::from_secs(120);

/* -------------------------------------------------------------------------- */
/* Installed state                                                            */
/* -------------------------------------------------------------------------- */

#[tauri::command]
pub fn plugin_state() -> Result<PluginState> {
    state_of()
}

fn state_of() -> Result<PluginState> {
    let profile = hd_core::projects::active_profile().unwrap_or_else(hd_core::profiles::selected);
    let dir = hd_core::paths::profile_dir(&profile);
    let Some(manifest) = pkg::read_manifest(&dir) else {
        return Ok(PluginState {
            profile,
            profile_dir: dir.to_string_lossy().into_owned(),
            initialized: false,
            plugins: Vec::new(),
            package_manager: false,
        });
    };
    let disabled = pkg::switches::switched_off(&profile);
    let plugins = pkg::list(&manifest, &disabled);

    // A package manager is reachable when the harness's own Node carries npm —
    // the same pair the preflight and every install go through.
    let package_manager = runtime_env_node()
        .map(|node| hd_runtime::harness::install::npm_cli(&node).is_some())
        .unwrap_or(false);

    Ok(PluginState {
        profile,
        profile_dir: dir.to_string_lossy().into_owned(),
        initialized: true,
        plugins,
        package_manager,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginState {
    pub profile: String,
    pub profile_dir: String,
    pub initialized: bool,
    pub plugins: Vec<pkg::InstalledPlugin>,
    pub package_manager: bool,
}

/* -------------------------------------------------------------------------- */
/* Catalog: sources, search, health                                           */
/* -------------------------------------------------------------------------- */

#[tauri::command]
pub fn plugin_sources() -> Vec<CatalogSource> {
    catalog_sources()
}

fn catalog_sources() -> Vec<CatalogSource> {
    pkg::SourceStore::managed()
        .sources()
        .into_iter()
        .map(|source| CatalogSource {
            id: source.id.as_str().to_string(),
            label: source.label,
            kind: source.kind.to_string(),
            endpoint: source.endpoint.map(|value| value.to_string()),
            built_in: source.built_in,
            active: source.active,
        })
        .collect()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogSource {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub endpoint: Option<String>,
    pub built_in: bool,
    pub active: bool,
}

#[tauri::command]
pub fn plugin_source_select(id: String) -> Vec<CatalogSource> {
    let _ = pkg::SourceStore::managed().select(&id);
    catalog_sources()
}

#[tauri::command]
pub fn plugin_source_add(label: String, endpoint: String) -> Result<Vec<CatalogSource>> {
    pkg::SourceStore::managed().add(&custom_id(&label), &label, &endpoint)?;
    Ok(catalog_sources())
}

#[tauri::command]
pub fn plugin_source_remove(id: String) -> Result<Vec<CatalogSource>> {
    pkg::SourceStore::managed().remove(&id)?;
    Ok(catalog_sources())
}

/// A stable, unique id for a custom source: the label, slugged, with a numeric
/// suffix when a previous source already claimed the slug.
fn custom_id(label: &str) -> String {
    let slug: String = label
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_ascii_lowercase();
    let base = if slug.is_empty() { "source".into() } else { slug };
    let existing = pkg::SourceStore::managed().sources();
    if existing.iter().all(|source| source.id.as_str() != base) {
        return base;
    }
    for suffix in 2..100 {
        let id = format!("{base}-{suffix}");
        if existing.iter().all(|source| source.id.as_str() != id) {
            return id;
        }
    }
    format!("{base}-{}", std::process::id())
}

#[tauri::command]
pub async fn plugin_source_health(id: String) -> Result<hd_runtime::market::Health> {
    let endpoint = endpoint_of(&id)?;
    let client = client_of()?;
    Ok(hd_runtime::market::health(&client, &id, endpoint.as_deref()).await?)
}

/// Search the active source and answer one page.
///
/// Community catalogs are whole documents, so filtering and paging happen
/// here; the npm source is a search endpoint whose query does the filtering
/// and hd-core's matcher refines it.
#[tauri::command]
pub async fn plugin_search(
    state: State<'_, AppState>,
    query: String,
    category: Option<String>,
    sort: String,
    page: usize,
    _refresh: Option<bool>,
) -> Result<PluginPage> {
    let _busy_guard = claim_market(&state)?;

    // `active()` names the source; the roster carries its label and endpoint.
    let active_id = pkg::SourceStore::managed().active();
    let active = pkg::SourceStore::managed()
        .sources()
        .into_iter()
        .find(|source| source.id == active_id)
        .ok_or_else(|| Error::Plugin("the active source is not in the roster".into()))?;
    let (id_string, endpoint) = (
        active.id.as_str().to_string(),
        active.endpoint.map(|value| value.to_string()),
    );
    let client = client_of()?;
    let entries = hd_runtime::market::fetch_catalog(&client, &id_string, endpoint.as_deref(), &query).await?;

    let mut matched: Vec<CatalogEntry> =
        pkg::catalog::search(&entries, &query, category.as_deref())
            .into_iter()
            .cloned()
            .collect();
    sort_entries(&mut matched, &sort);

    let total = matched.len();
    let categories = distinct_categories(&entries);
    let start = page * PAGE_SIZE;
    let items: Vec<CatalogItem> = matched
        .into_iter()
        .skip(start)
        .take(PAGE_SIZE)
        .map(CatalogItem::from_entry)
        .collect();
    let has_more = start + items.len() < total;

    Ok(PluginPage {
        items,
        categories,
        total,
        page,
        page_size: PAGE_SIZE,
        has_more,
        // A whole-document catalog has one fetch time; npm answers per query.
        indexed_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    })
}

fn sort_entries(entries: &mut [CatalogEntry], sort: &str) {
    match sort {
        "name" => entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
        "downloads" => entries.sort_by_key(|entry| std::cmp::Reverse(entry.downloads.unwrap_or(0))),
        "updated" | _ => {
            // Source order is the source's own ranking, which is the closest
            // thing to "recently worth looking at" a catalog offers.
        }
    }
}

fn distinct_categories(entries: &[CatalogEntry]) -> Vec<String> {
    let mut categories: Vec<String> = entries
        .iter()
        .filter_map(|entry| entry.category.clone())
        .collect();
    categories.sort();
    categories.dedup();
    categories
}

/// One grid card, flattened from the catalog entry the source produced.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogItem {
    pub name: String,
    pub version: String,
    pub description: String,
    pub publisher: String,
    pub updated: String,
    pub weekly_downloads: u64,
    pub link: Option<String>,
    pub source_id: String,
    pub source_label: String,
    pub installable: bool,
    pub categories: Vec<String>,
    pub has_icon: bool,
}

impl CatalogItem {
    fn from_entry(entry: CatalogEntry) -> Self {
        let description = entry
            .summary_zh
            .clone()
            .filter(|text| !text.is_empty())
            .or(entry.summary_en.clone())
            .or_else(|| entry.npm_name.as_ref().map(|_| String::new()))
            .unwrap_or_default();
        CatalogItem {
            name: entry.npm_name.clone().unwrap_or_else(|| entry.name.clone()),
            version: entry
                .npm_spec
                .as_deref()
                .and_then(|spec| spec.split('@').last())
                .unwrap_or_default()
                .to_string(),
            description,
            publisher: String::new(),
            updated: String::new(),
            weekly_downloads: entry.downloads.unwrap_or(0),
            link: None,
            source_id: entry.source.as_str().to_string(),
            source_label: entry.source.as_str().to_string(),
            // Installable means the source pinned an exact name@version: the
            // one shape a confirmation can honestly promise to install.
            installable: entry.npm_spec.is_some(),
            categories: entry.category.iter().cloned().collect(),
            has_icon: false,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginPage {
    pub items: Vec<CatalogItem>,
    pub categories: Vec<String>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
    pub has_more: bool,
    pub indexed_at: u64,
}

#[tauri::command]
pub async fn plugin_detail(
    source_id: String,
    name: String,
    version: String,
) -> Result<pkg::detail::PackageDetail> {
    let mut detail = hd_runtime::market::detail(&client_of()?, &name, &version).await?;
    detail.source = source_id;
    Ok(detail)
}

/// Marketplace images are deferred: a `null` renders the placeholder, which is
/// honest, rather than fetching foreign media into the shell.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginMedia {
    pub data_url: String,
}

#[tauri::command]
pub fn plugin_media(
    _source_id: String,
    _name: String,
    _version: String,
) -> Option<PluginMedia> {
    None
}

/* -------------------------------------------------------------------------- */
/* Preview, confirm, install                                                  */
/* -------------------------------------------------------------------------- */

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallPreview {
    pub token: String,
    pub expires_in_seconds: u64,
}

#[derive(Clone)]
pub struct Intent {
    pub token: String,
    pub spec: String,
    pub source_id: String,
    pub item_id: String,
    pub display_name: String,
    pub issued: Instant,
}

impl Intent {
    /// The token the shell issued is the token that may install; anything else
    /// — stale, mistyped, replayed — is the same refusal.
    pub fn answers(&self, token: &str) -> bool {
        self.token == token && self.issued.elapsed() <= INTENT_TTL
    }
}

/// Resolve a spec in the preflight scratch project and, when it resolves,
/// hand back the one token that may install it.
#[tauri::command]
pub async fn plugin_preview(
    state: State<'_, AppState>,
    spec: String,
    source_id: String,
    item_id: String,
    display_name: String,
) -> Result<InstallPreview> {
    if !pkg::is_package_spec(&spec) {
        return Err(Error::Plugin(format!("{spec} is not an installable package spec")));
    }

    let _busy_guard = claim_market(&state)?;
    let node = runtime_env_node().ok_or_else(|| {
        Error::Plugin("no usable Node runtime was found; provision one from the console".into())
    })?;
    let (node, npm) = hd_runtime::market::npm_pair(&node)?;

    let resolved = hd_runtime::market::preflight(
        &node,
        &npm,
        &spec,
        &hd_runtime::market::preflight_dir(),
        |_, line| {
            state.supervisor.note(hd_runtime::harness::supervisor::Stream::Stdout, line);
        },
    )
    .await?;
    let _ = resolved;

    let token = new_token();
    *state.intent.lock().expect("intent poisoned") = Some(Intent {
        token: token.clone(),
        spec,
        source_id,
        item_id,
        display_name,
        issued: Instant::now(),
    });

    Ok(InstallPreview {
        token,
        expires_in_seconds: INTENT_TTL.as_secs(),
    })
}

/// Install the previewed spec, consuming its token.
#[tauri::command]
pub async fn plugin_add(app: AppHandle, state: State<'_, AppState>, token: String) -> Result<PluginState> {
    let intent = state.intent.lock().expect("intent poisoned").take();
    let Some(intent) = intent else {
        return Err(Error::Plugin(
            "nothing was previewed; search again and confirm from the dialog".into(),
        ));
    };
    if !intent.answers(&token) {
        return Err(Error::Plugin(
            "that confirmation expired; preview the install again".into(),
        ));
    }

    operate(
        &app,
        &state,
        Operation::Add {
            spec: intent.spec.clone(),
            source_id: intent.source_id.clone(),
            item_id: intent.item_id.clone(),
            display_name: intent.display_name.clone(),
        },
    )
    .await
}

#[tauri::command]
pub async fn plugin_remove(app: AppHandle, state: State<'_, AppState>, name: String) -> Result<PluginState> {
    operate(&app, &state, Operation::Remove { name }).await
}

/// Flip a switch. Nothing is fetched, nothing is deleted; the plugin leaves or
/// rejoins the composition at the harness's next start, via the runtime patch
/// the supervisor launches with.
#[tauri::command]
pub fn plugin_switch(_name: String, enabled: bool) -> Result<PluginState> {
    let profile = hd_core::projects::active_profile().unwrap_or_else(hd_core::profiles::selected);
    if enabled {
        pkg::switches::enable(&profile, &_name)?;
    } else {
        pkg::switches::disable(&profile, &_name)?;
    }
    write_disabled_patch()?;
    state_of()
}

enum Operation {
    Add {
        spec: String,
        source_id: String,
        item_id: String,
        display_name: String,
    },
    Remove {
        name: String,
    },
}

/// One market operation at a time, journaled, driven through the harness.
async fn operate(app: &AppHandle, state: &AppState, operation: Operation) -> Result<PluginState> {
    let _busy_guard = claim_market(state)?;

    let profile = hd_core::projects::active_profile().unwrap_or_else(hd_core::profiles::selected);
    let node = runtime_env_node().ok_or_else(|| {
        Error::Plugin("no usable Node runtime was found; provision one from the console".into())
    })?;

    let (operation_name, subject, retry): (&'static str, String, serde_json::Value) = match &operation {
        Operation::Add { spec, source_id, item_id, display_name } => (
            "add",
            spec.clone(),
            serde_json::json!({
                "kind": "add",
                "spec": spec,
                "sourceId": source_id,
                "itemId": item_id,
                "displayName": display_name,
            }),
        ),
        Operation::Remove { name } => ("remove", name.clone(), serde_json::json!({
            "kind": "remove",
            "name": name,
        })),
    };

    // The journal is what makes a crash mid-pnpm answerable: what was about to
    // happen, to what, and to which profile.
    journal_open(&profile, operation_name, &subject, &retry)?;

    let (node_exe, _npm) = hd_runtime::market::npm_pair(&node)?;
    let entry = hd_core::paths::harness_entry();
    let patch = crate::runtime_env::integration_patch();
    let supervisor = Arc::clone(&state.supervisor);
    let result = hd_runtime::market::run_harness_plugin(
        &node_exe,
        &entry,
        &profile,
        patch.as_deref(),
        operation_name,
        &subject,
        move |stream, line| {
            supervisor.note(
                if stream == "stderr" {
                    hd_runtime::harness::supervisor::Stream::Stderr
                } else {
                    hd_runtime::harness::supervisor::Stream::Stdout
                },
                line,
            );
        },
    )
    .await;

    match result {
        Ok(()) => {
            journal_close();
            // A fresh install must not wake up enabled if the user had already
            // switched it off before — and a remove takes its switches with it.
            if let Operation::Remove { name } = &operation {
                let _ = pkg::switches::enable(&profile, name);
            }
            write_disabled_patch()?;
            state_of()
        }
        Err(failure) => {
            // The journal stays: the recovery centre is how this operation gets
            // finished or explicitly given up on.
            Err(Error::Plugin(format!(
                "the {operation_name} did not complete ({failure}); the recovery centre can retry it"
            )))
        }
    }
    .map(|outcome| {
        let _ = app;
        outcome
    })
}

/* -------------------------------------------------------------------------- */
/* Recovery journal                                                           */
/* -------------------------------------------------------------------------- */

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Journal {
    generation: String,
    profile: String,
    operation: String,
    subject: String,
    /// The profile manifest as it was, so a crashed remove can be undone.
    #[serde(default)]
    before_image: Option<String>,
    detail: String,
    retry: serde_json::Value,
}

fn journal_path() -> PathBuf {
    hd_core::paths::app_data_dir().join("plugin-recovery.json")
}

fn journal_open(profile: &str, operation: &str, subject: &str, retry: &serde_json::Value) -> Result<()> {
    let before_image = std::fs::read_to_string(hd_core::paths::profile_dir(profile).join("package.json")).ok();
    let journal = Journal {
        generation: new_token(),
        profile: profile.into(),
        operation: operation.into(),
        subject: subject.into(),
        before_image,
        detail: format!("a plugin {operation} of {subject} was interrupted"),
        retry: retry.clone(),
    };
    write_journal(&journal)
}

fn journal_close() {
    let _ = std::fs::remove_file(journal_path());
}

fn write_journal(journal: &Journal) -> Result<()> {
    let path = journal_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|cause| Error::Plugin(format!("{} could not be created: {cause}", parent.display())))?;
    }
    let body = serde_json::to_vec_pretty(journal)
        .map_err(|cause| Error::Plugin(format!("the journal could not be encoded: {cause}")))?;
    hd_core::atomic::write(&path, body)
        .map_err(|cause| Error::Plugin(format!("{} could not be written: {cause}", path.display())))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryNotice {
    pub generation: String,
    pub profile: String,
    pub operation: String,
    pub subject: String,
    pub restored: bool,
    pub detail: String,
    pub retry: serde_json::Value,
}

/// The interrupted operation, with any before-image already put back.
#[tauri::command]
pub fn plugin_recovery_notice() -> Result<Option<RecoveryNotice>> {
    let Some(journal) = read_journal() else {
        return Ok(None);
    };

    // A crashed `remove` may have taken the manifest with it; the before-image
    // is the truth the profile goes back to, applied once, here.
    let mut restored = false;
    if journal.operation == "remove" {
        if let Some(before) = &journal.before_image {
            let manifest = hd_core::paths::profile_dir(&journal.profile).join("package.json");
            if std::fs::read_to_string(&manifest).map(|current| current != *before).unwrap_or(true) {
                hd_core::atomic::write(&manifest, before.as_bytes()).map_err(|cause| {
                    Error::Plugin(format!("the profile manifest could not be restored: {cause}"))
                })?;
                restored = true;
            }
        }
    }

    Ok(Some(RecoveryNotice {
        generation: journal.generation,
        profile: journal.profile,
        operation: journal.operation,
        subject: journal.subject,
        restored,
        detail: journal.detail,
        retry: journal.retry,
    }))
}

#[tauri::command]
pub fn plugin_recovery_acknowledge() -> Result<()> {
    journal_close();
    Ok(())
}

/// Finish the interrupted operation, exactly as it was asked.
#[tauri::command]
pub async fn plugin_recovery_retry(app: AppHandle, state: State<'_, AppState>, generation: String) -> Result<PluginState> {
    let Some(journal) = read_journal() else {
        return Err(Error::Plugin("there is no interrupted operation to retry".into()));
    };
    if journal.generation != generation {
        return Err(Error::Plugin("that recovery notice is out of date; reload the centre".into()));
    }

    let retry = journal.retry;
    match retry.get("kind").and_then(serde_json::Value::as_str) {
        Some("add") => {
            let spec = retry.get("spec").and_then(serde_json::Value::as_str).unwrap_or_default().to_string();
            let source_id = retry.get("sourceId").and_then(serde_json::Value::as_str).unwrap_or_default().to_string();
            let item_id = retry.get("itemId").and_then(serde_json::Value::as_str).unwrap_or_default().to_string();
            let display_name = retry.get("displayName").and_then(serde_json::Value::as_str).unwrap_or_default().to_string();
            operate(&app, &state, Operation::Add { spec, source_id, item_id, display_name }).await
        }
        Some("remove") => {
            let name = retry.get("name").and_then(serde_json::Value::as_str).unwrap_or_default().to_string();
            operate(&app, &state, Operation::Remove { name }).await
        }
        _ => Err(Error::Plugin("the interrupted operation cannot be identified".into())),
    }
}

fn read_journal() -> Option<Journal> {
    let body = std::fs::read(journal_path()).ok()?;
    serde_json::from_slice(&body).ok()
}

/* -------------------------------------------------------------------------- */
/* Archives                                                                   */
/* -------------------------------------------------------------------------- */

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchivePackage {
    pub name: String,
    pub version: String,
    pub description: String,
    pub bundle: bool,
    pub path: String,
    pub bytes: u64,
}

/// Read a picked `.tgz` without installing anything from it.
#[tauri::command]
pub async fn plugin_archive(path: String) -> Result<ArchivePackage> {
    let path = PathBuf::from(&path);
    let bytes = std::fs::metadata(&path)
        .map(|meta| meta.len())
        .map_err(|cause| Error::Plugin(format!("{} could not be read: {cause}", path.display())))?;
    let archive = path.clone();
    let package = tokio::task::spawn_blocking(move || hd_runtime::market::archive_manifest(&archive))
        .await
        .map_err(|cause| Error::Plugin(format!("the archive could not be read: {cause}")))??;

    Ok(ArchivePackage {
        name: package.name,
        version: package.version,
        description: package.description,
        bundle: package.bundle,
        path: path.to_string_lossy().into_owned(),
        bytes,
    })
}

/// Install a local archive: copied into the shell's own imports directory
/// first, because the profile records where a file install came from and the
/// original may be on a stick that is gone tomorrow.
#[tauri::command]
pub async fn plugin_import(app: AppHandle, state: State<'_, AppState>, path: String) -> Result<PluginState> {
    let source = PathBuf::from(&path);
    let imports = hd_core::paths::app_data_dir().join("imports");
    std::fs::create_dir_all(&imports)
        .map_err(|cause| Error::Plugin(format!("{} could not be created: {cause}", imports.display())))?;
    let file_name = source
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or_else(|| Error::Plugin(format!("{} does not name a file", source.display())))?;
    let kept = imports.join(&file_name);
    std::fs::copy(&source, &kept)
        .map_err(|cause| Error::Plugin(format!("{} could not be kept: {cause}", kept.display())))?;

    let package = hd_runtime::market::archive_manifest(&kept)?;
    let _ = package;

    operate(&app, &state, Operation::Add {
        spec: kept.to_string_lossy().into_owned(),
        source_id: "file".into(),
        item_id: String::new(),
        display_name: file_name,
    })
    .await
}

/* -------------------------------------------------------------------------- */
/* Plumbing                                                                   */
/* -------------------------------------------------------------------------- */

impl Intent {
    // See `answers` above.
}

fn new_token() -> String {
    let seed = format!(
        "{}\0{:?}",
        std::process::id(),
        std::time::SystemTime::now()
    );
    hex(&Sha256::digest(seed.as_bytes()))
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    out
}

/// The Node the runtime layer works with: the machine's best, already judged.
fn runtime_env_node() -> Option<PathBuf> {
    crate::runtime_env::environment().node.map(|install| install.path)
}

fn client_of() -> Result<reqwest::Client> {
    hd_runtime::node::http::client()
}

fn endpoint_of(id: &str) -> Result<Option<String>> {
    let store = pkg::SourceStore::managed();
    let source = store
        .sources()
        .into_iter()
        .find(|source| source.id.as_str() == id)
        .ok_or_else(|| Error::Plugin(format!("there is no source called {id}")))?;
    Ok(source.endpoint.map(|value| value.to_string()))
}

/// One market operation at a time, shared with the Node provisioning gate.
fn claim_market(state: &AppState) -> Result<MarketGuard<'_>> {
    if state.installing.load(Ordering::SeqCst) || state.provisioning.load(Ordering::SeqCst) {
        return Err(Error::Plugin(
            "an install or a Node download is already running".into(),
        ));
    }
    if state.market_busy.swap(true, Ordering::SeqCst) {
        return Err(Error::Plugin("a market operation is already running".into()));
    }
    Ok(MarketGuard(&state.market_busy))
}

struct MarketGuard<'a>(&'a std::sync::atomic::AtomicBool);

impl Drop for MarketGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// The patch that turns the user's switches into the composition: one entry
/// per disabled plugin, `disabled: true`, applied at every launch. Written
/// when a switch flips and read by the launch plan.
fn write_disabled_patch() -> Result<()> {
    let profile = hd_core::projects::active_profile().unwrap_or_else(hd_core::profiles::selected);
    let disabled = pkg::switches::switched_off(&profile);
    let mut body = String::from("# Composed by HarnessLite from the plugin switches.\n");
    for name in disabled {
        body.push_str(&format!("- id: '{name}'\n  name: '{name}'\n  disabled: true\n"));
    }
    let path = disabled_patch_path();
    hd_core::atomic::write(&path, body.as_bytes()).map_err(|cause| {
        Error::Plugin(format!("{} could not be written: {cause}", path.display()))
    })
}

/// The runtime patch that carries the user's switches, when any is recorded.
pub fn disabled_patch() -> Option<PathBuf> {
    let path = disabled_patch_path();
    path.is_file().then_some(path)
}

fn disabled_patch_path() -> PathBuf {
    hd_core::paths::app_data_dir().join("disabled.patch.yml")
}
