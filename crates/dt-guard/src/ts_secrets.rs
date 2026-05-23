//! `ts-no-secrets` subcommand — port of
//! `scripts/guards/simple/ts/no-secrets-in-ts.sh`.
//!
//! Detects hardcoded secrets in changed `.ts` / `.tsx` / `.svelte` files.
//! Wave-2 group (a) port — verbatim mechanical port of the bash logic, with
//! one cross-stack consolidation: **Check 2 consumes the canonical
//! [`crate::secret_patterns::HYGIENE_PATTERNS`] catalog** instead of a
//! module-local API-key regex. Closes the cross-stack dupe per ADR-0034 §6
//! second paragraph (paired with the rust-no-hardcoded-secrets port).
//!
//! Per @semantic-guard Q1 — every finding routes through
//! [`crate::common::explain::print_secret_finding`] (no `matched=` field;
//! `pattern=<name>` only). VIOLATION lines also omit the raw matched bytes;
//! the wire emits a redacted descriptor + the file/line.
//!
//! Per @paired-client F1: file collection uses
//! [`crate::common::git_changes`] with literal-suffix matching (tightening
//! #2). Per @paired-security S1(a) (task #37 ruling): bare `token` is dropped
//! from Check 1's identifier set — `const token = await ...` is routine
//! browser/SDK code; real token leaks are still caught by Check 2 (HYGIENE
//! prefix patterns) and Check 5 (JWT shape).

use crate::common::explain::{print_secret_finding, SecretFinding};
use crate::common::git_changes::get_all_changed_files;
use crate::common::status::emit_ok;
use crate::common::test_code_filter::is_scan_exempt;
use crate::secret_patterns::source_scan_patterns;
use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use std::path::{Path, PathBuf};

pub const SECRET_ASSIGNMENT_RULE_ID: &str = "secret_assignment";
pub const API_KEY_PREFIX_RULE_ID: &str = "api_key_prefix";
pub const CONN_STRING_RULE_ID: &str = "conn_string_credentials";
pub const AUTH_HEADER_RULE_ID: &str = "authorization_header";
pub const JWT_LITERAL_RULE_ID: &str = "jwt_literal";

/// File extensions in scope. Literal-suffix matched.
const EXTENSIONS: &[&str] = &[".ts", ".tsx", ".svelte"];

/// Path substrings that exempt a file from the secret scan (test/build
/// artifacts). Bash today inlines this per-file; the Rust port preserves the
/// inline shape per @dry-reviewer ADR-0019 threshold (premature extraction
/// at 2-3 callers — Wave-2 has only ts_secrets + ts_pii using this shape).
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

/// File-suffix exclusions (.d.ts declaration files + test files).
const TEST_FILE_SUFFIXES: &[&str] = &[".d.ts", ".test.ts", ".spec.ts", ".test.tsx", ".spec.tsx"];

// --- Check 1: secret variable assignments with literal RHS ---
//
// Pattern from `no-secrets-in-ts.sh:94`. Bare `token` is intentionally
// dropped from the identifier list per task-#37 paired-security S1(a).
// Excludes env-lookups and build-time defines.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static SECRET_ASSIGNMENT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"(?i)(password|secret|api_key|credential|master_key|private_key|client_secret)\s*[:=]\s*["'`][^"'`]+"#,
    )
    .expect("static pattern compiles")
});

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static ENV_LOOKUP_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"process\.env\.|import\.meta\.env\.|Deno\.env\.get\(|Bun\.env\.|globalThis\.__VITE_DEFINE__|\b__[A-Z_]+__\b",
    )
    .expect("static pattern compiles")
});

// --- Check 3: connection-string credentials ---
//
// `"postgresql://user:password@host"` etc. — bash `no-secrets-in-ts.sh:136-138`.
// The password segment must have actual content (`[^@{$]+` excludes
// variable-reference shapes like `${PASSWORD}`).
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static CONN_STRING_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"["'`](postgresql|mysql|redis|mongodb|amqp)://[^:]+:[^@{$]+@"#)
        .expect("static pattern compiles")
});

// --- Check 4: Authorization header literals ---
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static AUTH_HEADER_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?i)"(Authorization:\s*(Bearer|Basic)\s+[A-Za-z0-9+/=_.~\-]{20,})""#)
        .expect("static pattern compiles")
});

