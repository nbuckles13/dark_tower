//! Shared regex for parsing `docs/observability/metrics/*.md` catalog files.
//!
//! Per @dry-reviewer F-DRY-2 2026-05-19: `application_metrics::CATALOG_HEAD_RE`
//! and `dashboard_panels::CATALOG_HEAD_RE` were byte-identical `Lazy<Regex>`
//! statics walking the same metric-catalog markdown. Consolidated here.

use once_cell::sync::Lazy;
use regex::Regex;

/// Matches `### \`metric_name\`` heading lines in metric-catalog markdown.
/// Capture group 1 = the metric name (snake_case lowercase).
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
pub static CATALOG_HEAD_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?m)^###\s+`([a-z_][a-z0-9_]*)`").expect("static pattern compiles"));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_metric_name_from_heading() {
        let src = "### `ac_token_validations_total`\n\nSome description.\n";
        let names: Vec<&str> = CATALOG_HEAD_RE
            .captures_iter(src)
            .filter_map(|c| c.get(1).map(|m| m.as_str()))
            .collect();
        assert_eq!(names, vec!["ac_token_validations_total"]);
    }

    #[test]
    fn rejects_non_h3_or_uppercase_metric_name() {
        // h2 not matched
        assert!(CATALOG_HEAD_RE.captures("## `foo`").is_none());
        // Uppercase metric name doesn't match the lowercase-only class
        assert!(CATALOG_HEAD_RE.captures("### `Foo`").is_none());
    }
}
