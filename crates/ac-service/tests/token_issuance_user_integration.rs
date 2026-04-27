// Every `#[sqlx::test]` is implicitly `flavor = "current_thread"`.
//
//! Component tests for `ac_token_issuance_total{grant_type=password|registration,status}`
//! + duration per ADR-0032 Step 4 §Cluster 3.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "common/mod.rs"]
mod test_common;

use ac_service::handlers::auth_handler::{
    handle_register, handle_user_token, UserRegistrationRequest, UserTokenRequest,
};
use ac_service::middleware::org_extraction::OrgContext;
use axum::extract::{ConnectInfo, Extension, State};
use axum::http::HeaderMap;
use axum::Json;
use common::observability::testing::MetricAssertion;
use sqlx::PgPool;
use std::net::SocketAddr;
use uuid::Uuid;

use test_common::test_state::{make_app_state, seed_signing_key};

const TEST_ADDR: &str = "127.0.0.1:54321";

async fn seed_org(pool: &PgPool) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO organizations (subdomain, display_name) VALUES ($1, $2) RETURNING org_id",
    )
    .bind(format!("token-org-{}", Uuid::new_v4()))
    .bind("Token Test Org")
    .fetch_one(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn handle_user_token_success_emits_grant_type_password_status_success(pool: PgPool) {
    seed_signing_key(&pool).await.unwrap();
    let org_id = seed_org(&pool).await;

    // Seed a user with a known password.
    let pwd = "valid-test-password-1234";
    let pwd_hash =
        ac_service::crypto::hash_client_secret(pwd, ac_service::config::MIN_BCRYPT_COST).unwrap();
    sqlx::query(
        "INSERT INTO users (org_id, email, password_hash, display_name) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(org_id)
    .bind("password-tok-user@example.com")
    .bind(&pwd_hash)
    .bind("Pwd User")
    .execute(&pool)
    .await
    .unwrap();
    let state = make_app_state(pool);

    let snap = MetricAssertion::snapshot();
    let _ = handle_user_token(
        State(state),
        ConnectInfo(TEST_ADDR.parse::<SocketAddr>().unwrap()),
        Extension(OrgContext {
            org_id,
            subdomain: "token-test".to_string(),
        }),
        HeaderMap::new(),
        Json(UserTokenRequest {
            email: "password-tok-user@example.com".to_string(),
            password: pwd.to_string().into(),
        }),
    )
    .await
    .unwrap();

    snap.histogram("ac_token_issuance_duration_seconds")
        .with_labels(&[("grant_type", "password"), ("status", "success")])
        .assert_observation_count(1);

    snap.counter("ac_token_issuance_total")
        .with_labels(&[("grant_type", "password"), ("status", "success")])
        .assert_delta(1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn handle_register_success_emits_grant_type_registration_status_success(pool: PgPool) {
    seed_signing_key(&pool).await.unwrap();
    let org_id = seed_org(&pool).await;
    let state = make_app_state(pool);

    let snap = MetricAssertion::snapshot();
    let _ = handle_register(
        State(state),
        ConnectInfo(TEST_ADDR.parse::<SocketAddr>().unwrap()),
        Extension(OrgContext {
            org_id,
            subdomain: "token-test".to_string(),
        }),
        HeaderMap::new(),
        Json(UserRegistrationRequest {
            email: "register-success@example.com".to_string(),
            password: "valid-registration-password-1234".to_string().into(),
            display_name: "Reg Success".to_string(),
        }),
    )
    .await
    .unwrap();

    snap.histogram("ac_token_issuance_duration_seconds")
        .with_labels(&[("grant_type", "registration"), ("status", "success")])
        .assert_observation_count(1);

    snap.counter("ac_token_issuance_total")
        .with_labels(&[("grant_type", "registration"), ("status", "success")])
        .assert_delta(1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn handle_user_token_bad_credentials_emits_grant_type_password_status_error(pool: PgPool) {
    // Per @observability F3 close: drive `handle_user_token` with credentials
    // that don't match any seeded user → `token_service::issue_user_token`
    // returns AcError::InvalidCredentials → record_token_issuance("password",
    // "error", duration) fires at auth_handler.rs:124.
    seed_signing_key(&pool).await.unwrap();
    let org_id = seed_org(&pool).await;
    let state = make_app_state(pool);

    let snap = MetricAssertion::snapshot();
    let _ = handle_user_token(
        State(state),
        ConnectInfo(TEST_ADDR.parse::<SocketAddr>().unwrap()),
        Extension(OrgContext {
            org_id,
            subdomain: "token-test".to_string(),
        }),
        HeaderMap::new(),
        Json(UserTokenRequest {
            email: "nonexistent@example.com".to_string(),
            password: "anything".to_string().into(),
        }),
    )
    .await
    .unwrap_err();

    snap.histogram("ac_token_issuance_duration_seconds")
        .with_labels(&[("grant_type", "password"), ("status", "error")])
        .assert_observation_count(1);
    snap.counter("ac_token_issuance_total")
        .with_labels(&[("grant_type", "password"), ("status", "error")])
        .assert_delta(1);

    // Adjacency: success cell must NOT fire on this path.
    snap.counter("ac_token_issuance_total")
        .with_labels(&[("grant_type", "password"), ("status", "success")])
        .assert_delta(0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn handle_register_duplicate_email_emits_grant_type_registration_status_error(pool: PgPool) {
    // Per @observability F3 close: drive `handle_register` with an already-
    // taken email → `user_service::register_user` returns Err(Database) on
    // the duplicate-email constraint violation → record_token_issuance(
    // "registration", "error", duration) fires at auth_handler.rs:200-area.
    seed_signing_key(&pool).await.unwrap();
    let org_id = seed_org(&pool).await;
    let state = make_app_state(pool.clone());

    // Seed a user that the second registration will collide with.
    let existing_email = "duplicate@example.com";
    let pwd_hash = ac_service::crypto::hash_client_secret(
        "existing-pwd-1234",
        ac_service::config::MIN_BCRYPT_COST,
    )
    .unwrap();
    sqlx::query(
        "INSERT INTO users (org_id, email, password_hash, display_name) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(org_id)
    .bind(existing_email)
    .bind(&pwd_hash)
    .bind("Existing User")
    .execute(&pool)
    .await
    .unwrap();

    let snap = MetricAssertion::snapshot();
    let _ = handle_register(
        State(state),
        ConnectInfo(TEST_ADDR.parse::<SocketAddr>().unwrap()),
        Extension(OrgContext {
            org_id,
            subdomain: "token-test".to_string(),
        }),
        HeaderMap::new(),
        Json(UserRegistrationRequest {
            email: existing_email.to_string(),
            password: "valid-pwd-9999".to_string().into(),
            display_name: "Dup".to_string(),
        }),
    )
    .await
    .unwrap_err();

    snap.histogram("ac_token_issuance_duration_seconds")
        .with_labels(&[("grant_type", "registration"), ("status", "error")])
        .assert_observation_count(1);
    snap.counter("ac_token_issuance_total")
        .with_labels(&[("grant_type", "registration"), ("status", "error")])
        .assert_delta(1);

    snap.counter("ac_token_issuance_total")
        .with_labels(&[("grant_type", "registration"), ("status", "success")])
        .assert_delta(0);
}
