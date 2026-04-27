// Every `#[tokio::test]` in this file is pinned to `flavor = "current_thread"`
// and that pinning is LOAD-BEARING — `MetricAssertion` binds a per-thread
// recorder. See `crates/common/src/observability/testing.rs:60-72`.
//
//! Component tests for AC's `crypto::verify_jwt` / `verify_user_jwt` driving
//! real `ac_token_validations_total{status,error_category}` emissions per
//! ADR-0032 Step 4 §Cluster 5.
//!
//! # Production-reachable label set is narrow (orphan-style finding)
//!
//! Per the plan-stage @observability re-review: the `record_token_validation`
//! wrapper at `metrics.rs:95` is `#[allow(dead_code)]` with comment
//! "Will be used in Phase 4 token validation endpoints." Production has
//! exactly TWO call sites — both `("error", Some("clock_skew"))` from
//! `crypto/mod.rs:284` (`verify_jwt`) and `:439` (`verify_user_jwt`). The
//! 4 other label combos shown in the in-src smoke test are forward-looking
//! reservations.
//!
//! This file asserts on the production-reachable label combo only. The
//! 4 reserved-but-unreachable sibling combos are asserted absent under
//! partial-label adjacency, demonstrating the per-failure-class fidelity
//! is bounded by production emission. See `docs/TODO.md` §Observability
//! Debt for the orphan disposition tracker.
//!
//! # `clock_skew` cardinality drift (NEW FINDING during Step 4 plan stage)
//!
//! Catalog `docs/observability/metrics/ac-service.md:39` declares
//! `error_category ∈ {authentication, authorization, cryptographic, internal, none}`.
//! Production emits a 5th value `clock_skew`. This file asserts on the
//! production ground truth (`clock_skew`); disposition pending team-lead
//! decision (see TODO entry).

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "common/mod.rs"]
mod test_common;

use ac_service::crypto::{self, Claims};
use ac_service::errors::AcError;
use chrono::Utc;
use common::observability::testing::MetricAssertion;
use sqlx::PgPool;
use std::time::Duration;

use test_common::jwt_fixtures::{sign_service_token, sign_user_token};
use test_common::test_state::seed_signing_key;

/// All bounded `error_category` values the wrapper can record (per
/// `metrics.rs:94` `#[allow(dead_code)]` comment + `crypto/mod.rs` callers).
/// Used for `assert_delta(0)` adjacency on every reserved-but-unreachable
/// sibling combo per ADR-0032 §Pattern #3.
const ALL_ERROR_CATEGORIES: &[&str] = &[
    "clock_skew",     // production-reachable (crypto/mod.rs:284,439)
    "authentication", // reserved for Phase 4
    "authorization",  // reserved for Phase 4
    "cryptographic",  // reserved for Phase 4
    "internal",       // reserved for Phase 4
];

const CLOCK_SKEW_TOLERANCE: Duration = Duration::from_secs(300); // 5 min

