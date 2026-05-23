//! `rust-no-secrets-in-logs` subcommand — port of
//! `scripts/guards/simple/no-secrets-in-logs.sh`.
//!
//! Detects secret-identifier leakage via tracing/log call sites. Six checks:
//! 1. `#[instrument]` without `skip(...)` for secret params.
//! 2. Secret variable in log macro arguments.
//! 3. `expose_secret()` invocation inside log macro (defeats SecretString).
//! 4. PII named tracing fields with secret names.
//! 5. Secret in error/anyhow messages.
//! 6. Debug `{:?}` formatting on request/response/auth structs (WARN heuristic).
//!
//! **This module matches Rust identifier vocabulary (variable names like
//! `password`, `token`, `credential`) — consumed from the canonical
//! `crate::common::pii_vocabulary::PII_TOKENS_CATEGORY_A` SoT. DO NOT
//! consolidate with `crate::secret_patterns::HYGIENE_PATTERNS` — HYGIENE
//! matches value shapes (JWTs, AWS keys, bearer tokens) and answers a
//! different question. Two separate catalogs, two separate maintenance
//! axes.** (Per @dry-reviewer commitment.)
//!
//! Per @semantic-guard Q1 — findings emit via [`print_secret_finding`].

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

pub const INSTRUMENT_NO_SKIP_RULE_ID: &str = "instrument_no_skip";
pub const SECRET_IN_LOG_RULE_ID: &str = "secret_in_log_macro";
pub const EXPOSE_SECRET_IN_LOG_RULE_ID: &str = "expose_secret_in_log";
pub const SECRET_IN_TRACING_FIELD_RULE_ID: &str = "secret_in_tracing_field";
pub const SECRET_IN_ERROR_MSG_RULE_ID: &str = "secret_in_error_message";
pub const DEBUG_FMT_HEURISTIC_RULE_ID: &str = "debug_fmt_heuristic";

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
static SECRET_WORD_RE: Lazy<Regex> = Lazy::new(|| {
    let alternation = PII_TOKENS_CATEGORY_A.join("|");
    Regex::new(&format!(r"\b({alternation})\b")).expect("vocab alternation compiles")
});

/// Bash Check-2 shape per @team-lead Wave-2 port-fidelity fix (2026-05-21):
/// only fire when the secret-vocab word appears in one of three interpolation
/// shapes inside a log macro:
///
/// * `{X}` — format-arg interpolation (`info!("...{password}...")`).
/// * `%X` / `?X` — tracing display/debug marker (`info!(password = %x)`).
/// * `, X,` / `, X)` — positional after format string (`info!("...", token)`).
///
/// The earlier (looser) version matched ANY occurrence of the secret-vocab
/// word on the same line as a log macro, which produced FPs on production
/// `info!("authorize", token = ?req.token.id())` shapes where `?req.token`
/// is the redacted display marker but the `.id()` access is not a secret.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static SECRET_IN_LOG_SHAPE_RE: Lazy<Regex> = Lazy::new(|| {
    let alternation = PII_TOKENS_CATEGORY_A.join("|");
    // Match any of:
    //   \{[^}]*WORD[^}]*\}   — secret word inside `{}` interpolation
    //   [%?]\s*WORD\b         — `%word` or `?word` tracing display
    //   ,\s*WORD\s*[,)]       — positional secret arg
    Regex::new(&format!(
        r"\{{[^}}]*\b({a})\b[^}}]*\}}|[%?]\s*({a})\b|,\s*({a})\s*[,)]",
        a = alternation
    ))
    .expect("vocab alternation compiles")
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
static EXPOSE_SECRET_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"expose_secret\s*\(").expect("static pattern compiles"));

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
static DEBUG_FMT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"\{:\?\}.*\b(request|req|response|res|body|payload|credentials|auth|login)\b|\b(request|req|response|res|body|payload|credentials|auth|login)\b.*\{:\?\}"#)
        .expect("static pattern compiles")
});

fn secret_hit(line: &str) -> Option<&'static str> {
    let m = SECRET_WORD_RE.find(line)?;
    let s = m.as_str();
    PII_TOKENS_CATEGORY_A.iter().copied().find(|t| *t == s)
}

