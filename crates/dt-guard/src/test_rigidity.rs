//! `test-rigidity` subcommand — port of
//! `scripts/guards/simple/test-rigidity.sh`.
//!
//! Detects patterns in `crates/env-tests/tests/*.rs` that accept failures,
//! unavailability, or error conditions as "passing" — false confidence that
//! features work. Six check classes:
//!
//! 1. Early `return;` after a service-availability check / SKIPPED/Warning
//!    message (within 4 lines).
//! 2. Standalone `"Warning"`/`"WARNING"` strings used as assertions (no
//!    preceding `assert!`/`panic!` within 15 lines).
//! 3. Aspirational-non-enforcement strings in executable code (NOT comments).
//! 4. Multi-status acceptance in assertions (`==NNN || ==MMM`).
//! 5. Assertion-free match arms — split into 5a (numeric `NNN =>`),
//!    5b (`Ok(...) =>`), 5c (`Err(...status: NNN...) =>`).
//! 6. Placeholder `unimplemented!()` stubs (unless preceded by `#[ignore`).

use crate::common::explain::{print_finding, Finding};
use crate::common::status::emit_ok;
use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use std::path::{Path, PathBuf};

pub const EARLY_RETURN_RULE_ID: &str = "early_return";
pub const WARNING_AS_ASSERT_RULE_ID: &str = "warning_as_assertion";
pub const ASPIRATIONAL_RULE_ID: &str = "aspirational_non_enforcement";
pub const MULTI_STATUS_RULE_ID: &str = "multi_status_acceptance";
pub const ASSERT_FREE_ARM_RULE_ID: &str = "assertion_free_match_arm";
pub const PLACEHOLDER_STUB_RULE_ID: &str = "placeholder_stub";

const ENV_TESTS_TESTS_SUBDIR: &str = "crates/env-tests/tests";

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static ASPIRATIONAL_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(Don.?t fail|aspirational|future enhancement|not a hard failure)")
        .expect("static pattern compiles")
});

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static MULTI_STATUS_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"==\s*[0-9]{3}.*\|\|").expect("static pattern compiles"));

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static NUMERIC_ARM_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*[0-9]{3}\s*=>\s*\{").expect("static pattern compiles"));

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static OK_ARM_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*Ok\(.*\)\s*=>\s*\{").expect("static pattern compiles"));

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static ERR_STATUS_ARM_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*Err\(.*status:\s*[0-9]{3}.*\)\s*=>\s*\{").expect("static pattern compiles")
});

#[derive(Debug)]
struct Hit {
    rule_id: &'static str,
    detail: String,
    file: PathBuf,
    line: usize,
}

fn is_comment_line(line: &str) -> bool {
    let stripped = line.trim_start();
    stripped.starts_with("//") || stripped.starts_with("/*") || stripped.starts_with("*")
}

/// True if the line is a bare `return;` (whitespace + `return;`).
fn is_bare_return(line: &str) -> bool {
    line.trim() == "return;"
}

/// Brace-depth walk from `start_line` (0-based) until the arm's outer `}`
/// closes (depth ≤ 0). Returns whether any `assert!`/`panic!` was seen.
/// Safety bound: 30 lines per arm.
#[expect(
    clippy::indexing_slicing,
    reason = "`start_line` is always an index from `lines.iter().enumerate()` upstream"
)]
fn arm_has_enforcement(lines: &[&str], start_line: usize) -> bool {
    let mut depth: i32 = 0;
    let mut has_enforcement = false;
    let mut line_count = 0usize;
    for line in &lines[start_line..] {
        for c in line.chars() {
            match c {
                '{' => depth += 1,
                '}' => depth -= 1,
                _ => {}
            }
        }
        if line.contains("assert") || line.contains("panic!") {
            has_enforcement = true;
        }
        line_count += 1;
        if depth <= 0 || line_count > 30 {
            break;
        }
    }
    has_enforcement
}

