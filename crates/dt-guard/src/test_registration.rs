//! `test-registration` subcommand — port of
//! `scripts/guards/simple/test-registration.sh`.
//!
//! Verifies test files in `crates/*/tests/<subdir>/*.rs` are registered in
//! the corresponding entry-point file `crates/*/tests/<subdir>_tests.rs` via
//! a `#[path = "<subdir>/<file>.rs"]` directive. Catches the common mistake
//! of adding a test file in a subdirectory without registering it — those
//! tests silently don't run.

use crate::common::explain::{print_finding, Finding};
use crate::common::status::emit_ok;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub const UNREGISTERED_RULE_ID: &str = "unregistered_test_file";

#[derive(Debug)]
struct Hit {
    test_file: PathBuf,
    entry_file: PathBuf,
}

pub fn run(repo_root: &Path, explain: bool) -> Result<()> {
    let crates_dir = repo_root.join("crates");
    if !crates_dir.is_dir() {
        emit_ok("test-registration-no-crates-dir");
        return Ok(());
    }

    let mut hits: Vec<Hit> = Vec::new();
    let mut entry_count = 0usize;

    for crate_entry in std::fs::read_dir(&crates_dir)
        .with_context(|| format!("reading {}", crates_dir.display()))?
        .flatten()
    {
        let tests_dir = crate_entry.path().join("tests");
        if !tests_dir.is_dir() {
            continue;
        }
        // For each `*_tests.rs` entry-point file at the crate's tests root...
        let entries = match std::fs::read_dir(&tests_dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !name.ends_with("_tests.rs") {
                continue;
            }
            entry_count += 1;
            // Derive the subdir name: `integration_tests.rs` → `integration`.
            let subdir_name = name.trim_end_matches(".rs").trim_end_matches("_tests");
            let subdir = tests_dir.join(subdir_name);
            if !subdir.is_dir() {
                continue;
            }

            let entry_content = std::fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?;

            for sub_entry in std::fs::read_dir(&subdir)
                .with_context(|| format!("reading {}", subdir.display()))?
                .flatten()
            {
                let test_file = sub_entry.path();
                if !test_file.is_file() {
                    continue;
                }
                let Some(fname) = test_file.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                if !fname.ends_with(".rs") || fname == "mod.rs" {
                    continue;
                }
                // Check for `"<subdir_name>/<fname>"` in the entry file.
                let needle = format!("\"{subdir_name}/{fname}\"");
                if !entry_content.contains(&needle) {
                    hits.push(Hit {
                        test_file: test_file
                            .strip_prefix(repo_root)
                            .unwrap_or(&test_file)
                            .to_path_buf(),
                        entry_file: path.strip_prefix(repo_root).unwrap_or(&path).to_path_buf(),
                    });
                }
            }
        }
    }

    if hits.is_empty() {
        emit_ok(format!(
            "test-registration-all-{entry_count}-entry-points-clean"
        ));
        return Ok(());
    }

    for hit in &hits {
        let file_disp = hit.test_file.display().to_string();
        let entry_disp = hit.entry_file.display().to_string();
        if explain {
            print_finding(&Finding {
                file: &file_disp,
                row: 0,
                col: 0,
                policy: "test-registration::unregistered_test_file",
                matched: &hit.test_file.display().to_string(),
                extras: &[("entry_point", &entry_disp)],
                src_file: file!(),
                src_line: line!(),
            });
        } else {
            println!(
                "VIOLATION: {} not registered in {} — add `#[path = \"…\"]` + `mod …;` directives",
                file_disp, entry_disp
            );
        }
    }

    anyhow::bail!("test-unregistered-found-{}", hits.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registered_test_passes() {
        let dir = tempfile::tempdir().unwrap();
        let crates = dir.path().join("crates/foo/tests");
        std::fs::create_dir_all(crates.join("integration")).unwrap();
        std::fs::write(
            crates.join("integration_tests.rs"),
            "#[path = \"integration/a.rs\"]\nmod a;\n",
        )
        .unwrap();
        std::fs::write(crates.join("integration/a.rs"), "// test\n").unwrap();

        let result = run(dir.path(), false);
        assert!(result.is_ok(), "got {result:?}");
    }

    #[test]
    fn unregistered_test_fails() {
        let dir = tempfile::tempdir().unwrap();
        let crates = dir.path().join("crates/foo/tests");
        std::fs::create_dir_all(crates.join("integration")).unwrap();
        std::fs::write(
            crates.join("integration_tests.rs"),
            "#[path = \"integration/a.rs\"]\nmod a;\n",
        )
        .unwrap();
        std::fs::write(crates.join("integration/a.rs"), "// test\n").unwrap();
        std::fs::write(crates.join("integration/b.rs"), "// test\n").unwrap();

        let result = run(dir.path(), false);
        assert!(result.is_err(), "expected unregistered hit");
    }

    #[test]
    fn mod_rs_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let crates = dir.path().join("crates/foo/tests");
        std::fs::create_dir_all(crates.join("integration")).unwrap();
        std::fs::write(
            crates.join("integration_tests.rs"),
            "#[path = \"integration/a.rs\"]\nmod a;\n",
        )
        .unwrap();
        std::fs::write(crates.join("integration/a.rs"), "// test\n").unwrap();
        std::fs::write(crates.join("integration/mod.rs"), "// not a test\n").unwrap();

        let result = run(dir.path(), false);
        assert!(result.is_ok(), "mod.rs should be skipped, got {result:?}");
    }
}
