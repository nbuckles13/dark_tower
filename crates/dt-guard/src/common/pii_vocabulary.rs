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
    // Wave-3 cohort — ADR-0036 media path (story task 10). @security + @observability
    // sign-off 2026-09-02; classified into NON_CREDENTIAL_TOKENS below.
    //
    // FLOOR, NOT THE CONTROL. ADR-0036 §11: "Vocabulary additions cannot be cited as
    // the protection… The directory-scoped deny catches by shape." A KEK read into a
    // local named `bytes` or `material` matches nothing here and is still a leak. The
    // control is the credential-leak semantic check (items 11-13); this is a backstop
    // under it.
    "meeting_kek",
    "transmit_key",
    // Bare `kek` is DELIBERATELY ABSENT, and not merely because it is redundant.
    // Stated by matcher shape, per the `accessToken` precedent above:
    //   * word-boundary matchers (`rust_log_secrets`, `instrument_skip_all`, `rust_pii`)
    //     cannot reach `meeting_kek` or `kek_generation` via `\bkek\b` — `_` is a word
    //     character — so a bare entry would be inert for them.
    //   * `metric_labels` does NOT use word boundaries on its single-word path
    //     (`metric_labels.rs:543-551`): it splits the label on `_` and tests set
    //     membership, so `kek_generation` -> {kek, generation} -> Category A HIT.
    // So a bare `kek` would be inert where you want it and a FALSE POSITIVE where you
    // don't — firing on a plausible `kek_generation` label, which is metadata
    // identifying WHICH key, never key material. Do not add it.
    //
    // KNOWN LIMIT, stated rather than assumed away: under word-boundary matching
    // `\btransmit_key\b` does NOT match a compound on EITHER side, because `_` is a word
    // character and so there is no boundary at the join. That covers both
    // `wrapped_transmit_key` (leading) and `transmit_key_bytes` (TRAILING) — the trailing
    // form is the one a KEK-adjacent field is most likely to be spelled, and it was
    // verified against the shipped guard on 2026-09-08 (story task 23) rather than
    // inferred: `rust-no-secrets-in-logs` fires on `meeting_kek` at an MC log site and is
    // silent on `transmit_key_bytes` at the same site. `metric_labels` covers both via its
    // substring path; `rust_log_secrets` and `instrument_skip_all` cover neither.
    //
    // MC never holds a wrapped transmit key, so no entry is added here — and adding one
    // would not help anyway, which is the point worth carrying: a new entry spelled
    // `transmit_key_bytes` would be inert against `transmit_key_material` and every other
    // unenumerated compound. Do not read this cohort as covering compound spellings, and
    // do not "fix" a fixture by widening this list — ADR-0036 §11 forecloses that in terms
    // ("Vocabulary additions cannot be cited as the protection"). The control for these
    // spellings is the credential-leak semantic check (items 11-13), which judges the
    // value; the structural fix for the matcher is tracked separately (promote
    // `segments()` to `crate::common::` and repoint the word-boundary consumers).
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
/// First inhabitants: `meeting_kek` and `transmit_key` (ADR-0036 §4). They are
/// CATEGORY_A secrets for the log/label/span question, but the retained-credential
/// question — "is this a secret whose holder should stop holding it?" — answers **no**:
/// §4 makes a client an ENTITLED LONG-LIVED HOLDER that caches the KEK for the meeting
/// and must retain the previous KEK across a rotation window. Filing them under
/// [`CREDENTIAL_TOKENS`] would make `ts_retained_credentials` fire on SDK session state
/// the ADR mandates, which gets "resolved" by an allowlist entry that silently kills
/// the Rust-side coverage too.
///
/// **Membership here is inert at runtime.** The bucket is never passed as a `subset`
/// to `matches_subset` (`ts_retained_credentials.rs:267`, `:271`), and `spellings()`
/// (`:230-240`) iterates `CREDENTIAL_TOKENS.chain(SESSION_TOKENS)`, returning
/// `Vec::new()` for a non-member — two independent routes to the same place. A future
/// author who wires this bucket into a `matches_subset` call site therefore gets
/// **silence, not coverage**: it compiles, runs, and matches nothing. Extending
/// `spellings()` is required alongside any such wiring.
///
/// Its members remain full [`PII_TOKENS_CATEGORY_A`] members and are still read by
/// `rust_log_secrets`, `instrument_skip_all` and `metric_labels` — this classification
/// weakens nothing.
///
/// The bucket also exists so that the next term fitting neither of the other two has a
/// declared home instead of silently widening one of them, and is consumed by
/// `partition_is_total_over_category_a`.
// Consumed only by `partition_is_total_over_category_a`, so it is dead in the
// non-test build. `cfg_attr` rather than a bare `expect` because an unconditional
// expect is itself unfulfilled under `cfg(test)`.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "third bucket of a total partition, read only by the partition test; its existence is what forces a deliberate classification for a future CATEGORY_A term"
    )
)]
pub(crate) const NON_CREDENTIAL_TOKENS: &[&str] = &["meeting_kek", "transmit_key"];

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
    // ADR-0036 §2/§4 (story task 10). Restores coverage the deleted proto `user_id`
    // field provided incidentally: `sender_id` splits to {sender, id}, CATEGORY_B has
    // no bare `id`, and no multi-word entry is a substring of it — so before this entry
    // it matched nothing at all.
    //
    // B, not A: it is an identifier, not a secret. Filing it under A would drag it into
    // the retained-credential partition and the TS consumer for no reason.
    //
    // WHY THIS IS NOT THE INERT CASE `label-taxonomy.md` R1 WARNS ABOUT. R1's argument
    // turns on the word *realistic*: for `meeting_id` the realistic spelling IS the
    // hashed one — the SDK already emits `meeting_id_hash`
    // (`packages/sdk-core/src/session/MeetingSession.ts:279`,
    // `packages/sdk-core/src/media/events.ts:44`) — so a plain entry is defeated on
    // arrival. For `sender_id` the realistic spelling is the PLAIN one: nothing in the
    // tree hashes a sender id, no `sender_id_hash` exists, and the accidental form is
    // `"sender_id" => id.to_string()` in a label position. This entry bites the
    // spelling that will actually be written. INVERSE of the `meeting_id` case, not an
    // instance of it.
    //
    // Coverage BY MATCHER SHAPE, not as blanket protection:
    //   * `metric_labels` covers it INCLUDING compounds — `token_hit_in_set`'s
    //     multi-word substring pass catches `pinned_sender_id`. That is the
    //     "never a metric label" bar.
    //   * `rust_pii` covers `sender_id` and the `sender_id = %x` tracing-field shape,
    //     but NOT `pinned_sender_id`: it builds `\b(alternation)\b` and `_` is a word
    //     character.
    //   * `sender_id_hash` is NOT covered — `is_hashed_label()` is tested BEFORE the
    //     CATEGORY_B lookup in `pii_token_hit`. See `label-taxonomy.md`
    //     §Enforcement reality, which carries the trigger: if a `sender_id_hash` is
    //     ever proposed, this entry's premise is void.
    //   * `#[instrument]` span params are NOT covered — `instrument_skip_all` reads
    //     CATEGORY_A only. **Do not read `skip_all` as suppression here**: it bounds
    //     *captured arguments* and does NOT bound an explicit `fields(...)` list,
    //     which tracing records regardless. The explicit-field case belongs to
    //     `rust_pii` Check 3, ON SINGLE-LINE ATTRIBUTES ONLY; a live instance, the
    //     multi-line gap, and the open work to narrow that check to the
    //     `fields(...)` group are in `docs/TODO.md` §Observability Debt.
    "sender_id",
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