#[expect(
    clippy::indexing_slicing,
    reason = "every `lines[from..idx]` / `lines[idx..to]` has `from <= idx <= to <= lines.len()` by construction (saturating_sub + min)"
)]
fn scan_file(path: &Path, rel_path: &Path) -> Result<Vec<Hit>> {
    let content =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let lines: Vec<&str> = content.lines().collect();
    let mut hits: Vec<Hit> = Vec::new();

    // Track recent markers for Check 1.
    for (idx, raw_line) in lines.iter().enumerate() {
        let line_no = idx + 1;

        // --- Check 1: early return after availability check / skip/warning ---
        // Per @test F2 2026-05-23: ONE Hit per line. Both lookbacks update a
        // single `triggered` accumulator; bash semantics = last-write-wins, so
        // the SKIPPED/Warning sweep (within 3 lines) runs AFTER the
        // availability-check sweep (within 4 lines) and overwrites when both
        // match the same `return;`.
        if is_bare_return(raw_line) {
            let mut triggered: Option<&'static str> = None;
            // Availability-check trigger (lookback 4).
            let from = idx.saturating_sub(4);
            for prior in &lines[from..idx] {
                if prior.contains("is_") && prior.contains("_available") {
                    triggered = Some("return; after service-availability check");
                    break;
                }
            }
            // Skip/Warning marker trigger (lookback 3). Last-write-wins.
            let from3 = idx.saturating_sub(3);
            for prior in &lines[from3..idx] {
                if prior.contains("println!(\"SKIPPED")
                    || prior.contains("eprintln!(\"Warning")
                    || prior.contains("eprintln!(\"Skipping")
                {
                    triggered = Some("return; after SKIPPED/Warning/Skipping message");
                    break;
                }
            }
            if let Some(detail) = triggered {
                hits.push(Hit {
                    rule_id: EARLY_RETURN_RULE_ID,
                    detail: detail.to_string(),
                    file: rel_path.to_path_buf(),
                    line: line_no,
                });
            }
        }

        // --- Check 2: standalone "Warning"/"WARNING" string used as assertion ---
        if !is_comment_line(raw_line) && raw_line.contains("\"Warning") {
            // Skip if return; appears within 5 lines after (Check 1 territory).
            let to = (idx + 5).min(lines.len());
            let nearby_return = lines[idx..to].iter().any(|l| l.contains("return;"));
            if nearby_return {
                continue;
            }
            // Skip if assert!/panic! within 15 lines before.
            let from = idx.saturating_sub(15);
            let preceding_enforcement = lines[from..idx]
                .iter()
                .any(|l| l.contains("assert") || l.contains("panic!"));
            if preceding_enforcement {
                continue;
            }
            hits.push(Hit {
                rule_id: WARNING_AS_ASSERT_RULE_ID,
                detail: "\"Warning\" string used in executable code (no preceding assert)"
                    .to_string(),
                file: rel_path.to_path_buf(),
                line: line_no,
            });
        }

        // --- Check 3: aspirational-non-enforcement string in executable code ---
        if !is_comment_line(raw_line) && ASPIRATIONAL_RE.is_match(raw_line) {
            hits.push(Hit {
                rule_id: ASPIRATIONAL_RULE_ID,
                detail: "aspirational non-enforcement string in executable code".to_string(),
                file: rel_path.to_path_buf(),
                line: line_no,
            });
        }

        // --- Check 4: multi-status acceptance in assertions ---
        if !is_comment_line(raw_line) && MULTI_STATUS_RE.is_match(raw_line) {
            hits.push(Hit {
                rule_id: MULTI_STATUS_RULE_ID,
                detail: "multi-status `== NNN || ...` acceptance".to_string(),
                file: rel_path.to_path_buf(),
                line: line_no,
            });
        }

        // --- Check 5a/5b/5c: assertion-free match arms ---
        let (is_arm, kind) = if NUMERIC_ARM_RE.is_match(raw_line) {
            (true, "5a_numeric")
        } else if OK_ARM_RE.is_match(raw_line) {
            (true, "5b_ok")
        } else if ERR_STATUS_ARM_RE.is_match(raw_line) {
            (true, "5c_err_status")
        } else {
            (false, "")
        };
        if is_arm && !arm_has_enforcement(&lines, idx) {
            hits.push(Hit {
                rule_id: ASSERT_FREE_ARM_RULE_ID,
                detail: format!("assertion-free match arm ({kind})"),
                file: rel_path.to_path_buf(),
                line: line_no,
            });
        }

        // --- Check 6: placeholder unimplemented!() stubs ---
        if raw_line.contains("unimplemented!") && !is_comment_line(raw_line) {
            // Skip if #[ignore appears within 15 lines before.
            let from = idx.saturating_sub(15);
            let preceding_ignore = lines[from..idx].iter().any(|l| l.contains("#[ignore"));
            if preceding_ignore {
                continue;
            }
            hits.push(Hit {
                rule_id: PLACEHOLDER_STUB_RULE_ID,
                detail: "unimplemented!() stub without #[ignore]".to_string(),
                file: rel_path.to_path_buf(),
                line: line_no,
            });
        }
    }

    Ok(hits)
}

