//! `ts-no-pii-in-logs` subcommand — port of
//! `scripts/guards/simple/ts/no-pii-in-logs-ts.sh`.
//!
//! Two BLOCKING checks + one WARNING (non-blocking) check against
//! `.ts`/`.svelte` log-sink call sites: `console.*` and `logger.*` /
//! OpenTelemetry `logger.emit()`.
//!
//! Per @observability Q1 + @code-reviewer item 2: vocabulary comes from
//! [`crate::common::pii_vocabulary::PII_TOKENS_CATEGORY_B`] — single SoT
//! across `metric_labels`, `rust_pii`, and this module. Wave-2 strictly
//! broadens vs bash today's narrower `PII_PATTERNS`.
//!
//! Per @semantic-guard Q1: findings emit via
//! [`crate::common::explain::print_secret_finding`] (no `matched=`; the
//! matched span CAN be PII bytes from source).

use crate::common::explain::{print_secret_finding, SecretFinding};
use crate::common::git_changes::get_all_changed_files;
use crate::common::pii_vocabulary::PII_TOKENS_CATEGORY_B;
use crate::common::status::emit_ok;
use crate::common::test_code_filter::is_scan_exempt;
use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use std::path::{Path, PathBuf};

pub const PII_IN_LOG_SINK_RULE_ID: &str = "pii_in_log_sink";
pub const PII_IN_STRUCTURED_OBJECT_RULE_ID: &str = "pii_in_structured_object";
pub const PII_IN_ERROR_MSG_RULE_ID: &str = "pii_in_error_message";

const EXTENSIONS: &[&str] = &[".ts", ".svelte"];

/// Path substrings that exempt a file (test/build artifacts). Wave-2 plan
/// preserves the bash inline-per-module shape per dry-reviewer threshold;
/// 4th true-rhyming caller would extract.
const TEST_PATH_PATTERNS: &[&str] = &[
    "/node_modules/",
    "/dist/",
    "/build/",
    "/.svelte-kit/",
    "/coverage/",
    "/__tests__/",
    "/test-utils/",
    "/fixtures/",
];

const TEST_FILE_SUFFIXES: &[&str] = &[".d.ts", ".test.ts", ".spec.ts", ".test.tsx", ".spec.tsx"];

// `console.*` / `logger.*` / `log.*` invocation opener.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static LOG_SINK_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b(console|logger|log)\.(log|info|warn|error|debug|trace|emit)\s*\(")
        .expect("static pattern compiles")
});

// `throw new Error(...)` / `new Error(...)` / `Error(...)`.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static ERROR_CTOR_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(throw\s+new\s+Error|new\s+Error|Error\s*\()").expect("static pattern compiles")
});

// PII identifier alternation, regex-anchored on word boundaries.
//
// Built once from PII_TOKENS_CATEGORY_B. Per @code-reviewer item 2 the vocab
// is the canonical-home SoT — additions land there, not in this regex.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static PII_WORD_RE: Lazy<Regex> = Lazy::new(|| {
    let alternation = PII_TOKENS_CATEGORY_B.join("|");
    Regex::new(&format!(r"\b({alternation})\b")).expect("vocab alternation compiles")
});

/// Object-literal-field shape for Check 2: `{ ... <pii_token>: ... }`. Bash
/// today (`no-pii-in-logs-ts.sh:122`) requires the PII identifier `:`-suffixed
/// inside a `{...}` span, not anywhere on the line. Per @paired-client F-CLIENT-1
/// 2026-05-22: the prior loose `line.contains('{') && line.contains(':')` shape
/// double-counted inline-template-literal cases like
/// `logger.info(`User ${email}`, { context: 'foo' })`.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static PII_IN_OBJECT_FIELD_RE: Lazy<Regex> = Lazy::new(|| {
    let alternation = PII_TOKENS_CATEGORY_B.join("|");
    Regex::new(&format!(r"\{{[^}}]*\b({alternation})\s*:")).expect("vocab alternation compiles")
});

/// Returns `Some(matched_token)` if the line contains a PII identifier word.
fn pii_hit(line: &str) -> Option<&'static str> {
    let m = PII_WORD_RE.find(line)?;
    // Resolve back to the canonical-home const for the redacted descriptor.
    let s = m.as_str();
    PII_TOKENS_CATEGORY_B.iter().copied().find(|t| *t == s)
}

/// Bash's `filter_allowed` — discard lines where the hit is in a
/// `// pii-safe:` annotation, a REDACTED/masked/hashed literal, or
/// `_hash`/`Hash` shape.
fn is_allowed_line(line: &str) -> bool {
    line.contains("// pii-safe:")
        || line.contains("REDACTED")
        || line.contains("[REDACTED]")
        || line.contains("masked")
        || line.contains("hashed")
        || line.contains("_hash")
        || line.contains("Hash")
}

