//! Canonical home for the `metrics` crate macro-invocation parser.
//!
//! Per ADR-0034 §1 + @dry-reviewer concern #1: a single home for the
//! `(?:metrics::)?(?:counter|gauge|histogram)!` (plus `describe_*!`)
//! invocation finder + first-arg extractor. Three Wave 2 subcommands
//! consume this — `application_metrics`, `metric_labels`,
//! `infrastructure_metrics` — instead of each re-inlining its own
//! `Lazy<Regex>` (which would defeat ADR §6's "structural duplication
//! impossible by construction").
//!
//! Scaffold only in Bundle 1 — the balanced-paren walker + string-literal
//! parser land with the first consumer (Bundle 5c metric-labels).

use once_cell::sync::Lazy;
use regex::Regex;

/// Discriminated `metrics` crate macro kind. Per @dry-reviewer 2026-05-19
/// ergonomic note — replaces stringly-typed `macro_name` matching with
/// compile-time-exhaustive variants. Consumers match-arm on `kind` and the
/// compiler flags any new variant that lands without a handler.
///
/// Per @team-lead E-DRY-1 fold-in 2026-05-19: this enum is the single source
/// of truth for the macro family. Both the `LABEL_MACROS` / `DESCRIBE_MACROS`
/// slice accessors and the regex alternation in `MACRO_INVOCATION_RE` /
/// `MACRO_INVOCATION_WITH_FIRST_ARG_RE` are derived from `Self::ALL` below.
/// Adding a new variant auto-extends every downstream consumer.
///
/// `is_describe()` is the canonical Cat A / Cat B classifier — describe-*
/// macros take a literal first arg + description and don't bear runtime
/// labels; the base macros (`counter!`/`gauge!`/`histogram!`) bear
/// arbitrary `"label" => value` pairs and need PII / cardinality checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacroKind {
    Counter,
    Gauge,
    Histogram,
    DescribeCounter,
    DescribeGauge,
    DescribeHistogram,
}

impl MacroKind {
    /// All variants in regex-alternation order — describe-* first so the
    /// regex engine prefers the longer match at each position (`describe_counter!`
    /// is NOT a `counter!` invocation with `describe_` junk).
    ///
    /// This is the SoT array consumed by `LABEL_MACROS` / `DESCRIBE_MACROS`
    /// slice accessors and by the `Lazy<Regex>` initializers below.
    pub const ALL: &'static [Self] = &[
        Self::DescribeCounter,
        Self::DescribeGauge,
        Self::DescribeHistogram,
        Self::Counter,
        Self::Gauge,
        Self::Histogram,
    ];

    /// Parse from the macro-name string emitted by `MACRO_INVOCATION_RE`
    /// capture group 2. Returns `None` for anything outside the known set
    /// (regex won't normally emit such a value, but `None` keeps the parser
    /// boundary explicit).
    ///
    /// Named `parse` rather than `from_str` to avoid the `FromStr` trait
    /// signature collision — `std::str::FromStr::from_str` returns
    /// `Result<Self, Self::Err>`, this returns `Option<Self>` because the
    /// only failure mode is "not a known macro" and a typed Err would add
    /// nothing actionable.
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|k| k.as_str() == s)
    }

    /// True for `describe_*` variants (Cat A — length-checked literal,
    /// no runtime labels). False for base macros (Cat B — label-bearing).
    pub const fn is_describe(self) -> bool {
        matches!(
            self,
            Self::DescribeCounter | Self::DescribeGauge | Self::DescribeHistogram
        )
    }

    /// The bare-string form (matches the regex capture). Used in error
    /// messages and the `<macro>!` interpolation in parse-error diagnostics.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Counter => "counter",
            Self::Gauge => "gauge",
            Self::Histogram => "histogram",
            Self::DescribeCounter => "describe_counter",
            Self::DescribeGauge => "describe_gauge",
            Self::DescribeHistogram => "describe_histogram",
        }
    }
}

