//! Shared regex for parsing `docs/observability/metrics/*.md` catalog files.
//!
//! Per @dry-reviewer F-DRY-2 2026-05-19: `application_metrics::CATALOG_HEAD_RE`
//! and `dashboard_panels::CATALOG_HEAD_RE` were byte-identical `Lazy<Regex>`
//! statics walking the same metric-catalog markdown. Consolidated here.
//!
//! This module ALSO owns the per-metric **annotation reader** consumed by BOTH
//! `counter_zero_init` (the `Zero-init: exempt` marker) and `dashboard_panels`
//! (the `Expected-empty: yes` marker), so the two guards read one catalog with
//! one parser and cannot disagree about which counters are exempt / expected-
//! empty. A metric can carry BOTH markers at once — in fact the exempt
//! (unbounded-domain) counters ARE the absent-when-healthy set — so the two
//! markers are modelled as TWO INDEPENDENT tri-state fields, never one enum
//! (a single-variant-per-marker enum would silently drop one by parse order).

use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;

use crate::ignore::is_lazy_reason;

/// Matches `### \`metric_name\`` heading lines in metric-catalog markdown.
/// Capture group 1 = the metric name (snake_case lowercase).
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
pub static CATALOG_HEAD_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?m)^###\s+`([a-z_][a-z0-9_]*)`").expect("static pattern compiles"));

/// Any markdown heading line (`#`..`######`). Used to close a metric's section:
/// a `## Section` / non-metric `### Heading` after a metric ends its span, so a
/// marker in a later prose section is not misattributed to the prior metric.
///
/// NOTE (deliberate, infra): this closes a metric's section on ANY heading level
/// including `####`, so a `#### Subheading` under a metric would orphan every
/// marker below it. Acceptable ONLY because no catalog file uses `####` today
/// (0 occurrences across all five). The two guards read the marker in OPPOSITE
/// directions, so an orphaned marker fails LOUD in one and SILENT in the other
/// (the P7 asymmetry): an orphaned `Zero-init: exempt` makes the counter no
/// longer exempt → GUARD 1 demands zero-init → **red, visible**; but an orphaned
/// `Expected-empty: yes` makes GUARD 2 STOP requiring the description marker on
/// that counter's panels → a **SILENT relaxation no current assertion catches**
/// (the collection-level non-empty check does not help — the map stays
/// non-empty). If a `####` is ever introduced, THAT is what breaks.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static ANY_HEADING_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*#{1,6}\s").expect("static pattern compiles"));

/// `- **Zero-init**: exempt <sep> <reason>`. Group 1 = the tail after `exempt`
/// (may be empty). The separator (em/en-dash, hyphen, colon) + reason are peeled
/// off `tail` by [`reason_tail`].
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static ZERO_INIT_EXEMPT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?im)^\s*-\s*\*\*Zero-init\*\*\s*:\s*exempt\b(.*)$")
        .expect("static pattern compiles")
});

/// `- **Expected-empty**: yes <sep> <reason>`. Group 1 = the tail after `yes`.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static EXPECTED_EMPTY_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?im)^\s*-\s*\*\*Expected-empty\*\*\s*:\s*yes\b(.*)$")
        .expect("static pattern compiles")
});

/// One marker's tri-state for one metric. The absent-vs-present-but-unset seam
/// (infra P7): a caller MUST be able to tell "no marker" from "marker present
/// but reason blank/lazy". `Malformed` is **fail-closed** — it never satisfies
/// the marker, so a blank/misspelled exemption resolves to REQUIRED, never to
/// silently-exempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Marker {
    /// No marker line for this metric.
    Absent,
    /// Marker present with a substantive reason.
    Present(String),
    /// Marker present but reason blank or lazy (`is_lazy_reason`) — fail-closed.
    Malformed(String),
}

impl Marker {
    /// True only when the marker is present AND its reason is substantive.
    #[must_use]
    pub fn is_present(&self) -> bool {
        matches!(self, Marker::Present(_))
    }

    /// True when a marker line exists at all (present or malformed) — used to
    /// distinguish "no marker" from "marker present but rejected".
    #[must_use]
    pub fn is_declared(&self) -> bool {
        !matches!(self, Marker::Absent)
    }
}

/// Two independent tri-state annotation fields for one metric.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetricAnnotations {
    pub zero_init_exempt: Marker,
    pub expected_empty: Marker,
}

impl Default for MetricAnnotations {
    fn default() -> Self {
        Self {
            zero_init_exempt: Marker::Absent,
            expected_empty: Marker::Absent,
        }
    }
}

/// Peel the separator + reason off a marker tail (`" — reason"`, `": reason"`,
/// `" - reason"`), classify via `is_lazy_reason`. An empty/blank tail is
/// `Malformed` (a marker line with no reason at all).
fn classify_tail(tail: &str) -> Marker {
    // Strip a leading separator run (em-dash, en-dash, hyphen, colon) + spaces.
    let reason = tail.trim().trim_start_matches(['—', '–', '-', ':']).trim();
    if reason.is_empty() || is_lazy_reason(reason) {
        Marker::Malformed(reason.to_string())
    } else {
        Marker::Present(reason.to_string())
    }
}

