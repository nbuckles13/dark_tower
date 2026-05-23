//! Canonical home for the PII / secret-identifier vocabulary.
//!
//! Per ADR-0034 §6 + Wave-2 plan @security F1 + @observability Q1 + @code-reviewer
//! item 2 (Pattern-B trigger when N≥3 consumers share semantic vocabulary).
//!
//! Three Wave-1 + Wave-2 consumers of overlapping PII catalogs converge here:
//! * [`metric_labels`](crate::metric_labels) (Wave 1) — label-key string check.
//! * [`rust_pii`](crate::rust_pii) (Wave 2) — log/tracing macro argument check.
//! * [`ts_pii`](crate::ts_pii) (Wave 2) — TS log call-site check.
//!
//! Plus two secret-identifier consumers:
//! * [`rust_log_secrets`](crate::rust_log_secrets) (Wave 2) — secret variables in
//!   `info!/debug!/etc.` macros.
//! * [`instrument_skip_all`](crate::instrument_skip_all) (Wave 2) — sensitive
//!   parameters under `#[instrument]` without `skip_all`.
//!
//! ## CATEGORY_A — non-bypassable secret identifiers
//!
//! Wave-2 widening per @security F1+F3 (with explicit security sign-off):
//! * `pwd` — bash today's `no-secrets-in-logs.sh` SECRET_PATTERNS identifier.
//! * `cred` — same source.
//! * `bearer` — same source.
//! * `auth_code` — bash today's `instrument-skip-all.sh:108` literal.
//!
//! ## CATEGORY_B — user-PII (hashed-suffix exempt)
//!
//! Wave-2 widening per bash today's `no-pii-in-logs.sh` `PII_PATTERNS`:
//! * `full_name` / `first_name` / `last_name` / `real_name` / `user_name` —
//!   bash today's `PII_PATTERNS` identifiers not in Wave-1 CATEGORY_B.
//!
//! ## Non-consolidation note
//!
//! This module hosts the IDENTIFIER vocabulary (variable names like `password`,
//! `token`, `email`). It is structurally distinct from
//! [`crate::secret_patterns::HYGIENE_PATTERNS`], which matches VALUE SHAPES
//! (JWT bytes, AWS keys, bearer tokens in flight). Two separate catalogs answer
//! two separate questions — DO NOT consolidate.

/// Category A — secret-identifier vocabulary. Non-bypassable, no hashed-suffix
/// exemption. Match scope is identifier-shaped (variable names, `#[instrument]`
/// params, labels). Additions require security sign-off and an entry in
/// [`CATEGORY_A_ALLOWLIST`] for any exception.
///
/// Ordering is alphabetical (within each addition cohort). Wave-1 cohort first,
/// Wave-2 cohort tagged inline so reviewers can scan additions at a glance.
pub(crate) const PII_TOKENS_CATEGORY_A: &[&str] = &[
    // Wave-1 cohort.
    "password",
    "passwd",
    "api_key",
    "apikey",
    "secret",
    // Bare `token` per Lead ruling 2026-04-17.
    "token",
    "bearer_token",
    "access_token",
    "refresh_token",
    "session_token",
    "id_token",
    "private_key",
    "privkey",
    "signing_key",
    "jwt",
    "auth_header",
    "authorization",
    // Wave-2 cohort — @security F1+F3 sign-off 2026-05-20.
    "pwd",
    "cred",
    "bearer",
    "auth_code",
];

/// Identifier names that contain a CATEGORY_A substring but are NOT secrets.
/// Additions require security co-owner sign-off (mirror of Wave-1 Python
/// L128-135 contract).
pub(crate) const CATEGORY_A_ALLOWLIST: &[&str] = &["token_type"];

