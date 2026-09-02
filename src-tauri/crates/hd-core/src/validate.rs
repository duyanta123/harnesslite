//! Admit a project directory only when its filesystem can preserve the
//! semantics the harness and its tools rely on. Network and removable
//! filesystems are not a safe place for atomic package writes, links or
//! process-owned lock files.

use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Admission {
    pub state: &'static str,
    pub filesystem: Option<String>,
    pub reason: Option<String>,
}

impl Admission {
    pub fn blocked(&self) -> bool {
        self.state == "blocked"
    }

    pub fn safe(path: &Path, filesystem: &str) -> Self {
        let _ = path;
        Self {
            state: "safe",
            filesystem: Some(filesystem.to_string()),
            reason: None,
        }
    }

    fn warning(filesystem: Option<&str>, reason: impl Into<String>) -> Self {
        Self {
            state: "warning",
            filesystem: filesystem.map(str::to_string),
            reason: Some(reason.into()),
        }
    }

    fn blocked(reason: impl Into<String>) -> Self {
        Self {
            state: "blocked",
            filesystem: None,
            reason: Some(reason.into()),
        }
    }
}

/// Validate a candidate directory without touching it.
pub fn inspect(path: &Path) -> Admission {
    if !path.is_dir() {
        return Admission::blocked("the workspace directory does not exist or is not a directory");
    }
    platform(path)
}

#[cfg(not(windows))]
fn platform(_path: &Path) -> Admission {
    Admission {
        state: "safe",
        filesystem: None,
        reason: None,
    }
}

#[cfg(windows)]
fn platform(path: &Path) -> Admission {
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;

    use windows_sys::Win32::Storage::FileSystem::{
        GetDriveTypeW, GetVolumeInformationW, GetVolumePathNameW,
    };
    use windows_sys::Win32::System::WindowsProgramming::{
        DRIVE_FIXED, DRIVE_REMOTE, DRIVE_REMOVABLE,
    };

    let input: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut root = vec![0_u16; 32_768];
    // SAFETY: both buffers are NUL-terminated/writable and the length matches
    // the allocation. The APIs write no more than the supplied capacities.
    if unsafe { GetVolumePathNameW(input.as_ptr(), root.as_mut_ptr(), root.len() as u32) } == 0 {
        return Admission::warning(None, "Windows could not identify the workspace volume");
    }

    let drive = unsafe { GetDriveTypeW(root.as_ptr()) };
    if drive == DRIVE_REMOTE || drive == DRIVE_REMOVABLE {
        return Admission::blocked(if drive == DRIVE_REMOTE {
            "network workspaces are blocked because package and lock-file writes are not reliable"
        } else {
            "removable workspaces are blocked because the volume can disappear during a session"
        });
    }
    if drive != DRIVE_FIXED {
        return Admission::warning(None, "the workspace is not on a fixed local drive");
    }

    let mut name = vec![0_u16; 64];
    if unsafe {
        GetVolumeInformationW(
            root.as_ptr(),
            ptr::null_mut(),
            0,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            name.as_mut_ptr(),
            name.len() as u32,
        )
    } == 0
    {
        return Admission::warning(None, "Windows could not identify the workspace filesystem");
    }
    let end = name
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(name.len());
    classify(&String::from_utf16_lossy(&name[..end]))
}

#[cfg(windows)]
fn classify(filesystem: &str) -> Admission {
    let normalized = filesystem.to_ascii_uppercase();
    if matches!(normalized.as_str(), "NTFS" | "REFS") {
        Admission::safe(Path::new(""), filesystem)
    } else if matches!(normalized.as_str(), "FAT" | "FAT32" | "EXFAT") {
        Admission::blocked(format!(
            "{filesystem} cannot provide the links and atomic writes required by the workspace"
        ))
    } else {
        Admission::warning(
            Some(filesystem),
            format!("{filesystem} has not been qualified for agent workspaces"),
        )
    }
}

/// Canonicalize a candidate project path and apply the admission rules.
pub fn inspect_canonical(path: &std::path::PathBuf) -> Result<std::path::PathBuf, String> {
    let canonical = path.canonicalize().map_err(|cause| {
        format!("{} could not be opened: {cause}", path.display())
    })?;
    let canonical = node_runtime::plain_path(canonical);
    let admission = inspect(&canonical);
    if admission.blocked() {
        return Err(admission.reason.unwrap_or_else(|| {
            "the selected directory is not a safe workspace".into()
        }));
    }
    Ok(canonical)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_directory_is_blocked() {
        let path = std::env::temp_dir().join("harnesslite-admission-that-must-not-exist");
        assert!(inspect(&path).blocked());
    }

    #[cfg(windows)]
    #[test]
    fn windows_filesystem_contract_is_explicit() {
        assert_eq!(classify("NTFS").state, "safe");
        assert_eq!(classify("ReFS").state, "safe");
        assert_eq!(classify("exFAT").state, "blocked");
        assert_eq!(classify("mysteryfs").state, "warning");
    }
}