/// Regex alternation built from `MacroKind::ALL` at `Lazy::new` time. Used
/// by both opener regexes below so adding a new variant updates the regex
/// automatically. Variants are emitted in `ALL` order (describe-* first)
/// so the regex engine prefers the longer alternative at each position.
///
/// # Blast radius — read this before widening or narrowing `MacroKind`
///
/// FOUR consumers derive from this alternation, two in this module and two
/// outside it. Exposing the alternation rather than having each consumer
/// rebuild its own `.iter().map(as_str).join("|")` is what keeps
/// `MacroKind::ALL` the single source of truth: a seventh variant widens all
/// four with zero edits in their modules — and, by the same mechanism, a
/// DELETED variant narrows all four silently, which is what the hand-written
/// pin in this module (and its sibling in `telemetry_macros`) exists to red.
/// Consumers carry the complementary oracle — a DERIVATION test proving the
/// name still arrives through their own path; see
/// `metric_labels::tests::every_macro_kind_is_discovered_through_the_opener`
/// and `media_telemetry_deny::tests::metrics_family_is_derived_from_macro_kind_all`.
///
/// * `MACRO_INVOCATION_RE` and `MACRO_INVOCATION_WITH_FIRST_ARG_RE`, below —
///   **still `(`-only**, and still narrower than Rust's grammar in both the
///   whitespace and delimiter classes. Tracked in `docs/TODO.md`
///   §Observability Debt; they are the last two anchors in that entry.
/// * `media_telemetry_deny::DENIED_MACRO_RE` (`pub` since 2026-09-07).
/// * `metric_labels::MACRO_OPENER_RE` (since 2026-09-08) — previously a
///   re-inlined literal copy of these six names, which is why the SSoT claim
///   in this doc was true of the docs and false of the code for a while.
///
/// Both external consumers accept whitespace before the `!` and all three
/// delimiters (`(`, `[`, `{`); the two statics below accept neither. That
/// asymmetry is the tracked gap, not a design choice.
pub static MACRO_NAME_ALTERNATION: Lazy<String> = Lazy::new(|| {
    MacroKind::ALL
        .iter()
        .map(|k| k.as_str())
        .collect::<Vec<_>>()
        .join("|")
});

/// Bare-string names for label-bearing macros (`counter!`/`gauge!`/`histogram!`).
/// Derived from `MacroKind::ALL` at first access so the slice and the regex
/// alternation cannot drift.
pub static LABEL_MACROS: Lazy<Vec<&'static str>> = Lazy::new(|| {
    MacroKind::ALL
        .iter()
        .filter(|k| !k.is_describe())
        .map(|k| k.as_str())
        .collect()
});

/// Bare-string names for describe macros (`describe_counter!` / etc.).
/// Derived from `MacroKind::ALL` at first access so the slice and the regex
/// alternation cannot drift.
pub static DESCRIBE_MACROS: Lazy<Vec<&'static str>> = Lazy::new(|| {
    MacroKind::ALL
        .iter()
        .filter(|k| k.is_describe())
        .map(|k| k.as_str())
        .collect()
});

/// Opener regex for any `metrics` macro invocation. Captures:
///   - group 1: optional `metrics::` prefix (presence is informational)
///   - group 2: macro name (one of `LABEL_MACROS` / `DESCRIBE_MACROS`)
///
/// Used to seed the balanced-paren walker that extracts the call body.
/// Pattern matches Python `validate-application-metrics.sh` heredoc line:
///   `(?:\bmetrics\s*::\s*)?\b(?:describe_counter|...|counter|gauge|histogram)!\s*\(`
///
/// Ordering puts `describe_*` before bare `counter` so the regex engine
/// prefers the longer match at each position.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
pub static MACRO_INVOCATION_RE: Lazy<Regex> = Lazy::new(|| {
    let pattern = format!(
        r"(?:\b(metrics)\s*::\s*)?\b({})!\s*\(",
        *MACRO_NAME_ALTERNATION
    );
    Regex::new(&pattern).expect("static pattern compiles")
});