pub fn run(repo_root: &Path, explain: bool) -> Result<()> {
    let tests_dir = repo_root.join(ENV_TESTS_TESTS_SUBDIR);
    if !tests_dir.is_dir() {
        emit_ok("test-rigidity-no-env-tests-dir");
        return Ok(());
    }

    let mut test_files: Vec<PathBuf> = Vec::new();
    for entry in std::fs::read_dir(&tests_dir)
        .with_context(|| format!("reading {}", tests_dir.display()))?
        .flatten()
    {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) != Some("rs") {
            continue;
        }
        test_files.push(path);
    }
    test_files.sort();

    if test_files.is_empty() {
        emit_ok("test-rigidity-no-test-files");
        return Ok(());
    }

    let mut all_hits: Vec<Hit> = Vec::new();
    for path in &test_files {
        let rel = path
            .strip_prefix(repo_root)
            .unwrap_or(path.as_path())
            .to_path_buf();
        all_hits.extend(scan_file(path, &rel)?);
    }

    if all_hits.is_empty() {
        emit_ok(format!("test-rigidity-clean-{}-files", test_files.len()));
        return Ok(());
    }

    for hit in &all_hits {
        let file_disp = hit.file.display().to_string();
        if explain {
            let policy = format!("test-rigidity::{}", hit.rule_id);
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

    anyhow::bail!("test-rigidity-violation-found-{}", all_hits.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, content: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
        path
    }

    fn scan_inline(content: &str) -> Vec<Hit> {
        let dir = tempfile::tempdir().unwrap();
        let path = write(dir.path(), "t.rs", content);
        scan_file(&path, Path::new("t.rs")).unwrap()
    }

    #[test]
    fn check1_early_return_after_availability_check() {
        let src = "fn t() {\n    if !is_grpc_available() {\n        return;\n    }\n}\n";
        let hits = scan_inline(src);
        assert!(hits.iter().any(|h| h.rule_id == EARLY_RETURN_RULE_ID));
    }

    #[test]
    fn check1_early_return_after_skipped_message() {
        let src = "fn t() {\n    println!(\"SKIPPED: no auth\");\n    return;\n}\n";
        let hits = scan_inline(src);
        assert!(hits.iter().any(|h| h.rule_id == EARLY_RETURN_RULE_ID));
    }

    #[test]
    fn check2_warning_as_assertion() {
        // Standalone Warning string with no preceding assertion.
        let src = "fn t() {\n    eprintln!(\"Warning: thing failed\");\n}\n";
        let hits = scan_inline(src);
        assert!(hits.iter().any(|h| h.rule_id == WARNING_AS_ASSERT_RULE_ID));
    }

    #[test]
    fn check2_warning_skipped_when_assert_precedes() {
        let src = "fn t() {\n    assert_eq!(1, 1);\n    eprintln!(\"Warning: cleanup\");\n}\n";
        let hits = scan_inline(src);
        assert!(!hits.iter().any(|h| h.rule_id == WARNING_AS_ASSERT_RULE_ID));
    }

    #[test]
    fn check3_aspirational_in_executable_code() {
        let src = "fn t() {\n    let msg = \"This is aspirational\";\n}\n";
        let hits = scan_inline(src);
        assert!(hits.iter().any(|h| h.rule_id == ASPIRATIONAL_RULE_ID));
    }

    #[test]
    fn check3_aspirational_skipped_in_comment() {
        let src = "fn t() {\n    // aspirational hint\n}\n";
        let hits = scan_inline(src);
        assert!(!hits.iter().any(|h| h.rule_id == ASPIRATIONAL_RULE_ID));
    }

    #[test]
    fn check4_multi_status_acceptance() {
        let src = "fn t() {\n    assert!(s == 401 || s == 403);\n}\n";
        let hits = scan_inline(src);
        assert!(hits.iter().any(|h| h.rule_id == MULTI_STATUS_RULE_ID));
    }

    #[test]
    fn check5a_numeric_arm_no_assert() {
        let src = "match s {\n    404 => {\n        log(\"not found\");\n    }\n    _ => {}\n}\n";
        let hits = scan_inline(src);
        assert!(hits
            .iter()
            .any(|h| h.rule_id == ASSERT_FREE_ARM_RULE_ID && h.detail.contains("5a")));
    }

    #[test]
    fn check5b_ok_arm_no_assert() {
        let src = "match r {\n    Ok(x) => {\n        log(\"got\");\n    }\n    Err(_) => {}\n}\n";
        let hits = scan_inline(src);
        assert!(hits
            .iter()
            .any(|h| h.rule_id == ASSERT_FREE_ARM_RULE_ID && h.detail.contains("5b")));
    }

    #[test]
    fn check5c_err_status_arm_no_assert() {
        let src = "match r {\n    Err(MyErr { status: 401, .. }) => {\n        log(\"unauth\");\n    }\n    _ => {}\n}\n";
        let hits = scan_inline(src);
        assert!(hits
            .iter()
            .any(|h| h.rule_id == ASSERT_FREE_ARM_RULE_ID && h.detail.contains("5c")));
    }

    #[test]
    fn check5_arm_with_assert_passes() {
        let src = "match s {\n    404 => {\n        assert_eq!(s, 404);\n    }\n}\n";
        let hits = scan_inline(src);
        assert!(!hits.iter().any(|h| h.rule_id == ASSERT_FREE_ARM_RULE_ID));
    }

    #[test]
    fn check6_unimplemented_without_ignore() {
        let src = "#[test]\nfn t() {\n    unimplemented!()\n}\n";
        let hits = scan_inline(src);
        assert!(hits.iter().any(|h| h.rule_id == PLACEHOLDER_STUB_RULE_ID));
    }

    #[test]
    fn check6_unimplemented_with_ignore_skipped() {
        let src = "#[ignore = \"WIP\"]\n#[test]\nfn t() {\n    unimplemented!()\n}\n";
        let hits = scan_inline(src);
        assert!(!hits.iter().any(|h| h.rule_id == PLACEHOLDER_STUB_RULE_ID));
    }

    #[test]
    fn brace_depth_30_line_safety_cap() {
        // Construct an arm whose body is >30 lines and contains an assert
        // beyond the cap — the arm should be classified as assertion-free
        // (safety bound: assert beyond 30 lines doesn't count).
        let mut src = String::from("match s {\n    200 => {\n");
        for _ in 0..40 {
            src.push_str("        let _ = 1;\n");
        }
        src.push_str("        assert!(true);\n");
        src.push_str("    }\n}\n");
        let hits = scan_inline(&src);
        assert!(hits.iter().any(|h| h.rule_id == ASSERT_FREE_ARM_RULE_ID));
    }
}
