//! `rust-no-hardcoded-secrets` subcommand — port of
//! `scripts/guards/simple/no-hardcoded-secrets.sh`.
//!
//! Detects hardcoded secrets in changed `.rs` source. Five check classes:
//!
//! 1. Secret-identifier literal assignments (`password = "..."` etc.).
//! 2. **API-key prefixes** — consumes the canonical
//!    [`crate::secret_patterns::HYGIENE_PATTERNS`] catalog (Wave-2 closes
//!    the cross-stack dupe per ADR-0034 §6). NOTE: the Wave-2 consumption
//!    widens vs bash today (Slack/JWT/PEM/internal-DNS/prod-stage hostname);
//!    @security signed off per F1.
//! 3. Connection-string credentials.
//! 4. Authorization header literals.
//! 5. Long base64 strings (WARN-only).
//!
//! Per @semantic-guard Q1 — findings emit via
//! [`crate::common::explain::print_secret_finding`] (no `matched=` field;
//! `pattern=<name>` only). The matched bytes ARE the secret; redacted by
//! construction.

use crate::common::explain::{print_secret_finding, SecretFinding};
use crate::common::git_changes::get_all_changed_files;
use crate::common::status::emit_ok;
use crate::common::test_code_filter::{
    compute_test_block_ranges, is_line_in_test_block, is_scan_exempt,
};
use crate::secret_patterns::source_scan_patterns;
use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use std::path::{Path, PathBuf};

pub const SECRET_ASSIGNMENT_RULE_ID: &str = "secret_assignment";
pub const HYGIENE_PATTERN_RULE_ID: &str = "hygiene_pattern";
pub const CONN_STRING_RULE_ID: &str = "conn_string_credentials";
pub const AUTH_HEADER_RULE_ID: &str = "authorization_header";

const EXTENSIONS: &[&str] = &[".rs"];

// Path-shape exclusions live in `common::test_code_filter::is_test_path` per
// @team-lead Wave-2 port-fidelity fix 2026-05-21.

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static SECRET_ASSIGNMENT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"(?i)(password|secret|token|api_key|credential|master_key|private_key|client_secret)\s*[=:]\s*"[^"]+"#,
    )
    .expect("static pattern compiles")
});

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static ENV_LOOKUP_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"std::env|env::var|dotenvy").expect("static pattern compiles"));

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static CONN_STRING_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#""(postgresql|mysql|redis|mongodb|amqp)://[^:]+:[^@{$]+@"#)
        .expect("static pattern compiles")
});

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static AUTH_HEADER_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?i)"(Authorization:\s*(Bearer|Basic)\s+[A-Za-z0-9+/=_.~\-]{20,})""#)
        .expect("static pattern compiles")
});

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
}

