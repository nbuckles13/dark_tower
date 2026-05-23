//! `rust-no-pii-in-logs` subcommand — port of
//! `scripts/guards/simple/no-pii-in-logs.sh`.
//!
//! Detects PII in tracing/log call sites. Four checks:
//! 1. PII identifier in `info!/debug!/warn!/error!/trace!` macro args.
//! 2. PII in named tracing fields (`email = %x` shape).
//! 3. PII in `#[instrument(fields(...))]` without `skip(...)`.
//! 4. PII in error/anyhow messages (WARN-only).
//!
//! **Per @security F1 + @observability Q1 + @code-reviewer item 2**: PII
//! vocabulary comes from [`crate::common::pii_vocabulary::PII_TOKENS_CATEGORY_B`]
//! — single SoT. Wave-2 strictly broadens vs bash today's narrower set.
//!
//! Per @semantic-guard Q1 — findings emit via [`print_secret_finding`] (no
//! `matched=`; matched span CAN be PII bytes from source).

use crate::common::explain::{print_secret_finding, SecretFinding};
use crate::common::git_changes::get_all_changed_files;
use crate::common::pii_vocabulary::PII_TOKENS_CATEGORY_B;
use crate::common::status::emit_ok;
use crate::common::test_code_filter::{
    compute_test_block_ranges, is_line_in_test_block, is_scan_exempt,
};
use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use std::path::{Path, PathBuf};

pub const PII_IN_LOG_MACRO_RULE_ID: &str = "pii_in_log_macro";
pub const PII_IN_NAMED_FIELD_RULE_ID: &str = "pii_in_named_field";
pub const PII_IN_INSTRUMENT_RULE_ID: &str = "pii_in_instrument";
pub const PII_IN_ERROR_MSG_RULE_ID: &str = "pii_in_error_message";

const EXTENSIONS: &[&str] = &[".rs"];

// Path-shape exclusions live in `common::test_code_filter::is_test_path` per
// @team-lead Wave-2 port-fidelity fix 2026-05-21.

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static LOG_MACRO_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b(info|debug|warn|error|trace)!\s*\(").expect("static pattern compiles")
});

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static TRACING_NAMED_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"tracing::(info|debug|warn|error|trace)!\s*\(|#\[instrument")
        .expect("static pattern compiles")
});

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
static ERROR_CTOR_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(Err\(|anyhow!|bail!|context\()").expect("static pattern compiles"));

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static PII_WORD_RE: Lazy<Regex> = Lazy::new(|| {
    let alternation = PII_TOKENS_CATEGORY_B.join("|");
    Regex::new(&format!(r"\b({alternation})\b")).expect("vocab alternation compiles")
});

fn pii_hit(line: &str) -> Option<&'static str> {
    let m = PII_WORD_RE.find(line)?;
    let s = m.as_str();
    PII_TOKENS_CATEGORY_B.iter().copied().find(|t| *t == s)
}

fn is_allowed_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    line.contains("REDACTED")
        || line.contains("[REDACTED]")
        || line.contains("masked")
        || line.contains("hashed")
        || line.contains("h:")
        || lower.contains("// pii-safe:")
        // tracing skip(...) — masks PII params explicitly
        || (line.contains("#[instrument") && line.contains("skip"))
}

fn is_excluded_path(path: &Path) -> bool {
    is_scan_exempt(path)
}

fn is_comment_line(line: &str) -> bool {
    let stripped = line.trim_start();
    stripped.starts_with("//") || stripped.starts_with("/*") || stripped.starts_with("*")
}

#[derive(Debug)]
struct Hit {
    rule_id: &'static str,
    pattern_name: &'static str,
    file: PathBuf,
    line: usize,
    is_warning: bool,
}

