// Every `#[sqlx::test]` is implicitly `flavor = "current_thread"` (sqlx::test
// default), and the pinning is LOAD-BEARING — see
// `crates/common/src/observability/testing.rs:60-72`.
//
//! Component tests for `ac_token_issuance_total{grant_type=client_credentials,status}`
//! + `ac_token_issuance_duration_seconds{grant_type=client_credentials,status}`
//!   per ADR-0032 Step 4 §Cluster 2.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "common/mod.rs"]
mod test_common;

use ac_service::handlers::auth_handler::{handle_service_token, ServiceTokenRequest};
use axum::extract::{ConnectInfo, State};
use axum::http::HeaderMap;
use axum::Json;
use common::observability::testing::MetricAssertion;
use sqlx::PgPool;
use std::net::SocketAddr;

use test_common::test_state::{
    make_app_state, seed_service_credential, seed_signing_key, TEST_CLIENT_SECRET,
};

const TEST_ADDR: &str = "127.0.0.1:54321";

#[sqlx::test(migrations = "../../migrations")]
async fn handle_service_token_success_emits_grant_type_client_credentials_status_success(
    pool: PgPool,
) {
    seed_signing_key(&pool).await.unwrap();
    seed_service_credential(&pool, "tok-svc-success", &["service.write"])
        .await
        .unwrap();
    let state = make_app_state(pool);

    let snap = MetricAssertion::snapshot();
    let result = handle_service_token(
        State(state),
        ConnectInfo(TEST_ADDR.parse::<SocketAddr>().unwrap()),
        HeaderMap::new(),
        Json(ServiceTokenRequest {
            grant_type: "client_credentials".to_string(),
            client_id: Some("tok-svc-success".to_string()),
            client_secret: Some(TEST_CLIENT_SECRET.to_string().into()),
            scope: None,
        }),
    )
    .await;
    assert!(
        result.is_ok(),
        "service token should succeed: {:?}",
        result.err()
    );

    snap.histogram("ac_token_issuance_duration_seconds")
        .with_labels(&[("grant_type", "client_credentials"), ("status", "success")])
        .assert_observation_count(1);

    snap.counter("ac_token_issuance_total")
        .with_labels(&[("grant_type", "client_credentials"), ("status", "success")])
        .assert_delta(1);

    // Adjacency: error variant must NOT fire.
    snap.counter("ac_token_issuance_total")
        .with_labels(&[("grant_type", "client_credentials"), ("status", "error")])
        .assert_delta(0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn handle_service_token_invalid_grant_type_emits_status_error(pool: PgPool) {
    seed_signing_key(&pool).await.unwrap();
    let state = make_app_state(pool);

    let snap = MetricAssertion::snapshot();
    let _ = handle_service_token(
        State(state),
        ConnectInfo(TEST_ADDR.parse::<SocketAddr>().unwrap()),
        HeaderMap::new(),
        Json(ServiceTokenRequest {
            grant_type: "password".to_string(), // Wrong grant_type
            client_id: Some("anything".to_string()),
            client_secret: Some("anything".to_string().into()),
            scope: None,
        }),
    )
    .await
    .unwrap_err();

    snap.histogram("ac_token_issuance_duration_seconds")
        .with_labels(&[("grant_type", "client_credentials"), ("status", "error")])
        .assert_observation_count(1);

    snap.counter("ac_token_issuance_total")
        .with_labels(&[("grant_type", "client_credentials"), ("status", "error")])
        .assert_delta(1);

    snap.counter("ac_token_issuance_total")
        .with_labels(&[("grant_type", "client_credentials"), ("status", "success")])
        .assert_delta(0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn handle_service_token_bad_credentials_emits_status_error(pool: PgPool) {
    seed_signing_key(&pool).await.unwrap();
    seed_service_credential(&pool, "tok-svc-bad-creds", &["service.write"])
        .await
        .unwrap();
    let state = make_app_state(pool);

    let snap = MetricAssertion::snapshot();
    let _ = handle_service_token(
        State(state),
        ConnectInfo(TEST_ADDR.parse::<SocketAddr>().unwrap()),
        HeaderMap::new(),
        Json(ServiceTokenRequest {
            grant_type: "client_credentials".to_string(),
            client_id: Some("tok-svc-bad-creds".to_string()),
            client_secret: Some("wrong-secret".to_string().into()),
            scope: None,
        }),
    )
    .await
    .unwrap_err();

    snap.histogram("ac_token_issuance_duration_seconds")
        .with_labels(&[("grant_type", "client_credentials"), ("status", "error")])
        .assert_observation_count(1);

    snap.counter("ac_token_issuance_total")
        .with_labels(&[("grant_type", "client_credentials"), ("status", "error")])
        .assert_delta(1);
}
