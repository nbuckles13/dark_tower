//! Shared regex / constants for Grafana dashboard JSON parsing.
//!
//! Per @dry-reviewer F-DRY-4 2026-05-19: `grafana_datasources::TEMPLATED_REF_RE`
//! and `dashboard_panels::TEMPLATED_DS_RE` were two semantically-identical
//! statics (cosmetic char-class differences only) walking the same Grafana
//! template-variable shape (`$var` / `${var}` / `${var:raw}`). Consolidated
//! here as the single source of truth.

use once_cell::sync::Lazy;
use regex::Regex;

/// Grafana template-variable reference — matches `$var`, `${var}`, or
/// `${var:raw}`. Capture group 0 = the whole match; no inner capture
/// (consumers use `is_match`, not field extraction).
///
/// Char class `[A-Za-z_][\w:]*` accepts the union of the two prior forms
/// (`[a-zA-Z_][a-zA-Z0-9_:]*` from `dashboard_panels` and
/// `[A-Za-z_][\w:]*` from `grafana_datasources` — these are semantically
/// equivalent: `\w` = `[A-Za-z0-9_]`).
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
pub static GRAFANA_TEMPLATE_VAR_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\$\{?[A-Za-z_][\w:]*\}?$").expect("static pattern compiles"));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_three_grafana_template_var_shapes() {
        assert!(GRAFANA_TEMPLATE_VAR_RE.is_match("$datasource"));
        assert!(GRAFANA_TEMPLATE_VAR_RE.is_match("${datasource}"));
        assert!(GRAFANA_TEMPLATE_VAR_RE.is_match("${datasource:raw}"));
    }

    #[test]
    fn rejects_non_template_strings() {
        assert!(!GRAFANA_TEMPLATE_VAR_RE.is_match("prometheus"));
        assert!(!GRAFANA_TEMPLATE_VAR_RE.is_match("loki-uid-1"));
        assert!(!GRAFANA_TEMPLATE_VAR_RE.is_match("$"));
        assert!(!GRAFANA_TEMPLATE_VAR_RE.is_match("$$double"));
    }
}
