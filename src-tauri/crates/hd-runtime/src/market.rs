//! The plugin market's process and network half.
//!
//! hd-core owns the contracts: what a catalog entry is, which sources exist,
//! what the npm response must look like. This module is the hands — fetching
//! catalog bodies within a budget, reading one package's metadata out of the
//! registry, running the isolated preflight, and driving the harness's own
//! `plugin` commands for the installs the user confirms.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::Serialize;

use hd_core::contract;
use hd_core::error::{Error, Result};
use hd_core::plugins as pkg;
use reqwest::Client;

use crate::harness::install;

/// How long a catalog or registry response may take overall.
///
/// npm metadata for one package is bounded a little tighter.
const CATALOG_CEILING: Duration = Duration::from_secs(30);
const DETAIL_CEILING: Duration = Duration::from_secs(20);

/// A TCP connection that has not opened within this budget is not going to,
/// proxy or not.
const CONNECT_CEILING: Duration = Duration::from_secs(15);

/// Preflight and install both walk real dependency trees; a wedged registry
/// must fail the dialog rather than hold it open forever.
const OPERATION_TOTAL: Duration = Duration::from_secs(8 * 60);

/// The user-agent every market request carries.
fn user_agent() -> String {
    format!("harnesslite-market/{}", hd_core::VERSION)
}

/// The HTTPS client the market's registry and catalog calls go through.
///
/// `proxy` is the user's market proxy setting, `Some("http://host:port")` when
/// they configured one. An explicit proxy overrides whatever proxy variables
/// the desktop process inherited; `None` keeps the default behaviour, which is
/// reqwest's own environment probing.
pub fn client(proxy: Option<&str>) -> Result<Client> {
    let mut builder = Client::builder()
        .user_agent(user_agent())
        .connect_timeout(CONNECT_CEILING);
    if let Some(proxy) = proxy {
        let proxy = reqwest::Proxy::all(proxy)
            .map_err(|cause| Error::Plugin(format!("the proxy could not be used: {cause}")))?;
        builder = builder.proxy(proxy);
    }
    builder.build().map_err(|cause| {
        Error::Plugin(format!("no HTTPS client could be built for the market: {cause}"))
    })
}

/// The environment variables that point npm and pnpm at the user's proxy.
///
/// Every subprocess the market spawns — the preflight's npm, the harness CLI's
/// pnpm — inherits the desktop process's environment, where a proxy may be
/// absent (GUI launch) or stale. Both spellings of each variable are set
/// because different tools read different cases, and loopback is exempted so a
/// proxy setting can never black-hole a call to the user's own machine.
pub fn proxy_env(proxy: Option<&str>) -> Vec<(&'static str, String)> {
    let Some(proxy) = proxy else {
        return Vec::new();
    };
    vec![
        ("HTTP_PROXY", proxy.to_string()),
        ("HTTPS_PROXY", proxy.to_string()),
        ("http_proxy", proxy.to_string()),
        ("https_proxy", proxy.to_string()),
        ("NO_PROXY", "localhost,127.0.0.1,::1".to_string()),
        ("no_proxy", "localhost,127.0.0.1,::1".to_string()),
    ]
}

/// A conformance report for one source, the way the sources dialog shows it.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Health {
    pub source_id: String,
    pub contract: String,
    /// Unix seconds when this report was produced.
    pub checked_at: u64,
    pub items: usize,
    pub installable: usize,
    pub latency_ms: u64,
    pub warnings: Vec<String>,
}

