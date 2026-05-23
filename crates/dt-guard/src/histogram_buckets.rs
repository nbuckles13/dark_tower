//! `histogram-buckets` subcommand — port of
//! `scripts/guards/simple/validate-histogram-buckets.sh`.
//!
//! For each service, scans `crates/<svc>/src/observability/metrics.rs` and
//! enforces that every `histogram!("name", ...)` has a matching
//! `set_buckets_for_metric(Matcher::Prefix("<prefix>"), ...)` whose prefix
//! is a `starts_with` match for the metric name.
//!
//! Per @observability S2: pure prefix-match semantics preserved (`metric_name
//! .starts_with(prefix)`). NO equality requirement, NO longest-prefix-wins,
//! NO `Matcher::Suffix`/`Matcher::Full` variants.
//!
//! Per `[metrics-path-completeness]`: macro-form set pinned to
//! `{histogram}` ONLY — `describe_histogram` (documentation macro) is
//! NOT counted.

use crate::common::explain::{print_finding, Finding};
use crate::common::scan::warn_skip;
use crate::common::status::emit_ok;
use crate::metric_macros::{MacroKind, MACRO_INVOCATION_WITH_FIRST_ARG_RE};
use anyhow::Result;
use once_cell::sync::Lazy;
use regex::Regex;
use std::path::Path;

pub const UNCONFIGURED_RULE_ID: &str = "unconfigured_histogram";

use crate::common::services::CANONICAL_SERVICES;

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static MATCHER_PREFIX_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"Matcher::Prefix\s*\(\s*"([^"]+)""#).expect("static pattern compiles")
});

fn extract_histogram_names(content: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for caps in MACRO_INVOCATION_WITH_FIRST_ARG_RE.captures_iter(content) {
        let (Some(kind_m), Some(name_m)) = (caps.get(2), caps.get(3)) else {
            continue;
        };
        // `[metrics-path-completeness]` filter: only emission histograms.
        let Some(kind) = MacroKind::parse(kind_m.as_str()) else {
            continue;
        };
        if !matches!(kind, MacroKind::Histogram) {
            continue;
        }
        names.push(name_m.as_str().to_string());
    }
    names.sort();
    names.dedup();
    names
}

fn extract_bucket_prefixes(content: &str) -> Vec<String> {
    let mut prefixes: Vec<String> = Vec::new();
    for caps in MATCHER_PREFIX_RE.captures_iter(content) {
        if let Some(m) = caps.get(1) {
            prefixes.push(m.as_str().to_string());
        }
    }
    prefixes.sort();
    prefixes.dedup();
    prefixes
}

#[derive(Debug)]
struct Hit {
    metric: String,
    service: &'static str,
}

pub fn run(repo_root: &Path, explain: bool) -> Result<()> {
    let mut all_hits: Vec<Hit> = Vec::new();
    let mut total_histograms: usize = 0;

    for (prefix, dir) in CANONICAL_SERVICES {
        let metrics_file = repo_root
            .join("crates")
            .join(dir)
            .join("src/observability/metrics.rs");
        if !metrics_file.is_file() {
            continue;
        }
        let content = match std::fs::read_to_string(&metrics_file) {
            Ok(s) => s,
            Err(e) => {
                warn_skip("metrics.rs read", &metrics_file, &e);
                continue;
            }
        };
        let histograms = extract_histogram_names(&content);
        let bucket_prefixes = extract_bucket_prefixes(&content);
        total_histograms += histograms.len();
        for h in &histograms {
            // Per @observability S2: pure `starts_with`.
            if !bucket_prefixes.iter().any(|bp| h.starts_with(bp)) {
                all_hits.push(Hit {
                    metric: h.clone(),
                    service: prefix,
                });
            }
        }
    }

    if all_hits.is_empty() {
        emit_ok(format!(
            "histogram-buckets-all-{total_histograms}-configured"
        ));
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
                policy: "histogram-buckets::unconfigured_histogram",
                matched: &hit.metric,
                extras: &[("service", hit.service)],
                src_file: file!(),
                src_line: line!(),
            });
        } else {
            println!(
                "VIOLATION: {} [{}] histogram {:?} has no set_buckets_for_metric() configuration",
                file_disp, UNCONFIGURED_RULE_ID, hit.metric
            );
        }
    }

    anyhow::bail!("histogram-buckets-unconfigured-{}", all_hits.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_histogram_names_finds_emissions_only() {
        let content = r#"
            histogram!("foo_total", labels);
            metrics::histogram!("bar_seconds", labels);
            describe_histogram!("ignored_doc", "this is documentation");
            counter!("not_a_histogram", labels);
        "#;
        let names = extract_histogram_names(content);
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"foo_total".to_string()));
        assert!(names.contains(&"bar_seconds".to_string()));
        // describe_histogram is documentation, not emission — excluded.
        assert!(!names.contains(&"ignored_doc".to_string()));
    }

    #[test]
    fn extract_bucket_prefixes_collects_quoted_args() {
        let content = r#"
            set_buckets_for_metric(Matcher::Prefix("mc_grpc_"), &[...]).unwrap();
            set_buckets_for_metric(Matcher::Prefix("ac_"), &[0.1, 0.5]).unwrap();
        "#;
        let prefixes = extract_bucket_prefixes(content);
        assert_eq!(prefixes.len(), 2);
        assert!(prefixes.contains(&"mc_grpc_".to_string()));
        assert!(prefixes.contains(&"ac_".to_string()));
    }

    #[test]
    fn pure_prefix_match_per_observability_s2() {
        // Documented edge case: `Matcher::Prefix("mc_grpc_")` covers
        // `mc_grpc_register_meeting_duration_seconds`.
        let metric = "mc_grpc_register_meeting_duration_seconds";
        let prefix = "mc_grpc_";
        assert!(metric.starts_with(prefix));
        // Longest-prefix-wins NOT enforced — short prefix wins as long as
        // it's a valid starts_with match.
        let short_prefix = "mc_";
        assert!(metric.starts_with(short_prefix));
    }
}
