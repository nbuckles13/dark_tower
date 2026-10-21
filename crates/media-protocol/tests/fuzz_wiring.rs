//! WORKSPACE-SCOPED: every fuzz target in the repository is wired three ways
//! consistently — its source file, its `[[bin]]`, and its nightly matrix entry.
//!
//! The target list lives in three places per fuzz crate — the files in
//! `crates/<crate>/fuzz/fuzz_targets/`, the `[[bin]]`s in
//! `crates/<crate>/fuzz/Cargo.toml`, and `.github/workflows/fuzz-nightly.yml`'s
//! matrix grouped by `crate:` — so a target added to one and not another is
//! either dead code or silently never fuzzed nightly. This compares all three
//! as sets, both directions, for **every** `crates/*/fuzz/` crate (not only
//! `media-protocol`, whose cut-through targets prompted it), and requires every
//! matrix crate to have a fuzz directory. Nothing may be empty.
//!
//! It lives in `media-protocol`'s tests only because that crate's fuzz targets
//! are the ones this check was introduced with; the scope is the workspace.

#![allow(clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// `(bin name, path stem)` for every `[[bin]]` in a fuzz manifest.
fn bins(manifest: &str) -> BTreeSet<(String, String)> {
    manifest
        .split("[[bin]]")
        .skip(1)
        .map(|block| {
            let field = |key: &str| {
                block
                    .lines()
                    .find_map(|l| l.trim().strip_prefix(key))
                    .map(|v| v.trim_matches('"').to_string())
                    .unwrap_or_else(|| panic!("[[bin]] block without `{key}`: {block}"))
            };
            let name = field("name = ");
            let path = field("path = ");
            let stem = Path::new(&path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_else(|| panic!("bad bin path {path}"))
                .to_string();
            (name, stem)
        })
        .collect()
}

/// Matrix entries, grouped by crate.
fn matrix() -> BTreeMap<String, BTreeSet<String>> {
    let workflow = read(&repo_root().join(".github/workflows/fuzz-nightly.yml"));
    let lines: Vec<&str> = workflow.lines().map(str::trim).collect();
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for pair in lines.windows(2) {
        if let Some(krate) = pair[0].strip_prefix("- crate: ") {
            let target = pair[1]
                .strip_prefix("target: ")
                .unwrap_or_else(|| panic!("matrix entry without a target line: {}", pair[1]));
            out.entry(krate.to_string())
                .or_default()
                .insert(target.to_string());
        }
    }
    out
}

#[test]
fn every_workspace_fuzz_target_has_a_file_a_bin_and_a_nightly_entry() {
    let matrix = matrix();
    assert!(!matrix.is_empty(), "no matrix entries in fuzz-nightly.yml");

    let crates_dir = repo_root().join("crates");
    let mut fuzz_crates: BTreeSet<String> = BTreeSet::new();
    for entry in std::fs::read_dir(&crates_dir).expect("crates/ is readable") {
        let dir = entry.expect("dir entry").path();
        let manifest = dir.join("fuzz/Cargo.toml");
        if !manifest.is_file() {
            continue;
        }
        let krate = dir
            .file_name()
            .and_then(|n| n.to_str())
            .expect("crate dir name")
            .to_string();
        fuzz_crates.insert(krate.clone());

        let bins = bins(&read(&manifest));
        assert!(!bins.is_empty(), "{krate}: no [[bin]] in fuzz/Cargo.toml");
        let bin_names: BTreeSet<String> = bins.iter().map(|(n, _)| n.clone()).collect();
        let bin_paths: BTreeSet<String> = bins.iter().map(|(_, p)| p.clone()).collect();

        let files: BTreeSet<String> = std::fs::read_dir(dir.join("fuzz/fuzz_targets"))
            .unwrap_or_else(|e| panic!("{krate}: fuzz/fuzz_targets unreadable: {e}"))
            .filter_map(|e| {
                let p = e.expect("dir entry").path();
                (p.extension().and_then(|x| x.to_str()) == Some("rs"))
                    .then(|| {
                        p.file_stem()
                            .and_then(|s| s.to_str())
                            .map(ToString::to_string)
                    })
                    .flatten()
            })
            .collect();
        assert!(!files.is_empty(), "{krate}: no fuzz target files");
        assert_eq!(
            files, bin_paths,
            "{krate}: fuzz_targets/*.rs and the [[bin]] paths differ (dead target file, or a bin \
             naming a missing file)"
        );
        assert_eq!(
            matrix.get(&krate),
            Some(&bin_names),
            "{krate}: [[bin]]s and the fuzz-nightly.yml matrix entries differ"
        );
    }
    assert!(!fuzz_crates.is_empty(), "no crates/*/fuzz/Cargo.toml found");
    let matrix_crates: BTreeSet<String> = matrix.keys().cloned().collect();
    assert_eq!(
        matrix_crates, fuzz_crates,
        "a fuzz-nightly.yml matrix crate has no crates/<crate>/fuzz/, or a fuzz crate is missing \
         from the matrix"
    );
}
