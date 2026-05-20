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
static MACRO_NAME_ALTERNATION: Lazy<String> = Lazy::new(|| {
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
