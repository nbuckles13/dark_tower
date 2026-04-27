// Every `#[sqlx::test]` in this file is implicitly `flavor = "current_thread"`
// (sqlx::test default), and that pinning is LOAD-BEARING — `MetricAssertion`
// binds a per-thread recorder. See
// `crates/common/src/observability/testing.rs:60-72`.
//
//! Component tests for AC's key-management metrics per ADR-0032 Step 4
//! §Cluster 6 + Cluster 7:
//!
//! - `ac_key_rotation_total{status}` — `success` and `error` paths
//! - `ac_active_signing_keys` (gauge, no labels)
//! - `ac_signing_key_age_days` (gauge, no labels)
//! - `ac_key_rotation_last_success_timestamp` (gauge, no labels)
//!
//! # Gauge production-emission sites
//!
//! All three gauges are set inside `key_management_service::initialize_signing_key`
//! (which fires on first-key bootstrap) and `handle_rotate_keys` (admin
//! endpoint). The drive seam used here is `initialize_signing_key` directly,
//! which is simpler than the admin handler stack and exercises the same
//! production emission sites at lines 100-102.
//!
//! # Failure-path adjacency uses `assert_unobserved` (ADR-0032 Step 4)
//!
//! Per @test reviewer T1: failure paths must NOT touch the gauges. The new
//! `MetricAssertion::*::assert_unobserved` API (landed alongside this file)
//! is exactly the per-failure-class adjacency rail for gauges. Without it,
//! a future refactor that silently binds the gauge to a wrong call site
//! would pass these tests.
//!
//! All gauge values asserted here are produced by REAL key-management code
//! paths, never set by test fixtures (per @security review).

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "common/mod.rs"]
mod test_common;

use ac_service::services::key_management_service;
use ac_test_utils::crypto_fixtures::test_master_key;
use chrono::Utc;
use common::observability::testing::MetricAssertion;
use sqlx::PgPool;

#[sqlx::test(migrations = "../../migrations")]
async fn initialize_signing_key_emits_gauges_and_no_rotation_counter(pool: PgPool) {
    // Production note: `initialize_signing_key` is the bootstrap path; it
    // sets the 3 gauges but does NOT emit `ac_key_rotation_total` (that's
    // for the rotate path). This test validates both behaviors.
    let snap = MetricAssertion::snapshot();
    let master_key = test_master_key();
    key_management_service::initialize_signing_key(&pool, &master_key, "test-cluster")
        .await
        .unwrap();

    snap.gauge("ac_active_signing_keys").assert_value(1.0);
    snap.gauge("ac_signing_key_age_days").assert_value(0.0);

    let now = Utc::now().timestamp() as f64;
    snap.gauge("ac_key_rotation_last_success_timestamp")
        .assert_value_in_range((now - 5.0)..=(now + 5.0));

    // The rotation counter is NOT emitted on bootstrap — only on the
    // explicit rotate path (`rotate_signing_key`/`handle_rotate_keys`).
    snap.counter("ac_key_rotation_total")
        .with_labels(&[("status", "success")])
        .assert_unobserved();
    snap.counter("ac_key_rotation_total")
        .with_labels(&[("status", "error")])
        .assert_unobserved();
}

