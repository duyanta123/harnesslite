//! Reading the preflight's verdict out of npm's `--json` output.
//!
//! The preflight resolves a spec in a scratch project so a bad dependency
//! graph is refused before a confirmation dialog ever offers it. npm answers
//! in JSON on stdout when it succeeds and in prose on stderr when it does not;
//! the shell's part is only to count what the resolution added.

use serde::Serialize;
use serde_json::Value;

use crate::error::{Error, Result};

/// What one successful dry-run resolved.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Resolution {
    /// The spec that was asked about, echoed for the confirmation dialog.
    pub spec: String,
    /// Packages the resolution would add, top of the tree first.
    pub packages: Vec<String>,
    /// Everything the tree would bring in, counted.
    pub added: u64,
}

/// Read a completed dry-run. Failures never get here — a non-zero exit is the
/// runtime layer's error — so a document that will not parse is npm changing
/// shape, and it fails loudly rather than passing an empty tree as fine.
pub fn parse_resolution(stdout: &str, spec: &str) -> Result<Resolution> {
    let value: Value = serde_json::from_str(stdout.trim())
        .map_err(|cause| Error::Plugin(format!("npm's preflight reply made no sense: {cause}")))?;

    let mut packages = Vec::new();
    let mut added = 0u64;
    if let Some(added_nodes) = value.get("added").and_then(Value::as_array) {
        for node in added_nodes {
            added += 1;
            if let Some(name) = node
                .get("name")
                .and_then(Value::as_str)
                .filter(|name| !name.is_empty())
            {
                if !packages.contains(&name.to_string()) {
                    packages.push(name.to_string());
                }
            }
        }
    }

    Ok(Resolution {
        spec: spec.to_string(),
        packages,
        added,
    })
}

#[cfg(test)]
mod tests {
    use super::parse_resolution;

    #[test]
    fn a_resolution_counts_what_npm_would_add() {
        // npm spells its dry-run summary as an array of nodes, which is where
        // both the count and the names come from.
        let resolution = parse_resolution(
            r#"{ "added": [
                    { "name": "dsh-demo", "version": "1.2.0" },
                    { "name": "left-pad", "version": "1.3.0" }
                ],
                "removed": [],
                "dependencies": {} }"#,
            "dsh-demo@1.2.0",
        )
        .expect("a listing");

        assert_eq!(resolution.spec, "dsh-demo@1.2.0");
        assert_eq!(resolution.added, 2);
        assert_eq!(resolution.packages, vec!["dsh-demo".to_string(), "left-pad".to_string()]);
    }

    #[test]
    fn named_nodes_become_the_top_of_the_tree() {
        let resolution = parse_resolution(
            r#"{ "added": [
                { "name": "dsh-demo", "version": "1.2.0", "location": "node_modules/dsh-demo" },
                { "metadata": true },
                { "name": "dsh-demo", "version": "1.2.0", "location": "again" }
            ] }"#,
            "dsh-demo@1.2.0",
        )
        .expect("a detailed listing");

        assert_eq!(resolution.packages, vec!["dsh-demo".to_string()]);
        assert_eq!(resolution.added, 3, "the count is npm's word, not the dedup");
    }

    #[test]
    fn an_unparseable_reply_is_an_error() {
        assert!(parse_resolution("E404 not found", "dsh-demo").is_err());
    }
}
