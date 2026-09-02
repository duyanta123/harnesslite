//! Persisted window placement.
//!
//! One JSON file, written on close and read on open: the window reappears
//! where the user left it, and a maximized window comes back maximized.

use serde::{Deserialize, Serialize};

use hd_core::error::{Error, Result};
use hd_core::paths;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Placement {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub maximized: bool,
}

pub fn load() -> Option<Placement> {
    let body = std::fs::read(paths::window_file()).ok()?;
    serde_json::from_slice(&body).ok()
}

pub fn save(placement: &Placement) -> Result<()> {
    let file = paths::window_file();
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|cause| {
            Error::Store(format!("{} could not be created: {cause}", parent.display()))
        })?;
    }
    // Compact, like every other machine-written window file: the values are
    // numbers, and pretty-printing them would be six lines of noise.
    let mut body = serde_json::to_vec(placement)
        .map_err(|cause| Error::Store(format!("window placement could not be encoded: {cause}")))?;
    body.push(b'\n');
    hd_core::atomic::write(&file, body).map_err(|cause| {
        Error::Store(format!(
            "{} could not be committed: {cause}",
            file.display()
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placement_round_trips_through_the_frozen_fixture_shape() {
        // The fixture is HarnessDeck's real window.json; the schema is shared.
        let body = include_str!("../../../tests/fixtures/harnessdeck/window.json");
        let placement: Placement = serde_json::from_str(body).expect("fixture parses");
        assert_eq!(placement.width, 2560);
        assert!(placement.maximized);

        let root = std::env::temp_dir().join(format!("harnesslite-window-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("window.json");
        std::fs::write(&file, body).unwrap();

        let stored = std::fs::read(&file).unwrap();
        let round: Placement = serde_json::from_slice(&stored).unwrap();
        assert_eq!(round, placement);
        let _ = std::fs::remove_dir_all(root);
    }
}
