//! Disabled-plugin switches, recorded per profile.
//!
//! Disabling is a user decision that must survive the next install: the
//! harness's installer re-enables everything it touches, so the shell records
//! the user's choices here and re-asserts them after every package change.
//! One file, one object: profile name → the plugins the user switched off.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::error::{Error, Result};
use crate::paths;

type Switches = BTreeMap<String, Vec<String>>;

#[derive(Clone)]
pub struct Store {
    file: PathBuf,
}

impl Store {
    pub fn managed() -> Self {
        Self {
            file: paths::app_data_dir().join("plugins.json"),
        }
    }

    fn read(&self) -> Switches {
        std::fs::read(&self.file)
            .ok()
            .and_then(|body| serde_json::from_slice(&body).ok())
            .unwrap_or_default()
    }

    fn write(&self, switches: &Switches) -> Result<()> {
        if let Some(parent) = self.file.parent() {
            std::fs::create_dir_all(parent).map_err(|cause| {
                Error::Store(format!("{} could not be created: {cause}", parent.display()))
            })?;
        }
        let mut body = serde_json::to_vec_pretty(switches).map_err(|cause| {
            Error::Store(format!("plugin switches could not be encoded: {cause}"))
        })?;
        body.push(b'\n');
        crate::atomic::write(&self.file, body).map_err(|cause| {
            Error::Store(format!(
                "{} could not be committed: {cause}",
                self.file.display()
            ))
        })
    }

    fn clean(names: &[String]) -> Vec<String> {
        let mut names: Vec<String> = names
            .iter()
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty())
            .collect();
        names.sort();
        names.dedup();
        names
    }

    /// The plugins the user switched off in one profile, sorted.
    pub fn switched_off(&self, profile: &str) -> Vec<String> {
        self.read()
            .get(profile)
            .cloned()
            .map(|names| Self::clean(&names))
            .unwrap_or_default()
    }

    /// Record that a plugin is switched off in a profile.
    pub fn disable(&self, profile: &str, name: &str) -> Result<()> {
        let mut switches = self.read();
        let entry = switches.entry(profile.to_string()).or_default();
        if !entry.iter().any(|existing| existing == name) {
            entry.push(name.to_string());
        }
        self.write(&switches)
    }

    /// Record that a plugin is back on in a profile.
    pub fn enable(&self, profile: &str, name: &str) -> Result<()> {
        let mut switches = self.read();
        let Some(entry) = switches.get_mut(profile) else {
            return Ok(());
        };
        entry.retain(|existing| existing != name);
        if entry.is_empty() {
            switches.remove(profile);
        }
        self.write(&switches)
    }

    /// Move a profile's switches when the profile is renamed.
    pub fn rename(&self, from: &str, to: &str) -> Result<()> {
        let mut switches = self.read();
        let Some(entry) = switches.remove(from) else {
            return Ok(());
        };
        switches.entry(to.to_string()).or_insert(entry);
        self.write(&switches)
    }

    /// Forget a profile's switches when the profile is removed.
    pub fn remove(&self, profile: &str) -> Result<()> {
        let mut switches = self.read();
        if switches.remove(profile).is_none() {
            return Ok(());
        }
        self.write(&switches)
    }

    /// Give a new profile the source's switches.
    pub fn copy(&self, from: &str, to: &str) -> Result<()> {
        let Some(entry) = self.read().get(from).cloned() else {
            return Ok(());
        };
        let mut switches = self.read();
        switches.entry(to.to_string()).or_insert(entry);
        self.write(&switches)
    }
}

pub fn switched_off(profile: &str) -> Vec<String> {
    Store::managed().switched_off(profile)
}

pub fn disable(profile: &str, name: &str) -> Result<()> {
    Store::managed().disable(profile, name)
}

pub fn enable(profile: &str, name: &str) -> Result<()> {
    Store::managed().enable(profile, name)
}

pub fn rename(from: &str, to: &str) -> Result<()> {
    Store::managed().rename(from, to)
}

pub fn remove(profile: &str) -> Result<()> {
    Store::managed().remove(profile)
}

pub fn copy(from: &str, to: &str) -> Result<()> {
    Store::managed().copy(from, to)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(label: &str) -> Store {
        let root = std::env::temp_dir().join(format!(
            "harnesslite-switches-{label}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("root");
        Store {
            file: root.join("plugins.json"),
        }
    }

    #[test]
    fn a_disabled_plugin_is_recorded_and_can_come_back() {
        let store = store("cycle");
        store.disable("work", "dsh-plugin-b").expect("disable");
        assert_eq!(store.switched_off("work"), vec!["dsh-plugin-b".to_string()]);

        store.enable("work", "dsh-plugin-b").expect("enable");
        assert!(store.switched_off("work").is_empty());
    }

    #[test]
    fn a_rename_moves_the_record_whole() {
        let store = store("rename");
        store.disable("from", "dsh-plugin-a").expect("disable");

        store.rename("from", "to").expect("rename");
        assert!(store.switched_off("from").is_empty());
        assert_eq!(store.switched_off("to"), vec!["dsh-plugin-a".to_string()]);

        store.remove("to").expect("remove");
        assert!(store.switched_off("to").is_empty());
    }

    #[test]
    fn a_copy_gives_the_new_profile_the_source_switches() {
        let store = store("copy");
        store.disable("source", "dsh-plugin-a").expect("disable");

        store.copy("source", "target").expect("copy");
        assert_eq!(store.switched_off("target"), vec!["dsh-plugin-a".to_string()]);
        assert_eq!(store.switched_off("source"), vec!["dsh-plugin-a".to_string()]);
    }
}
