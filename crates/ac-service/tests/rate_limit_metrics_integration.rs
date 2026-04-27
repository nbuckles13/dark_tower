// Every `#[sqlx::test]` in this file is implicitly `flavor = "current_thread"`
// (sqlx::test default), and that pinning is LOAD-BEARING — `MetricAssertion`
// binds a per-thread recorder; multi-thread runtime would route emissions
// through a different OS thread. See
// `crates/common/src/observability/testing.rs:60-72`.
//
//! Component tests for AC's `record_rate_limit_decision{action}` driving
//! per (gate × outcome) combination per ADR-0032 Step 4 §Cluster 12.
//!
//! # Per @security: 6 cells (3 gates × 2 outcomes) hard rule
//!
//! Three production gates emit rate-limit decisions:
//! - service-token (`token_service::issue_service_token` rate-check)
//! - user-token (`token_service::issue_user_token` rate-check)
//! - registration (`user_service::register_user` rate-check)
//!
//! Each gate × {`allowed`, `rejected`} = 6 distinct test cells. Each test
//! takes a fresh `MetricAssertion::snapshot()` immediately before the
//! decision-under-test fires, then asserts the named outcome with adjacency
//! on the sibling outcome.
//!
//! @security review: "If the suite ends up with fewer than 6 (e.g., only the
//! `rejected` path is tested for one gate because rejection is 'easier to
//! set up'), the per-gate fidelity argument breaks down."

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "common/mod.rs"]
mod test_common;

use ac_service::services::token_service;
use common::observability::testing::MetricAssertion;
use common::secret::ExposeSecret;
use sqlx::PgPool;

use test_common::test_state::{
    make_app_state, seed_service_credential, seed_signing_key, TEST_CLIENT_SECRET,
};