/// Category B — user-PII vocabulary. Hashed-suffix exempt (see [`HASHED_SUFFIXES`]).
/// Match scope is identifier-shaped.
///
/// Wave-2 widening per @observability Q1 + bash today's `no-pii-in-logs.sh`.
pub(crate) const PII_TOKENS_CATEGORY_B: &[&str] = &[
    // Wave-1 cohort.
    "email",
    "phone",
    "phone_number",
    "display_name",
    "user_id",
    "name",
    "username",
    "nickname",
    "handle",
    // Bare `address` REMOVED per @team-lead 2026-05-21 — over-broad: matches
    // `listen_address` / `metrics_address` config plumbing in service main.rs
    // files (network addresses, not user PII). Replaced by compound forms
    // below. @observability + @security original widening recommendation
    // surfaces here as Gate-2 finding-closure (NOT a re-vote).
    "ip_address",
    "email_address",
    "mac_address",
    "postal_code",
    "zip",
    "zipcode",
    "ip",
    "ip_addr",
    "ipv4",
    "ipv6",
    "device_id",
    "user_agent",
    "fingerprint",
    "ssn",
    "dob",
    "passport",
    "driver_license",
    "credit_card",
    "card_number",
    "cvv",
    "latitude",
    "longitude",
    "geolocation",
    "geoip",
    // Wave-2 cohort — bash `no-pii-in-logs.sh` `PII_PATTERNS` parity.
    "full_name",
    "first_name",
    "last_name",
    "real_name",
    "user_name",
];

/// Prefixes that force-fire as PII regardless of suffix (mirrors Wave-1
/// Python `PII_PREFIX_DENYLIST`).
pub(crate) const PII_PREFIX_DENYLIST: &[&str] = &["raw_"];

/// Suffixes that exempt a CATEGORY_B match (hashed correlation IDs).
/// CATEGORY_A is NOT eligible for the hashed-suffix exemption.
pub(crate) const HASHED_SUFFIXES: &[&str] = &["_hash", "_hashed", "_id_hash", "_sha256", "_digest"];

/// Identifiers that contain a CATEGORY_B substring but are not PII
/// (false-positive suppression). Mirror of Wave-1 `LABEL_ALLOWLIST`.
pub(crate) const LABEL_ALLOWLIST: &[&str] = &[
    "hostname",
    "filename",
    "pathname",
    "typename",
    "nameservice",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_a_includes_wave2_additions() {
        for tok in &["pwd", "cred", "bearer", "auth_code"] {
            assert!(
                PII_TOKENS_CATEGORY_A.contains(tok),
                "expected CATEGORY_A to contain {tok}"
            );
        }
    }

    #[test]
    fn category_a_includes_wave1_baseline() {
        for tok in &["password", "secret", "token", "jwt", "private_key"] {
            assert!(
                PII_TOKENS_CATEGORY_A.contains(tok),
                "expected CATEGORY_A to contain {tok}"
            );
        }
    }

    #[test]
    fn category_b_includes_wave2_additions() {
        for tok in &[
            "full_name",
            "first_name",
            "last_name",
            "real_name",
            "user_name",
        ] {
            assert!(
                PII_TOKENS_CATEGORY_B.contains(tok),
                "expected CATEGORY_B to contain {tok}"
            );
        }
    }

    #[test]
    fn category_b_includes_wave1_baseline() {
        for tok in &["email", "phone", "ip_addr", "user_agent", "ssn"] {
            assert!(
                PII_TOKENS_CATEGORY_B.contains(tok),
                "expected CATEGORY_B to contain {tok}"
            );
        }
    }

    #[test]
    fn allowlist_contains_token_type() {
        assert!(CATEGORY_A_ALLOWLIST.contains(&"token_type"));
    }

    #[test]
    fn hashed_suffixes_cover_common_shapes() {
        for suf in &["_hash", "_sha256", "_digest"] {
            assert!(HASHED_SUFFIXES.contains(suf));
        }
    }

    #[test]
    fn category_a_b_disjoint() {
        // No token should appear in both categories — different match semantics.
        for a in PII_TOKENS_CATEGORY_A {
            assert!(
                !PII_TOKENS_CATEGORY_B.contains(a),
                "{a:?} appears in both CATEGORY_A and CATEGORY_B"
            );
        }
    }
}