#[sqlx::test(migrations = "../../migrations")]
async fn verify_jwt_with_iat_too_far_future_emits_clock_skew(pool: PgPool) {
    seed_signing_key(&pool).await.unwrap();
    let snap = MetricAssertion::snapshot();

    // Build a token with iat way in the future (beyond the 5-min skew).
    let now = Utc::now().timestamp();
    let claims = Claims {
        sub: "test-subject".to_string(),
        exp: now + 7200,
        iat: now + 3600, // 1 hour in the future, beyond 5-min skew
        scope: "service.write".to_string(),
        service_type: Some("service".to_string()),
    };
    let master_key = ac_test_utils::crypto_fixtures::test_master_key();
    let token = sign_service_token(&pool, &master_key, &claims).await;
    let signing_key = ac_service::repositories::signing_keys::get_active_key(&pool)
        .await
        .unwrap()
        .unwrap();

    let result = crypto::verify_jwt(&token, &signing_key.public_key, CLOCK_SKEW_TOLERANCE);

    // Verification MUST reject the token (security correctness).
    assert!(result.is_err(), "iat-future token must be rejected");
    assert!(
        matches!(result.unwrap_err(), AcError::InvalidToken(_)),
        "expected InvalidToken error variant"
    );

    // Metric: production-reachable label combo emits.
    snap.counter("ac_token_validations_total")
        .with_labels(&[("status", "error"), ("error_category", "clock_skew")])
        .assert_delta(1);

    // Adjacency: every reserved-but-unreachable sibling combo absent under
    // partial-label filter (label-swap-bug catcher per ADR-0032 §Pattern #3).
    for sibling in ALL_ERROR_CATEGORIES.iter().filter(|c| **c != "clock_skew") {
        snap.counter("ac_token_validations_total")
            .with_labels(&[("status", "error"), ("error_category", *sibling)])
            .assert_delta(0);
    }
    // Also absent: status=success path (no production code path emits this today).
    snap.counter("ac_token_validations_total")
        .with_labels(&[("status", "success")])
        .assert_delta(0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn verify_user_jwt_with_iat_too_far_future_emits_clock_skew(pool: PgPool) {
    seed_signing_key(&pool).await.unwrap();
    let snap = MetricAssertion::snapshot();

    // Build a user token with iat way in the future.
    let now = Utc::now().timestamp();
    let user_claims = common::jwt::UserClaims {
        sub: uuid::Uuid::new_v4().to_string(),
        org_id: uuid::Uuid::new_v4().to_string(),
        email: "test@example.com".to_string(),
        roles: vec!["user".to_string()],
        exp: now + 7200,
        iat: now + 3600,
        jti: uuid::Uuid::new_v4().to_string(),
    };
    let master_key = ac_test_utils::crypto_fixtures::test_master_key();
    let token = sign_user_token(&pool, &master_key, &user_claims).await;
    let signing_key = ac_service::repositories::signing_keys::get_active_key(&pool)
        .await
        .unwrap()
        .unwrap();

    let result = crypto::verify_user_jwt(&token, &signing_key.public_key, CLOCK_SKEW_TOLERANCE);

    assert!(result.is_err(), "iat-future user token must be rejected");
    assert!(
        matches!(result.unwrap_err(), AcError::InvalidToken(_)),
        "expected InvalidToken error variant"
    );

    snap.counter("ac_token_validations_total")
        .with_labels(&[("status", "error"), ("error_category", "clock_skew")])
        .assert_delta(1);

    for sibling in ALL_ERROR_CATEGORIES.iter().filter(|c| **c != "clock_skew") {
        snap.counter("ac_token_validations_total")
            .with_labels(&[("status", "error"), ("error_category", *sibling)])
            .assert_delta(0);
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn verify_jwt_happy_path_does_not_emit_token_validations(pool: PgPool) {
    // Adjacency: a successful verify_jwt call must NOT emit
    // ac_token_validations_total — production today only emits on
    // clock-skew error. This catches a regression where someone wires
    // up the success path emission incorrectly.
    seed_signing_key(&pool).await.unwrap();
    let snap = MetricAssertion::snapshot();

    let now = Utc::now().timestamp();
    let claims = Claims {
        sub: "test-subject".to_string(),
        exp: now + 3600,
        iat: now,
        scope: "service.write".to_string(),
        service_type: Some("service".to_string()),
    };
    let master_key = ac_test_utils::crypto_fixtures::test_master_key();
    let token = sign_service_token(&pool, &master_key, &claims).await;
    let signing_key = ac_service::repositories::signing_keys::get_active_key(&pool)
        .await
        .unwrap()
        .unwrap();

    let result = crypto::verify_jwt(&token, &signing_key.public_key, CLOCK_SKEW_TOLERANCE);
    assert!(result.is_ok(), "happy-path verify must succeed");

    // No emissions on the happy path. Hard assertion (assert_unobserved)
    // would catch a regression where the success path accidentally emits
    // under wrong kind. This is the load-bearing case for assert_unobserved
    // per @test T1.
    snap.counter("ac_token_validations_total")
        .assert_unobserved();
}
