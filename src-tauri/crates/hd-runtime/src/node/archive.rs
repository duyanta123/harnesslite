//! Unpack an official Node archive.
//!
//! Two formats, because Node ships two: a zip on Windows and a gzipped tar
//! everywhere else. Both wrap the whole release in one directory named after it,
//! so neither branch strips anything — the archive is expanded into a staging
//! directory as-is and the caller is handed the release directory that appeared
//! inside. That leaves the last step of an install as a rename of a complete
//! tree, which is as close to atomic as this gets, and it keeps the extraction
//! itself on the path each crate tests: `..` entries, symlinks, hard links and
//! file modes are all handled by code whose job that is, not by a loop here.

use std::path::{Path, PathBuf};

use hd_core::error::{Error, Result};

use super::provision_error;

/// Expand `archive` into `staging` and return the release directory inside it.
pub fn unpack(archive: &Path, staging: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(staging)
        .map_err(|cause| provision_error(format!("nowhere to unpack the download: {cause}")))?;
    expand(archive, staging)?;
    sole_directory(staging)
}

#[cfg(windows)]
fn expand(archive: &Path, staging: &Path) -> Result<()> {
    let file = std::fs::File::open(archive).map_err(unreadable)?;
    let mut zip = zip::ZipArchive::new(std::io::BufReader::new(file))
        .map_err(|cause| provision_error(format!("the download is not a readable zip: {cause}")))?;

    zip.extract(staging)
        .map_err(|cause| provision_error(format!("the download could not be unpacked: {cause}")))
}

#[cfg(not(windows))]
fn expand(archive: &Path, staging: &Path) -> Result<()> {
    let file = std::fs::File::open(archive).map_err(unreadable)?;
    let decoded = flate2::read::GzDecoder::new(std::io::BufReader::new(file));

    // Node's tarballs contain symlinks — `bin/npm` points into
    // `lib/node_modules` — and the runtime is unusable without them, so this
    // must be `tar`'s own unpacker rather than a copy loop.
    tar::Archive::new(decoded)
        .unpack(staging)
        .map_err(|cause| provision_error(format!("the download could not be unpacked: {cause}")))
}

fn unreadable(cause: std::io::Error) -> Error {
    provision_error(format!("the download could not be read back: {cause}"))
}

/// The one directory inside `staging`, or a refusal.
///
/// An archive that expanded to nothing, or to a shape with no single root, is
/// not the release this code knows how to install — and guessing which of two
/// directories was meant would install half a runtime that fails later, further
/// from the cause.
fn sole_directory(staging: &Path) -> Result<PathBuf> {
    let mut directories = std::fs::read_dir(staging)
        .map_err(|cause| {
            provision_error(format!("the unpacked download could not be read: {cause}"))
        })?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir());

    match (directories.next(), directories.next()) {
        (Some(release), None) => Ok(release),
        _ => Err(provision_error(
            "the download did not unpack to a single Node release directory",
        )),
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::unpack;

    /// A per-test sandbox that outlives naming collisions between tests in one
    /// process, cleaned up by the caller.
    fn sandbox(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "harnesslite-node-archive-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("sandbox");
        root
    }

    /// A synthetic Node archive: one release directory per given root, each
    /// holding a single member that claims to be the runtime.
    #[cfg(windows)]
    fn write_release_zip(archive: &Path, roots: &[&str]) {
        use std::io::Write;

        use zip::write::SimpleFileOptions;

        let file = std::fs::File::create(archive).expect("archive file");
        let mut zip = zip::ZipWriter::new(file);
        for root in roots {
            zip.add_directory(*root, SimpleFileOptions::default())
                .expect("release directory");
            zip.start_file(format!("{root}/node.exe"), SimpleFileOptions::default())
                .expect("member");
            zip.write_all(b"not really a runtime").expect("member bytes");
        }
        zip.finish().expect("a finished zip");
    }

    #[cfg(not(windows))]
    fn write_release_tarball(archive: &Path, roots: &[&str]) {
        let file = std::fs::File::create(archive).expect("archive file");
        let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut tar = tar::Builder::new(encoder);
        for root in roots {
            let mut header = tar::Header::new_gnu();
            header.set_size(b"not really a runtime".len() as u64);
            header.set_entry_type(tar::EntryType::Regular);
            header.set_cksum();
            tar.append_data(&mut header, format!("{root}/node.exe"), b"not really a runtime".as_slice())
                .expect("member");
        }
        tar.into_inner()
            .expect("a finished tar")
            .finish()
            .expect("a finished gzip stream");
    }

    #[test]
    fn unpacks_to_the_single_release_directory_inside() {
        let root = sandbox("one-release");
        let staging = root.join("staging");
        let archive = root.join("node-v24.19.0.zip");
        write_archive(&archive, &["node-v24.19.0"]);

        let release = unpack(&archive, &staging).expect("a single release directory");
        assert_eq!(release, staging.join("node-v24.19.0"));
        assert!(release.join("node.exe").is_file());

        std::fs::remove_dir_all(&root).expect("removing the sandbox this test made");
    }

    #[test]
    fn refuses_an_archive_without_a_single_root() {
        let root = sandbox("two-roots");
        let staging = root.join("staging");
        let archive = root.join("broken.zip");
        write_archive(&archive, &["node-v24.19.0", "something-else"]);

        let failure = unpack(&archive, &staging).expect_err("two roots are not a release");
        assert!(
            failure.to_string().contains("single Node release directory"),
            "the refusal should say what was wrong: {failure}"
        );

        std::fs::remove_dir_all(&root).expect("removing the sandbox this test made");
    }

    #[cfg(windows)]
    fn write_archive(archive: &Path, roots: &[&str]) {
        write_release_zip(archive, roots);
    }

    #[cfg(not(windows))]
    fn write_archive(archive: &Path, roots: &[&str]) {
        write_release_tarball(archive, roots);
    }
}
