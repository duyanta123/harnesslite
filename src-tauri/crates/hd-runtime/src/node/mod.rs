//! Getting a Node runtime onto this machine.
//!
//! Three steps with very different failure modes, each in its own file:
//! [`catalog`] decides *what* to download from the published release index,
//! [`http`] moves the bytes and answers for integrity, [`archive`] unpacks the
//! two archive shapes Node ships. This file is the transaction around them:
//! what is already installed, and the download → verify → unpack → promote
//! sequence that ends in one rename.

pub mod archive;
pub mod catalog;
pub mod http;

use std::path::{Path, PathBuf};

use hd_core::error::{Error, Result};

use super::node::catalog::Release;

/// The shared refusal shape for every step of provisioning.
pub fn provision_error(message: impl Into<String>) -> Error {
    Error::Node(message.into())
}

/// The executable inside an unpacked release directory.
///
/// Windows ships `node.exe` at the archive root; everywhere else it hides in
/// `bin/`, and that file is the symlink tar is careful to restore.
pub fn node_exe(release_dir: &Path) -> PathBuf {
    if cfg!(windows) {
        release_dir.join("node.exe")
    } else {
        release_dir.join("bin").join("node")
    }
}

/// Parse a release directory's name into a comparable version.
///
/// The managed store names directories the way every version manager does —
/// `v24.19.0`, which is also what the detector scans for — and an unpacked
/// archive's own root is `node-v24.19.0`; both spellings parse.
fn version_of(release_dir_name: &str) -> Option<node_runtime::Version> {
    node_runtime::Version::parse(release_dir_name.strip_prefix("node-").unwrap_or(release_dir_name))
}

/// The newest already-installed runtime this machine can actually run.
///
/// "Can run" is checked two ways: the executable has to exist (a half-removed
/// directory must not present itself as a runtime) and the version has to meet
/// the harness's minimum (an old install is a tombstone, not an option).
pub fn installed_newest(runtimes_dir: &Path) -> Option<PathBuf> {
    let mut best: Option<(node_runtime::Version, PathBuf)> = None;

    let entries = std::fs::read_dir(runtimes_dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(version) = version_of(path.file_name()?.to_str()?) else {
            continue;
        };
        if version < node_runtime::MINIMUM_SUPPORTED || !node_exe(&path).is_file() {
            continue;
        }
        if best.as_ref().is_none_or(|(kept, _)| version > *kept) {
            best = Some((version, path));
        }
    }

    best.map(|(_, path)| path)
}

/// Install the newest supported LTS release and return its directory.
///
/// The whole download and unpack happens inside `.staging`, so a crash, a full
/// disk or a killed process leaves `runtimes_dir` exactly as it was: every
/// visible step before the final rename is invisible. The rename itself is the
/// commit — a complete tree appears at once or not at all.
pub async fn install_newest_lts<P>(runtimes_dir: &Path, progress: P) -> Result<PathBuf>
where
    P: FnMut(u64, Option<u64>),
{
    let client = http::client()?;
    let release = catalog::newest_lts(&client).await?;
    install_release(runtimes_dir, &release, &client, progress).await
}