/// Parse per-metric annotations out of ONE catalog markdown source.
///
/// Sectioning: a marker line is attributed to the metric whose `### \`name\``
/// heading most recently preceded it; any other heading (`##`, or a `###` that
/// is not a metric heading) closes the section so a marker in a later prose
/// block is not misattributed. A metric with no markers is simply absent from
/// the map (callers treat a missing entry as [`MetricAnnotations::default`]).
#[must_use]
pub fn parse_annotations(src: &str) -> HashMap<String, MetricAnnotations> {
    let mut out: HashMap<String, MetricAnnotations> = HashMap::new();
    let mut current: Option<String> = None;
    for line in src.lines() {
        if let Some(caps) = CATALOG_HEAD_RE.captures(line) {
            current = caps.get(1).map(|m| m.as_str().to_string());
            continue;
        }
        if ANY_HEADING_RE.is_match(line) {
            // A non-metric heading ends the current metric's section.
            current = None;
            continue;
        }
        let Some(name) = current.as_ref() else {
            continue;
        };
        if let Some(caps) = ZERO_INIT_EXEMPT_RE.captures(line) {
            let tail = caps.get(1).map_or("", |m| m.as_str());
            let field = &mut out.entry(name.clone()).or_default().zero_init_exempt;
            *field = redeclare_fail_closed(field, classify_tail(tail), "Zero-init");
        } else if let Some(caps) = EXPECTED_EMPTY_RE.captures(line) {
            let tail = caps.get(1).map_or("", |m| m.as_str());
            let field = &mut out.entry(name.clone()).or_default().expected_empty;
            *field = redeclare_fail_closed(field, classify_tail(tail), "Expected-empty");
        }
    }
    out
}

/// A SECOND marker line of the same kind on one metric is itself a defect (the
/// catalog is then ambiguous about a policy a guard enforces) — fail closed: a
/// redeclaration yields `Malformed`, so a later `Present` can NEVER overwrite an
/// earlier `Malformed` into silently-exempt (infra F2).
fn redeclare_fail_closed(existing: &Marker, incoming: Marker, kind: &str) -> Marker {
    if existing.is_declared() {
        Marker::Malformed(format!(
            "duplicate {kind} marker on one metric — ambiguous, rejected"
        ))
    } else {
        incoming
    }
}

/// Read every `*.md` catalog under `dir` and merge their per-metric annotations
/// into ONE map, so both guards consume a single directory-level parse (F-DRY-2)
/// and one merge policy.
///
/// Returns `Err` on ANY I/O failure — a missing directory or a catalog file we
/// cannot read is a **precondition failure, not an empty result** (infra F5): a
/// silently-dropped file would make that service's annotations vanish while the
/// map stays non-empty, relaxing a policy with nothing red. A cross-file
/// redeclaration of one metric's marker is treated fail-closed, the SAME policy
/// as the in-file case (infra F4): a colliding declaration downgrades the field
/// to `Malformed` rather than silently first-wins.
pub fn parse_annotations_dir(
    dir: &std::path::Path,
) -> anyhow::Result<HashMap<String, MetricAnnotations>> {
    use anyhow::Context as _;
    let mut merged: HashMap<String, MetricAnnotations> = HashMap::new();
    let entries =
        std::fs::read_dir(dir).with_context(|| format!("read catalog dir {}", dir.display()))?;
    let mut paths: Vec<std::path::PathBuf> = Vec::new();
    for entry in entries {
        let entry =
            entry.with_context(|| format!("read catalog dir entry in {}", dir.display()))?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("md") {
            paths.push(path);
        }
    }
    paths.sort(); // deterministic order
    for path in paths {
        let src = std::fs::read_to_string(&path)
            .with_context(|| format!("read catalog file {}", path.display()))?;
        for (name, incoming) in parse_annotations(&src) {
            merge_fail_closed(&mut merged, name, incoming);
        }
    }
    Ok(merged)
}