#[sqlx::test(migrations = "../../migrations")]
async fn handle_rotate_keys_missing_auth_does_not_emit_rotation_counter(pool: PgPool) {
    // Drive the admin handler at `handle_rotate_keys` — the only production
    // call site for `record_key_rotation` (admin_handler.rs:173,204,236,320,385).
    // Missing-Authorization-header path returns InvalidToken before reaching
    // any of the `record_key_rotation` sites, so this test EXPECTS no
    // `ac_key_rotation_total` emission AND no AcError-from-record_error
    // either. (Renamed per @observability F5: the prior name
    // `handle_rotate_keys_missing_auth_emits_status_error` falsely implied a
    // `status="error"` emission; the correct ground truth is "does not emit
    // ac_key_rotation_total at all" — the test uses `assert_unobserved`.)
    use ac_service::handlers::admin_handler::handle_rotate_keys;
    use axum::extract::{Request, State};

    let master_key = test_master_key();
    key_management_service::initialize_signing_key(&pool, &master_key, "test-cluster")
        .await
        .unwrap();
    let state = test_common::test_state::make_app_state(pool.clone());

    let snap = MetricAssertion::snapshot();
    let req = Request::builder()
        .method("POST")
        .uri("/internal/rotate-keys")
        .body(axum::body::Body::empty())
        .unwrap();
    let result = handle_rotate_keys(State(state), req).await;
    assert!(result.is_err(), "missing auth should error");

    // Per `admin_handler.rs:128-130` — missing Authorization header returns
    // before any `record_key_rotation` call. This is intentional production
    // behavior; the wrapper-side failure paths only fire after token
    // extraction.
    snap.counter("ac_key_rotation_total")
        .with_labels(&[("status", "error")])
        .assert_unobserved();
    snap.counter("ac_key_rotation_total")
        .with_labels(&[("status", "success")])
        .assert_unobserved();
}

#[sqlx::test(migrations = "../../migrations")]
async fn handle_rotate_keys_user_token_emits_status_error(pool: PgPool) {
    // Drive the user-token-rejection branch at `admin_handler.rs:204`.
    // This is one of the per-failure-class adjacency cells.
    use ac_service::crypto::Claims;
    use ac_service::handlers::admin_handler::handle_rotate_keys;
    use axum::extract::{Request, State};

    let master_key = test_master_key();
    key_management_service::initialize_signing_key(&pool, &master_key, "test-cluster")
        .await
        .unwrap();
    let state = test_common::test_state::make_app_state(pool.clone());

    // Sign a user token (service_type = None) with the active signing key.
    let now = Utc::now().timestamp();
    let claims = Claims {
        sub: "user-id".to_string(),
        exp: now + 3600,
        iat: now,
        scope: "service.rotate-keys.ac".to_string(),
        service_type: None, // <- this is what triggers the user-token rejection branch
    };
    let token = test_common::jwt_fixtures::sign_service_token(&pool, &master_key, &claims).await;

    let snap = MetricAssertion::snapshot();
    let req = Request::builder()
        .method("POST")
        .uri("/internal/rotate-keys")
        .header("Authorization", format!("Bearer {}", token))
        .body(axum::body::Body::empty())
        .unwrap();
    let result = handle_rotate_keys(State(state), req).await;
    assert!(result.is_err(), "user token must be rejected");

    snap.counter("ac_key_rotation_total")
        .with_labels(&[("status", "error")])
        .assert_delta(1);
    // Adjacency: success label must NOT fire on the failure path.
    snap.counter("ac_key_rotation_total")
        .with_labels(&[("status", "success")])
        .assert_delta(0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn initialize_signing_key_idempotent_does_not_re_emit_gauges(pool: PgPool) {
    // Bootstrap (out of snapshot) so the second call hits the early-return path.
    let master_key = test_master_key();
    key_management_service::initialize_signing_key(&pool, &master_key, "test-cluster")
        .await
        .unwrap();

    let snap = MetricAssertion::snapshot();
    key_management_service::initialize_signing_key(&pool, &master_key, "test-cluster")
        .await
        .unwrap();

    // Per `key_management_service.rs:42-45` — when an active key already
    // exists, initialize_signing_key returns Ok without setting gauges.
    // Per-failure-class adjacency: the early-return path must NOT touch
    // the gauges. This is the load-bearing assert_unobserved use case
    // from @test T1.
    snap.gauge("ac_active_signing_keys").assert_unobserved();
    snap.gauge("ac_signing_key_age_days").assert_unobserved();
    snap.gauge("ac_key_rotation_last_success_timestamp")
        .assert_unobserved();
    snap.counter("ac_key_rotation_total")
        .with_labels(&[("status", "success")])
        .assert_unobserved();
    snap.counter("ac_key_rotation_total")
        .with_labels(&[("status", "error")])
        .assert_unobserved();
}
