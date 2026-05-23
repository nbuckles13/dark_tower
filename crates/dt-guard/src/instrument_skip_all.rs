//! `rust-instrument-skip-all` subcommand — port of
//! `scripts/guards/simple/instrument-skip-all.sh`.
//!
//! Detects `#[instrument]` attributes using denylist (`skip(...)`) instead of
//! allowlist (`skip_all`) discipline. Two checks:
//!
//! 1. **Check 1 — denylist `skip(...)` not `skip_all`**: any
//!    `#[instrument(... skip(...) ...)]` that does NOT also contain
//!    `skip_all` is flagged. REASON token `rust-instrument-skip-not-all`.
//! 2. **Check 2 — sensitive params without `skip_all`**: heuristic. A
//!    function carrying a sensitive parameter (from CATEGORY_A) under an
//!    `#[instrument]` that doesn't have `skip_all` within a 3-line lookahead
//!    is flagged. **Per @observability Q2**: this fires as FAIL (preserves
//!    bash counting behavior — bash today incremented despite the
//!    "POTENTIAL" wording). REASON token
//!    `rust-instrument-sensitive-param-no-skip-all`.
//!
//! Per @security F3 + @code-reviewer item 2: sensitive-param vocabulary
//! comes from [`crate::common::pii_vocabulary::PII_TOKENS_CATEGORY_A`].
//!
//! Per @semantic-guard Q1: findings emit via [`print_secret_finding`].
//! Param names CAN carry secret vocabulary; conservatively redact for the
//! whole module.

use crate::common::explain::{print_secret_finding, SecretFinding};
use crate::common::git_changes::get_all_changed_files;
use crate::common::pii_vocabulary::PII_TOKENS_CATEGORY_A;
use crate::common::status::emit_ok;
use crate::common::test_code_filter::{
    compute_test_block_ranges, is_line_in_test_block, is_scan_exempt,
};
use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use std::path::{Path, PathBuf};

pub const SKIP_NOT_ALL_RULE_ID: &str = "skip_not_all";
pub const SENSITIVE_PARAM_NO_SKIP_ALL_RULE_ID: &str = "sensitive_param_no_skip_all";

const EXTENSIONS: &[&str] = &[".rs"];
// Path-shape exclusions live in `common::test_code_filter::is_test_path` per
// @team-lead Wave-2 port-fidelity fix 2026-05-21.

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static INSTRUMENT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"#\[instrument").expect("static pattern compiles"));

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static SECRET_WORD_RE: Lazy<Regex> = Lazy::new(|| {
    let alternation = PII_TOKENS_CATEGORY_A.join("|");
    Regex::new(&format!(r"\b({alternation})\b")).expect("vocab alternation compiles")
});

fn is_excluded_path(path: &Path) -> bool {
    is_scan_exempt(path)
}

fn is_comment_line(line: &str) -> bool {
    let stripped = line.trim_start();
    stripped.starts_with("//") || stripped.starts_with("/*") || stripped.starts_with("*")
}

/// Find the first sensitive-param token at-or-near the `#[instrument]`
/// header. Looks at the next 8 lines (function-signature window) for a
/// CATEGORY_A token.
fn sensitive_param_in_window(lines: &[&str], start: usize, window: usize) -> Option<&'static str> {
    let end = (start + window).min(lines.len());
    for line in lines.get(start..end).unwrap_or(&[]) {
        if let Some(m) = SECRET_WORD_RE.find(line) {
            let s = m.as_str();
            if let Some(tok) = PII_TOKENS_CATEGORY_A.iter().copied().find(|t| *t == s) {
                return Some(tok);
            }
        }
    }
    None
}

/// Find `skip_all` within `window` lines starting at `start`.
fn skip_all_in_window(lines: &[&str], start: usize, window: usize) -> bool {
    let end = (start + window).min(lines.len());
    lines
        .get(start..end)
        .unwrap_or(&[])
        .iter()
        .any(|l| l.contains("skip_all"))
}

#[derive(Debug)]
struct Hit {
    rule_id: &'static str,
    pattern_name: &'static str,
    file: PathBuf,
    line: usize,
}

fn scan_file(repo_root: &Path, path: &Path) -> Result<Vec<Hit>> {
    let abs = repo_root.join(path);
    let content =
        std::fs::read_to_string(&abs).with_context(|| format!("reading {}", abs.display()))?;
    let lines: Vec<&str> = content.lines().collect();
    let mut hits: Vec<Hit> = Vec::new();
    // @team-lead Wave-2 port-fidelity fix 2026-05-21: skip inline
    // `#[cfg(test)] mod tests {...}` blocks. (Bash today MISSES this for
    // Check 2 — its `find` loop bypasses `filter_test_code`; the Rust port
    // strictly fixes that bug — surface as Rust-bug-fix in the comparison.)
    let test_block_ranges = compute_test_block_ranges(&content);

    for (idx, line) in lines.iter().enumerate() {
        let line_no = idx + 1;
        if is_comment_line(line) {
            continue;
        }
        if is_line_in_test_block(&test_block_ranges, line_no) {
            continue;
        }
        if !INSTRUMENT_RE.is_match(line) {
            continue;
        }

        // Check 1: skip(...) without skip_all in the 3-line lookahead.
        if line.contains("skip(") && !skip_all_in_window(&lines, idx, 4) {
            hits.push(Hit {
                rule_id: SKIP_NOT_ALL_RULE_ID,
                pattern_name: "skip_denylist",
                file: path.to_path_buf(),
                line: line_no,
            });
        }

        // Check 2: sensitive params on the function signature without
        // skip_all in the same 4-line attribute window.
        if !skip_all_in_window(&lines, idx, 4) {
            if let Some(token) = sensitive_param_in_window(&lines, idx, 9) {
                hits.push(Hit {
                    rule_id: SENSITIVE_PARAM_NO_SKIP_ALL_RULE_ID,
                    pattern_name: token,
                    file: path.to_path_buf(),
                    line: line_no,
                });
            }
        }
    }

    Ok(hits)
}

