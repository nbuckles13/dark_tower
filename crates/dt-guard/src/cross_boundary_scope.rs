//! `cross-boundary-scope` subcommand (Layer A) — port of
//! `scripts/guards/simple/validate-cross-boundary-scope.sh`.
//!
//! Compares the active devloop's plan against the active edit's diff to
//! prevent scope drift. Flags:
//! * **Inbound drift**: files in the diff but absent from the plan.
//! * **Planned-untouched**: files listed in the plan but absent from the diff.
//!
//! ## User-story exemption — verbatim bash behavior
//!
//! `docs/user-stories/*.md` files are exempt **whole-file** from drift
//! detection (matching bash today). The row-level tightening originally
//! contemplated in the Wave-2 plan was rolled back per @team-lead 2026-05-21
//! redirect — preserving the existing exemption keeps the policy function
//! pure (no diff-hunk parsing) and the Rust port behaviorally identical to
//! bash. The 2026-05-19 absorption finding (substantive user-story edits
//! bypass classification) remains tracked under `docs/TODO.md` §Polyglot
//! Pipeline Follow-ups for future focused work; PR review catches
//! substantive edits in the interim.
//!
//! ## Architecture
//!
//! Policy logic lives in [`check_scope`] — a pure function taking already-
//! resolved string inputs (changed files, plan paths, exempt extras) and
//! returning a [`ScopeReport`]. The `run()` orchestrator is the only
//! function that touches git / filesystem; it parses inputs out of the
//! repo and delegates verdict computation to [`check_scope`].

use crate::common::explain::{print_finding, Finding};
use crate::common::manifest_match::path_matches_glob;
use crate::common::markdown_table::{is_template_placeholder_row, parse_table_under_heading};
use crate::common::status::emit_ok;
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::path::Path;

pub const INBOUND_DRIFT_RULE_ID: &str = "scope_drift_inbound";
pub const PLANNED_UNTOUCHED_RULE_ID: &str = "scope_drift_planned_untouched";

/// One verdict entry — either inbound drift (path in diff, not in plan) or
/// planned-untouched (path in plan, not in diff).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeFinding {
    pub rule_id: &'static str,
    pub path: String,
}

/// Verdict from [`check_scope`]. Empty `findings` means no drift.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ScopeReport {
    pub findings: Vec<ScopeFinding>,
}

/// True if `path` is a depth-2 `docs/user-stories/<slug>.md` file.
pub fn is_user_story_path(path: &str) -> bool {
    if !path.starts_with("docs/user-stories/") || !path.ends_with(".md") {
        return false;
    }
    let rest = &path["docs/user-stories/".len()..];
    !rest.contains('/')
}

/// Symmetric exclusions applied to BOTH plan_paths and diff_paths.
///
/// * The active main.md itself — self-referential drift is not useful.
/// * `docs/TODO.md` — explicit append target (ADR-0019).
/// * `docs/specialist-knowledge/*/INDEX.md` — reflection-phase artifacts.
/// * `docs/user-stories/*.md` — whole-file exemption per bash (Step-9
///   Tracking append target).
pub fn is_symmetric_exclusion(rel_main_md: &str, path: &str) -> bool {
    if path == rel_main_md {
        return true;
    }
    if path == "docs/TODO.md" {
        return true;
    }
    if path_matches_glob(path, "docs/specialist-knowledge/*/INDEX.md") {
        return true;
    }
    if is_user_story_path(path) {
        return true;
    }
    false
}

