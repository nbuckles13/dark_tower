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

/// `(crate, target, seed_cmd)` for every matrix entry that declares a `seed_cmd:`
/// (the line after `target:`). Non-empty, or the seed checks below would be vacuous.
fn seed_cmds() -> Vec<(String, String, String)> {
    let workflow = read(&repo_root().join(".github/workflows/fuzz-nightly.yml"));
    let lines: Vec<&str> = workflow.lines().map(str::trim).collect();
    let out: Vec<(String, String, String)> = lines
        .windows(3)
        .filter_map(|w| {
            let krate = w[0].strip_prefix("- crate: ")?;
            let target = w[1].strip_prefix("target: ")?;
            let cmd = w[2].strip_prefix("seed_cmd: ")?;
            Some((krate.to_string(), target.to_string(), cmd.to_string()))
        })
        .collect();
    assert!(
        !out.is_empty(),
        "no `seed_cmd:` in fuzz-nightly.yml (expected one after `target:`)"
    );
    out
}

/// The value after `--example` in a seed command, if any.
fn example_of(cmd: &str) -> Option<&str> {
    let mut words = cmd.split_whitespace();
    words.find(|w| *w == "--example")?;
    words.next()
}

/// `(crate name, crate dir)` for every directory under `crates/`.
fn crate_dirs() -> Vec<(String, PathBuf)> {
    std::fs::read_dir(repo_root().join("crates"))
        .expect("crates/ is readable")
        .map(|e| {
            let dir = e.expect("dir entry").path();
            let name = dir
                .file_name()
                .and_then(|n| n.to_str())
                .expect("crate dir name")
                .to_string();
            (name, dir)
        })
        .collect()
}

// Seeds are generated at fuzz time, never committed, so a seed generator that a
// matrix entry should run but does not means that target is silently fuzzed
// unseeded: the nightly's "Verify seed corpus" step only covers entries that
// DECLARE a `seed_cmd`. The four tests below close that from each direction.

/// Every `--example X` a `seed_cmd` names exists as `crates/<crate>/fuzz/examples/X.rs`.
#[test]
fn every_seed_cmd_example_exists() {
    for (krate, target, cmd) in &seed_cmds() {
        if let Some(example) = example_of(cmd) {
            let file = repo_root().join(format!("crates/{krate}/fuzz/examples/{example}.rs"));
            assert!(
                file.is_file(),
                "{krate}/{target}: seed_cmd names --example {example}, but {} does not exist",
                file.display()
            );
        }
    }
}

/// A crate-wide generator (a `seed_cmd` naming no `--example`, e.g. a `--test` that
/// writes every target's corpus) is declared by EVERY matrix entry of its crate, so
/// a deleted field or a new target added without one cannot fuzz unseeded.
#[test]
fn crate_wide_seed_cmd_is_declared_by_every_entry_of_its_crate() {
    let seed_cmds = seed_cmds();
    let matrix = matrix();
    for (krate, _, cmd) in seed_cmds.iter().filter(|(_, _, c)| example_of(c).is_none()) {
        for target in matrix.get(krate).into_iter().flatten() {
            let declared = seed_cmds
                .iter()
                .find(|(k, t, _)| k == krate && t == target)
                .map(|(_, _, c)| c);
            assert_eq!(
                declared,
                Some(cmd),
                "{krate}/{target}: {krate} has the crate-wide seed_cmd `{cmd}`, so every \
                 {krate} matrix entry must declare it"
            );
        }
    }
}

/// Anchor for the crate-wide rule, which passes vacuously if EVERY entry's
/// `seed_cmd` is deleted: a crate with a `tests/corpus_seeds.rs` generator has at
/// least one `seed_cmd` running it.
#[test]
fn every_corpus_seeds_test_is_run_by_a_seed_cmd() {
    let seed_cmds = seed_cmds();
    let mut checked = 0;
    for (krate, dir) in crate_dirs() {
        if !dir.join("tests/corpus_seeds.rs").is_file() {
            continue;
        }
        assert!(
            seed_cmds
                .iter()
                .any(|(k, _, c)| *k == krate && c.contains("--test corpus_seeds")),
            "{krate} has tests/corpus_seeds.rs (a seed generator) but no fuzz-nightly \
             seed_cmd runs `--test corpus_seeds`"
        );
        checked += 1;
    }
    assert!(checked > 0, "no crates/*/tests/corpus_seeds.rs found");
}

/// Every `crates/<crate>/fuzz/examples/*.rs` is the `seed_cmd` (`--example <stem>`)
/// of exactly one matrix entry of that crate.
#[test]
fn every_fuzz_seed_example_is_run_by_exactly_one_nightly_entry() {
    let seed_cmds = seed_cmds();
    let mut checked = 0;
    for (krate, dir) in crate_dirs() {
        let Ok(examples) = std::fs::read_dir(dir.join("fuzz/examples")) else {
            continue;
        };
        for example in examples {
            let path = example.expect("dir entry").path();
            if path.extension().and_then(|x| x.to_str()) != Some("rs") {
                continue;
            }
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .expect("example stem")
                .to_string();
            let runners: Vec<&String> = seed_cmds
                .iter()
                .filter(|(k, _, cmd)| *k == krate && example_of(cmd) == Some(stem.as_str()))
                .map(|(_, t, _)| t)
                .collect();
            assert_eq!(
                runners.len(),
                1,
                "{krate}/fuzz/examples/{stem}.rs must be the seed_cmd (`--example {stem}`) of \
                 exactly one fuzz-nightly matrix entry; found {runners:?}"
            );
            checked += 1;
        }
    }
    assert!(checked > 0, "no crates/*/fuzz/examples/*.rs found");
}