pub fn run(repo_root: &Path, explain: bool) -> Result<()> {
    let changed = get_all_changed_files(repo_root, Path::new("."), EXTENSIONS)
        .context("collecting changed Rust files")?;

    let in_scope: Vec<PathBuf> = changed
        .into_iter()
        .filter(|p| !is_excluded_path(p))
        .filter(|p| repo_root.join(p).is_file())
        .collect();

    if in_scope.is_empty() {
        emit_ok("rust-instrument-skip-all-no-files");
        return Ok(());
    }

    let mut all_hits: Vec<Hit> = Vec::new();
    for path in &in_scope {
        let hits = scan_file(repo_root, path)?;
        all_hits.extend(hits);
    }

    if all_hits.is_empty() {
        emit_ok(format!(
            "rust-instrument-skip-all-clean-{}-files",
            in_scope.len()
        ));
        return Ok(());
    }

    for hit in &all_hits {
        let file_disp = hit.file.display().to_string();
        if explain {
            let policy = format!("rust-instrument-skip-all::{}", hit.rule_id);
            print_secret_finding(&SecretFinding {
                file: &file_disp,
                row: hit.line,
                col: 0,
                policy: &policy,
                pattern_name: hit.pattern_name,
                extras: &[],
                src_file: file!(),
                src_line: line!(),
            });
        } else {
            println!(
                "VIOLATION: {}:{} [{}] {} (redacted)",
                file_disp, hit.line, hit.rule_id, hit.pattern_name
            );
        }
    }

    // Per @observability Q2: Check 1 (`SKIP_NOT_ALL_RULE_ID`) and Check 2
    // (`SENSITIVE_PARAM_NO_SKIP_ALL_RULE_ID`) get distinct REASON tokens so
    // operators can tell which heuristic tripped. Pick the more-common
    // class for the wire token; the per-rule findings appear in the
    // VIOLATION lines above.
    let check1_count = all_hits
        .iter()
        .filter(|h| h.rule_id == SKIP_NOT_ALL_RULE_ID)
        .count();
    let check2_count = all_hits
        .iter()
        .filter(|h| h.rule_id == SENSITIVE_PARAM_NO_SKIP_ALL_RULE_ID)
        .count();
    if check1_count >= check2_count {
        anyhow::bail!(
            "rust-instrument-skip-not-all-{check1_count}-of-{}-files",
            in_scope.len()
        )
    } else {
        anyhow::bail!(
            "rust-instrument-sensitive-param-no-skip-all-{check2_count}-of-{}-files",
            in_scope.len()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instrument_re_recognizes_attribute() {
        assert!(INSTRUMENT_RE.is_match("    #[instrument(skip(self))]"));
        assert!(INSTRUMENT_RE.is_match("#[instrument]"));
        // Comment lines are filtered upstream by `is_comment_line`, not by
        // the regex itself — the regex is intentionally loose so it catches
        // both bare and parameterized forms.
        assert!(is_comment_line("// #[instrument]"));
    }

    #[test]
    fn skip_all_in_window_walks_lookahead() {
        let lines = vec![
            "#[instrument(",
            "    level = \"debug\",",
            "    skip_all,",
            "    fields(x = %x)",
            ")]",
        ];
        assert!(skip_all_in_window(&lines, 0, 5));
        // No-skip_all variant.
        let lines2 = vec!["#[instrument(skip(x))]"];
        assert!(!skip_all_in_window(&lines2, 0, 4));
    }

    #[test]
    fn sensitive_param_in_window_finds_category_a() {
        let lines = vec![
            "#[instrument]",
            "fn f(password: String, x: u32) -> R {",
            "    todo!()",
            "}",
        ];
        assert_eq!(sensitive_param_in_window(&lines, 0, 4), Some("password"));
    }

    #[test]
    fn sensitive_param_in_window_recognizes_wave2_auth_code() {
        // Wave-2 F3 addition.
        let lines = vec!["#[instrument]", "fn cb(auth_code: String) {}"];
        assert_eq!(sensitive_param_in_window(&lines, 0, 4), Some("auth_code"));
    }

    #[test]
    fn scan_detects_skip_without_skip_all() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src.rs");
        std::fs::write(
            &src,
            "#[instrument(skip(self, password))]\nfn handle(&self, password: String) {}\n",
        )
        .unwrap();
        let hits = scan_file(dir.path(), Path::new("src.rs")).unwrap();
        assert!(hits.iter().any(|h| h.rule_id == SKIP_NOT_ALL_RULE_ID));
    }

    #[test]
    fn scan_passes_skip_all_form() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src.rs");
        std::fs::write(
            &src,
            "#[instrument(skip_all, fields(user_id = %user_id))]\nfn handle(password: String) {}\n",
        )
        .unwrap();
        let hits = scan_file(dir.path(), Path::new("src.rs")).unwrap();
        // Should NOT fire — skip_all is the allowlist form.
        assert!(hits.is_empty(), "got hits: {hits:?}");
    }

    #[test]
    fn scan_detects_sensitive_param_without_skip_all() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src.rs");
        std::fs::write(
            &src,
            "#[instrument]\nfn handle(password: String) -> R { todo!() }\n",
        )
        .unwrap();
        let hits = scan_file(dir.path(), Path::new("src.rs")).unwrap();
        assert!(hits
            .iter()
            .any(|h| h.rule_id == SENSITIVE_PARAM_NO_SKIP_ALL_RULE_ID));
    }
}
