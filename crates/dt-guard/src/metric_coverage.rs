//! `metric-coverage` subcommand — port of
//! `scripts/guards/simple/validate-metric-coverage.sh` (ADR-0032).
//!
//! For each service, scans `crates/<svc>/src/observability/metrics.rs` for
//! emission-macro sites and verifies that every emitted metric name is
//! referenced by at least one file under `crates/<svc>/tests/**/*.rs`.
//!
//! Per `[metrics-path-completeness]` (per @semantic-guard): macro-form set is
//! pinned to `{counter, gauge, histogram}` ONLY — `describe_*` (documentation
//! macros) are EXCLUDED, matching bash `validate-metric-coverage.sh:82`.
//!
//! Per @observability S3: extracted names are post-filtered against
//! `^[a-z][a-z0-9_]+$` (bash line 85) to discard stray captures.

use crate::common::explain::{print_finding, Finding};
use crate::common::scan::warn_skip;
use crate::common::status::emit_ok;
use crate::metric_macros::{MacroKind, MACRO_INVOCATION_WITH_FIRST_ARG_RE};
use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use std::path::Path;
use walkdir::WalkDir;

pub const UNCOVERED_RULE_ID: &str = "uncovered_metric";

use crate::common::services::CANONICAL_SERVICES;

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static METRIC_NAME_FILTER_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[a-z][a-z0-9_]+$").expect("static pattern compiles"));

fn extract_emission_metric_names(content: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for caps in MACRO_INVOCATION_WITH_FIRST_ARG_RE.captures_iter(content) {
        let (Some(kind_m), Some(name_m)) = (caps.get(2), caps.get(3)) else {
            continue;
        };
        // `[metrics-path-completeness]` filter: emission macros only.
        let Some(kind) = MacroKind::parse(kind_m.as_str()) else {
            continue;
        };
        if !matches!(
            kind,
            MacroKind::Counter | MacroKind::Gauge | MacroKind::Histogram
        ) {
            continue;
        }
        let name = name_m.as_str();
        // @observability S3: post-filter to `^[a-z][a-z0-9_]+$`.
        if !METRIC_NAME_FILTER_RE.is_match(name) {
            continue;
        }
        names.push(name.to_string());
    }
    names.sort();
    names.dedup();
    names
}

/// Walk `tests_dir/**/*.rs` and return concatenated content for `contains` checks.
/// Returns an empty `String` if `tests_dir` doesn't exist (caller treats this
/// as a per-service uncovered signal).
fn read_tests_blob(tests_dir: &Path) -> Result<String> {
    if !tests_dir.is_dir() {
        return Ok(String::new());
    }
    let mut out = String::new();
    for entry in WalkDir::new(tests_dir).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if path
            .extension()
            .and_then(|s| s.to_str())
            .is_none_or(|s| s != "rs")
        {
            continue;
        }
        match std::fs::read_to_string(path) {
            Ok(s) => {
                out.push_str(&s);
                out.push('\n');
            }
            Err(e) => {
                warn_skip("tests read", path, &e);
            }
        }
    }
    Ok(out)
}

#[derive(Debug)]
struct Hit {
    service: &'static str,
    metric: String,
}

pub fn run(repo_root: &Path, explain: bool) -> Result<()> {
    let mut all_hits: Vec<Hit> = Vec::new();

    for (prefix, dir) in CANONICAL_SERVICES {
        let metrics_file = repo_root
            .join("crates")
            .join(dir)
            .join("src/observability/metrics.rs");
        if !metrics_file.is_file() {
            // Bash today emits a WARNING — preserve parity via warn_skip.
            warn_skip(
                "metrics.rs absent",
                &metrics_file,
                &std::io::Error::other("no metrics.rs in this service"),
            );
            continue;
        }
        let metrics_content = std::fs::read_to_string(&metrics_file)
            .with_context(|| format!("reading {}", metrics_file.display()))?;
        let metrics = extract_emission_metric_names(&metrics_content);
        if metrics.is_empty() {
            // Per bash: emit WARNING then continue.
            warn_skip(
                "no emissions",
                &metrics_file,
                &std::io::Error::other("no counter/gauge/histogram emissions found"),
            );
            continue;
        }

        let tests_dir = repo_root.join("crates").join(dir).join("tests");
        let tests_blob = read_tests_blob(&tests_dir)?;

        for metric in &metrics {
            if !tests_blob.contains(metric.as_str()) {
                all_hits.push(Hit {
                    service: prefix,
                    metric: metric.clone(),
                });
            }
        }
    }

    if all_hits.is_empty() {
        emit_ok("metric-coverage-all-covered");
        return Ok(());
    }

    for hit in &all_hits {
        let file_disp = format!(
            "crates/{}-service/src/observability/metrics.rs",
            hit.service
        );
        if explain {
            print_finding(&Finding {
                file: &file_disp,
                row: 0,
                col: 0,
                policy: "metric-coverage::uncovered_metric",
                matched: &hit.metric,
                extras: &[("service", hit.service)],
                src_file: file!(),
                src_line: line!(),
            });
        } else {
            println!(
                "VIOLATION: {} [{}] metric {:?} not referenced by any test",
                file_disp, UNCOVERED_RULE_ID, hit.metric
            );
        }
    }

    anyhow::bail!("metric-coverage-uncovered-{}", all_hits.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_emission_names_excludes_describe_macros() {
        let content = r#"
            counter!("ac_requests_total", labels);
            histogram!("ac_request_duration_seconds", labels);
            gauge!("ac_active_sessions", labels);
            describe_counter!("describes_only", "for documentation");
            describe_histogram!("doc_only_hist", "doc");
        "#;
        let names = extract_emission_metric_names(content);
        assert_eq!(names.len(), 3);
        assert!(names.contains(&"ac_requests_total".to_string()));
        assert!(names.contains(&"ac_request_duration_seconds".to_string()));
        assert!(names.contains(&"ac_active_sessions".to_string()));
        // describe_* are documentation, not emission — excluded.
        assert!(!names.contains(&"describes_only".to_string()));
        assert!(!names.contains(&"doc_only_hist".to_string()));
    }

    #[test]
    fn metrics_qualified_form_matches() {
        let content = r#"metrics::counter!("ac_x", labels);"#;
        let names = extract_emission_metric_names(content);
        assert_eq!(names, vec!["ac_x".to_string()]);
    }

    #[test]
    fn name_filter_drops_invalid_shape() {
        // Names like `1bad` (leading digit) or `BadName` (uppercase) get
        // post-filtered out by `^[a-z][a-z0-9_]+$`.
        assert!(METRIC_NAME_FILTER_RE.is_match("ac_request_total"));
        assert!(!METRIC_NAME_FILTER_RE.is_match("1bad"));
        assert!(!METRIC_NAME_FILTER_RE.is_match("BadName"));
        assert!(!METRIC_NAME_FILTER_RE.is_match("a"));
    }
}
