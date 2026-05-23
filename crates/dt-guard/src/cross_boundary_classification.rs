//! `cross-boundary-classification` subcommand (Layer B) — port of
//! `scripts/guards/simple/validate-cross-boundary-classification.sh`.
//!
//! Enforces two mechanical rules on the `## Cross-Boundary Classification`
//! table in devloop main.md files:
//!
//! * **Rule (a)**: GSA path cannot be classified `Mechanical`.
//! * **Rule (b)**: GSA path with a non-`Mine` classification must have an
//!   Owner field set, and that Owner must appear in the manifest's
//!   specialist list for the path (union across matching globs).
//!
//! All semantic judgment — "is this really Mechanical?", "is the
//! intersection rule honored?" — stays at Gate 1 human review per
//! ADR-0024 §6.6 design rationale.
//!
//! Invocation modes:
//! * **Explicit (Gate 1)**: `--main-md <path>` — Lead invokes directly
//!   before issuing "Plan approved".
//! * **Default (Gate 2)**: scans the diff for modified
//!   `docs/devloop-outputs/**/main.md` files via git.

use crate::common::explain::{print_finding, Finding};
use crate::common::manifest_match::path_matches_glob;
use crate::common::markdown_table::{is_template_placeholder_row, parse_table_under_heading};
use crate::common::status::emit_ok;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub const GSA_MECHANICAL_RULE_ID: &str = "gsa_path_mechanical";
pub const GSA_MISSING_OWNER_RULE_ID: &str = "gsa_path_missing_owner";
pub const OWNER_NOT_IN_MANIFEST_RULE_ID: &str = "owner_not_in_manifest";

const MANIFEST_PATH: &str = "scripts/guards/simple/cross-boundary-ownership.yaml";

#[derive(Debug, Clone)]
struct Manifest {
    /// Ordered list of `(glob, specialists)` from the YAML file.
    entries: Vec<(String, Vec<String>)>,
}

impl Manifest {
    fn load(repo_root: &Path) -> Result<Self> {
        let path = repo_root.join(MANIFEST_PATH);
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        Self::parse(&content).with_context(|| format!("parsing {}", path.display()))
    }

    fn parse(content: &str) -> Result<Self> {
        let mut entries: Vec<(String, Vec<String>)> = Vec::new();
        for (lineno, raw) in content.lines().enumerate() {
            let trimmed = raw.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            // Format: `"glob": [a, b, ...]`
            let Some(rest) = trimmed.strip_prefix('"') else {
                anyhow::bail!("line {}: not a quoted key: {raw:?}", lineno + 1);
            };
            let Some(end_q) = rest.find('"') else {
                anyhow::bail!("line {}: unterminated quoted key", lineno + 1);
            };
            let glob = &rest[..end_q];
            let after = rest[end_q + 1..].trim_start();
            let Some(after) = after.strip_prefix(':') else {
                anyhow::bail!("line {}: expected `:` after key", lineno + 1);
            };
            let after = after.trim_start();
            let Some(after) = after.strip_prefix('[') else {
                anyhow::bail!("line {}: expected `[`", lineno + 1);
            };
            let Some(end_b) = after.find(']') else {
                anyhow::bail!("line {}: unterminated specialist list", lineno + 1);
            };
            let list = &after[..end_b];
            let specialists: Vec<String> = list
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            entries.push((glob.to_string(), specialists));
        }
        Ok(Self { entries })
    }

    /// Specialists valid as Owner for `path` — union across matching globs.
    fn specialists_for_path(&self, path: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for (glob, specialists) in &self.entries {
            if path_matches_glob(path, glob) {
                for s in specialists {
                    if !out.contains(s) {
                        out.push(s.clone());
                    }
                }
            }
        }
        out.sort();
        out
    }

    /// Is `path` a GSA path (matches at least one manifest glob)?
    fn is_gsa(&self, path: &str) -> bool {
        self.entries
            .iter()
            .any(|(glob, _)| path_matches_glob(path, glob))
    }
}

#[derive(Debug)]
struct Hit {
    rule_id: &'static str,
    detail: String,
    file: PathBuf,
    line: usize,
}

