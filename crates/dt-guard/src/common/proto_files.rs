//! Discovery of the `.proto` files the proto guards scan.
//!
//! POLICY-FREE, like `common::proto_lex`: it lists files and reads them. It
//! has three consumers (`proto_per_kind_fields`, `proto_no_any`,
//! `internal_proto_no_key_material`), none of which imports from a sibling
//! policy module for it.
//!
//! A missing `proto/` directory yields `Ok(empty)`, NOT an error: each caller
//! maps an empty set to its OWN `*-scan-vacuous` token, so the vacuity reason
//! names the guard that could not evaluate its input.

use crate::common::path_safety::to_repo_relative;
use anyhow::Result;
use std::path::Path;

/// Every `proto/**/*.proto` under `root`, root-relative and sorted, with its
/// content.
///
/// # Errors
///
/// An unreadable file or directory entry.
pub fn proto_files(root: &Path) -> Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    let base = root.join("proto");
    if !base.is_dir() {
        return Ok(out);
    }
    for entry in walkdir::WalkDir::new(&base).sort_by_file_name() {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type().is_file() && path.extension().is_some_and(|e| e == "proto") {
            let rel = to_repo_relative(root, path).replace('\\', "/");
            out.push((rel, std::fs::read_to_string(path)?));
        }
    }
    Ok(out)
}
