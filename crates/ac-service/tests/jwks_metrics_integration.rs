// Every `#[sqlx::test]` is implicitly `flavor = "current_thread"` (sqlx::test
// default), and the pinning is LOAD-BEARING — see
// `crates/common/src/observability/testing.rs:60-72`.
//
//! Component test for `ac_jwks_requests_total{cache_status}` per ADR-0032
//! Step 4 §Cluster 10.
//!
//! # Production-reachable label set: `cache_status="miss"` only
//!
//! Plan-stage @observability finding: `jwks_handler.rs:33` always emits
//! `cache_status="miss"`. The `hit` and `bypass` label values exist in
//! the catalog (`docs/observability/metrics/ac-service.md:46-54`) but no
//! production code path emits them — CDN/browser caches handle that
//! upstream.
//!
//! This test asserts the production-reachable combo only. The two
//! reserved-but-unreachable sibling labels are asserted absent under
//! partial-label adjacency. Disposition entry filed in `docs/TODO.md`
//! §Observability Debt for Step 6 cleanup.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "common/mod.rs"]
mod test_common;

use ac_service::handlers::jwks_handler::handle_get_jwks;
use ac_service::services::key_management_service;
use ac_test_utils::crypto_fixtures::test_master_key;
use axum::extract::State;
use common::observability::testing::MetricAssertion;
use sqlx::PgPool;

use test_common::test_state::make_app_state;

#[sqlx::test(migrations = "../../migrations")]
async fn handle_get_jwks_emits_cache_status_miss(pool: PgPool) {
    let master_key = test_master_key();
    key_management_service::initialize_signing_key(&pool, &master_key, "test-cluster")
        .await
        .unwrap();
    let state = make_app_state(pool);

    let snap = MetricAssertion::snapshot();
    let result = handle_get_jwks(State(state)).await;
    assert!(result.is_ok(), "JWKS endpoint must succeed");

    snap.counter("ac_jwks_requests_total")
        .with_labels(&[("cache_status", "miss")])
        .assert_delta(1);

    // Adjacency: the unreachable label values must NOT be observed under
    // any production code path. If a future change wires server-side
    // caching that emits `hit` or `bypass`, this test must be updated
    // (per orphan-disposition TODO entry).
    snap.counter("ac_jwks_requests_total")
        .with_labels(&[("cache_status", "hit")])
        .assert_delta(0);
    snap.counter("ac_jwks_requests_total")
        .with_labels(&[("cache_status", "bypass")])
        .assert_delta(0);
}
