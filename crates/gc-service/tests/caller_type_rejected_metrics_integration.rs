//! Integration cover for `gc_caller_type_rejected_total` per ADR-0032 Step 5
//! §Cluster 7. ADR-0003 Layer 2 (caller-type validation).
//!
//! `MetricAssertion`'s per-thread recorder isolation applies. No tokio
//! runtime pinning needed — `record_caller_type_rejected` is synchronous.
//!
//! # WRAPPER-CAT-C framing
//!
//! The production recording site is `crates/gc-service/src/grpc/auth_layer.rs:250`
//! inside `GrpcAuthLayer::call`, fired when a caller's `service_type` claim
//! does not match the target gRPC service. Driving the full bounded label
//! domain (2 grpc_services × 4 actual_types) end-to-end requires the same
//! tonic harness as `jwt_validation_metrics_integration.rs` plus
//! distinct-service-type token fixtures. Real-recording-site partial coverage
//! exists in `crates/gc-service/tests/auth_tests.rs` (one happy + one
//! rejected); full per-failure-class fidelity is wrapper-driven here.
//!
//! See `docs/TODO.md` §Observability Debt for the orphan disposition tracker
//! covering the cluster's full real-recording-site drive — deferred per
//! ADR-0032 Step 5 plan-stage scope.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use ::common::observability::testing::MetricAssertion;
use gc_service::observability::metrics::record_caller_type_rejected;

const GRPC_SERVICES: &[&str] = &["GlobalControllerService", "MediaHandlerRegistryService"];
// Realistic actual_type values that flow into this metric from
// `crates/gc-service/src/grpc/auth_layer.rs:241` — `claims.service_type`
// (the bounded production set: `meeting-controller`, `media-handler`,
// `global-controller`) plus `"unknown"` for the `service_type=None` case.
// Per @security ADR-0032 Step 5 watch item: do NOT seed fresh strings just
// to drive rejection — reuse `"unknown"`.
const ACTUAL_TYPES: &[&str] = &[
    "meeting-controller",
    "media-handler",
    "global-controller",
    "unknown",
];

#[test]
fn caller_type_rejected_mc_path_with_mh_token() {
    let snap = MetricAssertion::snapshot();

    record_caller_type_rejected(
        "GlobalControllerService",
        "meeting-controller",
        "media-handler",
    );

    snap.counter("gc_caller_type_rejected_total")
        .with_labels(&[
            ("grpc_service", "GlobalControllerService"),
            ("expected_type", "meeting-controller"),
            ("actual_type", "media-handler"),
        ])
        .assert_delta(1);

    // Adjacency: no other (grpc_service, actual_type) tuples emit.
    for actual in ACTUAL_TYPES.iter().filter(|t| **t != "media-handler") {
        snap.counter("gc_caller_type_rejected_total")
            .with_labels(&[
                ("grpc_service", "GlobalControllerService"),
                ("actual_type", *actual),
            ])
            .assert_delta(0);
    }
}

#[test]
fn caller_type_rejected_mh_registry_with_mc_token() {
    let snap = MetricAssertion::snapshot();

    record_caller_type_rejected(
        "MediaHandlerRegistryService",
        "media-handler",
        "meeting-controller",
    );

    snap.counter("gc_caller_type_rejected_total")
        .with_labels(&[
            ("grpc_service", "MediaHandlerRegistryService"),
            ("expected_type", "media-handler"),
            ("actual_type", "meeting-controller"),
        ])
        .assert_delta(1);
    // Label-swap catcher: other grpc_service must not absorb this emission.
    snap.counter("gc_caller_type_rejected_total")
        .with_labels(&[
            ("grpc_service", "GlobalControllerService"),
            ("actual_type", "meeting-controller"),
        ])
        .assert_delta(0);
}

#[test]
fn caller_type_rejected_unknown_actual_type_per_grpc_service() {
    // The "unknown" actual_type fires when claims.service_type is None
    // (`grpc/auth_layer.rs:241`). Verify both gRPC services emit a distinct
    // label tuple for this case.
    for grpc_service in GRPC_SERVICES {
        let snap = MetricAssertion::snapshot();
        let expected_type = if *grpc_service == "GlobalControllerService" {
            "meeting-controller"
        } else {
            "media-handler"
        };
        record_caller_type_rejected(grpc_service, expected_type, "unknown");

        snap.counter("gc_caller_type_rejected_total")
            .with_labels(&[
                ("grpc_service", grpc_service),
                ("expected_type", expected_type),
                ("actual_type", "unknown"),
            ])
            .assert_delta(1);
        // Adjacency: no other actual_type fires under the same grpc_service.
        for sibling in ACTUAL_TYPES.iter().filter(|t| **t != "unknown") {
            snap.counter("gc_caller_type_rejected_total")
                .with_labels(&[("grpc_service", grpc_service), ("actual_type", *sibling)])
                .assert_delta(0);
        }
    }
}

#[test]
fn caller_type_rejected_global_controller_actual_type() {
    // A GC service token presented at the GC gRPC surface is a valid bounded
    // `actual_type` value (a misconfigured GC could route a self-token to
    // a different GC instance — exotic but emits to the metric). Reuses the
    // production `service_type` string `global-controller`; per @security
    // ADR-0032 Step 5 watch item, do NOT seed fresh strings just to test
    // the rejection path.
    let snap = MetricAssertion::snapshot();

    record_caller_type_rejected(
        "GlobalControllerService",
        "meeting-controller",
        "global-controller",
    );

    snap.counter("gc_caller_type_rejected_total")
        .with_labels(&[
            ("grpc_service", "GlobalControllerService"),
            ("expected_type", "meeting-controller"),
            ("actual_type", "global-controller"),
        ])
        .assert_delta(1);
}