/// Fetch one catalog and parse it into normalized entries.
///
/// The query only means something to the npm source (it is a search endpoint);
/// the community catalogs are whole documents fetched as-is and filtered
/// locally by [`pkg::catalog::search`].
pub async fn fetch_catalog(
    client: &Client,
    source_id: &str,
    endpoint: Option<&str>,
    query: &str,
) -> Result<Vec<pkg::catalog::CatalogEntry>> {
    let source = pkg::sources::SourceId::parse(source_id)?;
    let url = match (&source, endpoint) {
        (pkg::sources::SourceId::Npm, _) => {
            let text = if query.trim().is_empty() {
                // Browsing rather than searching: the registry needs a text,
                // and the ecosystem's own keyword is the honest "everything".
                "keywords:dsh-plugin".to_string()
            } else {
                query.trim().to_string()
            };
            format!(
                "https://registry.npmjs.org/-/v1/search?text={}&size=250",
                urlencode(&text)
            )
        }
        (_, Some(endpoint)) => endpoint.to_string(),
        _ => {
            return Err(Error::Plugin(format!(
                "source {source_id} has no endpoint to fetch"
            )))
        }
    };

    let response = client
        .get(&url)
        .timeout(CATALOG_CEILING)
        .header("user-agent", user_agent())
        .send()
        .await
        .map_err(|cause| Error::Plugin(format!("{url} could not be reached: {cause}")))?;
    if !response.status().is_success() {
        return Err(Error::Plugin(format!("{url} answered {}", response.status())));
    }
    let body = response
        .text()
        .await
        .map_err(|cause| Error::Plugin(format!("{url} sent an unreadable reply: {cause}")))?;

    pkg::catalog::parse(&source, &body)
}

/// One exact registry answer for a package name, or None when the registry
/// knows no such package.
///
/// The search endpoint ranks by popularity, and a fresh package loses to every
/// established one even when the query is its full name; a query shaped like
/// one exact name therefore also gets this direct lookup, to pin to the top of
/// whatever the relevance search brings back.
pub async fn exact_entry(
    client: &Client,
    name: &str,
) -> Result<Option<pkg::catalog::CatalogEntry>> {
    let url = format!("https://registry.npmjs.org/{}/latest", urlencode(name));
    let response = client
        .get(&url)
        .timeout(DETAIL_CEILING)
        .header("user-agent", user_agent())
        .send()
        .await
        .map_err(|cause| Error::Plugin(format!("{url} could not be reached: {cause}")))?;
    if response.status().as_u16() == 404 {
        return Ok(None);
    }
    if !response.status().is_success() {
        return Err(Error::Plugin(format!("{url} answered {}", response.status())));
    }
    let body = response
        .text()
        .await
        .map_err(|cause| Error::Plugin(format!("{url} sent an unreadable reply: {cause}")))?;
    pkg::catalog::npm_manifest_entry(name, &body).map(Some)
}

/// Probe one source and report what came back, within the dialog's patience.
pub async fn health(
    client: &Client,
    source_id: &str,
    endpoint: Option<&str>,
) -> Result<Health> {
    let started = Instant::now();
    let entries = fetch_catalog(client, source_id, endpoint, "").await?;
    let installable = entries.iter().filter(|entry| entry.npm_spec.is_some()).count();
    Ok(Health {
        source_id: source_id.to_string(),
        contract: match pkg::sources::SourceId::parse(source_id) {
            Ok(pkg::sources::SourceId::Npm) => "npm",
            Ok(pkg::sources::SourceId::AwesomeDsh) => "snapshot-http",
            Ok(pkg::sources::SourceId::Reviewed1024) => "reviewed-http",
            _ => "standard-http-v1",
        }
        .into(),
        checked_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        items: entries.len(),
        installable,
        latency_ms: started.elapsed().as_millis() as u64,
        warnings: Vec::new(),
    })
}

/// One package, as the registry describes it, shaped for the detail dialog.
pub async fn detail(
    client: &Client,
    name: &str,
    version: &str,
) -> Result<pkg::detail::PackageDetail> {
    let url = format!("https://registry.npmjs.org/{}", urlencode(name));
    let response = client
        .get(&url)
        .timeout(DETAIL_CEILING)
        .header("user-agent", user_agent())
        .send()
        .await
        .map_err(|cause| Error::Plugin(format!("{url} could not be reached: {cause}")))?;
    if !response.status().is_success() {
        return Err(Error::Plugin(format!(
            "the registry does not know {name} ({})",
            response.status()
        )));
    }
    let body = response
        .text()
        .await
        .map_err(|cause| Error::Plugin(format!("the registry sent an unreadable reply: {cause}")))?;

    pkg::detail::parse(&body, name, version)
}

