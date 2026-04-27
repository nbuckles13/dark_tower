//! Integration cover for `gc_meeting_join_total`,
//! `gc_meeting_join_duration_seconds`, and `gc_meeting_join_failures_total`
//! per ADR-0032 Step 5 §Cluster 11.
//!
//! `MetricAssertion`'s per-thread recorder isolation applies. No tokio
//! runtime pinning needed — `record_meeting_join` is synchronous.
//!
//! # Per-failure-class fidelity (per @observability Step 5)
//!
//! `gc_meeting_join_*` is shared between `join_meeting` (`participant=user`)
//! and `get_guest_token` (`participant=guest`). The `participant` label
//! discriminates so operators can triage user-vs-guest failures without
//! log-diving (e.g. `error_type=forbidden` on user means cross-org denial;
//! on guest means `meeting.allow_guests=false` reaches `error_type=guests_disabled`).
//!
//! ## Branch parity table (mirror of `handlers/meetings.rs` documentation)
//!
//! | Branch                                        | error_type        | user | guest |
//! |-----------------------------------------------|-------------------|:----:|:-----:|
//! | `find_meeting_by_code` fails                  | not_found         |  ✓   |  ✓    |
//! | status not active/scheduled                   | bad_status        |  ✓   |  ✓    |
//! | `parse_user_id(sub)` / `org_id` parse fails   | unauthorized      |  ✓   |  N/A  |
//! | external denied / `!allow_guests`             | forbidden / guests_disabled | ✓ | ✓ (different value) |
//! | `assign_meeting_with_mh` fails                | mc_assignment     |  ✓   |  ✓    |
//! | `create_ac_client` / `generate_guest_id` fail | internal          |  ✓   |  ✓    |
//! | `request_meeting_token` / `request_guest_token` fails | ac_request |  ✓   |  ✓    |
//! | request body validation fails                 | bad_request       | N/A  |  ✓    |
//! | success                                       | (none)            |  ✓   |  ✓    |
//!
//! # WRAPPER-CAT-C framing
//!
//! Real-recording-site coverage of the success + a subset of error branches
//! exists in `crates/gc-service/tests/meeting_tests.rs` (drives `join_meeting`
//! handler end-to-end against a real DB + wiremock JWKS + wiremock AC).
//! `get_guest_token` end-to-end coverage is added by the same test harness in
//! that file, but the wrapper-level per-(participant, error_type) cartesian
//! lives here so every cell is asserted with full adjacency fidelity.
//!
//! See `docs/TODO.md` §Observability Debt for the orphan disposition tracker
//! covering full real-recording-site drives.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use ::common::observability::testing::MetricAssertion;
use gc_service::observability::metrics::record_meeting_join;

const SHARED_ERROR_TYPES: &[&str] = &[
    "not_found",
    "bad_status",
    "forbidden",
    "mc_assignment",
    "internal",
    "ac_request",
];
const USER_ONLY_ERROR_TYPES: &[&str] = &["unauthorized"];
const GUEST_ONLY_ERROR_TYPES: &[&str] = &["bad_request", "guests_disabled"];

/// Every error_type value the wrapper might see — used to drive
/// `assert_delta(0)` adjacency on non-target labels (label-swap-bug catcher
/// per ADR-0032 §Pattern #3).
const ALL_ERROR_TYPES: &[&str] = &[
    "not_found",
    "bad_status",
    "unauthorized",
    "forbidden",
    "guests_disabled",
    "bad_request",
    "mc_assignment",
    "internal",
    "ac_request",
];

// ============================================================================
// Success cells (one per participant)
// ============================================================================