// --- Check 5: JWT-shaped literals ---
//
// `"eyJ...header.payload.signature"` — three base64 segments separated by `.`.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static JWT_LITERAL_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"["'`]eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}"#)
        .expect("static pattern compiles")
});

// --- Comment / allowance pre-filter (lines that should NOT be flagged) ---
//
// Bash `no-secrets-in-ts.sh` discards lines that start with `//` or `/*`
// AFTER finding a hit. The Rust port applies the same filter at line
// inspection time.
fn is_comment_line(line: &str) -> bool {
    let stripped = line.trim_start();
    stripped.starts_with("//") || stripped.starts_with("/*")
}

/// Path-shape filter — drop node_modules / dist / build / test paths.
/// Layers TS-specific build-artifact exclusions (`/node_modules/`, `/dist/`,
/// `/build/`, `.d.ts`, etc.) on top of the shared
/// [`crate::common::test_code_filter::is_scan_exempt`] test-path + guard-internal check.
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
}

/// Scan one file for all 5 check classes. Returns one [`Hit`] per (rule,
/// line) — same line can fire multiple rules if it has multiple secrets.
fn scan_file(repo_root: &Path, path: &Path) -> Result<Vec<Hit>> {
    let abs = repo_root.join(path);
    let content =
        std::fs::read_to_string(&abs).with_context(|| format!("reading {}", abs.display()))?;
    let mut hits: Vec<Hit> = Vec::new();

    for (idx, line) in content.lines().enumerate() {
        let line_no = idx + 1;
        if is_comment_line(line) {
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
        // only). Per @paired-client F-CLIENT-2 + @security co-sign 2026-05-22:
        // alert-rule-annotation-shaped Class-C patterns stay out (would FP on
        // legitimate TS like `const PROD_HOST = "api-prod-east-1.example.com"`).
        // See `secret_patterns::HYGIENE_SOURCE_SCAN_SUBSET` for the framing.
        for (name, regex) in source_scan_patterns() {
            if regex.is_match(line) {
                hits.push(Hit {
                    rule_id: API_KEY_PREFIX_RULE_ID,
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

        // Check 5: JWT-shaped literals.
        if JWT_LITERAL_RE.is_match(line) {
            hits.push(Hit {
                rule_id: JWT_LITERAL_RULE_ID,
                pattern_name: "jwt_literal",
                file: path.to_path_buf(),
                line: line_no,
            });
        }
    }

    Ok(hits)
}

/// Entry point for the `ts-no-secrets` subcommand.
pub fn run(repo_root: &Path, explain: bool) -> Result<()> {
    let changed = get_all_changed_files(repo_root, Path::new("."), EXTENSIONS)
        .context("collecting changed TS files")?;

    let in_scope: Vec<PathBuf> = changed
        .into_iter()
        .filter(|p| !is_excluded_path(p))
        .filter(|p| repo_root.join(p).is_file())
        .collect();

    if in_scope.is_empty() {
        emit_ok("ts-no-secrets-no-files");
        return Ok(());
    }

    let mut all_hits: Vec<Hit> = Vec::new();
    for path in &in_scope {
        let hits = scan_file(repo_root, path)?;
        all_hits.extend(hits);
    }

    if all_hits.is_empty() {
        emit_ok(format!("ts-no-secrets-clean-{}-files", in_scope.len()));
        return Ok(());
    }

    for hit in &all_hits {
        let file_disp = hit.file.display().to_string();
        if explain {
            let policy = format!("ts-no-secrets::{}", hit.rule_id);
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

    anyhow::bail!(
        "ts-secrets-violation-found-{}-of-{}-files",
        all_hits.len(),
        in_scope.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_assignment_matches_password_literal() {
        assert!(SECRET_ASSIGNMENT_RE.is_match(r#"const password = "hunter2""#));
        assert!(SECRET_ASSIGNMENT_RE.is_match(r#"secret: 'abc123def'"#));
        assert!(SECRET_ASSIGNMENT_RE.is_match(r#"api_key = `XYZ`"#));
    }

    #[test]
    fn secret_assignment_skips_bare_token() {
        // Bash dropped bare `token` per S1(a). The Rust regex's identifier set
        // does NOT include `token`, so this assignment shouldn't match.
        assert!(!SECRET_ASSIGNMENT_RE.is_match(r#"const token = "abc""#));
    }

    #[test]
    fn env_lookup_filter_two_modes() {
        // Mode 1: plain `process.env.X` RHS has no quoted literal, so
        // SECRET_ASSIGNMENT_RE doesn't match at all — the env-lookup filter
        // is a defense-in-depth secondary check.
        let plain = r#"const password = process.env.PASSWORD"#;
        assert!(!SECRET_ASSIGNMENT_RE.is_match(plain));

        // Mode 2: template-literal with embedded env-lookup — primary regex
        // hits the backtick literal, ENV_LOOKUP_RE matches → caller skips.
        let template = r#"const password = `${process.env.PASSWORD}_suffix`"#;
        assert!(SECRET_ASSIGNMENT_RE.is_match(template));
        assert!(ENV_LOOKUP_RE.is_match(template));
    }

    #[test]
    fn conn_string_matches_postgres_with_inline_credentials() {
        assert!(CONN_STRING_RE.is_match(r#""postgresql://user:hunter2@host/db""#));
        assert!(!CONN_STRING_RE.is_match(r#""postgresql://user:${PASSWORD}@host/db""#));
    }

    #[test]
    fn auth_header_matches_bearer_literal() {
        assert!(AUTH_HEADER_RE.is_match(r#""Authorization: Bearer abcdef1234567890XYZ=""#));
    }

    #[test]
    fn jwt_literal_matches_three_segment_shape() {
        // Each segment >=10 chars in the `{10,}` post-`eyJ` portion. Using a
        // realistic-length first segment.
        assert!(JWT_LITERAL_RE
            .is_match(r#""eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJ1c2VyMTIzIn0.A1B2C3D4E5F6G7""#));
    }

    #[test]
    fn check2_source_scan_subset_catches_class_v_only() {
        // Per @paired-client F-CLIENT-2 + @security co-sign 2026-05-22:
        // `ts_secrets` Check 2 consumes the Class-V source-scan subset,
        // NOT the full HYGIENE catalog. Class-C patterns would FP on
        // legitimate TS (`const PROD_HOST = "api-prod-east-1.example.com"`,
        // kustomize-side `.svc.cluster.local`, doc-string Bearer markers).
        use crate::secret_patterns::source_scan_patterns;
        let aws = r#"const KEY = "AKIA1234567890ABCDEF";"#;
        let jwt = r#"const T = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJ1c2VyMTIzIn0.A1B2C3D4E5F6G7";"#;
        let prod_host = r#"const PROD_HOST = "api-prod-east-1.example.com";"#;
        let dns = r#"const URL = "https://api.internal.example.com/v1";"#;
        let bearer_doc = r#"// example: "Authorization: Bearer eyJabcdefghijklmnopqrst""#;
        // Class V — must fire.
        assert!(source_scan_patterns().any(|(name, re)| re.is_match(aws) && name.contains("AWS")));
        assert!(source_scan_patterns().any(|(name, re)| re.is_match(jwt) && name.contains("JWT")));
        // Class C — must NOT fire.
        assert!(!source_scan_patterns().any(|(_, re)| re.is_match(prod_host)));
        assert!(!source_scan_patterns().any(|(_, re)| re.is_match(dns)));
        assert!(!source_scan_patterns().any(|(_, re)| re.is_match(bearer_doc)));
    }

    #[test]
    fn is_excluded_path_drops_test_artifacts() {
        assert!(is_excluded_path(Path::new(
            "packages/x/node_modules/foo.ts"
        )));
        assert!(is_excluded_path(Path::new("packages/x/dist/bundle.js")));
        assert!(is_excluded_path(Path::new("packages/x/src/foo.test.ts")));
        assert!(is_excluded_path(Path::new("packages/x/src/types.d.ts")));
        assert!(!is_excluded_path(Path::new("packages/x/src/index.ts")));
    }

    #[test]
    fn is_comment_skips_line_and_block_starts() {
        assert!(is_comment_line("    // password = \"hunter2\""));
        assert!(is_comment_line("/* secret */"));
        assert!(!is_comment_line(r#"const password = "hunter2""#));
    }
}
