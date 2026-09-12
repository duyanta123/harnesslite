//! The plugin market's proxy preference.
//!
//! The market reaches the registry three different ways on every install: the
//! catalog and metadata calls this shell makes itself (reqwest), the isolated
//! preflight's npm resolution, and the harness CLI's forwarded pnpm, which does
//! the real download. A proxy setting only helps the user if all three go the
//! same way — a search that arrives through the proxy and an install that
//! ignores it is a bug report waiting to happen. So the setting is stored once
//! here, and every market path takes it as a parameter.
//!
//! Off by default. Off means today's behaviour: requests inherit whatever proxy
//! variables the desktop process was started with. Nothing here touches the
//! harness runtime install or the Node download — those have their own
//! documented proxy story and their own lockfile rules.

use serde::{Deserialize, Serialize};

use hd_core::error::{Error, Result};
use hd_core::paths;

/// Config file schema. Bumped when the stored shape changes meaning.
const SCHEMA: u8 = 1;

/// The proxy as the user configured it, before validation.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProxyStore {
    schema: u8,
    enabled: bool,
    /// Empty when disabled. `http://` or `https://`, optionally with userinfo
    /// for proxies that authenticate.
    #[serde(default)]
    url: String,
}

impl Default for ProxyStore {
    fn default() -> Self {
        Self {
            schema: SCHEMA,
            enabled: false,
            url: String::new(),
        }
    }
}

/// What the settings surface renders and what the market commands consume.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyState {
    pub enabled: bool,
    pub url: String,
    /// The proxy that market traffic would actually use right now: the stored
    /// URL when enabled and valid, otherwise none.
    pub effective: Option<String>,
}

/// Read the stored preference. Missing, partial or unreadable files all mean
/// the same thing: off, and the market behaves exactly as it did before this
/// setting existed.
fn load() -> ProxyStore {
    std::fs::read(paths::market_proxy_file())
        .ok()
        .and_then(|body| serde_json::from_slice(&body).ok())
        .unwrap_or_default()
}

fn store(preferences: &ProxyStore) -> Result<()> {
    let path = paths::market_proxy_file();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|cause| {
            Error::Plugin(format!("{} could not be created: {}", parent.display(), cause))
        })?;
    }
    let body = serde_json::to_vec_pretty(preferences)
        .map_err(|cause| Error::Plugin(format!("the proxy preference could not be encoded: {cause}")))?;
    hd_core::atomic::write(&path, body).map_err(|cause| {
        Error::Plugin(format!("{} could not be written: {}", path.display(), cause))
    })
}

/// The proxy URL market traffic should use right now, or None to inherit.
///
/// The one function every market path starts from: a disabled or invalid
/// setting degrades to no proxy rather than failing an install the user can
/// still reach directly.
pub fn effective() -> Option<String> {
    effective_from(&paths::market_proxy_file())
}

fn effective_from(path: &std::path::Path) -> Option<String> {
    let stored: ProxyStore = std::fs::read(path)
        .ok()
        .and_then(|body| serde_json::from_slice(&body).ok())
        .unwrap_or_default();
    if !stored.enabled {
        return None;
    }
    validate(&stored.url).ok().map(|_| stored.url)
}

/// Whether `url` is a proxy URL this shell will actually apply.
///
/// The registry calls are plain HTTPS over HTTP CONNECT, so the scheme must be
/// `http` or `https` (a local proxy client's TLS-wrapped listener counts), a
/// host is required, and the whole thing must be free of whitespace and control
/// characters — a mangled URL must be refused at the settings surface, not
/// discovered as a confusing install failure later.
pub fn validate(url: &str) -> Result<()> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err(Error::Plugin("the proxy address is empty".into()));
    }
    if trimmed.chars().any(|character| character.is_whitespace() || character.is_control()) {
        return Err(Error::Plugin(
            "the proxy address contains whitespace or control characters".into(),
        ));
    }
    let parsed = url::Url::parse(trimmed).map_err(|cause| {
        // The common typo — `127.0.0.1:7890` with no scheme — arrives as a
        // "relative URL without a base" parse failure. Name the fix, not the
        // parser's disappointment.
        if cause == url::ParseError::RelativeUrlWithoutBase {
            Error::Plugin(
                "the proxy address needs a scheme, e.g. http://127.0.0.1:7890".into(),
            )
        } else {
            Error::Plugin(format!("the proxy address is not a valid URL: {cause}"))
        }
    })?;
    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(Error::Plugin(format!(
            "the proxy scheme must be http or https, not {scheme}"
        )));
    }
    if parsed.host_str().unwrap_or_default().is_empty() {
        return Err(Error::Plugin("the proxy address names no host".into()));
    }
    Ok(())
}