/// Resolve `spec` in an isolated scratch project, without installing it.
///
/// `npm install --dry-run` walks the whole dependency tree against the
/// registry — every name, every range — while writing nothing but the log.
/// A spec that cannot resolve is refused here, before any confirmation dialog
/// offers it as installable; the harness's own installer stays the only thing
/// that ever writes into a profile.
pub async fn preflight<F>(
    node: &Path,
    npm: &Path,
    spec: &str,
    scratch: &Path,
    proxy: Option<&str>,
    mut report: F,
) -> Result<pkg::preflight::Resolution>
where
    F: FnMut(&'static str, String),
{
    let _ = std::fs::remove_dir_all(scratch);
    std::fs::create_dir_all(scratch).map_err(|cause| {
        Error::Plugin(format!("{} could not be created: {cause}", scratch.display()))
    })?;
    std::fs::write(
        scratch.join("package.json"),
        r#"{ "name": "harnesslite-preflight", "private": true }"#,
    )
    .map_err(|cause| Error::Plugin(format!("the preflight project could not be staged: {cause}")))?;

    let mut command = tokio::process::Command::new(node);
    command
        .arg(npm)
        .args([
            "install",
            "--dry-run",
            "--ignore-scripts",
            "--no-audit",
            "--no-fund",
            "--json",
            spec,
        ])
        .current_dir(scratch)
        // The resolution is the user's own network path: with a proxy set it
        // must reach the registry the same way the real install will.
        .envs(proxy_env(proxy))
        .env("PATH", install::path_with_node(node))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        command.creation_flags(0x0800_0000);
    }

    report("stdout", format!("resolving {spec} against the registry"));
    let started = Instant::now();

    let output = tokio::time::timeout(
        OPERATION_TOTAL,
        async {
            let child = command.spawn().map_err(|cause| {
                Error::Plugin(format!("the preflight could not start: {cause}"))
            })?;
            child.wait_with_output().await.map_err(|cause| {
                Error::Plugin(format!("the preflight broke off: {cause}"))
            })
        },
    )
    .await
    .map_err(|_| {
        Error::Plugin(format!(
            "the preflight did not finish within {}s",
            OPERATION_TOTAL.as_secs()
        ))
    })??;

    let _ = started;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    if !output.status.success() {
        let detail = if stderr.trim().is_empty() {
            stdout
        } else {
            stderr
        };
        let detail = detail.trim().chars().take(2_000).collect::<String>();
        return Err(Error::Plugin(format!(
            "{spec} cannot be installed: {detail}"
        )));
    }

    pkg::preflight::parse_resolution(&stdout, spec)
}

/// The harness CLI command for one plugin operation.
///
/// The exact form is the one the installed CLI parses, and it is strict: the
/// `plugin` subcommand refuses every parent-level option ("plugin takes none
/// of parent --profile, --patch, …"), and re-declares `--profile` as its own
/// required option, so both `dsh --profile P plugin …` and
/// `dsh --patch X plugin --profile P …` die in argument parsing before pnpm is
/// ever started. Everything must ride *after* `plugin`.
///
/// The launcher's `--patch` overlays are therefore absent on purpose — they
/// are boot-time composition, applied by the supervisor when the harness
/// starts, and the CLI forbids them here anyway.
fn plugin_command(
    node: &Path,
    entry: &Path,
    profile: &str,
    operation: &str,
    spec: &str,
    proxy: Option<&str>,
) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(node);
    command
        .arg(entry)
        .arg("plugin")
        .arg("--profile")
        .arg(profile)
        .arg(operation)
        .arg(spec)
        .env("DSH_DESKTOP", "1")
        .env(contract::ENV_PROFILE, profile)
        .env(contract::ENV_DSH_HOME, hd_core::paths::dsh_home())
        .env(
            contract::ENV_PROFILE_DIR,
            hd_core::paths::profile_dir(profile),
        )
        // The harness CLI forwards the install to `pnpm`, which it finds on
        // PATH. The selected runtime's directory goes in front, so a bare
        // `pnpm` resolves inside the same Node family the shell chose instead
        // of whatever a GUI-launched process inherited — or nothing at all.
        .env("PATH", install::path_with_node(node))
        .envs(proxy_env(proxy))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        command.creation_flags(0x0800_0000);
    }
    command
}

