//! Prometheus duration parsing.
//!
//! Ports `parse_prometheus_duration` from `validate-alert-rules.sh` Python
//! kernel L112-127. Used by `alert_rules::validate_for` (for-floor check) and
//! `find_qualifying_expr_window` (rate-window exemption check).

use once_cell::sync::Lazy;
use regex::Regex;

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static DURATION_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(\d+)([smhdwy])").expect("static pattern compiles"));

/// Parse a Prometheus duration string (e.g. `"30s"`, `"5m"`, `"1h30m"`) to seconds.
///
/// Returns `None` if the input is unparseable, empty, or contains trailing
/// non-duration characters. Multi-unit strings are summed: `"1h30m" → 5400`.
///
/// Matches the Python kernel's behavior verbatim, including the "concatenated
/// match must equal input" check that rejects `"30sjunk"`.
pub fn parse_prometheus_duration(s: &str) -> Option<u64> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut total: u64 = 0;
    let mut matched = String::new();
    for caps in DURATION_RE.captures_iter(trimmed) {
        let num_str = caps.get(1)?.as_str();
        let unit = caps.get(2)?.as_str();
        let num: u64 = num_str.parse().ok()?;
        let mult: u64 = match unit {
            "s" => 1,
            "m" => 60,
            "h" => 3600,
            "d" => 86400,
            "w" => 604800,
            "y" => 31_536_000,
            _ => return None,
        };
        total = total.checked_add(num.checked_mul(mult)?)?;
        matched.push_str(num_str);
        matched.push_str(unit);
    }
    if matched != trimmed {
        return None;
    }
    Some(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_unit() {
        assert_eq!(parse_prometheus_duration("30s"), Some(30));
        assert_eq!(parse_prometheus_duration("5m"), Some(300));
        assert_eq!(parse_prometheus_duration("1h"), Some(3600));
        assert_eq!(parse_prometheus_duration("1d"), Some(86_400));
        assert_eq!(parse_prometheus_duration("1w"), Some(604_800));
        assert_eq!(parse_prometheus_duration("1y"), Some(31_536_000));
    }

    #[test]
    fn parses_multi_unit() {
        assert_eq!(parse_prometheus_duration("1h30m"), Some(5400));
        assert_eq!(parse_prometheus_duration("2h15m45s"), Some(8145));
    }

    #[test]
    fn rejects_unparseable() {
        assert_eq!(parse_prometheus_duration(""), None);
        assert_eq!(parse_prometheus_duration("   "), None);
        assert_eq!(parse_prometheus_duration("30"), None); // missing unit
        assert_eq!(parse_prometheus_duration("30sjunk"), None); // trailing junk
        assert_eq!(parse_prometheus_duration("30x"), None); // bad unit
        assert_eq!(parse_prometheus_duration("foo"), None);
    }

    #[test]
    fn trims_whitespace() {
        assert_eq!(parse_prometheus_duration("  30s  "), Some(30));
    }
}