/// Install one resolved release. Split from [`install_newest_lts`] so the
/// transaction can be exercised against a prepared archive without a live
/// mirror in the loop.
pub async fn install_release<P>(
    runtimes_dir: &Path,
    release: &Release,
    client: &reqwest::Client,
    progress: P,
) -> Result<PathBuf>
where
    P: FnMut(u64, Option<u64>),
{
    let target = runtimes_dir.join(&release.version);
    if node_exe(&target).is_file() {
        return Ok(target);
    }

    let staging = runtimes_dir.join(".staging");
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(|cause| {
        provision_error(format!("the runtime staging area could not be created: {cause}"))
    })?;

    let archive_path = staging.join(&release.archive);
    let received_sha256 = http::download(client, &release.url, &archive_path, progress).await?;

    if received_sha256 != release.sha256 {
        let _ = std::fs::remove_file(&archive_path);
        return Err(provision_error(format!(
            "the downloaded archive does not match the published checksum (got {received_sha256}, expected {})",
            release.sha256
        )));
    }

    let unpacked = staging.join("unpack");
    let release_dir = archive::unpack(&archive_path, &unpacked)?;

    // Replacing an existing install (a re-provision over a broken tree) is a
    // two-step dance on Windows: a rename onto a live directory fails, so the
    // old one steps aside first. The outgoing tree is removed only after the
    // new one is in place, so a crash mid-swap still leaves one working runtime.
    if target.exists() {
        let outgoing = runtimes_dir.join(format!(".outgoing-{}", release.version));
        let _ = std::fs::remove_dir_all(&outgoing);
        std::fs::rename(&target, &outgoing)
            .map_err(|cause| provision_error(format!("the old runtime would not step aside: {cause}")))?;
        std::fs::rename(&release_dir, &target)
            .map_err(|cause| provision_error(format!("the new runtime could not be moved into place: {cause}")))?;
        let _ = std::fs::remove_dir_all(&outgoing);
    } else {
        std::fs::rename(&release_dir, &target)
            .map_err(|cause| provision_error(format!("the runtime could not be moved into place: {cause}")))?;
    }

    let _ = std::fs::remove_dir_all(&staging);

    if !node_exe(&target).is_file() {
        return Err(provision_error(
            "the installed runtime has no executable where one was expected",
        ));
    }
    Ok(target)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{installed_newest, node_exe, version_of};

    fn sandbox(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "harnesslite-node-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("sandbox");
        root
    }

    /// A plausible release directory: `node-v<version>/` with the executable
    /// this platform looks for inside.
    fn fake_release(runtimes: &Path, version: &str, with_exe: bool) -> PathBuf {
        let dir = runtimes.join(format!("node-{version}"));
        std::fs::create_dir_all(&dir).expect("release dir");
        let exe = node_exe(&dir);
        if with_exe {
            std::fs::create_dir_all(exe.parent().unwrap()).expect("exe dir");
            std::fs::write(&exe, b"not really a runtime").expect("exe");
        }
        dir
    }

    use std::path::Path;

    #[test]
    fn reads_versions_out_of_release_directory_names() {
        assert_eq!(
            version_of("node-v24.19.0").map(|v| v.to_string()),
            Some("24.19.0".into())
        );
        // The managed store's own convention, shared with the version managers
        // the detector scans: the `v` stays on.
        assert_eq!(
            version_of("v24.19.0").map(|v| v.to_string()),
            Some("24.19.0".into())
        );
        assert_eq!(version_of("node-junk"), None);
        assert_eq!(version_of(".staging"), None);
    }

    #[test]
    fn picks_the_newest_installation_that_can_actually_run() {
        let runtimes = sandbox("newest");
        let old = fake_release(&runtimes, "v22.19.0", true);
        let broken = fake_release(&runtimes, "v26.0.0", false);
        let new = fake_release(&runtimes, "v24.19.0", true);
        let _ = broken;

        assert_eq!(installed_newest(&runtimes).as_deref(), Some(new.as_path()));

        // Removing the newest executable makes the older install the answer
        // rather than leaving no answer at all.
        std::fs::remove_file(node_exe(&new)).expect("removing the exe");
        assert_eq!(installed_newest(&runtimes).as_deref(), Some(old.as_path()));

        std::fs::remove_dir_all(&runtimes).expect("cleanup");
    }

    #[test]
    fn an_empty_runtimes_directory_installs_nothing() {
        let runtimes = sandbox("empty");
        assert_eq!(installed_newest(&runtimes), None);
        std::fs::remove_dir_all(&runtimes).expect("cleanup");
    }

    #[test]
    fn an_install_below_the_harness_minimum_is_not_a_runtime() {
        let runtimes = sandbox("too-old");
        let _ = fake_release(&runtimes, "v20.0.0", true);
        assert_eq!(installed_newest(&runtimes), None);
        std::fs::remove_dir_all(&runtimes).expect("cleanup");
    }
}
