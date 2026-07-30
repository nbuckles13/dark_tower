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
//! Plus three secret-identifier consumers:
//! * [`rust_log_secrets`](crate::rust_log_secrets) (Wave 2) — secret variables in
//!   `info!/debug!/etc.` macros.
//! * [`instrument_skip_all`](crate::instrument_skip_all) (Wave 2) — sensitive
//!   parameters under `#[instrument]` without `skip_all`.
//! * [`ts_retained_credentials`](crate::ts_retained_credentials) (task #58) — the
//!   **first TypeScript-side consumer of CATEGORY_A**. Every other CATEGORY_A
//!   consumer is Rust-side, which is why camelCase reachability had never been
//!   exercised before (see the `accessToken` note below).
//!
//! ## Consumer → category map (verified 2026-07-30, @code-reviewer)
//!
//! | Consumer | Reads |
//! |---|---|
//! | `rust_log_secrets`, `instrument_skip_all` | CATEGORY_A only |
//! | `rust_pii`, `ts_pii` | CATEGORY_B only |
//! | `metric_labels` | **BOTH** (sole dual consumer) |
//! | `ts_retained_credentials` | CATEGORY_A only (TS-side) |
//!
//! This grouping is a *why-they-converge* summary, not an import manifest. Do not
//! restate a consumer→category claim in an entry comment without checking it against
//! this table — an inline claim of that shape has already been wrong once (below).
//!
//! ## CATEGORY_A is now read by TWO MATCHER FAMILIES — say which, always
//!
//! * **Word-boundary** (`\b(alternation)\b`) — `rust_log_secrets`, `instrument_skip_all`,
//!   `metric_labels`. Cannot see inside camelCase: `\btoken\b` matches neither
//!   `accessToken` nor `userToken`. Needs an explicit entry per camelCase spelling.
//! * **Segment-equality** — `ts_retained_credentials` (task #58). Splits identifiers on
//!   `_` and camelCase boundaries, so one `token` entry covers `userToken`,
//!   `access_token`, `meetingToken`, … and per-spelling entries are redundant to it.
//!
//! **Consequence for anyone editing this catalog**: an entry can be simultaneously
//! load-bearing for one family and redundant for the other, so **a redundancy claim
//! that does not name a matcher family is not a claim.** Removing an entry because it
//! is redundant under segment matching silently weakens the three word-boundary
//! consumers — the exact mirror of the mistake the `accessToken` note records. State
//! reachability by matcher SHAPE, not by consumer name, so the reasoning survives the
//! next consumer being added.
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
    // camelCase counterpart of `access_token` (task #11, @security sign-off
    // 2026-06-23). Added so camelCase wire fields would be detectable under the
    // word-boundary consumers (`\b(alternation)\b`), which cannot reach
    // `accessToken` via `token` / `access_token`.
    //
    // CORRECTED 2026-07-30 (task #58 Gate 1, @code-reviewer + @paired-client). The
    // original comment justified this entry as "needed for `ts_pii` detection of the
    // camelCase wire field the browser SDK introduces". That was false the day it
    // landed: `ts_pii` reads CATEGORY_**B** (see the consumer map in the module
    // doc), so it never saw this entry. CATEGORY_A had no TypeScript consumer at all
    // until `ts_retained_credentials` (task #58).
    //
    // Status now, stated by MATCHER SHAPE rather than by consumer name (see the
    // two-matcher-families note in the module doc — naming consumers is what made the
    // original comment rot):
    //   * word-boundary matchers CANNOT reach `accessToken` via `token` /
    //     `access_token`, so this entry is LOAD-BEARING for them. It fires against raw
    //     line text, so it also covers literals, doc comments and
    //     `#[serde(rename = "accessToken")]`.
    //   * segment matchers resolve `accessToken` -> [access, token] -> `token` on their
    //     own, so this entry is REDUNDANT for them.
    //   * one word-boundary consumer (`metric_labels`) lowercases before matching, so
    //     it cannot see this OR any other camelCase entry — a consumer defect, and the
    //     next camelCase addition dies there on arrival too.
    //
    // KEEP. "Redundant under segment matching" is not "redundant" — removing it would
    // be correct for the segment consumer and wrong for all three word-boundary ones,
    // which is the mirror of the mistake this comment records.
    "accessToken",
];

/// Credential-shaped subset of [`PII_TOKENS_CATEGORY_A`] — "is this field name a
/// secret whose holder should stop holding it?"
///
/// A partition, not a second vocabulary: every entry is a CATEGORY_A member, and
/// [`CREDENTIAL_TOKENS`] ∪ [`SESSION_TOKENS`] ∪ [`NON_CREDENTIAL_TOKENS`] is asserted
/// **set-equal** to CATEGORY_A by `partition_is_total`. No fallthrough bucket, so a
/// new CATEGORY_A term fails the build until someone classifies it deliberately.
/// Set-equality rather than a member count (@dry-reviewer): a count test is invariant
/// under a *rename*, which is exactly the mutation that makes `contains()` fail
/// silently.
///
/// Co-located with the catalog on purpose: a security reviewer adding a CATEGORY_A
/// term is confronted with the classification obligation at the point of edit, rather
/// than leaving a consumer to under-match in silence.
pub(crate) const CREDENTIAL_TOKENS: &[&str] = &[
    "password",
    "passwd",
    "pwd",
    "secret",
    "cred",
    "api_key",
    "apikey",
    "private_key",
    "privkey",
    "signing_key",
];

/// Token-shaped subset of [`PII_TOKENS_CATEGORY_A`] — "does the holder already have a
/// bearer credential, which is what makes a retained password unnecessary?"
pub(crate) const SESSION_TOKENS: &[&str] = &[
    "token",
    "bearer_token",
    "access_token",
    "refresh_token",
    "session_token",
    "id_token",
    "jwt",
    "bearer",
    "accessToken",
    "authorization",
    "auth_header",
    "auth_code",
];