/// Prefixes that force-fire as PII regardless of suffix.
///
/// **No longer a mirror of the retired Wave-1 Python list**, and the note is
/// removed rather than updated: `meeting_id` was never in that list, so the
/// parity the old comment claimed does not exist and cannot be restored.
///
/// `pii_token_hit` scans this list FIRST — before Category A, before
/// `LABEL_ALLOWLIST` and before `is_hashed_label()` — which is what a hashed
/// spelling cannot slip past. Per-entry meaning lives in
/// `docs/observability/label-taxonomy.md` §Prefix denylist; the two entries
/// share a mechanism, not a rationale.
///
/// **Bypassable.** The `PiiCategory::Prefix` finding arm is gated on
/// `pii_safe.is_none()`, so `# pii-safe: <reason>` suppresses it. Only
/// Category A is non-bypassable. Do not describe a term here as unbreakable.
pub(crate) const PII_PREFIX_DENYLIST: &[&str] = &["raw_", "meeting_id"];

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
    fn category_a_includes_wave3_additions() {
        for tok in &["meeting_kek", "transmit_key"] {
            assert!(
                PII_TOKENS_CATEGORY_A.contains(tok),
                "expected CATEGORY_A to contain {tok}"
            );
        }
    }

    #[test]
    fn category_b_includes_wave3_additions() {
        assert!(
            PII_TOKENS_CATEGORY_B.contains(&"sender_id"),
            "expected CATEGORY_B to contain sender_id"
        );
    }

    /// The bucket went from empty to inhabited on 2026-09-02, and its members'
    /// correctness argument is which partition they land in — so pin the
    /// membership rather than leaving it to the totality assert alone.
    #[test]
    fn non_credential_tokens_has_expected_members() {
        assert_eq!(
            NON_CREDENTIAL_TOKENS,
            &["meeting_kek", "transmit_key"],
            "NON_CREDENTIAL_TOKENS membership changed; the entitled-long-lived-holder \
             reasoning in the module docs must be re-checked before adding to it"
        );
    }

    /// The bare token `kek` is deliberately NOT a vocabulary entry: it would be
    /// inert for the word-boundary consumers and a false positive for the
    /// segment-matching one. Pin the absence so a future author re-derives the
    /// reasoning instead of "helpfully" adding it.
    #[test]
    fn bare_kek_is_not_a_vocabulary_entry() {
        assert!(!PII_TOKENS_CATEGORY_A.contains(&"kek"));
        assert!(!PII_TOKENS_CATEGORY_B.contains(&"kek"));
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