fn check_main_md(main_md: &Path, manifest: &Manifest, repo_root: &Path) -> Result<Vec<Hit>> {
    let content = std::fs::read_to_string(main_md)
        .with_context(|| format!("reading {}", main_md.display()))?;
    let rel = main_md
        .strip_prefix(repo_root)
        .unwrap_or(main_md)
        .to_path_buf();
    let rows = parse_table_under_heading(&content, "Cross-Boundary Classification");
    let mut hits: Vec<Hit> = Vec::new();

    for row in &rows {
        if is_template_placeholder_row(row) {
            continue;
        }
        let path = row.cells.first().cloned().unwrap_or_default();
        let classification = row.cells.get(1).cloned().unwrap_or_default();
        let owner = row.cells.get(2).cloned().unwrap_or_default();

        // Skip "Mine" rows — no cross-boundary concern.
        if classification == "Mine" {
            continue;
        }

        let is_mechanical = classification.contains("Mechanical");
        let is_gsa_path = manifest.is_gsa(&path);

        // Rule (a): GSA path cannot be Mechanical.
        if is_gsa_path && is_mechanical {
            hits.push(Hit {
                rule_id: GSA_MECHANICAL_RULE_ID,
                detail: format!(
                    "path {path:?} is a Guarded Shared Area per ADR-0024 §6.4; cannot be Mechanical"
                ),
                file: rel.clone(),
                line: row.line_no,
            });
            continue;
        }

        // Rule (b): GSA path with non-Mine classification needs Owner.
        if is_gsa_path {
            if owner.is_empty() || owner == "—" || owner == "-" {
                hits.push(Hit {
                    rule_id: GSA_MISSING_OWNER_RULE_ID,
                    detail: format!(
                        "GSA path {path:?} needs Owner field naming a specialist from the manifest"
                    ),
                    file: rel.clone(),
                    line: row.line_no,
                });
                continue;
            }
            let valid = manifest.specialists_for_path(&path);
            if !valid.iter().any(|s| s == &owner) {
                hits.push(Hit {
                    rule_id: OWNER_NOT_IN_MANIFEST_RULE_ID,
                    detail: format!(
                        "path {path:?} lists Owner={owner:?} but manifest allows only {{{}}}",
                        valid.join(", ")
                    ),
                    file: rel.clone(),
                    line: row.line_no,
                });
            }
        }
    }
    Ok(hits)
}

/// Find main.md files via git-diff scan for modifications under
/// `docs/devloop-outputs/`. Routes through `common::git_changes` helpers
/// (SoT-only-git rule per @team-lead 2026-05-21).
fn find_diff_main_md(repo_root: &Path) -> Vec<PathBuf> {
    let search = Path::new("docs/devloop-outputs/");
    let modified =
        crate::common::git_changes::get_modified_files(repo_root, search, &[]).unwrap_or_default();
    let untracked =
        crate::common::git_changes::get_untracked_files(repo_root, search, &[]).unwrap_or_default();
    let mut paths: Vec<PathBuf> = modified
        .into_iter()
        .chain(untracked)
        .filter(|p| p.to_str().is_some_and(|s| s.ends_with("/main.md")))
        .collect();
    paths.sort();
    paths.dedup();
    paths
}

pub fn run(repo_root: &Path, explain: bool) -> Result<()> {
    run_with_arg(repo_root, explain, None)
}