/// Macro invocation + first-arg string literal in one pass. Consumed by
/// `application_metrics` (needs only the metric name) and `dashboard_panels`
/// (needs both kind + name for the metric-type classifier).
///
/// Captures: group 1 = optional `metrics::` prefix; group 2 = macro kind
/// (one of `LABEL_MACROS` / `DESCRIBE_MACROS`); group 3 = first-arg string
/// literal value (the metric name in snake_case).
///
/// `(?s)` flag (DOTALL) lets `.` match newlines so a `counter!("name", ...)`
/// invocation spread across multiple lines still resolves the first arg.
/// Per @dry-reviewer F-DRY-1 2026-05-19: replaces 2 byte-similar Lazy<Regex>
/// statics in `application_metrics` (`METRIC_NAME_RE`) and `dashboard_panels`
/// (`MACRO_NAME_RE`), each of which routed around the canonical
/// `MACRO_INVOCATION_RE` opener.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
pub static MACRO_INVOCATION_WITH_FIRST_ARG_RE: Lazy<Regex> = Lazy::new(|| {
    let pattern = format!(
        r#"(?s)(?:\b(metrics)\s*::\s*)?\b({})!\s*\(\s*"([a-z_][a-z0-9_]*)""#,
        *MACRO_NAME_ALTERNATION
    );
    Regex::new(&pattern).expect("static pattern compiles")
});

/// One macro invocation parsed from Rust source. Consumers (`metric_labels`,
/// `application_metrics`, `infrastructure_metrics`) walk a `Vec<MacroInvocation>`
/// returned from a single source-walk per file.
///
/// Body extraction (balanced-paren walker + first-arg string literal) lands
/// with the first consumer; this struct is the contract.
#[derive(Debug, Clone)]
pub struct MacroInvocation {
    /// Discriminated macro family — replaces stringly-typed `macro_name`
    /// per @dry-reviewer 2026-05-19. Consumers match-arm on `kind`;
    /// `kind.is_describe()` is the Cat A / Cat B classifier.
    pub kind: MacroKind,
    /// First-arg string literal value, or `None` if the first arg is not a
    /// literal (e.g. a const or variable reference).
    pub metric_name: Option<String>,
    /// Raw body inside the outer `()`, suitable for `split_top_level_args`.
    pub body: String,
    /// 1-based source line of the opening `<macro>!(`.
    pub start_line: usize,
    /// 0-based source col of the opening `<macro>!(`.
    pub start_col: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ANTI-NARROWING PIN for the `MacroKind::ALL` derivation (2026-09-08).
    ///
    /// Lives HERE, at the source of truth, and not in a consumer. It was
    /// briefly written in `metric_labels`' test module, which is the consumer
    /// that motivated it — but the person who deletes a `MacroKind` variant is
    /// editing THIS file and would never have seen it there, and their natural
    /// next move would be to add a second pin beside the enum. Its own cited
    /// precedent, `telemetry_macros::level_group_membership_is_frozen`, lives
    /// in the module that owns the vocabulary for exactly this reason.
    ///
    /// # DELIBERATE DUPLICATION — not a DRY defect, and not a second
    /// # production encoding
    ///
    /// The literal below is HAND-WRITTEN and must never be built from
    /// `MACRO_NAME_ALTERNATION`, `MacroKind::ALL`, or anything derived from
    /// them. **A pin that compares a derived value against itself passes
    /// unchanged when a variant is DELETED** — i.e. it certifies nothing at
    /// the exact moment it matters. This is a test oracle: no code path
    /// consumes it, so it cannot drift into use; it can only fail. Do not
    /// "fix" it into the derived form — that would be a DRY cleanup deleting
    /// a security control. @dry-reviewer has ruled they will not flag it.
    ///
    /// Precedent: `telemetry_macros::tests::level_group_membership_is_frozen`,
    /// which does the same over a security-owned frozen vocabulary and was
    /// co-signed by @security + @observability.
    ///
    /// # ORDER IS LOAD-BEARING HERE, unlike the `Level` precedent
    ///
    /// `level_group_membership_is_frozen` SORTS before comparing, which is
    /// safe only because its doc rests on the premise that no `Level` member
    /// is a prefix of another. **`MacroKind::ALL` claims the opposite about
    /// itself** (`metric_macros.rs`: describe-\* first "so the regex engine
    /// prefers the longer match at each position"). Sorting this pin would
    /// silently drop an ordering guarantee the enum's own doc calls
    /// load-bearing, so it is asserted in `ALL` order and covers membership
    /// AND order in one assertion. Do not carry the "no member is a prefix of
    /// another" sentence across from `Level` — it is true there and unverified
    /// here.
    #[test]
    fn macro_name_alternation_is_frozen_against_narrowing() {
        assert_eq!(
            *MACRO_NAME_ALTERNATION,
            "describe_counter|describe_gauge|describe_histogram|counter|gauge|histogram",
            "`MacroKind::ALL` changed. If a variant was DELETED this NARROWS \
             `metric_labels` — a PII-relevant detection surface (ADR-0029 \
             bounded labels), whose opener is the discovery loop feeding the \
             label PII checks — plus `application_metrics`, `dashboard_panels`, \
             `metric_coverage` and `histogram_buckets`. If the ORDER changed, `describe_counter!` \
             may now be matched as `counter!` with a `describe_` prefix. \
             Updating this literal to green CI requires @security + \
             @observability sign-off (ADR-0024 §6.2); it is NOT a mechanical fix."
        );
    }

