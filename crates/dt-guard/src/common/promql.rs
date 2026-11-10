//! Shared PromQL lexing for guards that read dashboard and alert expressions:
//! comment/string masking, the label-position stripper, and service metric
//! reference extraction.
//!
//! # The invariant
//!
//! EVERY PromQL metric-reference extraction strips label positions first. A
//! label NAME may legally share a service prefix (story 4 GC-3's `gc_view` on
//! `gc_mh_replacement_requests_total`, ADR-0039 §8), and reading
//! `sum by(trigger, gc_view) (…)` as a reference to a metric `gc_view` is a
//! false positive. [`service_metric_refs`] is the only extractor guards use.
//! `common::services::SERVICE_METRIC_PREFIX_RE` is `pub(crate)`, so nothing
//! outside dt-guard can use it; inside the crate, guards go through
//! [`service_metric_refs`] (only `services.rs`'s own regex tests read it raw).
//!
//! # Ownership (CLAUDE.md guard-crate split)
//!
//! All of this is machinery (`infrastructure`): the lexer, the stripper and the
//! modifier list are PromQL grammar, not policy.

use crate::common::services::SERVICE_METRIC_PREFIX_RE;
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::BTreeSet;

/// The PromQL modifiers whose parenthesised argument is a LABEL LIST (never a
/// metric): grouping (`by`, `without`) and vector matching (`on`, `ignoring`,
/// `group_left`, `group_right`). The ONE home: `kind_labels`'s clause walk and
/// `client_metrics_export`'s grouping regex derive from it.
pub const LABEL_LIST_MODIFIERS: [&str; 6] = [
    "by",
    "without",
    "on",
    "ignoring",
    "group_left",
    "group_right",
];

/// `by|without|…` for building regexes from [`LABEL_LIST_MODIFIERS`].
#[must_use]
pub fn label_list_modifier_alternation() -> String {
    LABEL_LIST_MODIFIERS.join("|")
}

/// A label-only span: a `{…}` matcher (matchers do not nest) or a modifier's
/// label list (label lists hold no parens, so `[^)]*` is exact). Matched on
/// MASKED text, so a brace or paren inside a quoted string never opens a span.
/// Case-sensitive: an upper-case `BY(gc_x)` is left in place and fails loud.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static LABEL_POSITION_RE: Lazy<Regex> = Lazy::new(|| {
    let pattern = format!(
        r"\{{[^}}]*\}}|\b(?:{})\s*\([^)]*\)",
        label_list_modifier_alternation()
    );
    Regex::new(&pattern).expect("static pattern compiles")
});

/// An expression prepared for structural scanning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Masked {
    /// `#` comments removed; strings intact.
    pub code: String,
    /// As `code`, with quoted-string bodies blanked to spaces (quotes kept,
    /// newlines kept, so offsets and line structure survive).
    pub masked: String,
}

/// Remove `#` comments and blank quoted-string bodies (`"`, `'`, `` ` ``;
/// backslash escapes honoured).
#[must_use]
pub fn mask(expr: &str) -> Masked {
    let mut code = String::new();
    let mut masked = String::new();
    let mut quote: Option<char> = None;
    let mut chars = expr.chars().peekable();
    while let Some(c) = chars.next() {
        match quote {
            Some(q) => {
                code.push(c);
                if c == '\\' {
                    masked.push(' ');
                    if let Some(n) = chars.next() {
                        code.push(n);
                        masked.push(' ');
                    }
                    continue;
                }
                if c == q {
                    quote = None;
                    masked.push(c);
                } else {
                    masked.push(if c == '\n' { '\n' } else { ' ' });
                }
            }
            None => {
                if c == '#' {
                    while let Some(&n) = chars.peek() {
                        if n == '\n' {
                            break;
                        }
                        chars.next();
                    }
                    continue;
                }
                if matches!(c, '"' | '\'' | '`') {
                    quote = Some(c);
                }
                code.push(c);
                masked.push(c);
            }
        }
    }
    Masked { code, masked }
}

/// Blank every label-only span ([`LABEL_POSITION_RE`]) of already-MASKED text.
#[must_use]
pub fn strip_label_positions(masked: &str) -> String {
    LABEL_POSITION_RE.replace_all(masked, " ").into_owned()
}

