//! Which shell a new terminal opens.
//!
//! The rule is the same on every platform: run the shell the user already uses,
//! and only fall back to the one that is always installed. Nobody configures
//! this in a settings page — they configured it when they installed PowerShell 7
//! or set `$SHELL`, and a terminal that ignores that is a terminal they will not
//! use.

use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

/// Argv for a new terminal, resolved against `path` — the `PATH` the child will
/// itself be given, not this process's.
///
/// Those differ more often than they look like they should: the pty layer
/// rebuilds `PATH` from the registry on Windows, so a shell installed after this
/// application started is on the child's `PATH` and not on ours.
pub fn argv(path: &OsStr) -> Vec<OsString> {
    pick(path)
}

/// What the tab calls the shell: the program's file stem.
pub fn label(path: &OsStr) -> String {
    argv(path)
        .first()
        .map(|program| {
            PathBuf::from(program)
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_else(|| "shell".into())
        })
        .unwrap_or_else(|| "shell".into())
}

/// First of `names` that exists in `path`, searched in the order given.
///
/// Deliberately not a full `which`: no `PATHEXT` expansion, because every caller
/// here passes a name with its extension already on it, and guessing extensions
/// is how you end up launching `pwsh.bat` from somebody's project directory.
fn first_on_path(path: &OsStr, names: &[&str]) -> Option<PathBuf> {
    for name in names {
        for directory in std::env::split_paths(path) {
            let candidate = directory.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(windows)]
fn pick(path: &OsStr) -> Vec<OsString> {
    // PowerShell 7 first: installing it is a deliberate act, so having it means
    // wanting it. `powershell.exe` next, which is on every Windows there is.
    // `-NoLogo` because a copyright banner is not what the pane is for.
    if let Some(found) = first_on_path(path, &["pwsh.exe", "powershell.exe"]) {
        return vec![found.into_os_string(), "-NoLogo".into()];
    }

    // Only if neither PowerShell is reachable, which means something is wrong
    // with `PATH` rather than with the machine.
    vec![std::env::var_os("ComSpec").unwrap_or_else(|| "cmd.exe".into())]
}

#[cfg(not(windows))]
fn pick(path: &OsStr) -> Vec<OsString> {
    // `$SHELL` is the user's answer to this question, given to the system long
    // before this application existed. `PATH` is not consulted: the value is an
    // absolute path by definition, and honouring a relative one would let any
    // directory with a `sh` in it launch a shell.
    if let Some(shell) = std::env::var_os("SHELL") {
        let shell = PathBuf::from(&shell);
        if shell.is_absolute() && shell.is_file() {
            return vec![shell.into_os_string()];
        }
    }

    // The fallback ladder the POSIX world itself settles on.
    for candidate in ["/bin/bash", "/bin/sh"] {
        let shell = PathBuf::from(candidate);
        if shell.is_file() {
            return vec![shell.into_os_string()];
        }
    }
    vec!["/bin/sh".into()]
}

#[cfg(test)]
mod tests {
    use super::label;

    #[test]
    #[cfg(windows)]
    fn the_tab_names_the_shell_not_its_arguments() {
        use std::ffi::OsStr;

        // A directory that really contains a `pwsh.exe`, made for the probe:
        // the search must find what is actually on the PATH it is given, not
        // what this machine happens to have installed.
        let root = std::env::temp_dir().join(format!("harnesslite-shell-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("probe dir");
        std::fs::write(root.join("pwsh.exe"), b"not really a shell").expect("probe shell");

        let found = super::first_on_path(OsStr::new(&root), &["pwsh.exe", "powershell.exe"]);
        assert!(found.is_some(), "the probe shell was not found on its path");
        assert_eq!(label(OsStr::new(root.to_str().expect("ascii temp dir"))), "pwsh");

        // And nothing invented when the PATH has neither PowerShell on it.
        let empty = std::env::temp_dir().join(format!(
            "harnesslite-shell-empty-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&empty).expect("empty dir");
        let none = super::first_on_path(OsStr::new(&empty), &["pwsh.exe", "powershell.exe"]);
        assert!(none.is_none());

        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(empty);
    }

    #[test]
    #[cfg(not(windows))]
    fn the_tab_names_the_shell_not_its_arguments() {
        use std::ffi::OsStr;

        std::env::set_var("SHELL", "/bin/bash");
        assert_eq!(label(OsStr::new("/usr/bin:/bin")), "bash");
    }
}