fn is_excluded_path(path: &Path) -> bool {
    if is_scan_exempt(path) {
        return true;
    }
    let Some(s) = path.to_str() else {
        return true;
    };
    if TEST_PATH_PATTERNS.iter().any(|pat| s.contains(pat)) {
        return true;
    }
    if TEST_FILE_SUFFIXES.iter().any(|suf| s.ends_with(suf)) {
        return true;
    }
    false
}

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

    for (idx, line) in content.lines().enumerate() {
        let line_no = idx + 1;
        if is_allowed_line(line) {
            continue;
        }
        let Some(token) = pii_hit(line) else {
            continue;
        };

        // Check 1 (BLOCKING) — PII identifier on the same line as a log-sink
        // call. Bash matches `LOG_SINK_REGEX` AND PII pattern on the line.
        if LOG_SINK_RE.is_match(line) {
            hits.push(Hit {
                rule_id: PII_IN_LOG_SINK_RULE_ID,
                pattern_name: token,
                file: path.to_path_buf(),
                line: line_no,
                is_warning: false,
            });

            // Check 2 (BLOCKING) — PII field in a structured object inside
            // the log call. Matches bash `\{[^}]*\b(pii_token)\s*:` shape:
            // the PII identifier MUST be `:`-suffixed inside a `{...}` span,
            // not just anywhere on the line.
            if PII_IN_OBJECT_FIELD_RE.is_match(line) {
                hits.push(Hit {
                    rule_id: PII_IN_STRUCTURED_OBJECT_RULE_ID,
                    pattern_name: token,
                    file: path.to_path_buf(),
                    line: line_no,
                    is_warning: false,
                });
            }
        }

        // Check 3 (WARNING, non-blocking) — PII in error messages.
        if ERROR_CTOR_RE.is_match(line) {
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

/// Entry point for the `ts-no-pii-in-logs` subcommand.
pub fn run(repo_root: &Path, explain: bool) -> Result<()> {
    let changed = get_all_changed_files(repo_root, Path::new("."), EXTENSIONS)
        .context("collecting changed TS/Svelte files")?;

    let in_scope: Vec<PathBuf> = changed
        .into_iter()
        .filter(|p| !is_excluded_path(p))
        .filter(|p| repo_root.join(p).is_file())
        .collect();

    if in_scope.is_empty() {
        emit_ok("ts-no-pii-in-logs-no-files");
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
            let policy = format!("ts-no-pii-in-logs::{}", hit.rule_id);
            let kind_str = if hit.is_warning {
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
                extras: &[("kind", kind_str)],
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
        emit_ok(format!("ts-no-pii-in-logs-clean-{}-files", in_scope.len()));
        return Ok(());
    }

    anyhow::bail!(
        "ts-pii-violation-found-{blocking_count}-of-{}-files",
        in_scope.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_sink_matches_console_and_logger() {
        assert!(LOG_SINK_RE.is_match("console.log(email)"));
        assert!(LOG_SINK_RE.is_match("logger.info({ phone: x })"));
        assert!(LOG_SINK_RE.is_match("log.warn('user_agent = '+ ua)"));
        // emit() form (OpenTelemetry).
        assert!(LOG_SINK_RE.is_match("logger.emit({ body: x })"));
    }

    #[test]
    fn pii_hit_recognizes_category_b_tokens() {
        assert_eq!(pii_hit("user has email = foo@bar"), Some("email"));
        assert_eq!(pii_hit("the user_agent string"), Some("user_agent"));
        assert!(pii_hit("just regular code").is_none());
    }

    #[test]
    fn allowed_line_skips_pii_safe_annotation() {
        assert!(is_allowed_line(
            "logger.info(email) // pii-safe: hashed user-id"
        ));
        assert!(is_allowed_line("logger.info({ ip_addr: '[REDACTED]' })"));
        assert!(!is_allowed_line("logger.info({ email: actualEmail })"));
    }

    #[test]
    fn structured_object_check_matches_object_literal_field() {
        // PII identifier `:`-suffixed inside `{...}` — fires.
        assert!(PII_IN_OBJECT_FIELD_RE.is_match("logger.info({ email: actualEmail })"));
        assert!(PII_IN_OBJECT_FIELD_RE.is_match("log.warn({ ip_addr: addr, foo: 1 })"));
    }

    #[test]
    fn structured_object_check_skips_bare_log_call() {
        // No object literal: NO Check-2 hit.
        assert!(!PII_IN_OBJECT_FIELD_RE.is_match("logger.info(email)"));
    }

    #[test]
    fn structured_object_check_skips_inline_template_with_unrelated_object() {
        // Per @paired-client F-CLIENT-1 2026-05-22: inline template literal
        // mentioning a PII token + an unrelated object literal on the same
        // line must NOT double-count. Bash today fires Check 1 once (inline
        // `email`) and does NOT fire Check 2 (no `email:` inside the brace).
        let line = "logger.info(`User ${email}`, { context: 'foo' });";
        // Check 1 word matcher still finds the inline PII token.
        assert_eq!(pii_hit(line), Some("email"));
        // Check 2 object-field shape does NOT match — `email` is not `:`-suffixed
        // inside the `{...}` span; only `context: 'foo'` is.
        assert!(!PII_IN_OBJECT_FIELD_RE.is_match(line));
    }

    #[test]
    fn structured_object_check_matches_when_pii_token_is_field_inside_object() {
        // Spans with `{...}` containing the PII token followed by `:` — must
        // match. Adjacent non-PII fields in the same brace span must not block.
        let line = "logger.info({ context: 'foo', email: actual })";
        assert!(PII_IN_OBJECT_FIELD_RE.is_match(line));
    }

    #[test]
    fn excluded_path_drops_test_artifacts() {
        assert!(is_excluded_path(Path::new("packages/x/__tests__/foo.ts")));
        assert!(is_excluded_path(Path::new("packages/x/dist/x.ts")));
        assert!(!is_excluded_path(Path::new("packages/x/src/index.ts")));
    }

    #[test]
    fn wave2_pii_additions_are_recognized() {
        // Wave-2 additions to CATEGORY_B per @observability Q1.
        for tok in &["full_name", "first_name", "ipv4", "ssn"] {
            assert_eq!(pii_hit(&format!("user.{tok} = x")), Some(*tok));
        }
    }
}