fn is_allowed_line(line: &str) -> bool {
    line.contains("REDACTED")
        || line.contains("[REDACTED]")
        || line.contains("skip_all")
        || line.contains("// guard:ignore")
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

        let secret_tok = secret_hit(line);

        // Check 1: #[instrument] without skip for secret params.
        if INSTRUMENT_RE.is_match(line) && !line.contains("skip(") && !line.contains("skip_all") {
            if let Some(token) = secret_tok {
                hits.push(Hit {
                    rule_id: INSTRUMENT_NO_SKIP_RULE_ID,
                    pattern_name: token,
                    file: path.to_path_buf(),
                    line: line_no,
                    is_warning: false,
                });
            }
        }

        // Check 2: secret-named variable IN INTERPOLATION POSITION in log
        // macro. Per @team-lead Wave-2 port-fidelity fix 2026-05-21 — bash
        // requires the secret word to appear in `{X}` / `%X`/`?X` / `, X,)`
        // shape, not just anywhere on the line. The looser earlier check
        // FPs on production `info!("authorize", token = ?req.token.id())`.
        if LOG_MACRO_RE.is_match(line) {
            if let Some(caps) = SECRET_IN_LOG_SHAPE_RE.captures(line) {
                // Capture group 1/2/3 contains the matched secret-vocab word
                // (depending on which alternation branch fired). Resolve
                // back to the canonical-home const for the redacted token.
                let matched = (1..=3).find_map(|i| caps.get(i)).map(|m| m.as_str());
                if let Some(s) = matched {
                    if let Some(token) = PII_TOKENS_CATEGORY_A.iter().copied().find(|t| *t == s) {
                        hits.push(Hit {
                            rule_id: SECRET_IN_LOG_RULE_ID,
                            pattern_name: token,
                            file: path.to_path_buf(),
                            line: line_no,
                            is_warning: false,
                        });
                    }
                }
            }

            // Check 3: expose_secret() inside log macro.
            if EXPOSE_SECRET_RE.is_match(line) {
                hits.push(Hit {
                    rule_id: EXPOSE_SECRET_IN_LOG_RULE_ID,
                    pattern_name: "expose_secret",
                    file: path.to_path_buf(),
                    line: line_no,
                    is_warning: false,
                });
            }
        }

        // Check 4: named tracing field with secret name (`token = %x`).
        if (line.contains("tracing::") || line.contains("#[instrument"))
            && line.contains('=')
            && (line.contains('%') || line.contains('?'))
        {
            if let Some(token) = secret_tok {
                hits.push(Hit {
                    rule_id: SECRET_IN_TRACING_FIELD_RULE_ID,
                    pattern_name: token,
                    file: path.to_path_buf(),
                    line: line_no,
                    is_warning: false,
                });
            }
        }

        // Check 5: secret in error/anyhow message.
        if ERROR_CTOR_RE.is_match(line) && line.contains('{') {
            if let Some(token) = secret_tok {
                hits.push(Hit {
                    rule_id: SECRET_IN_ERROR_MSG_RULE_ID,
                    pattern_name: token,
                    file: path.to_path_buf(),
                    line: line_no,
                    is_warning: false,
                });
            }
        }

        // Check 6: Debug formatting heuristic (WARN-only).
        if DEBUG_FMT_RE.is_match(line) && !line.contains("#[derive") {
            hits.push(Hit {
                rule_id: DEBUG_FMT_HEURISTIC_RULE_ID,
                pattern_name: "debug_format_on_credential_struct",
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
        emit_ok("rust-no-secrets-in-logs-no-files");
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
            let policy = format!("rust-no-secrets-in-logs::{}", hit.rule_id);
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
                "{}: {}:{} [{}] suspected {} (redacted)",
                prefix, file_disp, hit.line, hit.rule_id, hit.pattern_name
            );
        }
    }

    if blocking_count == 0 {
        emit_ok(format!(
            "rust-no-secrets-in-logs-clean-{}-files",
            in_scope.len()
        ));
        return Ok(());
    }

    anyhow::bail!(
        "rust-log-secret-violation-{blocking_count}-of-{}-files",
        in_scope.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_hit_recognizes_category_a_tokens() {
        assert_eq!(secret_hit("let password = x"), Some("password"));
        assert_eq!(secret_hit("self.api_key"), Some("api_key"));
        assert_eq!(secret_hit("let jwt = y"), Some("jwt"));
        assert!(secret_hit("regular code").is_none());
    }

    #[test]
    fn wave2_category_a_additions_recognized() {
        for tok in &["pwd", "cred", "bearer", "auth_code"] {
            assert_eq!(secret_hit(&format!("let x = {tok};")), Some(*tok));
        }
    }

    #[test]
    fn expose_secret_match() {
        assert!(EXPOSE_SECRET_RE.is_match("info!(\"v = {}\", x.expose_secret())"));
        assert!(!EXPOSE_SECRET_RE.is_match("normal code"));
    }

    #[test]
    fn allowed_line_skips_skip_all_and_redacted() {
        assert!(is_allowed_line("#[instrument(skip_all, fields(x = %x))]"));
        assert!(is_allowed_line("info!(\"p = [REDACTED]\")"));
        assert!(is_allowed_line("info!(\"p = REDACTED\")"));
        assert!(!is_allowed_line("info!(\"password = {}\", pw)"));
    }

    #[test]
    fn debug_fmt_heuristic_matches_request_response() {
        assert!(DEBUG_FMT_RE.is_match("debug!(\"req = {:?}\", request)"));
        assert!(DEBUG_FMT_RE.is_match("info!(\"creds: {:?}\", credentials)"));
        assert!(!DEBUG_FMT_RE.is_match("info!(\"v = {}\", 1)"));
    }
}