/// Pure policy function: given the resolved sets, compute the drift verdict.
///
/// Inputs are already filtered (callers strip symmetric exclusions before
/// calling). Behavior:
/// 1. Expand glob plan entries against `changed_files` — a glob that
///    matches ≥1 path is replaced by those paths; a glob matching zero
///    paths stays literal so it surfaces as planned-untouched.
/// 2. Compute `diff ∖ plan` → inbound drift.
/// 3. Compute `plan ∖ diff` → planned-untouched.
///
/// `exempt_extras` are additional paths the caller wants treated as exempt
/// from drift on both sides (dynamic workflow exemptions; empty in the
/// common case).
pub fn check_scope(
    changed_files: &[String],
    plan_paths: &[String],
    exempt_extras: &HashSet<String>,
) -> ScopeReport {
    let diff_set: Vec<String> = changed_files
        .iter()
        .filter(|p| !exempt_extras.contains(*p))
        .cloned()
        .collect();

    let mut expanded_plan: Vec<String> = Vec::new();
    for plan_entry in plan_paths {
        if exempt_extras.contains(plan_entry) {
            continue;
        }
        if plan_entry.contains('*') || plan_entry.contains('?') || plan_entry.contains('[') {
            let matched: Vec<String> = diff_set
                .iter()
                .filter(|d| path_matches_glob(d, plan_entry))
                .cloned()
                .collect();
            if matched.is_empty() {
                expanded_plan.push(plan_entry.clone());
            } else {
                expanded_plan.extend(matched);
            }
        } else {
            expanded_plan.push(plan_entry.clone());
        }
    }
    expanded_plan.sort();
    expanded_plan.dedup();

    let mut findings: Vec<ScopeFinding> = Vec::new();
    // Inbound drift.
    for d in &diff_set {
        if !expanded_plan.iter().any(|p| p == d) {
            findings.push(ScopeFinding {
                rule_id: INBOUND_DRIFT_RULE_ID,
                path: d.clone(),
            });
        }
    }
    // Planned-untouched.
    for p in &expanded_plan {
        if !diff_set.iter().any(|d| d == p) {
            findings.push(ScopeFinding {
                rule_id: PLANNED_UNTOUCHED_RULE_ID,
                path: p.clone(),
            });
        }
    }
    ScopeReport { findings }
}

/// Parse the `## Cross-Boundary Classification` table from a main.md and
/// return path-column entries, filtered by `is_symmetric_exclusion`.
/// Pure helper — takes the file content directly.
pub fn parse_plan_paths(main_md_content: &str, rel_main_md: &str) -> Vec<String> {
    let rows = parse_table_under_heading(main_md_content, "Cross-Boundary Classification");
    let mut out: Vec<String> = Vec::new();
    for row in &rows {
        if is_template_placeholder_row(row) {
            continue;
        }
        let path = row.cells.first().cloned().unwrap_or_default();
        if path.is_empty() {
            continue;
        }
        if is_symmetric_exclusion(rel_main_md, &path) {
            continue;
        }
        if !out.contains(&path) {
            out.push(path);
        }
    }
    out
}