/// CATEGORY_A members that are neither credential- nor token-shaped for the
/// retained-credential question. Explicit rather than a fallthrough so a *forgotten*
/// classification is distinguishable from a *deliberate* one.
///
/// Empty today — every CATEGORY_A term classifies into one of the other two buckets.
/// It exists so that the next term which fits neither has a declared home instead of
/// silently widening one of them, and is consumed by `partition_is_total_over_category_a`.
// Consumed only by `partition_is_total_over_category_a`, so it is dead in the
// non-test build. `cfg_attr` rather than a bare `expect` because an unconditional
// expect is itself unfulfilled under `cfg(test)`.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "deliberately-empty third bucket of a total partition; its existence is what forces a deliberate classification for a future CATEGORY_A term"
    )
)]
pub(crate) const NON_CREDENTIAL_TOKENS: &[&str] = &[];

/// Alternate spellings for a CATEGORY_A term, matched by the **same segment-equality
/// primitive** as every other term — deliberately NOT prefix matching.
///
/// Why this exists (@dry-reviewer): `cred` was catalogued meaning "credential-ish"
/// while every consumer matched `\b…\b`, which cannot do morphology. The entry
/// worked; its *intent* did not; and no test could have caught that, because the
/// intent was not in the code. This converts implicit stem-intent into a declared,
/// testable enumeration.
///
/// Why not an open prefix (`segment.starts_with("cred")`) — the worked example: it
/// matches `creditCard` -> `[credit, card]`, and `credits`, `credited`,
/// `creditLimit`. So `BillingInfo { creditCard, accessToken }` would raise a
/// credential-retention finding whose rule ID misdescribes what it found.
/// `credit_card` is already in [`PII_TOKENS_CATEGORY_B`], so the collision is with
/// vocabulary this codebase already recognises. A finite list is auditable; a prefix
/// rule grows silently with the English language.
///
/// Keys MUST be CATEGORY_A members — asserted by
/// `stem_expansion_keys_are_catalog_members`. Without that test a rename of the key
/// silently orphans the expansion: `credential`/`credentials` stop being detected and
/// every existing test still passes.
pub(crate) const STEM_EXPANSIONS: &[(&str, &[&str])] =
    &[("cred", &["cred", "creds", "credential", "credentials"])];

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
        for tok in &["pwd", "cred", "bearer", "auth_code", "accessToken"] {
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

    /// The partition must be TOTAL over CATEGORY_A — union set-equal, no fallthrough.
    ///
    /// Set-equality rather than a member count (@dry-reviewer, task #58): a count test
    /// is invariant under a rename, which is the mutation that makes `contains()`-style
    /// subset filtering fail *silently* — the subset shrinks, the guard quietly
    /// under-detects, and every test still passes. Adding a CATEGORY_A term without
    /// classifying it fails HERE rather than degrading a downstream guard.
    #[test]
    fn partition_is_total_over_category_a() {
        use std::collections::BTreeSet;
        let catalog: BTreeSet<&str> = PII_TOKENS_CATEGORY_A.iter().copied().collect();
        let partition: BTreeSet<&str> = CREDENTIAL_TOKENS
            .iter()
            .chain(SESSION_TOKENS)
            .chain(NON_CREDENTIAL_TOKENS)
            .copied()
            .collect();

        let unclassified: Vec<&str> = catalog.difference(&partition).copied().collect();
        assert!(
            unclassified.is_empty(),
            "CATEGORY_A terms with no partition bucket (classify each in \
             CREDENTIAL_TOKENS / SESSION_TOKENS / NON_CREDENTIAL_TOKENS): {unclassified:?}"
        );

        let orphaned: Vec<&str> = partition.difference(&catalog).copied().collect();
        assert!(
            orphaned.is_empty(),
            "partition entries that are NOT CATEGORY_A members (renamed or removed \
             from the catalog?): {orphaned:?}"
        );
    }

    /// The three buckets must not overlap — a term classified twice is ambiguous.
    #[test]
    fn partition_buckets_are_disjoint() {
        for cred in CREDENTIAL_TOKENS {
            assert!(
                !SESSION_TOKENS.contains(cred),
                "{cred:?} is in both CREDENTIAL_TOKENS and SESSION_TOKENS"
            );
            assert!(
                !NON_CREDENTIAL_TOKENS.contains(cred),
                "{cred:?} is in both CREDENTIAL_TOKENS and NON_CREDENTIAL_TOKENS"
            );
        }
        for tok in SESSION_TOKENS {
            assert!(
                !NON_CREDENTIAL_TOKENS.contains(tok),
                "{tok:?} is in both SESSION_TOKENS and NON_CREDENTIAL_TOKENS"
            );
        }
    }

    /// Referential integrity: every STEM_EXPANSIONS key must exist in CATEGORY_A.
    ///
    /// Without this, renaming `cred` in the catalog orphans the expansion — the key
    /// matches nothing, `credential`/`credentials` quietly stop being detected, and
    /// every other test still passes. Same failure family as the `contains()` subset
    /// filtering above (task #58 §Lessons Learned, "Mode A").
    #[test]
    fn stem_expansion_keys_are_catalog_members() {
        for (key, expansions) in STEM_EXPANSIONS {
            assert!(
                PII_TOKENS_CATEGORY_A.contains(key),
                "STEM_EXPANSIONS key {key:?} is not a CATEGORY_A member — renamed?"
            );
            assert!(
                expansions.contains(key),
                "STEM_EXPANSIONS[{key:?}] must include the key itself"
            );
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