// ---------------------------------------------------------------------------
// Service-token gate
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn rate_limit_decision_allowed_emits_for_service_token_gate(pool: PgPool) {
    seed_signing_key(&pool).await.unwrap();
    seed_service_credential(&pool, "rate-allowed-svc", &["service.write"])
        .await
        .unwrap();
    let state = make_app_state(pool.clone());

    let snap = MetricAssertion::snapshot();
    let _ = token_service::issue_service_token(
        &pool,
        state.config.master_key.expose_secret(),
        state.config.hash_secret.expose_secret(),
        "rate-allowed-svc",
        TEST_CLIENT_SECRET,
        "client_credentials",
        None,
        None,
        None,
        state.config.rate_limit_window_minutes,
        state.config.rate_limit_max_attempts,
    )
    .await;

    snap.counter("ac_rate_limit_decisions_total")
        .with_labels(&[("action", "allowed")])
        .assert_delta(1);
    snap.counter("ac_rate_limit_decisions_total")
        .with_labels(&[("action", "rejected")])
        .assert_delta(0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn rate_limit_decision_rejected_emits_for_service_token_gate(pool: PgPool) {
    seed_signing_key(&pool).await.unwrap();
    seed_service_credential(&pool, "rate-rejected-svc", &["service.write"])
        .await
        .unwrap();
    let state = make_app_state(pool.clone());

    // Burn through max_attempts with bad creds to trigger rate-limit. The
    // first N attempts emit `allowed` then fail bcrypt. The (N+1)-th attempt
    // is the one we want to snapshot.
    for _ in 0..state.config.rate_limit_max_attempts {
        let _ = token_service::issue_service_token(
            &pool,
            state.config.master_key.expose_secret(),
            state.config.hash_secret.expose_secret(),
            "rate-rejected-svc",
            "wrong-secret",
            "client_credentials",
            None,
            None,
            None,
            state.config.rate_limit_window_minutes,
            state.config.rate_limit_max_attempts,
        )
        .await;
    }

    // Fresh snapshot before the (N+1)-th attempt — captures only that
    // decision per @security's "structural separation" guidance.
    let snap = MetricAssertion::snapshot();
    let _ = token_service::issue_service_token(
        &pool,
        state.config.master_key.expose_secret(),
        state.config.hash_secret.expose_secret(),
        "rate-rejected-svc",
        TEST_CLIENT_SECRET,
        "client_credentials",
        None,
        None,
        None,
        state.config.rate_limit_window_minutes,
        state.config.rate_limit_max_attempts,
    )
    .await;

    snap.counter("ac_rate_limit_decisions_total")
        .with_labels(&[("action", "rejected")])
        .assert_delta(1);
    snap.counter("ac_rate_limit_decisions_total")
        .with_labels(&[("action", "allowed")])
        .assert_delta(0);
}

// ---------------------------------------------------------------------------
// User-token gate
// ---------------------------------------------------------------------------

async fn seed_user(pool: &PgPool, email: &str, password: &str) -> uuid::Uuid {
    let org_id = sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO organizations (subdomain, display_name) VALUES ($1, $2) RETURNING org_id",
    )
    .bind(format!("rate-{}", uuid::Uuid::new_v4()))
    .bind("Rate test org")
    .fetch_one(pool)
    .await
    .unwrap();

    let pwd_hash =
        ac_service::crypto::hash_client_secret(password, ac_service::config::MIN_BCRYPT_COST)
            .unwrap();
    sqlx::query(
        "INSERT INTO users (org_id, email, password_hash, display_name) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(org_id)
    .bind(email)
    .bind(&pwd_hash)
    .bind("Rate Test User")
    .execute(pool)
    .await
    .unwrap();
    org_id
}

#[sqlx::test(migrations = "../../migrations")]
async fn rate_limit_decision_allowed_emits_for_user_token_gate(pool: PgPool) {
    seed_signing_key(&pool).await.unwrap();
    let org_id = seed_user(
        &pool,
        "rate-allowed-user@example.com",
        "test-password-12345",
    )
    .await;
    let state = make_app_state(pool.clone());

    let snap = MetricAssertion::snapshot();
    let _ = token_service::issue_user_token(
        &pool,
        state.config.master_key.expose_secret(),
        state.config.hash_secret.expose_secret(),
        org_id,
        "rate-allowed-user@example.com",
        "test-password-12345",
        None,
        None,
        state.config.rate_limit_window_minutes,
        state.config.rate_limit_max_attempts,
    )
    .await;

    snap.counter("ac_rate_limit_decisions_total")
        .with_labels(&[("action", "allowed")])
        .assert_delta(1);
    snap.counter("ac_rate_limit_decisions_total")
        .with_labels(&[("action", "rejected")])
        .assert_delta(0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn rate_limit_decision_rejected_emits_for_user_token_gate(pool: PgPool) {
    seed_signing_key(&pool).await.unwrap();
    let org_id = seed_user(&pool, "rate-rejected-user@example.com", "right-password").await;
    let state = make_app_state(pool.clone());

    for _ in 0..state.config.rate_limit_max_attempts {
        let _ = token_service::issue_user_token(
            &pool,
            state.config.master_key.expose_secret(),
            state.config.hash_secret.expose_secret(),
            org_id,
            "rate-rejected-user@example.com",
            "wrong-password",
            None,
            None,
            state.config.rate_limit_window_minutes,
            state.config.rate_limit_max_attempts,
        )
        .await;
    }

    let snap = MetricAssertion::snapshot();
    let _ = token_service::issue_user_token(
        &pool,
        state.config.master_key.expose_secret(),
        state.config.hash_secret.expose_secret(),
        org_id,
        "rate-rejected-user@example.com",
        "right-password",
        None,
        None,
        state.config.rate_limit_window_minutes,
        state.config.rate_limit_max_attempts,
    )
    .await;

    snap.counter("ac_rate_limit_decisions_total")
        .with_labels(&[("action", "rejected")])
        .assert_delta(1);
    snap.counter("ac_rate_limit_decisions_total")
        .with_labels(&[("action", "allowed")])
        .assert_delta(0);
}

// ---------------------------------------------------------------------------
// Registration gate (user_service::register_user)
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn rate_limit_decision_allowed_emits_for_registration_gate(pool: PgPool) {
    seed_signing_key(&pool).await.unwrap();
    let state = make_app_state(pool.clone());
    let org_id = sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO organizations (subdomain, display_name) VALUES ($1, $2) RETURNING org_id",
    )
    .bind(format!("reg-{}", uuid::Uuid::new_v4()))
    .bind("Reg test org")
    .fetch_one(&pool)
    .await
    .unwrap();

    let snap = MetricAssertion::snapshot();
    let request = ac_service::services::user_service::RegistrationRequest {
        email: "new-user@example.com".to_string(),
        password: "registration-password-123".to_string(),
        display_name: "New User".to_string(),
    };
    let _ = ac_service::services::user_service::register_user(
        &pool,
        state.config.master_key.expose_secret(),
        state.config.hash_secret.expose_secret(),
        org_id,
        request,
        Some("198.51.100.42"), // Documentation-range IP; rate-limit branch fires only with Some(ip).
        None,
        state.config.bcrypt_cost,
        state.config.registration_rate_limit_window_minutes,
        state.config.registration_rate_limit_max_attempts,
        state.config.rate_limit_window_minutes,
        state.config.rate_limit_max_attempts,
    )
    .await;

    // Successful registration emits 2 `allowed` decisions: one from the
    // registration gate (`user_service::register_user:91`) and one from
    // the chained user-token issuance (`token_service::issue_user_token:242`,
    // called at user_service.rs:148 for auto-login). Both are production
    // emissions; assert the cumulative count.
    snap.counter("ac_rate_limit_decisions_total")
        .with_labels(&[("action", "allowed")])
        .assert_delta(2);
    snap.counter("ac_rate_limit_decisions_total")
        .with_labels(&[("action", "rejected")])
        .assert_delta(0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn rate_limit_decision_rejected_emits_for_registration_gate(pool: PgPool) {
    seed_signing_key(&pool).await.unwrap();
    let state = make_app_state(pool.clone());
    let org_id = sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO organizations (subdomain, display_name) VALUES ($1, $2) RETURNING org_id",
    )
    .bind(format!("regreject-{}", uuid::Uuid::new_v4()))
    .bind("Reg test org")
    .fetch_one(&pool)
    .await
    .unwrap();

    // The registration rate-limit branch counts `user_login` events
    // (success=true) from the same IP within the window — see
    // `user_service::count_registrations_from_ip` at user_service.rs:206-230.
    // The auth_events table requires either user_id or credential_id (per
    // the `event_has_subject` CHECK constraint in
    // migrations/20250122000001_auth_controller_tables.sql). Seed a sentinel
    // user just to satisfy the FK + constraint, then stamp N events.
    let sentinel_user_id = sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO users (org_id, email, password_hash, display_name) \
         VALUES ($1, 'sentinel@example.com', 'unused', 'Sentinel') RETURNING user_id",
    )
    .bind(org_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let test_ip = "198.51.100.42";
    for _ in 0..state.config.registration_rate_limit_max_attempts {
        sqlx::query(
            "INSERT INTO auth_events \
             (event_type, user_id, success, ip_address, created_at) \
             VALUES ('user_login', $1, true, $2::inet, NOW())",
        )
        .bind(sentinel_user_id)
        .bind(test_ip)
        .execute(&pool)
        .await
        .unwrap();
    }

    let snap = MetricAssertion::snapshot();
    let request = ac_service::services::user_service::RegistrationRequest {
        email: "should-be-rejected@example.com".to_string(),
        password: "any-password".to_string(),
        display_name: "Rejected".to_string(),
    };
    let _ = ac_service::services::user_service::register_user(
        &pool,
        state.config.master_key.expose_secret(),
        state.config.hash_secret.expose_secret(),
        org_id,
        request,
        Some(test_ip),
        None,
        state.config.bcrypt_cost,
        state.config.registration_rate_limit_window_minutes,
        state.config.registration_rate_limit_max_attempts,
        state.config.rate_limit_window_minutes,
        state.config.rate_limit_max_attempts,
    )
    .await;

    snap.counter("ac_rate_limit_decisions_total")
        .with_labels(&[("action", "rejected")])
        .assert_delta(1);
    snap.counter("ac_rate_limit_decisions_total")
        .with_labels(&[("action", "allowed")])
        .assert_delta(0);
}
