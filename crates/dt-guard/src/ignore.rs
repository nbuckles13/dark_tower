//! Canonical home for `guard:ignore` marker parsing + lazy-reason rejection.
//!
//! Per ADR-0034 §6 (dry-reviewer R1 consolidation): a single home for
//! `LAZY_REASON_RE` + `IGNORE_MARKER_RE` shared across cite-extract
//! (this module's primary consumer), alert-rules' `load_ignore_lines`
//! (Bundle 5a), and metric-labels' `# pii-safe` parser (Bundle 5c).
//!
//! Re-inlining either regex outside this module trips the workspace
//! `clippy.toml disallowed_methods` ban on `regex::Regex::new` and is a
//! Gate 2 reject.

use once_cell::sync::Lazy;
use regex::Regex;

/// Minimum length for a non-lazy `guard:ignore` reason. Matches Python
/// `_MIN_REASON_LEN = 10` from `scripts/guards/lib/doc_cite_extract.py`.
pub const MIN_REASON_LEN: usize = 10;

/// Lazy-reason vocabulary. Reasons matching this regex (case-insensitive)
/// are rejected even if they meet the length floor — they signal the
/// author hasn't actually justified the bypass.
///
/// Ported verbatim from `_LAZY_REASON_RE` in the Python kernel.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
pub static LAZY_REASON_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^(test|tmp|todo|fix ?me|wip)\b").expect("static pattern compiles")
});

/// HTML-comment `guard:ignore` marker for markdown docs.
/// `<!-- guard:ignore(<reason>) -->` — captures the reason.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
pub static IGNORE_MARKER_HTML_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"<!--\s*guard:ignore\(\s*([^)]+?)\s*\)\s*-->").expect("static pattern compiles")
});

/// Hash-comment `guard:ignore` marker for YAML/Rust/shell sources.
/// `# guard:ignore(<reason>)` — captures the reason.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
pub static IGNORE_MARKER_HASH_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"#\s*guard:ignore\(\s*([^)]+?)\s*\)").expect("static pattern compiles")
});

/// Return `true` if `text` is too vague to honor as a `guard:ignore` reason.
///
/// Lazy reasons are < 10 chars OR match the vocabulary `test|tmp|todo|fix ?me|wip`
/// (case-insensitive). Per ADR §6 — three consumers (cite-extract, alert-rules,
/// metric-labels) must call this function, not re-implement the check.
pub fn is_lazy_reason(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.len() < MIN_REASON_LEN {
        return true;
    }
    LAZY_REASON_RE.is_match(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lazy_short_reasons_rejected() {
        assert!(is_lazy_reason(""));
        assert!(is_lazy_reason("short"));
        assert!(is_lazy_reason("123456789")); // 9 chars
        assert!(!is_lazy_reason("1234567890")); // exactly 10 chars and not vocab
    }

    #[test]
    fn lazy_vocabulary_rejected() {
        assert!(is_lazy_reason("test this thing"));
        assert!(is_lazy_reason("TEST CASE FAILING"));
        assert!(is_lazy_reason("todo: come back later"));
        assert!(is_lazy_reason("tmp workaround for issue"));
        assert!(is_lazy_reason("fixme need to refactor"));
        assert!(is_lazy_reason("fix me later please"));
        assert!(is_lazy_reason("wip don't merge yet"));
    }

    #[test]
    fn vocabulary_substring_not_rejected() {
        // `testing` is not a vocabulary match — `\b` ensures word boundary.
        // The vocabulary set is {test, tmp, todo, fix me, wip}; `testing`
        // would match `test` without `\b` but the boundary anchor prevents that.
        assert!(!is_lazy_reason("testimony from production debugging"));
        assert!(!is_lazy_reason("temporary workaround documented"));
    }

    #[test]
    fn accepts_meaningful_reasons() {
        assert!(!is_lazy_reason(
            "citing removed method from removed pr 12345"
        ));
        assert!(!is_lazy_reason("exception per security 2026-04-17 ruling"));
    }

    #[test]
    fn trims_whitespace() {
        assert!(is_lazy_reason("  short  "));
        assert!(!is_lazy_reason("  a valid reason text  "));
    }
}