fn state_of() -> ProxyState {
    let stored = load();
    let effective = if stored.enabled {
        validate(&stored.url).ok().map(|_| stored.url.clone())
    } else {
        None
    };
    ProxyState {
        enabled: stored.enabled,
        url: stored.url,
        effective,
    }
}

/// A change is only kept when it describes a usable proxy: enabling with a bad
/// address must be refused at the moment the user is looking at the field,
/// not the first time the market later fails through it.
fn after(enabled: bool, url: String) -> Result<ProxyState> {
    let trimmed = url.trim().to_string();
    if enabled {
        validate(&trimmed)?;
    }
    let preferences = ProxyStore {
        schema: SCHEMA,
        enabled,
        url: trimmed,
    };
    store(&preferences)?;
    Ok(state_of())
}

#[tauri::command]
pub fn plugin_proxy_state() -> ProxyState {
    state_of()
}

#[tauri::command]
pub fn plugin_proxy_set(enabled: bool, url: String) -> Result<ProxyState> {
    after(enabled, url)
}

#[cfg(test)]
mod tests {
    use super::{effective_from, validate};

    #[test]
    fn plain_local_http_proxies_are_accepted() {
        assert!(validate("http://127.0.0.1:7890").is_ok());
        assert!(validate("http://192.168.1.10:10809").is_ok());
        assert!(validate("https://proxy.example.com:8443").is_ok());
        // Userinfo for a proxy that authenticates: allowed, never logged.
        assert!(validate("http://user:pass@proxy.example.com:8080").is_ok());
        // Surrounding whitespace is the clipboard's gift, not an error.
        assert!(validate("  http://127.0.0.1:7890  ").is_ok());
    }

    #[test]
    fn unusable_addresses_are_refused_with_a_readable_reason() {
        for (address, fragment) in [
            ("", "empty"),
            ("   ", "empty"),
            ("127.0.0.1:7890", "scheme"), // a proxy with no scheme is the common typo
            ("ftp://127.0.0.1:7890", "scheme"),
            ("http://", "host"),
            ("http://127.0.0.1 :7890", "whitespace"),
            ("http://127.0.0.1\t:7890", "whitespace"),
            ("not a proxy at all", "whitespace"),
        ] {
            let failure = validate(address).expect_err(address);
            assert!(failure.to_string().contains(fragment), "{address}: {failure}");
        }
    }

    #[test]
    fn effective_follows_the_stored_preference() {
        let path = std::env::temp_dir().join(format!(
            "harnesslite-proxy-test-{}.json",
            std::process::id()
        ));
        let write = |enabled: bool, url: &str| {
            let body = serde_json::json!({ "schema": 1, "enabled": enabled, "url": url });
            std::fs::write(&path, serde_json::to_vec(&body).expect("encoded")).expect("store");
        };

        let _ = std::fs::remove_file(&path);
        assert_eq!(effective_from(&path), None, "no file means off — the default");

        write(true, "http://127.0.0.1:7890");
        assert_eq!(
            effective_from(&path).as_deref(),
            Some("http://127.0.0.1:7890"),
            "enabled and valid is passed through"
        );

        write(true, "not a proxy");
        assert_eq!(
            effective_from(&path),
            None,
            "enabled but unusable degrades to inherit"
        );

        write(false, "http://127.0.0.1:7890");
        assert_eq!(effective_from(&path), None, "a stored URL means nothing while off");

        let _ = std::fs::remove_file(&path);
    }
}