/// Run one `plugin` operation of the harness's own CLI, streaming its output.
///
/// The harness installs, removes and re-composes the profile itself; the shell
/// drives and watches. `report` receives every line, tagged by stream, which
/// is how the market panel keeps its progress honest.
pub async fn run_harness_plugin<F>(
    node: &Path,
    entry: &Path,
    profile: &str,
    operation: &str,
    spec: &str,
    proxy: Option<&str>,
    mut report: F,
) -> Result<()>
where
    F: FnMut(&'static str, String) + Send,
{
    let mut child = plugin_command(node, entry, profile, operation, spec, proxy)
        .spawn()
        .map_err(|cause| {
            Error::Plugin(format!("the harness plugin command could not start: {cause}"))
        })?;

    // Stream both pipes line by line while the child runs; the readers are
    // tasks, but they own the pipes and die with them.
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let out_task = tokio::spawn(stream_into(stdout, "stdout"));
    let err_task = tokio::spawn(stream_into(stderr, "stderr"));

    let status = tokio::time::timeout(OPERATION_TOTAL, child.wait())
        .await
        .map_err(|_| {
            Error::Plugin(format!(
                "the plugin {operation} did not finish within {}s",
                OPERATION_TOTAL.as_secs()
            ))
        })?
        .map_err(|cause| Error::Plugin(format!("the plugin {operation} broke off: {cause}")))?;

    for (stream, line) in out_task.await.unwrap_or_default() {
        report(stream, line);
    }
    for (stream, line) in err_task.await.unwrap_or_default() {
        report(stream, line);
    }

    if !status.success() {
        return Err(Error::Plugin(format!(
            "the harness refused the plugin {operation} (exit {status})"
        )));
    }
    Ok(())
}

/// Collect one pipe's lines until the child closes it.
async fn stream_into(
    pipe: impl tokio::io::AsyncRead + Unpin,
    stream: &'static str,
) -> Vec<(&'static str, String)> {
    use tokio::io::{AsyncBufReadExt, BufReader};
    let mut collected = Vec::new();
    let mut lines = BufReader::new(pipe).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        collected.push((stream, line));
    }
    collected
}

/// Percent-encode the one thing URLs here are built from: a query or a package
/// name. Package names are already restricted; this is for the spaces a
/// searcher will type.
fn urlencode(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

/// Where the preflight scratch project lives.
pub fn preflight_dir() -> PathBuf {
    hd_core::paths::app_data_dir().join("plugin-preflight")
}

/// A local archive's own manifest, read out of the tarball.
pub struct ArchiveManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    /// Whether it declares a profile patch — a plugin, not just a package.
    pub bundle: bool,
}

/// Read `package/package.json` out of an npm tarball.
///
/// npm ships every package as a gzipped tar whose members live under one
/// `package/` root — the same shape official Node archives use on Unix, read
/// by the same decoder. A tarball without that member is not a package this
/// shell can describe, and pretending otherwise would install a guess.
pub fn archive_manifest(archive: &Path) -> Result<ArchiveManifest> {
    let file = std::fs::File::open(archive)
        .map_err(|cause| Error::Plugin(format!("{} could not be opened: {cause}", archive.display())))?;
    let decoded = flate2::read::GzDecoder::new(std::io::BufReader::new(file));
    let mut tar = tar::Archive::new(decoded);

    for entry in tar
        .entries()
        .map_err(|cause| Error::Plugin(format!("{} is not a readable tarball: {cause}", archive.display())))?
    {
        let mut entry = entry
            .map_err(|cause| Error::Plugin(format!("{} is not a readable tarball: {cause}", archive.display())))?;
        let path = entry
            .path()
            .map_err(|cause| Error::Plugin(format!("{} is not a readable tarball: {cause}", archive.display())))?
            .into_owned();
        if path.file_name().and_then(|name| name.to_str()) != Some("package.json") {
            continue;
        }
        if path.parent().and_then(|parent| parent.file_name()) != Some(std::ffi::OsStr::new("package")) {
            continue;
        }
        let mut body = String::new();
        entry
            .read_to_string(&mut body)
            .map_err(|cause| Error::Plugin(format!("{} could not be read: {cause}", path.display())))?;
        let value: serde_json::Value = serde_json::from_str(&body)
            .map_err(|cause| Error::Plugin(format!("the archive's manifest made no sense: {cause}")))?;

        let name = value
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        if name.is_empty() {
            return Err(Error::Plugin("the archive's manifest names no package".into()));
        }
        return Ok(ArchiveManifest {
            name,
            version: value
                .get("version")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string(),
            description: value
                .get("description")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string(),
            bundle: value.pointer("/dsh/profile/bundles").is_some(),
        });
    }

    Err(Error::Plugin(format!(
        "{} holds no package manifest, so it is not an installable archive",
        archive.display()
    )))
}