/// The service-prefixed metric names an expression references: masked
/// (comments and string bodies out), label positions stripped, then
/// `SERVICE_METRIC_PREFIX_RE`.
///
/// Known limit, by design: a metric named ONLY through
/// `{__name__="gc_x_total"}` is not seen (it sits in a matcher). Nothing in
/// the tree selects a service metric that way (the one `__name__` in scope is
/// a grouping label over `ALERTS`); a test pins the behaviour so changing it
/// is a visible decision.
#[must_use]
pub fn service_metric_refs(expr: &str) -> BTreeSet<String> {
    let stripped = strip_label_positions(&mask(expr).masked);
    SERVICE_METRIC_PREFIX_RE
        .captures_iter(&stripped)
        .filter_map(|caps| caps.get(1).map(|m| m.as_str().to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refs(expr: &str) -> Vec<String> {
        service_metric_refs(expr).into_iter().collect()
    }

    #[test]
    fn label_lists_and_matchers_are_not_metric_refs() {
        assert_eq!(
            refs("sum by(trigger, gc_view) (increase(gc_x_total[5m]))"),
            ["gc_x_total"]
        );
        assert_eq!(refs(r#"gc_x_total{gc_view="healthy"}"#), ["gc_x_total"]);
        assert_eq!(refs("sum without(gc_view) (gc_x_total)"), ["gc_x_total"]);
        assert_eq!(
            refs("gc_a_total * on(gc_k) group_left(gc_v) gc_b_info"),
            ["gc_a_total", "gc_b_info"]
        );
        assert_eq!(
            refs("gc_a_total / ignoring(gc_k) group_right(gc_v) gc_b_total"),
            ["gc_a_total", "gc_b_total"]
        );
        // Infix form.
        assert_eq!(
            refs("sum(rate(gc_a_total[5m])) by (gc_view)"),
            ["gc_a_total"]
        );
    }

    #[test]
    fn undefined_metrics_outside_label_positions_are_still_refs() {
        assert_eq!(
            refs("sum by(le) (rate(gc_undefined_bucket[5m]))"),
            ["gc_undefined_bucket"]
        );
        assert_eq!(
            refs("sum(gc_a_total) / sum(gc_b_total)"),
            ["gc_a_total", "gc_b_total"]
        );
    }

    #[test]
    fn a_brace_inside_a_string_does_not_swallow_a_metric() {
        assert_eq!(
            refs(r#"label_replace(gc_a, "dst", "{", "src", ".*") + gc_undefined_total{x="y"}"#),
            ["gc_a", "gc_undefined_total"]
        );
    }

    #[test]
    fn a_modifier_inside_a_quoted_value_is_inert() {
        assert_eq!(
            refs(r#"x{reason="by(gc_y)"} + gc_real_total"#),
            ["gc_real_total"]
        );
    }

    #[test]
    fn comments_are_ignored_in_multi_line_exprs() {
        assert_eq!(
            refs("sum by(gc_view) (\n  increase(gc_x_total[5m]) # gc_view gc_ghost_total\n)"),
            ["gc_x_total"]
        );
    }

    #[test]
    fn string_args_are_not_selectors() {
        assert_eq!(
            refs(r#"label_replace(gc_a_total, "dst", "gc_not_a_metric", "src", "x")"#),
            ["gc_a_total"]
        );
    }

    /// KNOWN LIMIT (documented on `service_metric_refs`): a metric selected
    /// only by `__name__` inside a matcher is not seen.
    #[test]
    fn name_matcher_is_a_known_limit() {
        assert!(refs(r#"{__name__="gc_x_total"}"#).is_empty());
    }

    #[test]
    fn mask_removes_comments_and_blanks_strings() {
        let m = mask("sum(x{reason=\"by(a)\"}) # dt_client_ghost\n");
        assert!(!m.code.contains("dt_client_ghost"));
        assert!(m.code.contains("by(a)"));
        assert!(!m.masked.contains("by(a)"));
        assert_eq!(m.code.chars().count(), m.masked.chars().count());
    }

    #[test]
    fn modifier_list_is_the_regex_source() {
        for kw in LABEL_LIST_MODIFIERS {
            let expr = format!("sum {kw}(gc_label) (gc_x_total)");
            assert_eq!(refs(&expr), ["gc_x_total"], "{kw}");
        }
        // Identifier boundaries: a metric merely CONTAINING a keyword is kept.
        assert_eq!(refs("gc_by_total"), ["gc_by_total"]);
    }
}