/// Filter raw diff paths by the symmetric-exclusion predicate. Pure helper.
pub fn filter_diff_paths(diff_paths: &[String], rel_main_md: &str) -> Vec<String> {
    let mut out: Vec<String> = diff_paths
        .iter()
        .filter(|p| !is_symmetric_exclusion(rel_main_md, p))
        .cloned()
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Identify the active devloop's main.md from diff paths. Pure helper.
pub fn find_active_main_md(diff_paths: &[String]) -> Vec<String> {
    diff_paths
        .iter()
        .filter(|p| {
            p.starts_with("docs/devloop-outputs/")
                && p.ends_with("/main.md")
                && !p.starts_with("docs/devloop-outputs/_")
        })
        .cloned()
        .collect()
}

// -----------------------------------------------------------------------------
// Thin orchestrator (run) — fetches git data, delegates verdict to check_scope.
// -----------------------------------------------------------------------------

/// Fetch raw diff paths for the active edit's scope via
/// `common::git_changes::get_active_edit_paths`.
///
/// Per @team-lead 2026-05-21 SoT-only-git rule: this orchestrator no longer
/// inlines `Command::new("git")`. The dirty-vs-clean scope resolution
/// (working-tree vs HEAD when dirty, else HEAD^..HEAD with HEAD^2 merge
/// handling) lives in `common::git_changes::get_active_edit_paths`. This
/// is the bash `validate-cross-boundary-scope.sh::resolve_scope()` posture
/// — narrower than the polyglot-pipeline merge-base from `_get_base_ref.sh`
/// — so the guard detects drift in THIS edit only, not the cumulative
/// branch state.
fn fetch_diff_paths(repo_root: &Path) -> Result<Vec<String>> {
    let raw = crate::common::git_changes::get_active_edit_paths(repo_root)
        .context("fetching active-edit paths via common::git_changes")?;
    let mut paths: Vec<String> = raw
        .into_iter()
        .filter_map(|p| p.to_str().map(String::from))
        .collect();
    paths.sort();
    paths.dedup();
    Ok(paths)
}

pub fn run(repo_root: &Path, explain: bool) -> Result<()> {
    let diff_paths = fetch_diff_paths(repo_root)?;
    if diff_paths.is_empty() {
        emit_ok("cross-boundary-scope-no-changes");
        return Ok(());
    }

    let active_mds = find_active_main_md(&diff_paths);
    if active_mds.is_empty() {
        emit_ok("cross-boundary-scope-no-active-main-md");
        return Ok(());
    }
    if active_mds.len() > 1 {
        anyhow::bail!(
            "cross-boundary-multi-devloop-collision-{}-main-mds",
            active_mds.len()
        );
    }
    let Some(rel_main_md) = active_mds.into_iter().next() else {
        unreachable!("active_mds.len() == 1 here, checked above")
    };
    let abs = repo_root.join(&rel_main_md);
    if !abs.is_file() {
        emit_ok("cross-boundary-scope-main-md-deleted");
        return Ok(());
    }

    let main_md_content =
        std::fs::read_to_string(&abs).with_context(|| format!("reading {}", abs.display()))?;

    let plan_paths = parse_plan_paths(&main_md_content, &rel_main_md);
    let changed_files = filter_diff_paths(&diff_paths, &rel_main_md);
    let exempt_extras: HashSet<String> = HashSet::new();

    let report = check_scope(&changed_files, &plan_paths, &exempt_extras);

    if report.findings.is_empty() {
        emit_ok("cross-boundary-scope-no-drift");
        return Ok(());
    }

    for finding in &report.findings {
        if explain {
            let policy = format!("cross-boundary-scope::{}", finding.rule_id);
            print_finding(&Finding {
                file: &rel_main_md,
                row: 0,
                col: 0,
                policy: &policy,
                matched: &finding.path,
                extras: &[],
                src_file: file!(),
                src_line: line!(),
            });
        } else {
            println!(
                "VIOLATION: {} [{}] {:?}",
                rel_main_md, finding.rule_id, finding.path
            );
        }
    }

    let inbound = report
        .findings
        .iter()
        .filter(|f| f.rule_id == INBOUND_DRIFT_RULE_ID)
        .count();
    let untouched = report
        .findings
        .iter()
        .filter(|f| f.rule_id == PLANNED_UNTOUCHED_RULE_ID)
        .count();
    // Dominant-class fold: emit ONE REASON token reflecting the largest
    // finding class (per @test F3 2026-05-23 — deliberate operator-affordance,
    // not a bug). Per-finding detail is still emitted via VIOLATION lines
    // above; the wire token names the runbook entry to consult first.
    if inbound >= untouched {
        anyhow::bail!("cross-boundary-scope-drift-inbound-{inbound}")
    } else {
        anyhow::bail!("cross-boundary-scope-drift-planned-untouched-{untouched}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn user_story_path_predicate() {
        assert!(is_user_story_path("docs/user-stories/foo.md"));
        assert!(!is_user_story_path("docs/user-stories/sub/bar.md"));
        assert!(!is_user_story_path("docs/runbooks/foo.md"));
    }

    #[test]
    fn symmetric_exclusion_recognizes_canonical_paths() {
        let rel = "docs/devloop-outputs/x/main.md";
        assert!(is_symmetric_exclusion(rel, rel));
        assert!(is_symmetric_exclusion(rel, "docs/TODO.md"));
        assert!(is_symmetric_exclusion(
            rel,
            "docs/specialist-knowledge/security/INDEX.md"
        ));
        // Whole-file user-story exemption — verbatim bash behavior.
        assert!(is_symmetric_exclusion(rel, "docs/user-stories/story.md"));
        assert!(!is_symmetric_exclusion(rel, "crates/foo/src/lib.rs"));
    }

    #[test]
    fn check_scope_clean_when_diff_equals_plan() {
        let changed = s(&["crates/foo/src/lib.rs", "docs/x.md"]);
        let plan = s(&["crates/foo/src/lib.rs", "docs/x.md"]);
        let extras = HashSet::new();
        let report = check_scope(&changed, &plan, &extras);
        assert!(report.findings.is_empty(), "got {:?}", report.findings);
    }

    #[test]
    fn check_scope_flags_inbound_drift() {
        let changed = s(&["crates/foo/src/lib.rs", "crates/bar/src/lib.rs"]);
        let plan = s(&["crates/foo/src/lib.rs"]);
        let extras = HashSet::new();
        let report = check_scope(&changed, &plan, &extras);
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].rule_id, INBOUND_DRIFT_RULE_ID);
        assert_eq!(report.findings[0].path, "crates/bar/src/lib.rs");
    }

    #[test]
    fn check_scope_flags_planned_untouched() {
        let changed = s(&["crates/foo/src/lib.rs"]);
        let plan = s(&["crates/foo/src/lib.rs", "crates/baz/src/lib.rs"]);
        let extras = HashSet::new();
        let report = check_scope(&changed, &plan, &extras);
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].rule_id, PLANNED_UNTOUCHED_RULE_ID);
        assert_eq!(report.findings[0].path, "crates/baz/src/lib.rs");
    }

    #[test]
    fn check_scope_glob_in_plan_expands_against_diff() {
        let changed = s(&[
            "crates/foo/src/a.rs",
            "crates/foo/src/b.rs",
            "crates/foo/src/c.rs",
        ]);
        let plan = s(&["crates/foo/src/**"]);
        let extras = HashSet::new();
        let report = check_scope(&changed, &plan, &extras);
        assert!(
            report.findings.is_empty(),
            "glob should expand to cover all diff entries: {:?}",
            report.findings
        );
    }

    #[test]
    fn check_scope_glob_with_no_matches_stays_literal_untouched() {
        let changed = s(&["crates/foo/src/a.rs"]);
        let plan = s(&["crates/foo/src/a.rs", "crates/never-existed/**"]);
        let extras = HashSet::new();
        let report = check_scope(&changed, &plan, &extras);
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].rule_id, PLANNED_UNTOUCHED_RULE_ID);
        assert_eq!(report.findings[0].path, "crates/never-existed/**");
    }

    #[test]
    fn check_scope_exempt_extras_remove_from_both_sides() {
        let changed = s(&["crates/foo/src/lib.rs", "exempt.rs"]);
        let plan = s(&["crates/foo/src/lib.rs", "exempt.rs"]);
        let mut extras = HashSet::new();
        extras.insert("exempt.rs".to_string());
        let report = check_scope(&changed, &plan, &extras);
        assert!(report.findings.is_empty(), "got {:?}", report.findings);
    }

    #[test]
    fn parse_plan_paths_extracts_classification_rows() {
        let md = r#"# Devloop X

## Cross-Boundary Classification

| Path | Classification | Owner |
|---|---|---|
| `crates/foo/src/lib.rs` | Mine | — |
| `crates/bar/src/lib.rs` | Mechanical | code-reviewer |
| `{path}` | TBD | TBD |

## Other
"#;
        let paths = parse_plan_paths(md, "docs/devloop-outputs/x/main.md");
        assert_eq!(
            paths,
            vec![
                "crates/foo/src/lib.rs".to_string(),
                "crates/bar/src/lib.rs".to_string()
            ]
        );
    }

    #[test]
    fn parse_plan_paths_drops_symmetric_exclusions() {
        let md = r#"# Devloop X

## Cross-Boundary Classification

| Path | Classification | Owner |
|---|---|---|
| `docs/TODO.md` | Mine | — |
| `docs/user-stories/story.md` | Mine | — |
| `docs/specialist-knowledge/security/INDEX.md` | Mine | — |
| `crates/foo/src/lib.rs` | Mine | — |
"#;
        let paths = parse_plan_paths(md, "docs/devloop-outputs/x/main.md");
        // Only the non-excluded path survives.
        assert_eq!(paths, vec!["crates/foo/src/lib.rs".to_string()]);
    }

    #[test]
    fn filter_diff_paths_drops_user_stories_whole_file() {
        // Verbatim bash behavior: user-story files are exempt regardless of
        // what changed inside them.
        let diff = s(&[
            "crates/foo/src/lib.rs",
            "docs/user-stories/story-a.md",
            "docs/user-stories/story-b.md",
        ]);
        let filtered = filter_diff_paths(&diff, "docs/devloop-outputs/x/main.md");
        assert_eq!(filtered, vec!["crates/foo/src/lib.rs".to_string()]);
    }

    #[test]
    fn user_story_exemption_end_to_end() {
        // Edit a user-story file + a planned production file: the user-
        // story file is exempt whole-file, no drift fires.
        let raw_diff = s(&["crates/foo/src/lib.rs", "docs/user-stories/story.md"]);
        let filtered = filter_diff_paths(&raw_diff, "docs/devloop-outputs/x/main.md");
        let plan = s(&["crates/foo/src/lib.rs"]);
        let extras = HashSet::new();
        let report = check_scope(&filtered, &plan, &extras);
        assert!(
            report.findings.is_empty(),
            "whole-file user-story exemption should suppress drift: {:?}",
            report.findings
        );
    }

    #[test]
    fn find_active_main_md_picks_only_devloop_main() {
        let diff = s(&[
            "crates/foo/src/lib.rs",
            "docs/devloop-outputs/x/main.md",
            "docs/devloop-outputs/_template/main.md", // template skipped
            "docs/devloop-outputs/y/other.md",        // not main.md
        ]);
        let actives = find_active_main_md(&diff);
        assert_eq!(actives, vec!["docs/devloop-outputs/x/main.md".to_string()]);
    }
}