/// The npm CLI that belongs to the harness's own Node, or a refusal. The shell
/// never uses a global npm: the preflight must resolve with the same registry
/// configuration the profile install will run under.
pub fn npm_pair(node: &Path) -> Result<(PathBuf, PathBuf)> {
    let node = node.to_path_buf();
    let npm = install::npm_cli(&node)
        .ok_or_else(|| Error::Plugin("this Node install has no npm beside it".into()))?;
    Ok((node, npm))
}

#[cfg(test)]
mod tests {
    use super::{plugin_command, proxy_env, urlencode};
    use std::path::{Path, PathBuf};

    #[test]
    fn a_query_becomes_one_url_safe_word() {
        assert_eq!(urlencode("dsh"), "dsh");
        assert_eq!(urlencode("todo list"), "todo%20list");
        assert_eq!(urlencode("关键词"), "%E5%85%B3%E9%94%AE%E8%AF%8D");
    }

    /// The argument shape is load-bearing and strict: the `plugin` subcommand
    /// re-declares `--profile` as its own required option and refuses every
    /// parent-level option, so anything spelled before `plugin` — or any
    /// launcher `--patch` — kills the operation in argument parsing. This pins
    /// the one working shape the installed CLI actually parses.
    #[test]
    fn the_plugin_command_carries_the_profile_after_the_subcommand() {
        let command = plugin_command(
            Path::new("node"),
            &PathBuf::from("bin.js"),
            "web",
            "add",
            "dsh-demo@1.0.0",
            None,
        );
        let arguments = command
            .as_std()
            .get_args()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        let plugin = arguments
            .iter()
            .position(|value| value == "plugin")
            .expect("the subcommand is present");
        assert_eq!(plugin, 1, "nothing but the entry precedes the subcommand");
        let profile = arguments
            .iter()
            .position(|value| value == "--profile")
            .expect("the profile is present");
        assert!(plugin < profile, "--profile belongs to the subcommand");
        let operator = arguments
            .iter()
            .position(|value| value == "add")
            .expect("the operation is present");
        assert!(profile < operator, "options precede the forwarded arguments");
        assert!(
            !arguments.contains(&"--patch".to_string()),
            "launcher patches are boot-time and refused by the plugin subcommand"
        );
    }

    #[test]
    fn the_plugin_operation_runs_inside_the_selected_runtime() {
        let command = plugin_command(
            Path::new("/runtimes/v22/node"),
            &PathBuf::from("bin.js"),
            "web",
            "remove",
            "dsh-demo",
            None,
        );
        let path = command
            .as_std()
            .get_envs()
            .find(|(name, _)| *name == std::ffi::OsStr::new("PATH"))
            .map(|(_, value)| value.expect("PATH is not removed"))
            .expect("PATH is set")
            .to_string_lossy()
            .into_owned();
        assert!(
            path.starts_with("/runtimes/v22"),
            "the chosen runtime's directory goes in front: {path}"
        );
    }

    #[test]
    fn a_proxy_reaches_the_subprocesses_under_both_spellings() {
        let environment = proxy_env(Some("http://127.0.0.1:7890"));
        for variable in ["HTTP_PROXY", "HTTPS_PROXY", "http_proxy", "https_proxy"] {
            assert!(
                environment
                    .iter()
                    .any(|(name, value)| *name == variable && value == "http://127.0.0.1:7890"),
                "{variable} is set"
            );
        }
        // Loopback is exempt twice over: a proxy that black-holes the shell's
        // own machine would be worse than no proxy at all.
        for variable in ["NO_PROXY", "no_proxy"] {
            assert!(environment
                .iter()
                .any(|(name, value)| *name == variable && value.contains("127.0.0.1")));
        }
    }

    #[test]
    fn no_proxy_setting_touches_nothing() {
        assert!(proxy_env(None).is_empty());
    }
}