/// Merge one metric's annotations into `map` with the SAME fail-closed policy as
/// an in-file redeclaration (infra F4): a cross-file collision on a field that is
/// already declared downgrades it to `Malformed`, never silently first-wins.
fn merge_fail_closed(
    map: &mut HashMap<String, MetricAnnotations>,
    name: String,
    incoming: MetricAnnotations,
) {
    let cur = map.entry(name).or_default();
    if incoming.zero_init_exempt.is_declared() {
        cur.zero_init_exempt = redeclare_fail_closed(
            &cur.zero_init_exempt,
            incoming.zero_init_exempt,
            "Zero-init",
        );
    }
    if incoming.expected_empty.is_declared() {
        cur.expected_empty = redeclare_fail_closed(
            &cur.expected_empty,
            incoming.expected_empty,
            "Expected-empty",
        );
    }
}

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

    // ---- annotation reader ----

    #[test]
    fn parses_zero_init_exempt_with_substantive_reason() {
        let src = "### `gc_http_requests_total`\n- **Zero-init**: exempt — status_code is a raw u16 from the HTTP layer, an unbounded runtime-discovered domain\n";
        let ann = parse_annotations(src);
        let e = &ann["gc_http_requests_total"];
        assert!(e.zero_init_exempt.is_present());
        assert_eq!(e.expected_empty, Marker::Absent);
    }

    #[test]
    fn blank_or_lazy_exempt_reason_is_malformed_not_present() {
        // blank reason
        let a = parse_annotations("### `x_total`\n- **Zero-init**: exempt —\n");
        assert!(matches!(
            a["x_total"].zero_init_exempt,
            Marker::Malformed(_)
        ));
        assert!(!a["x_total"].zero_init_exempt.is_present());
        assert!(a["x_total"].zero_init_exempt.is_declared());
        // lazy reason (< 10 chars / vocabulary)
        let b = parse_annotations("### `y_total`\n- **Zero-init**: exempt — todo\n");
        assert!(matches!(
            b["y_total"].zero_init_exempt,
            Marker::Malformed(_)
        ));
    }

    #[test]
    fn metric_can_carry_both_markers_at_once() {
        // The case the two-field struct exists for — a single enum would drop one
        // by parse order. DO NOT collapse these into one field.
        let src = "### `mc_actor_panics_total`\n\
                   - **Zero-init**: exempt — reserved counter with no emit site, see errors.rs\n\
                   - **Expected-empty**: yes — any actor panic is a bug so this reads zero when healthy\n";
        let e = &parse_annotations(src)["mc_actor_panics_total"];
        assert!(e.zero_init_exempt.is_present(), "zero_init field populated");
        assert!(
            e.expected_empty.is_present(),
            "expected_empty field populated"
        );
    }

    #[test]
    fn marker_in_later_section_not_misattributed() {
        // DIFFERENTIAL (infra F1): the same marker line is attributed to the
        // metric when it is inside its section, and to NOBODY when a `##` heading
        // closes the section first. The `with_section` half is a positive control
        // — this test cannot pass vacuously if the marker regex breaks, because
        // that half demands a positive match.
        let marker = "- **Zero-init**: exempt — a real substantive exemption reason here";
        let with_heading = format!("### `a_total`\nprose\n\n## Unrelated Section\n{marker}\n");
        let without_heading = format!("### `a_total`\nprose\n{marker}\n");

        let a = parse_annotations(&with_heading);
        assert!(
            !a.contains_key("a_total"),
            "marker under a later `##` heading must attribute to nobody"
        );
        let b = parse_annotations(&without_heading);
        assert!(
            b["a_total"].zero_init_exempt.is_present(),
            "the SAME marker inside the section must attribute to a_total (positive control)"
        );
    }

    #[test]
    fn duplicate_marker_on_one_metric_fails_closed() {
        // infra F2: a blank (Malformed) marker followed by a good one must NOT
        // silently become exempt — a redeclaration is itself rejected.
        let src = "### `x_total`\n\
                   - **Zero-init**: exempt —\n\
                   - **Zero-init**: exempt — a perfectly substantive reason that would pass alone\n";
        let e = &parse_annotations(src)["x_total"];
        assert!(
            matches!(e.zero_init_exempt, Marker::Malformed(_)),
            "a second Zero-init marker must fail closed, not last-win into Present"
        );
        assert!(!e.zero_init_exempt.is_present());
    }

    #[test]
    fn no_markers_yields_absent_or_missing() {
        let ann = parse_annotations("### `plain_total`\n- **Type**: Counter\n");
        // Metric present in catalog but no markers -> not in the annotation map.
        assert!(!ann.contains_key("plain_total"));
    }

    #[test]
    fn cross_file_redeclaration_merges_fail_closed() {
        // infra F4: the SAME metric declared exempt in two files (e.g. a good
        // reason then a blank one) must NOT silently first-win into Present — a
        // colliding declaration downgrades to Malformed, matching the in-file rule.
        let mut map: HashMap<String, MetricAnnotations> = HashMap::new();
        merge_fail_closed(
            &mut map,
            "m_total".to_string(),
            MetricAnnotations {
                zero_init_exempt: Marker::Present("a good substantive reason here".to_string()),
                expected_empty: Marker::Absent,
            },
        );
        // second file redeclares the same field (even with a good reason) → Malformed.
        merge_fail_closed(
            &mut map,
            "m_total".to_string(),
            MetricAnnotations {
                zero_init_exempt: Marker::Present("another reason from a second file".to_string()),
                expected_empty: Marker::Absent,
            },
        );
        assert!(matches!(
            map["m_total"].zero_init_exempt,
            Marker::Malformed(_)
        ));
        assert!(!map["m_total"].zero_init_exempt.is_present());
    }

    #[test]
    fn expected_empty_yes_parsed() {
        let src = "### `mc_session_join_failures_total`\n- **Expected-empty**: yes — no joins fail on a healthy idle cluster so this series reads zero\n";
        let e = &parse_annotations(src)["mc_session_join_failures_total"];
        assert!(e.expected_empty.is_present());
        assert_eq!(e.zero_init_exempt, Marker::Absent);
    }
}