fn scan_file(repo_root: &Path, path: &Path) -> Result<Vec<Hit>> {
    let abs = repo_root.join(path);
    let content =
        std::fs::read_to_string(&abs).with_context(|| format!("reading {}", abs.display()))?;
    let mut hits: Vec<Hit> = Vec::new();
    // @team-lead Wave-2 port-fidelity fix 2026-05-21: skip inline
    // `#[cfg(test)] mod tests {...}` blocks.
    let test_block_ranges = compute_test_block_ranges(&content);

    for (idx, line) in content.lines().enumerate() {
        let line_no = idx + 1;
        if is_comment_line(line) || is_allowed_line(line) {
            continue;
        }
        if is_line_in_test_block(&test_block_ranges, line_no) {
            continue;
        }
        let Some(token) = pii_hit(line) else {
            continue;
        };

        // Check 1: PII identifier in log macro.
        if LOG_MACRO_RE.is_match(line) {
            hits.push(Hit {
                rule_id: PII_IN_LOG_MACRO_RULE_ID,
                pattern_name: token,
                file: path.to_path_buf(),
                line: line_no,
                is_warning: false,
            });
        }

        // Check 2: PII in named tracing field (`email = %x` shape).
        if TRACING_NAMED_RE.is_match(line) && line.contains('=') {
            hits.push(Hit {
                rule_id: PII_IN_NAMED_FIELD_RULE_ID,
                pattern_name: token,
                file: path.to_path_buf(),
                line: line_no,
                is_warning: false,
            });
        }

        // Check 3: PII inside `#[instrument(fields(...))]` without `skip(...)`.
        if INSTRUMENT_RE.is_match(line) && line.contains("fields(") && !line.contains("skip(") {
            hits.push(Hit {
                rule_id: PII_IN_INSTRUMENT_RULE_ID,
                pattern_name: token,
                file: path.to_path_buf(),
                line: line_no,
                is_warning: false,
            });
        }

        // Check 4: PII in error/anyhow message (WARN).
        if ERROR_CTOR_RE.is_match(line) && line.contains('{') {
            hits.push(Hit {
                rule_id: PII_IN_ERROR_MSG_RULE_ID,
                pattern_name: token,
                file: path.to_path_buf(),
                line: line_no,
                is_warning: true,
            });
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
        emit_ok("rust-no-pii-in-logs-no-files");
        return Ok(());
    }

    let mut all_hits: Vec<Hit> = Vec::new();
    for path in &in_scope {
        let hits = scan_file(repo_root, path)?;
        all_hits.extend(hits);
    }

    let blocking_count = all_hits.iter().filter(|h| !h.is_warning).count();

    for hit in &all_hits {
        let file_disp = hit.file.display().to_string();
        if explain {
            let policy = format!("rust-no-pii-in-logs::{}", hit.rule_id);
            let kind = if hit.is_warning {
                "warning"
            } else {
                "violation"
            };
            print_secret_finding(&SecretFinding {
                file: &file_disp,
                row: hit.line,
                col: 0,
                policy: &policy,
                pattern_name: hit.pattern_name,
                extras: &[("kind", kind)],
                src_file: file!(),
                src_line: line!(),
            });
        } else {
            let prefix = if hit.is_warning {
                "WARNING"
            } else {
                "VIOLATION"
            };
            println!(
                "{}: {}:{} [{}] suspected PII identifier {} (redacted)",
                prefix, file_disp, hit.line, hit.rule_id, hit.pattern_name
            );
        }
    }

    if blocking_count == 0 {
        emit_ok(format!(
            "rust-no-pii-in-logs-clean-{}-files",
            in_scope.len()
        ));
        return Ok(());
    }

    anyhow::bail!(
        "rust-pii-violation-found-{blocking_count}-of-{}-files",
        in_scope.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pii_hit_recognizes_category_b_tokens() {
        assert_eq!(pii_hit("let email = x;"), Some("email"));
        assert_eq!(pii_hit("self.ip_addr"), Some("ip_addr"));
        assert!(pii_hit("regular code").is_none());
    }

    #[test]
    fn wave2_pii_additions_recognized() {
        // Wave-2 widening per @observability Q1: tokens absent from
        // bash today's PII_PATTERNS but present in CATEGORY_B.
        for tok in &["ssn", "passport", "ipv4", "device_id", "credit_card"] {
            assert_eq!(pii_hit(&format!("let x = {tok};")), Some(*tok));
        }
    }

    #[test]
    fn log_macro_matches_canonical_openers() {
        assert!(LOG_MACRO_RE.is_match(r#"info!("user = {}", email);"#));
        assert!(LOG_MACRO_RE.is_match("debug!(target: \"x\", phone)"));
        assert!(LOG_MACRO_RE.is_match("error!(?email)"));
    }

    #[test]
    fn allowed_line_skips_redacted_and_skip() {
        assert!(is_allowed_line(
            "info!(\"e = {}\", email);  // pii-safe: hashed"
        ));
        assert!(is_allowed_line("info!(\"e = [REDACTED]\")"));
        assert!(is_allowed_line(
            "#[instrument(skip(email), fields(user_id = %id))]"
        ));
        assert!(!is_allowed_line("info!(\"email = {}\", email);"));
    }

    #[test]
    fn excluded_path_drops_tests_and_vendor() {
        assert!(is_excluded_path(Path::new(
            "crates/foo/tests/integration.rs"
        )));
        assert!(is_excluded_path(Path::new("vendor/x/lib.rs")));
        assert!(!is_excluded_path(Path::new("crates/foo/src/lib.rs")));
    }
}