    #[test]
    fn macro_re_matches_bare_and_qualified() {
        assert!(MACRO_INVOCATION_RE.is_match("counter!(\"ok\")"));
        assert!(MACRO_INVOCATION_RE.is_match("metrics::counter!(\"ok\")"));
        assert!(MACRO_INVOCATION_RE.is_match("describe_counter!(\"m\", \"desc\")"));
        assert!(MACRO_INVOCATION_RE.is_match("gauge!(\"g\")"));
        assert!(MACRO_INVOCATION_RE.is_match("histogram!(\"h\")"));
    }

    #[test]
    fn macro_re_prefers_describe_over_counter() {
        // Confirm the alternation order is right: describe_counter! must
        // NOT match as counter! with `describe_` prefix junk.
        let caps = MACRO_INVOCATION_RE
            .captures("describe_counter!(\"m\", \"desc\")")
            .unwrap();
        assert_eq!(caps.get(2).unwrap().as_str(), "describe_counter");
    }

    #[test]
    fn macro_re_rejects_unrelated() {
        assert!(!MACRO_INVOCATION_RE.is_match("vec!(\"ok\")"));
        assert!(!MACRO_INVOCATION_RE.is_match("println!(\"counter!\")"));
        // The string contents of println!() include "counter!" but the macro
        // opener requires the macro name immediately followed by `!\s*\(`,
        // so the regex sees `println!` (not `counter!`) at the start.
    }

    #[test]
    fn kind_parse_handles_all_six_variants_and_rejects_unknown() {
        assert_eq!(MacroKind::parse("counter"), Some(MacroKind::Counter));
        assert_eq!(MacroKind::parse("gauge"), Some(MacroKind::Gauge));
        assert_eq!(MacroKind::parse("histogram"), Some(MacroKind::Histogram));
        assert_eq!(
            MacroKind::parse("describe_counter"),
            Some(MacroKind::DescribeCounter)
        );
        assert_eq!(
            MacroKind::parse("describe_gauge"),
            Some(MacroKind::DescribeGauge)
        );
        assert_eq!(
            MacroKind::parse("describe_histogram"),
            Some(MacroKind::DescribeHistogram)
        );
        assert_eq!(MacroKind::parse("vec"), None);
        assert_eq!(MacroKind::parse(""), None);
    }

    #[test]
    fn kind_is_describe_classifies_cat_a_b() {
        assert!(!MacroKind::Counter.is_describe());
        assert!(!MacroKind::Gauge.is_describe());
        assert!(!MacroKind::Histogram.is_describe());
        assert!(MacroKind::DescribeCounter.is_describe());
        assert!(MacroKind::DescribeGauge.is_describe());
        assert!(MacroKind::DescribeHistogram.is_describe());
    }

    #[test]
    fn kind_as_str_roundtrips_parse() {
        for s in LABEL_MACROS.iter().chain(DESCRIBE_MACROS.iter()) {
            let kind = MacroKind::parse(s).expect("known macro");
            assert_eq!(kind.as_str(), *s, "as_str must roundtrip parse");
        }
    }
}
