//! The harness runtime layer: keeping one `dsh web` process alive and
//! observable.
//!
//! Everything here owns process lifecycle — install transactions, bounded
//! startup, streamed output, crash detection, backoff restart, process-tree
//! reclamation. Pure data logic stays in `hd-core`; anything that spawns or
//! listens lives here.

pub mod health;
pub mod install;
pub mod logging;
pub mod readiness;
pub mod supervisor;

/// Version of the managed runtime this shell installs and supervises.
pub const VERSION: &str = hd_core::contract::DSH_VERSION;

/// How long the upstream check may take. The caller bounds it again; a
/// registry on the far side of a slow proxy is the case to survive.
const UPSTREAM_CEILING: std::time::Duration = std::time::Duration::from_secs(15);

/// Ask the registry which release `latest` names for the managed package.
///
/// This is the "is there a newer Harness upstream?" question, answered once
/// per launch and never allowed to block startup: the caller treats every
/// failure as "unknown", because a shell that cannot see the registry still
/// works with the release it has.
pub async fn upstream_latest(client: &reqwest::Client) -> hd_core::error::Result<String> {
    let url = format!("https://registry.npmjs.org/{}/latest", install::PACKAGE);
    let body = client
        .get(&url)
        .timeout(UPSTREAM_CEILING)
        .send()
        .await
        .and_then(|response| response.error_for_status())
        .map_err(|cause| {
            hd_core::error::Error::Harness(format!("{url} could not be reached: {cause}"))
        })?
        .text()
        .await
        .map_err(|cause| {
            hd_core::error::Error::Harness(format!("{url} sent an unreadable reply: {cause}"))
        })?;
    let value: serde_json::Value = serde_json::from_str(&body).map_err(|cause| {
        hd_core::error::Error::Harness(format!("{url} sent a malformed manifest: {cause}"))
    })?;
    value
        .get("version")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .filter(|version| !version.is_empty())
        .ok_or_else(|| hd_core::error::Error::Harness(format!("{url} carries no version")))
}
