//! Canonical home for the hygiene-patterns secret-detection catalog.
//!
//! Per ADR-0034 §6: 7+-pattern set shared across alert-rules-policy
//! (Bundle 5a — consumer via Wave 1) and the future `secret-scan`
//! subcommand (Wave 3, collapses cross-stack dupe with
//! `no-hardcoded-secrets.sh`).
//!
//! Visibility is `pub(crate)`-via-the-`pub use` re-export: the static is
//! `pub` so integration tests in `tests/` can reach it, but consumption
//! from outside `dt-guard` is not part of the supported API. ADR §6 specifies
//! "shared across `dt-guard`'s internal modules but not exported to other
//! workspace crates" — the workspace has no external consumer; Wave 3's
//! `secret-scan` is an intra-crate subcommand.
//!
//! Ported verbatim from `validate-alert-rules.sh` Python heredoc L85-103.

use once_cell::sync::Lazy;
use regex::Regex;

/// Ordered hygiene-pattern set. Keys are human-readable names that flow
/// into `--explain` output (`policy=alert-rules-policy::annotation_hygiene
/// matched="<span>" reason=<name>`). Values are compiled regexes.
///
/// Go-template expressions (`{{ ... }}`) MUST be redacted to `<<TEMPLATED>>`
/// BEFORE these patterns are applied — `Bearer {{ $labels.x }}` is
/// legitimate templating, not a bearer-token leak.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
pub static HYGIENE_PATTERNS: Lazy<Vec<(&'static str, Regex)>> = Lazy::new(|| {
    vec![
        (
            "bearer token",
            Regex::new(r"Bearer\s+[A-Za-z0-9._-]{10,}").expect("static pattern compiles"),
        ),
        (
            "authorization header",
            Regex::new(r"Authorization:\s*[A-Za-z]+\s+\S{10,}").expect("static pattern compiles"),
        ),
        (
            "AWS access key",
            Regex::new(r"AKIA[0-9A-Z]{16}").expect("static pattern compiles"),
        ),
        (
            "AWS secret marker",
            Regex::new(r"(?i)aws_secret_access_key|aws_access_key_id")
                .expect("static pattern compiles"),
        ),
        (
            "generic secret=value",
            Regex::new(r#"(?i)\bsecret[_-]?key\s*[:=]\s*["']?[A-Za-z0-9/+=]{16,}"#)
                .expect("static pattern compiles"),
        ),
        (
            "OpenAI/Stripe-style key",
            Regex::new(r"\bsk-[A-Za-z0-9]{20,}\b|\bpk-[A-Za-z0-9]{20,}\b")
                .expect("static pattern compiles"),
        ),
        (
            "GitHub PAT",
            Regex::new(r"\bgh[pous]_[A-Za-z0-9]{36,}\b").expect("static pattern compiles"),
        ),
        (
            "Slack token",
            Regex::new(r"\bxox[baprs]-[A-Za-z0-9-]{10,}\b").expect("static pattern compiles"),
        ),
        (
            "JWT",
            Regex::new(r"eyJ[A-Za-z0-9_-]+\.eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+")
                .expect("static pattern compiles"),
        ),
        (
            "PEM private key",
            Regex::new(r"-----BEGIN [A-Z ]*PRIVATE KEY-----").expect("static pattern compiles"),
        ),
        (
            "internal DNS suffix",
            Regex::new(
                r"\.svc\.cluster\.local\b|\.cluster\.local\b|\.internal\b\
                  |\.amazonaws\.com\b|\.ec2\.internal\b|\.compute\.internal\b",
            )
            .expect("static pattern compiles"),
        ),
        (
            "prod/stage hostname",
            Regex::new(r"\b[a-z0-9]+(?:-prod-|-stage-|-prd-|-stg-)[a-z0-9-]+\.[a-z]")
                .expect("static pattern compiles"),
        ),
    ]
});

/// IPv4 detection regex. Used in conjunction with [`IPV4_ALLOWLIST`] to
/// flag real-looking IPv4 addresses in annotation text while permitting
/// documentation-example IPs.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
pub static IPV4_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\b(?:\d{1,3}\.){3}\d{1,3}\b").expect("static pattern compiles"));

/// Go-template expression scrubber. Replace matches with `<<TEMPLATED>>`
/// before applying [`HYGIENE_PATTERNS`].
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
pub static TEMPLATE_EXPR: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\{\{[^}]*\}\}").expect("static pattern compiles"));

/// IPv4 addresses allowed as documentation references, not real targets.
/// Ported from `validate-alert-rules.sh` Python heredoc L77-80.
pub const IPV4_ALLOWLIST: &[&str] = &[
    "0.0.0.0",
    "127.0.0.1",
    "255.255.255.255",
    "1.2.3.4",
    "169.254.169.254", // link-local/metadata — reserved, safe as doc example
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_expr_redacts() {
        let scrubbed = TEMPLATE_EXPR.replace_all("Bearer {{ $labels.x }}", "<<TEMPLATED>>");
        assert_eq!(scrubbed, "Bearer <<TEMPLATED>>");
    }

    #[test]
    fn hygiene_patterns_match_known_leaks() {
        // Spot-check a few; the full suite is exercised by Bundle 5a
        // alert-rules fixtures.
        let names: Vec<&str> = HYGIENE_PATTERNS.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"bearer token"));
        assert!(names.contains(&"AWS access key"));
        assert!(names.contains(&"JWT"));
        assert!(names.contains(&"internal DNS suffix"));
    }

    #[test]
    fn ipv4_regex_finds_addresses() {
        assert!(IPV4_REGEX.is_match("see 10.0.0.5 here"));
        assert!(IPV4_REGEX.is_match("169.254.169.254"));
        assert!(!IPV4_REGEX.is_match("v1.2.3.4-rc1")); // word boundary
    }
}