/// Entry shape that supports both explicit `--main-md` invocation and
/// default diff-scan mode.
pub fn run_with_arg(repo_root: &Path, explain: bool, explicit: Option<&Path>) -> Result<()> {
    let manifest = Manifest::load(repo_root)?;

    let files: Vec<PathBuf> = if let Some(path) = explicit {
        vec![path.to_path_buf()]
    } else {
        find_diff_main_md(repo_root)
    };

    if files.is_empty() {
        emit_ok("cross-boundary-classification-no-files");
        return Ok(());
    }

    let mut all_hits: Vec<Hit> = Vec::new();
    for path in &files {
        let abs = if path.is_absolute() {
            path.clone()
        } else {
            repo_root.join(path)
        };
        if !abs.is_file() {
            continue;
        }
        all_hits.extend(check_main_md(&abs, &manifest, repo_root)?);
    }

    if all_hits.is_empty() {
        emit_ok(format!(
            "cross-boundary-classification-clean-{}-files",
            files.len()
        ));
        return Ok(());
    }

    for hit in &all_hits {
        let file_disp = hit.file.display().to_string();
        if explain {
            let policy = format!("cross-boundary-classification::{}", hit.rule_id);
            print_finding(&Finding {
                file: &file_disp,
                row: hit.line,
                col: 0,
                policy: &policy,
                matched: &hit.detail,
                extras: &[],
                src_file: file!(),
                src_line: line!(),
            });
        } else {
            println!(
                "VIOLATION: {}:{} [{}] {}",
                file_disp, hit.line, hit.rule_id, hit.detail
            );
        }
    }

    let mech = all_hits
        .iter()
        .filter(|h| h.rule_id == GSA_MECHANICAL_RULE_ID)
        .count();
    let missing_owner = all_hits
        .iter()
        .filter(|h| h.rule_id == GSA_MISSING_OWNER_RULE_ID)
        .count();
    let owner_not = all_hits
        .iter()
        .filter(|h| h.rule_id == OWNER_NOT_IN_MANIFEST_RULE_ID)
        .count();
    // Dominant-class fold: ONE REASON token names the largest finding class
    // (per @test F3 2026-05-23 — deliberate operator-affordance, not a bug).
    // Per-finding detail is still emitted via VIOLATION lines; the wire token
    // points the runbook to the most-frequent rule.
    let token = if mech >= missing_owner && mech >= owner_not {
        "cross-boundary-gsa-mechanical"
    } else if missing_owner >= owner_not {
        "cross-boundary-gsa-missing-owner"
    } else {
        "cross-boundary-owner-not-in-manifest"
    };
    anyhow::bail!("{token}-{}", all_hits.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest_with(entries: &[(&str, &[&str])]) -> Manifest {
        Manifest {
            entries: entries
                .iter()
                .map(|(g, ss)| {
                    (
                        (*g).to_string(),
                        ss.iter().map(|s| (*s).to_string()).collect(),
                    )
                })
                .collect(),
        }
    }

    fn main_md_with_table(rows: &[(&str, &str, &str)]) -> String {
        let mut md = String::from("# Header\n\n## Cross-Boundary Classification\n\n| Path | Classification | Owner |\n|---|---|---|\n");
        for (p, c, o) in rows {
            md.push_str(&format!("| `{p}` | {c} | {o} |\n"));
        }
        md.push_str("\n## Next Section\n");
        md
    }

    fn run_check(md_content: &str, manifest: &Manifest) -> Vec<Hit> {
        let dir = tempfile::tempdir().unwrap();
        let main_md = dir.path().join("docs/devloop-outputs/x/main.md");
        std::fs::create_dir_all(main_md.parent().unwrap()).unwrap();
        std::fs::write(&main_md, md_content).unwrap();
        check_main_md(&main_md, manifest, dir.path()).unwrap()
    }

    // (1) FAIL — GSA path classified Mechanical.
    #[test]
    fn case_1_gsa_mechanical_flags() {
        let manifest =
            manifest_with(&[("crates/common/src/jwt.rs", &["auth-controller", "security"])]);
        let md = main_md_with_table(&[("crates/common/src/jwt.rs", "Mechanical", "—")]);
        let hits = run_check(&md, &manifest);
        assert!(hits.iter().any(|h| h.rule_id == GSA_MECHANICAL_RULE_ID));
    }

    // (2) FAIL — GSA non-Mine missing Owner.
    #[test]
    fn case_2_gsa_missing_owner() {
        let manifest =
            manifest_with(&[("crates/common/src/jwt.rs", &["auth-controller", "security"])]);
        // Non-Mechanical, non-Mine classification triggers rule (b) without (a).
        let md = main_md_with_table(&[("crates/common/src/jwt.rs", "Owner-implements", "—")]);
        let hits = run_check(&md, &manifest);
        assert!(hits.iter().any(|h| h.rule_id == GSA_MISSING_OWNER_RULE_ID));
    }

    // (3) FAIL — GSA Owner not in manifest list.
    #[test]
    fn case_3_owner_not_in_manifest() {
        let manifest =
            manifest_with(&[("crates/common/src/jwt.rs", &["auth-controller", "security"])]);
        let md = main_md_with_table(&[(
            "crates/common/src/jwt.rs",
            "Owner-implements",
            "infrastructure",
        )]);
        let hits = run_check(&md, &manifest);
        assert!(hits
            .iter()
            .any(|h| h.rule_id == OWNER_NOT_IN_MANIFEST_RULE_ID));
    }

    // (4) PASS — GSA Mine row skipped.
    #[test]
    fn case_4_gsa_mine_passes() {
        let manifest =
            manifest_with(&[("crates/common/src/jwt.rs", &["auth-controller", "security"])]);
        let md = main_md_with_table(&[("crates/common/src/jwt.rs", "Mine", "—")]);
        let hits = run_check(&md, &manifest);
        assert!(hits.is_empty(), "Mine row should skip, got {hits:?}");
    }

    // (5) PASS — non-GSA Mine row.
    #[test]
    fn case_5_non_gsa_mine_passes() {
        let manifest = manifest_with(&[("crates/common/src/jwt.rs", &["auth-controller"])]);
        let md = main_md_with_table(&[("crates/foo/src/lib.rs", "Mine", "—")]);
        let hits = run_check(&md, &manifest);
        assert!(hits.is_empty(), "non-GSA Mine should pass: {hits:?}");
    }

    // (6) PASS — non-GSA Mechanical (rule (a) only applies to GSA paths).
    #[test]
    fn case_6_non_gsa_mechanical_passes() {
        let manifest = manifest_with(&[("crates/common/src/jwt.rs", &["auth-controller"])]);
        let md = main_md_with_table(&[("crates/foo/src/lib.rs", "Mechanical", "—")]);
        let hits = run_check(&md, &manifest);
        assert!(hits.is_empty(), "non-GSA Mechanical should pass: {hits:?}");
    }

    // (7) PASS — explicit-mode invocation with single main.md arg.
    #[test]
    fn case_7_explicit_mode() {
        let dir = tempfile::tempdir().unwrap();
        // Manifest.
        std::fs::create_dir_all(dir.path().join("scripts/guards/simple")).unwrap();
        std::fs::write(
            dir.path().join(MANIFEST_PATH),
            "\"crates/common/src/jwt.rs\": [auth-controller]\n",
        )
        .unwrap();
        // Main.md with a clean Mine row.
        let main_md = dir.path().join("docs/devloop-outputs/x/main.md");
        std::fs::create_dir_all(main_md.parent().unwrap()).unwrap();
        std::fs::write(
            &main_md,
            main_md_with_table(&[("crates/foo/src/lib.rs", "Mine", "—")]),
        )
        .unwrap();
        let result = run_with_arg(dir.path(), false, Some(&main_md));
        assert!(result.is_ok(), "got {result:?}");
    }

    // (8) PASS — template-placeholder rows skipped.
    #[test]
    fn case_8_template_placeholder_skip() {
        let manifest = manifest_with(&[("crates/common/src/jwt.rs", &["auth-controller"])]);
        let md = main_md_with_table(&[("{path}", "TBD", "TBD"), ("TBD", "TBD", "TBD")]);
        let hits = run_check(&md, &manifest);
        assert!(
            hits.is_empty(),
            "template placeholders should skip: {hits:?}"
        );
    }

    // (9) PASS — header + separator rows skipped (parser correctness regression).
    #[test]
    fn case_9_header_and_separator_skip() {
        let manifest = manifest_with(&[("crates/foo/src/lib.rs", &["foo-owner"])]);
        // The table parser must not interpret the header row "| Path | ... |"
        // or the separator "|---|---|---|" as a data row.
        let md = main_md_with_table(&[("crates/foo/src/lib.rs", "Mine", "—")]);
        let hits = run_check(&md, &manifest);
        assert!(
            hits.is_empty(),
            "header/separator should not be data: {hits:?}"
        );
    }
}