#[test]
fn meeting_join_user_success_emits_participant_user_no_failure_counter() {
    let snap = MetricAssertion::snapshot();

    record_meeting_join("user", "success", None, Duration::from_millis(200));

    snap.histogram("gc_meeting_join_duration_seconds")
        .with_labels(&[("participant", "user"), ("status", "success")])
        .assert_observation_count(1);

    snap.counter("gc_meeting_join_total")
        .with_labels(&[("participant", "user"), ("status", "success")])
        .assert_delta(1);

    // Failures counter silent across every bounded error_type for this
    // (participant=user) row (label-swap catcher).
    for sibling in ALL_ERROR_TYPES {
        snap.counter("gc_meeting_join_failures_total")
            .with_labels(&[("participant", "user"), ("error_type", *sibling)])
            .assert_delta(0);
    }
    // Adjacency: guest path silent on user-success drive.
    snap.counter("gc_meeting_join_total")
        .with_labels(&[("participant", "guest")])
        .assert_delta(0);
}

#[test]
fn meeting_join_guest_success_emits_participant_guest_no_failure_counter() {
    let snap = MetricAssertion::snapshot();

    record_meeting_join("guest", "success", None, Duration::from_millis(180));

    snap.histogram("gc_meeting_join_duration_seconds")
        .with_labels(&[("participant", "guest"), ("status", "success")])
        .assert_observation_count(1);

    snap.counter("gc_meeting_join_total")
        .with_labels(&[("participant", "guest"), ("status", "success")])
        .assert_delta(1);

    // Failures counter silent across every bounded error_type for guest row.
    for sibling in ALL_ERROR_TYPES {
        snap.counter("gc_meeting_join_failures_total")
            .with_labels(&[("participant", "guest"), ("error_type", *sibling)])
            .assert_delta(0);
    }
    // Adjacency: user path silent on guest-success drive.
    snap.counter("gc_meeting_join_total")
        .with_labels(&[("participant", "user")])
        .assert_delta(0);
}

// ============================================================================
// Shared error_types — driven for both participants
// ============================================================================
//
// `SHARED_ERROR_TYPES` includes `forbidden` because both wrappers must accept
// the label (label-wiring proof + predicate-collapse-refactor catch per
// @code-reviewer Refinement-1). At the production-emission level, `forbidden`
// is user-only — the guest-path analogous gate (`!meeting.allow_guests`)
// routes to `guests_disabled`, a distinct label value. So the guest+forbidden
// cell drive below is wiring-only: it proves `record_meeting_join` accepts
// the label tuple but does not prove production `get_guest_token` ever emits
// it. See `crates/gc-service/src/handlers/meetings.rs` parity-note for the
// predicate detail. This is intentional: if a future refactor accidentally
// drops `forbidden` from one path's wiring (e.g., by adding an `if
// participant == "guest" { panic!() }` guard), this loop catches it.

#[test]
fn meeting_join_shared_error_types_per_participant() {
    // For each (participant, shared_error_type) cell: drive the wrapper,
    // assert the named cell fires + assert_delta(0) on the OTHER participant
    // for the same error_type (the load-bearing label-swap catcher per
    // @observability — guest vs user must NOT bleed through).
    for participant in ["user", "guest"] {
        for err in SHARED_ERROR_TYPES {
            let snap = MetricAssertion::snapshot();
            record_meeting_join(participant, "error", Some(err), Duration::from_millis(10));

            snap.histogram("gc_meeting_join_duration_seconds")
                .with_labels(&[("participant", participant), ("status", "error")])
                .assert_observation_count(1);
            snap.counter("gc_meeting_join_total")
                .with_labels(&[("participant", participant), ("status", "error")])
                .assert_delta(1);
            snap.counter("gc_meeting_join_failures_total")
                .with_labels(&[("participant", participant), ("error_type", *err)])
                .assert_delta(1);

            // Label-swap catcher: OTHER participant must NOT have absorbed
            // this emission under the same error_type.
            let other = if participant == "user" {
                "guest"
            } else {
                "user"
            };
            snap.counter("gc_meeting_join_failures_total")
                .with_labels(&[("participant", other), ("error_type", *err)])
                .assert_delta(0);

            // Adjacency: other shared error_types silent on this drive
            // (only the one we drove fired).
            for sibling in SHARED_ERROR_TYPES.iter().filter(|t| **t != *err) {
                snap.counter("gc_meeting_join_failures_total")
                    .with_labels(&[("participant", participant), ("error_type", *sibling)])
                    .assert_delta(0);
            }
        }
    }
}

