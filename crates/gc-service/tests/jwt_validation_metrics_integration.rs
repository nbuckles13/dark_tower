//! Integration cover for `gc_jwt_validations_total` per ADR-0032 Step 5
//! §Cluster 6.
//!
//! `MetricAssertion`'s per-thread recorder isolation applies. No tokio
//! runtime pinning needed — `record_jwt_validation` is synchronous.
//!
//! # WRAPPER-CAT-C framing
//!
//! The production recording site is `crates/gc-service/src/grpc/auth_layer.rs`
//! inside the gRPC `GrpcAuthLayer` (Tower middleware). Driving every
//! `failure_reason` value (signature_invalid, expired, scope_mismatch,
//! malformed, missing_token) end-to-end through the gRPC layer requires:
//! (a) an in-process tonic server fixture, (b) JWKS mock setup, and (c) one
//! token-fixture per failure path. The full per-failure-class matrix lives
//! in `crates/gc-service/tests/auth_tests.rs` — that file already exercises
//! the layer at the gRPC seam. The cluster file here exercises the WRAPPER
//! with full per-failure-class label fidelity (Cat C name-coverage) so that
//! the `validate-metric-coverage.sh` guard's `tests/**/*.rs` scan is
//! satisfied AND label-swap regressions on the wrapper's own
//! `failure_reason` enum are caught.
//!
//! See `docs/TODO.md` §Observability Debt for the orphan disposition tracker
//! covering the cluster's full real-recording-site drive (~50 LoC for the
//! tonic harness) — deferred per ADR-0032 Step 5 plan-stage scope.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use ::common::observability::testing::MetricAssertion;
use gc_service::observability::metrics::record_jwt_validation;

/// All bounded `failure_reason` values per `crates/gc-service/src/grpc/auth_layer.rs`
/// (`classify_jwt_error`) + the success sentinel "none". Used for label-swap-bug
/// catcher adjacency.
const ALL_FAILURE_REASONS: &[&str] = &[
    "none",
    "signature_invalid",
    "expired",
    "scope_mismatch",
    "malformed",
    "missing_token",
];

#[test]
fn jwt_validation_success_emits_result_success_with_failure_reason_none() {
    let snap = MetricAssertion::snapshot();

    record_jwt_validation("success", "service", "none");

    snap.counter("gc_jwt_validations_total")
        .with_labels(&[
            ("result", "success"),
            ("token_type", "service"),
            ("failure_reason", "none"),
        ])
        .assert_delta(1);

    // Adjacency: no failure_reason value should fire on the success path.
    for sibling in ALL_FAILURE_REASONS.iter().filter(|r| **r != "none") {
        snap.counter("gc_jwt_validations_total")
            .with_labels(&[("result", "failure"), ("failure_reason", *sibling)])
            .assert_delta(0);
    }
}

#[test]
fn jwt_validation_signature_invalid_emits_failure_reason_signature_invalid() {
    let snap = MetricAssertion::snapshot();

    record_jwt_validation("failure", "service", "signature_invalid");

    snap.counter("gc_jwt_validations_total")
        .with_labels(&[
            ("result", "failure"),
            ("token_type", "service"),
            ("failure_reason", "signature_invalid"),
        ])
        .assert_delta(1);

    // Adjacency: other 4 failure_reason values silent, success silent.
    for sibling in ALL_FAILURE_REASONS
        .iter()
        .filter(|r| **r != "signature_invalid")
    {
        snap.counter("gc_jwt_validations_total")
            .with_labels(&[("failure_reason", *sibling)])
            .assert_delta(0);
    }
}

#[test]
fn jwt_validation_expired_emits_failure_reason_expired() {
    let snap = MetricAssertion::snapshot();
    record_jwt_validation("failure", "service", "expired");
    snap.counter("gc_jwt_validations_total")
        .with_labels(&[
            ("result", "failure"),
            ("token_type", "service"),
            ("failure_reason", "expired"),
        ])
        .assert_delta(1);
    for sibling in ALL_FAILURE_REASONS.iter().filter(|r| **r != "expired") {
        snap.counter("gc_jwt_validations_total")
            .with_labels(&[("failure_reason", *sibling)])
            .assert_delta(0);
    }
}

#[test]
fn jwt_validation_scope_mismatch_emits_failure_reason_scope_mismatch() {
    let snap = MetricAssertion::snapshot();
    record_jwt_validation("failure", "service", "scope_mismatch");
    snap.counter("gc_jwt_validations_total")
        .with_labels(&[
            ("result", "failure"),
            ("token_type", "service"),
            ("failure_reason", "scope_mismatch"),
        ])
        .assert_delta(1);
    for sibling in ALL_FAILURE_REASONS
        .iter()
        .filter(|r| **r != "scope_mismatch")
    {
        snap.counter("gc_jwt_validations_total")
            .with_labels(&[("failure_reason", *sibling)])
            .assert_delta(0);
    }
}

#[test]
fn jwt_validation_malformed_emits_failure_reason_malformed() {
    let snap = MetricAssertion::snapshot();
    record_jwt_validation("failure", "service", "malformed");
    snap.counter("gc_jwt_validations_total")
        .with_labels(&[
            ("result", "failure"),
            ("token_type", "service"),
            ("failure_reason", "malformed"),
        ])
        .assert_delta(1);
    for sibling in ALL_FAILURE_REASONS.iter().filter(|r| **r != "malformed") {
        snap.counter("gc_jwt_validations_total")
            .with_labels(&[("failure_reason", *sibling)])
            .assert_delta(0);
    }
}

#[test]
fn jwt_validation_missing_token_emits_failure_reason_missing_token() {
    let snap = MetricAssertion::snapshot();
    record_jwt_validation("failure", "service", "missing_token");
    snap.counter("gc_jwt_validations_total")
        .with_labels(&[
            ("result", "failure"),
            ("token_type", "service"),
            ("failure_reason", "missing_token"),
        ])
        .assert_delta(1);
    for sibling in ALL_FAILURE_REASONS
        .iter()
        .filter(|r| **r != "missing_token")
    {
        snap.counter("gc_jwt_validations_total")
            .with_labels(&[("failure_reason", *sibling)])
            .assert_delta(0);
    }
}
