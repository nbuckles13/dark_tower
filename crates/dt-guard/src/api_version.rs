//! `api-version-check` subcommand — port of
//! `scripts/guards/simple/api-version-check.sh`.
//!
//! Enforces consistent API route patterns across services:
//! * `/api/v{N}/...` — versioned API routes (REQUIRED for all API routes).
//! * `/health`, `/ready`, `/metrics` — operational endpoints (NO version).
//! * `/.well-known/...` — RFC-defined (NO version).
//! * `/internal/...` — internal-only (NO version).
//!
//! Catches:
//! * API routes without `/api/v{N}/` prefix (e.g. `/v1/users` is wrong).
//! * Operational endpoints WITH a version prefix (e.g. `/v1/health` is wrong).

use crate::common::explain::{print_finding, Finding};
use crate::common::git_changes::get_all_changed_files;
use crate::common::status::emit_ok;
use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use std::path::{Path, PathBuf};

pub const ROUTE_UNVERSIONED_RULE_ID: &str = "api_route_unversioned";
pub const OPS_VERSIONED_RULE_ID: &str = "ops_route_versioned";

const EXTENSIONS: &[&str] = &[".rs"];

const EXCLUDED_PATH_PATTERNS: &[&str] = &["/tests/", "/test_utils/", "vendor/"];
const EXCLUDED_FILE_SUFFIXES: &[&str] = &["_test.rs"];

/// `.route("/...")` opener — captures the path literal in group 1.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static ROUTE_LITERAL_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"\.route\s*\(\s*"(/[^"]*)""#).expect("static pattern compiles"));

/// Operational endpoint with a version prefix (forbidden shape).
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static OPS_VERSIONED_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^/(api/)?v[0-9]+/(health|ready|metrics)").expect("static pattern compiles")
});

/// Correct API route prefix: `/api/v{N}/`.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static CORRECT_API_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^/api/v[0-9]+/").expect("static pattern compiles"));

fn classify_path(path: &str) -> Option<&'static str> {
    // Allowed unversioned shapes.
    if path.starts_with("/.well-known") || path.starts_with("/internal") {
        return None;
    }
    if path == "/health" || path == "/ready" || path == "/metrics" || path.starts_with("/metrics/")
    {
        return None;
    }
    // Operational endpoint WITH a version prefix → violation.
    if OPS_VERSIONED_RE.is_match(path) {
        return Some(OPS_VERSIONED_RULE_ID);
    }
    // Correct /api/v{N}/ prefix.
    if CORRECT_API_RE.is_match(path) {
        return None;
    }
    // Anything else is an unversioned API route or wrong-pattern.
    Some(ROUTE_UNVERSIONED_RULE_ID)
}

fn is_excluded_path(path: &Path) -> bool {
    let Some(s) = path.to_str() else {
        return true;
    };
    if EXCLUDED_PATH_PATTERNS.iter().any(|pat| s.contains(pat)) {
        return true;
    }
    EXCLUDED_FILE_SUFFIXES.iter().any(|suf| s.ends_with(suf))
}

fn is_comment_line(line: &str) -> bool {
    let stripped = line.trim_start();
    stripped.starts_with("//") || stripped.starts_with("/*") || stripped.starts_with("*")
}

#[derive(Debug)]
struct Hit {
    rule_id: &'static str,
    route: String,
    file: PathBuf,
    line: usize,
}

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
        for caps in ROUTE_LITERAL_RE.captures_iter(line) {
            let Some(m) = caps.get(1) else {
                continue;
            };
            let route = m.as_str();
            if let Some(rule_id) = classify_path(route) {
                hits.push(Hit {
                    rule_id,
                    route: route.to_string(),
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
        emit_ok("api-version-check-no-files");
        return Ok(());
    }

    let mut all_hits: Vec<Hit> = Vec::new();
    for path in &in_scope {
        all_hits.extend(scan_file(repo_root, path)?);
    }

    if all_hits.is_empty() {
        emit_ok(format!("api-version-check-clean-{}-files", in_scope.len()));
        return Ok(());
    }

    for hit in &all_hits {
        let file_disp = hit.file.display().to_string();
        if explain {
            let policy = format!("api-version-check::{}", hit.rule_id);
            print_finding(&Finding {
                file: &file_disp,
                row: hit.line,
                col: 0,
                policy: &policy,
                matched: &hit.route,
                extras: &[],
                src_file: file!(),
                src_line: line!(),
            });
        } else {
            println!(
                "VIOLATION: {}:{} [{}] route {:?}",
                file_disp, hit.line, hit.rule_id, hit.route
            );
        }
    }

    let unversioned = all_hits
        .iter()
        .filter(|h| h.rule_id == ROUTE_UNVERSIONED_RULE_ID)
        .count();
    let ops = all_hits
        .iter()
        .filter(|h| h.rule_id == OPS_VERSIONED_RULE_ID)
        .count();
    if unversioned >= ops {
        anyhow::bail!("api-version-route-unversioned-{unversioned}")
    } else {
        anyhow::bail!("api-version-ops-versioned-{ops}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn correct_api_route_passes() {
        assert!(classify_path("/api/v1/users").is_none());
        assert!(classify_path("/api/v2/meetings/123").is_none());
    }

    #[test]
    fn unversioned_api_route_flags() {
        assert_eq!(classify_path("/users"), Some(ROUTE_UNVERSIONED_RULE_ID));
        assert_eq!(classify_path("/v1/users"), Some(ROUTE_UNVERSIONED_RULE_ID));
    }

    #[test]
    fn ops_endpoints_unversioned_pass() {
        assert!(classify_path("/health").is_none());
        assert!(classify_path("/ready").is_none());
        assert!(classify_path("/metrics").is_none());
        assert!(classify_path("/metrics/something").is_none());
    }

    #[test]
    fn ops_endpoints_with_version_flag() {
        assert_eq!(classify_path("/v1/health"), Some(OPS_VERSIONED_RULE_ID));
        assert_eq!(classify_path("/api/v1/health"), Some(OPS_VERSIONED_RULE_ID));
    }

    #[test]
    fn well_known_and_internal_pass() {
        assert!(classify_path("/.well-known/jwks.json").is_none());
        assert!(classify_path("/internal/admin").is_none());
    }

    #[test]
    fn route_literal_extracts_path() {
        let line = r#"        .route("/api/v1/users", get(handler))"#;
        let caps = ROUTE_LITERAL_RE.captures(line).unwrap();
        assert_eq!(caps.get(1).unwrap().as_str(), "/api/v1/users");
    }
}