// ============================================================================
// User-only error_types
// ============================================================================

#[test]
fn meeting_join_user_unauthorized_does_not_fire_on_guest_path() {
    // `unauthorized` is user-only — guest path is documented PUBLIC.
    // Drive the user wrapper with this label and prove the guest counter
    // remains silent (semantic-difference invariant per @observability).
    for err in USER_ONLY_ERROR_TYPES {
        let snap = MetricAssertion::snapshot();
        record_meeting_join("user", "error", Some(err), Duration::from_millis(5));

        snap.counter("gc_meeting_join_failures_total")
            .with_labels(&[("participant", "user"), ("error_type", *err)])
            .assert_delta(1);

        // Guest must NEVER carry an `unauthorized` label.
        snap.counter("gc_meeting_join_failures_total")
            .with_labels(&[("participant", "guest"), ("error_type", *err)])
            .assert_delta(0);
    }
}

// ============================================================================
// Guest-only error_types
// ============================================================================

#[test]
fn meeting_join_guest_only_error_types_do_not_fire_on_user_path() {
    // `bad_request` (body validation — user has no body) and
    // `guests_disabled` (`!meeting.allow_guests` — guest-specific predicate)
    // are guest-only. Drive the guest wrapper for each and prove the user
    // counter remains silent.
    for err in GUEST_ONLY_ERROR_TYPES {
        let snap = MetricAssertion::snapshot();
        record_meeting_join("guest", "error", Some(err), Duration::from_millis(5));

        snap.counter("gc_meeting_join_failures_total")
            .with_labels(&[("participant", "guest"), ("error_type", *err)])
            .assert_delta(1);

        // User must NEVER carry these labels.
        snap.counter("gc_meeting_join_failures_total")
            .with_labels(&[("participant", "user"), ("error_type", *err)])
            .assert_delta(0);
    }
}

// ============================================================================
// Histogram label fidelity — participant axis disambiguates
// ============================================================================

#[test]
fn meeting_join_histograms_disambiguate_by_participant() {
    // Both participants emit a histogram observation under the same status
    // value — verify they remain distinct under the participant axis (no
    // label-cardinality bleed). Two snapshots, one per participant, because
    // `assert_observation_count` drains all histogram entries on a single
    // snapshot per `crates/common/src/observability/testing.rs` §"Histograms
    // DRAIN on snapshot."
    {
        let snap = MetricAssertion::snapshot();
        record_meeting_join("user", "success", None, Duration::from_millis(150));
        snap.histogram("gc_meeting_join_duration_seconds")
            .with_labels(&[("participant", "user"), ("status", "success")])
            .assert_observation_count(1);
    }
    {
        let snap = MetricAssertion::snapshot();
        record_meeting_join("guest", "success", None, Duration::from_millis(150));
        snap.histogram("gc_meeting_join_duration_seconds")
            .with_labels(&[("participant", "guest"), ("status", "success")])
            .assert_observation_count(1);
    }
}

// ============================================================================
// Label-domain invariant: `guests_disabled` is exclusive to guest path
// ============================================================================

#[test]
fn meeting_join_guests_disabled_is_guest_exclusive_label_domain_invariant() {
    // Drive every legitimate user-path emission, including all error types
    // production can emit on the user path. None of these should ever land
    // {participant="user", error_type="guests_disabled"}.
    let snap = MetricAssertion::snapshot();
    for et in USER_ONLY_ERROR_TYPES.iter().chain(SHARED_ERROR_TYPES) {
        record_meeting_join("user", "error", Some(et), Duration::from_millis(1));
    }
    record_meeting_join("user", "success", None, Duration::from_millis(1));

    // Label-domain invariant: guests_disabled is exclusive to guest path
    // by construction (predicate `!meeting.allow_guests` is only reachable
    // through `get_guest_token`). Pin it.
    snap.counter("gc_meeting_join_failures_total")
        .with_labels(&[("participant", "user"), ("error_type", "guests_disabled")])
        .assert_unobserved();
}