fn scan_file(repo_root: &Path, path: &Path) -> Result<Vec<Hit>> {
    let abs = repo_root.join(path);
    let content =
        std::fs::read_to_string(&abs).with_context(|| format!("reading {}", abs.display()))?;
    let mut hits: Vec<Hit> = Vec::new();

    // Per @team-lead 2026-05-21 port-fidelity fix: skip lines inside
    // `#[cfg(test)] mod tests {...}` blocks even in production-named files.
    // This catches the 92% FP case where the scanner module's own test
    // fixtures contained literal secret bytes.
    let test_block_ranges = compute_test_block_ranges(&content);

    for (idx, line) in content.lines().enumerate() {
        let line_no = idx + 1;
        if is_comment_line(line) {
            continue;
        }
        if is_line_in_test_block(&test_block_ranges, line_no) {
            continue;
        }

        // Check 1: secret-identifier literal assignment, excluding env-lookups.
        if SECRET_ASSIGNMENT_RE.is_match(line) && !ENV_LOOKUP_RE.is_match(line) {
            hits.push(Hit {
                rule_id: SECRET_ASSIGNMENT_RULE_ID,
                pattern_name: "secret_identifier_literal_assignment",
                file: path.to_path_buf(),
                line: line_no,
            });
        }

        // Check 2: HYGIENE source-scan subset (Class-V value-shape patterns
        // only). Closes ADR §6 cross-stack dupe via the shared catalog;
        // alert-rule-annotation-shaped Class-C patterns stay out per
        // `secret_patterns::HYGIENE_SOURCE_SCAN_SUBSET` rationale + the
        // in-tree FP survey @security ran 2026-05-22.
        for (name, regex) in source_scan_patterns() {
            if regex.is_match(line) {
                hits.push(Hit {
                    rule_id: HYGIENE_PATTERN_RULE_ID,
                    pattern_name: name,
                    file: path.to_path_buf(),
                    line: line_no,
                });
            }
        }

        // Check 3: connection-string credentials.
        if CONN_STRING_RE.is_match(line) {
            hits.push(Hit {
                rule_id: CONN_STRING_RULE_ID,
                pattern_name: "connection_string_credentials",
                file: path.to_path_buf(),
                line: line_no,
            });
        }

        // Check 4: Authorization header literals.
        if AUTH_HEADER_RE.is_match(line) {
            hits.push(Hit {
                rule_id: AUTH_HEADER_RULE_ID,
                pattern_name: "authorization_header_literal",
                file: path.to_path_buf(),
                line: line_no,
            });
        }

        // Check 5: long-base64 heuristic — DEFERRED to Wave 3. Same
        // residual-WARN behavior as bash today (no FAIL increment); the
        // heuristic is too noisy to gate without the syn-based test-code
        // line-range filter (Wave 3 Accepted Deferral).
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
        emit_ok("rust-no-hardcoded-secrets-no-files");
        return Ok(());
    }

    let mut all_hits: Vec<Hit> = Vec::new();
    for path in &in_scope {
        let hits = scan_file(repo_root, path)?;
        all_hits.extend(hits);
    }

    if all_hits.is_empty() {
        emit_ok(format!(
            "rust-no-hardcoded-secrets-clean-{}-files",
            in_scope.len()
        ));
        return Ok(());
    }

    for hit in &all_hits {
        let file_disp = hit.file.display().to_string();
        if explain {
            let policy = format!("rust-no-hardcoded-secrets::{}", hit.rule_id);
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
                "VIOLATION: {}:{} [{}] suspected {} (redacted)",
                file_disp, hit.line, hit.rule_id, hit.pattern_name
            );
        }
    }

    // Bail with a message that slugifies to the stable REASON token per the
    // Wave-1 pattern (main.rs catches the Err and calls `emit_fail(reason_token(&e))`).
    anyhow::bail!(
        "rust-secret-violation-found-{}-of-{}-files",
        all_hits.len(),
        in_scope.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secret_patterns::HYGIENE_PATTERNS;

    #[test]
    fn check1_matches_password_literal_in_rust() {
        // `\s*[=:]\s*"` requires the equals/colon to be immediately followed
        // (after whitespace) by `"`. Plain `let password = "x"` matches.
        assert!(SECRET_ASSIGNMENT_RE.is_match(r#"let password = "hunter2";"#));
        // Type-annotated forms (`const X: &str = "y"`) are NOT matched by
        // this regex because the `:` is followed by `&str`, not `"` — same
        // as bash today. (This is a known minor coverage gap; the variable
        // assignment with `=` direct-to-quote IS what we lint on.)
    }

    #[test]
    fn check1_skips_env_var_lookup() {
        // `env::var("PASS")` doesn't trip the regex on its own — the regex
        // requires the secret-identifier word followed by `=` and then `"`.
        // `let password = std::env::var(...)` has `= std::env::var` — the
        // char after `=` is `s`, not `"`. So Check 1's regex does NOT
        // match. ENV_LOOKUP_RE is defense-in-depth for the case where a
        // future regex tweak would have caught the line.
        let line = r#"let password = std::env::var("PASS").unwrap();"#;
        assert!(!SECRET_ASSIGNMENT_RE.is_match(line));
        // ENV_LOOKUP_RE itself recognizes the env-lookup shape so the
        // dispositive caller-side guard would skip it even if matched.
        assert!(ENV_LOOKUP_RE.is_match(line));
    }

    #[test]
    fn full_hygiene_catalog_catches_class_v_and_class_c() {
        // Full HYGIENE_PATTERNS (for alert-rule consumers) catches both
        // Class-V value-shape secrets AND Class-C context-shape shapes.
        let aws = "let key = \"AKIA1234567890ABCDEF\";";
        let jwt = "let t = \"eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJ4In0.A1B2C3D4E5F6\";";
        let bearer = "let h = \"Bearer abcdefghijklmnopqrst\";";
        assert!(HYGIENE_PATTERNS
            .iter()
            .any(|(name, re)| re.is_match(aws) && name.contains("AWS")));
        assert!(HYGIENE_PATTERNS
            .iter()
            .any(|(name, re)| re.is_match(jwt) && name.contains("JWT")));
        assert!(HYGIENE_PATTERNS
            .iter()
            .any(|(name, re)| re.is_match(bearer) && name.contains("bearer")));
    }

    #[test]
    fn source_scan_subset_catches_class_v_only() {
        // Per @paired-client F-CLIENT-2 + @security co-sign 2026-05-22:
        // source-code consumers (`rust_secrets`, `ts_secrets`) use the
        // Class-V subset; Class-C (bearer, hostname, DNS) stays out so
        // legitimate Rust source like `const PROD_HOST = "..."` or
        // `const URL = ".internal..."` doesn't FP.
        let aws = "let key = \"AKIA1234567890ABCDEF\";";
        let jwt = "let t = \"eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJ4In0.A1B2C3D4E5F6\";";
        let bearer = "let h = \"Bearer abcdefghijklmnopqrst\";";
        let dns = "let url = \"https://api.internal.example.com/v1\";";
        let prod_host = "let h = \"api-prod-east-1.example.com\";";
        // Class V — must fire.
        assert!(source_scan_patterns().any(|(name, re)| re.is_match(aws) && name.contains("AWS")));
        assert!(source_scan_patterns().any(|(name, re)| re.is_match(jwt) && name.contains("JWT")));
        // Class C — must NOT fire from the source-scan subset (would fire
        // from the full HYGIENE_PATTERNS catalog used by alert_rules).
        assert!(!source_scan_patterns().any(|(_, re)| re.is_match(bearer)));
        assert!(!source_scan_patterns().any(|(_, re)| re.is_match(dns)));
        assert!(!source_scan_patterns().any(|(_, re)| re.is_match(prod_host)));
    }

    #[test]
    fn check3_matches_connection_string_credentials() {
        assert!(CONN_STRING_RE.is_match(r#""postgresql://user:hunter2@host/db""#));
        assert!(!CONN_STRING_RE.is_match(r#""postgresql://user:${PASS}@host/db""#));
    }

    #[test]
    fn check4_matches_authorization_header_literal() {
        assert!(AUTH_HEADER_RE.is_match(r#""Authorization: Bearer abcdef1234567890XYZ=""#));
    }

    #[test]
    fn comment_lines_skipped() {
        assert!(is_comment_line("// let password = \"x\";"));
        assert!(is_comment_line("/* let secret = */"));
        assert!(is_comment_line(" * inside docblock"));
        assert!(!is_comment_line(r#"let password = "hunter2";"#));
    }

    #[test]
    fn excluded_path_drops_tests_and_vendor() {
        assert!(is_excluded_path(Path::new(
            "crates/foo/tests/integration.rs"
        )));
        assert!(is_excluded_path(Path::new("crates/foo/src/foo_test.rs")));
        assert!(is_excluded_path(Path::new("vendor/foo/src/lib.rs")));
        assert!(!is_excluded_path(Path::new("crates/foo/src/lib.rs")));
    }
}
