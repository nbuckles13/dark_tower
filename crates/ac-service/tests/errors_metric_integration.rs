// Every `#[sqlx::test]` is implicitly `flavor = "current_thread"`, and the
// pinning is LOAD-BEARING — `MetricAssertion` binds a per-thread recorder.
// See `crates/common/src/observability/testing.rs:60-72`.
//
//! Component tests for `ac_errors_total{operation, error_category, status_code}`
//! per ADR-0032 Step 4 §Cluster 14.
//!
//! # CR-4: per-`ErrorCategory` variant production-driven coverage
//!
//! Production has 30+ `record_error(...)` call sites across handlers. Per the
//! plan-stage @code-reviewer CR-4 ask, this file drives ONE production handler
//! per `ErrorCategory` enum variant from `observability/mod.rs:78`:
//!
//! - `Authentication` — `handle_service_token` with wrong `grant_type`
//!   → `AcError::InvalidCredentials` (`auth_handler.rs:232`)
//! - `Authorization`  — `handle_rotate_keys` with insufficient scope
//!   → `AcError::InsufficientScope` (`admin_handler.rs:231`)
//! - `Cryptographic`  — `handle_rotate_keys` with user JWT (no `service_type`)
//!   → `AcError::InvalidToken` (`admin_handler.rs:202`)
//! - `Internal`       — `handle_get_client` with non-existent UUID
//!   → `AcError::NotFound` → maps to `Internal` per
//!   `observability/mod.rs::From<&AcError>` (the `_ =>` arm at :110)
//!
//! Every test asserts `assert_delta(0)` adjacency on the OTHER three category
//! values for the same operation, demonstrating the label-swap-bug catcher
//! per ADR-0032 §Pattern #3.
//!
//! Transitive coverage for `Internal` from `AcError::Database(_)` is provided
//! by the `ErrorCategory::from(&AcError::Database(...))` unit test at
//! `observability/mod.rs::tests::test_error_category_database_variant` —
//! synthetic DB-fault-injection through a handler is unnecessary because the
//! `From<&AcError>` mapping is the only thing that can drift, and that mapping
//! is unit-tested directly.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "common/mod.rs"]
mod test_common;

use ac_service::crypto::Claims;
use ac_service::handlers::admin_handler::{handle_get_client, handle_rotate_keys};
use ac_service::handlers::auth_handler::{handle_service_token, ServiceTokenRequest};
use ac_service::services::key_management_service;
use ac_test_utils::crypto_fixtures::test_master_key;
use axum::body::Body;
use axum::extract::{ConnectInfo, Path, Request, State};
use axum::http::HeaderMap;
use axum::Json;
use chrono::Utc;
use common::observability::testing::MetricAssertion;
use sqlx::PgPool;
use std::net::SocketAddr;
use uuid::Uuid;

use test_common::jwt_fixtures::sign_service_token;
use test_common::test_state::make_app_state;

const TEST_ADDR: &str = "127.0.0.1:54321";

/// Every `error_category` value the wrapper can record at production sites.
/// Used for `assert_delta(0)` adjacency on the 3 non-target categories per
/// test (label-swap-bug catcher).
const ALL_CATEGORIES: &[&str] = &[
    "authentication",
    "authorization",
    "cryptographic",
    "internal",
];

// ---------------------------------------------------------------------------
// Authentication
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn handle_service_token_invalid_grant_type_emits_authentication_category(pool: PgPool) {
    // `auth_handler.rs:230-241` — wrong grant_type emits AcError::InvalidCredentials,
    // which maps to `ErrorCategory::Authentication` (status_code = 401).
    let master_key = test_master_key();
    key_management_service::initialize_signing_key(&pool, &master_key, "test-cluster")
        .await
        .unwrap();
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

    snap.counter("ac_errors_total")
        .with_labels(&[
            ("operation", "issue_service_token"),
            ("error_category", "authentication"),
            ("status_code", "401"),
        ])
        .assert_delta(1);

    // Adjacency: 3 sibling categories absent for the same operation.
    for sibling in ALL_CATEGORIES.iter().filter(|c| **c != "authentication") {
        snap.counter("ac_errors_total")
            .with_labels(&[
                ("operation", "issue_service_token"),
                ("error_category", *sibling),
            ])
            .assert_delta(0);
    }
}

// ---------------------------------------------------------------------------
// Authorization
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn handle_rotate_keys_insufficient_scope_emits_authorization_category(pool: PgPool) {
    // `admin_handler.rs:218-247` — service token without
    // `service.rotate-keys.ac` (or admin force scope) emits
    // AcError::InsufficientScope, which maps to ErrorCategory::Authorization
    // (status_code = 403).
    let master_key = test_master_key();
    key_management_service::initialize_signing_key(&pool, &master_key, "test-cluster")
        .await
        .unwrap();
    let state = make_app_state(pool.clone());

    // Sign a SERVICE token (service_type = Some) with WRONG scope —
    // passes the user-token check but fails the scope check.
    let now = Utc::now().timestamp();
    let claims = Claims {
        sub: "svc-bad-scope".to_string(),
        exp: now + 3600,
        iat: now,
        scope: "some.unrelated.scope".to_string(),
        service_type: Some("service".to_string()),
    };
    let token = sign_service_token(&pool, &master_key, &claims).await;

    let snap = MetricAssertion::snapshot();
    let req = Request::builder()
        .method("POST")
        .uri("/internal/rotate-keys")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let _ = handle_rotate_keys(State(state), req).await.unwrap_err();

    snap.counter("ac_errors_total")
        .with_labels(&[
            ("operation", "rotate_keys"),
            ("error_category", "authorization"),
            ("status_code", "403"),
        ])
        .assert_delta(1);

    for sibling in ALL_CATEGORIES.iter().filter(|c| **c != "authorization") {
        snap.counter("ac_errors_total")
            .with_labels(&[("operation", "rotate_keys"), ("error_category", *sibling)])
            .assert_delta(0);
    }
}

// ---------------------------------------------------------------------------
// Cryptographic
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn handle_rotate_keys_user_token_emits_cryptographic_category(pool: PgPool) {
    // `admin_handler.rs:201-210` — user JWT (service_type = None) emits
    // AcError::InvalidToken, which maps to ErrorCategory::Cryptographic
    // (status_code = 401).
    let master_key = test_master_key();
    key_management_service::initialize_signing_key(&pool, &master_key, "test-cluster")
        .await
        .unwrap();
    let state = make_app_state(pool.clone());

    let now = Utc::now().timestamp();
    let claims = Claims {
        sub: "user-id".to_string(),
        exp: now + 3600,
        iat: now,
        scope: "service.rotate-keys.ac".to_string(),
        service_type: None, // user token — triggers Cryptographic branch
    };
    let token = sign_service_token(&pool, &master_key, &claims).await;

    let snap = MetricAssertion::snapshot();
    let req = Request::builder()
        .method("POST")
        .uri("/internal/rotate-keys")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let _ = handle_rotate_keys(State(state), req).await.unwrap_err();

    snap.counter("ac_errors_total")
        .with_labels(&[
            ("operation", "rotate_keys"),
            ("error_category", "cryptographic"),
            ("status_code", "401"),
        ])
        .assert_delta(1);

    for sibling in ALL_CATEGORIES.iter().filter(|c| **c != "cryptographic") {
        snap.counter("ac_errors_total")
            .with_labels(&[("operation", "rotate_keys"), ("error_category", *sibling)])
            .assert_delta(0);
    }
}

// ---------------------------------------------------------------------------
// Internal
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn handle_get_client_not_found_emits_internal_category(pool: PgPool) {
    // `admin_handler.rs:625-654` — get_client with non-existent UUID emits
    // AcError::NotFound, which maps to ErrorCategory::Internal via the
    // `_ =>` arm in `observability/mod.rs::From<&AcError>` (status_code = 404).
    let state = make_app_state(pool);
    let snap = MetricAssertion::snapshot();
    let _ = handle_get_client(State(state), Path(Uuid::new_v4()))
        .await
        .unwrap_err();

    snap.counter("ac_errors_total")
        .with_labels(&[
            ("operation", "get_client"),
            ("error_category", "internal"),
            ("status_code", "404"),
        ])
        .assert_delta(1);

    for sibling in ALL_CATEGORIES.iter().filter(|c| **c != "internal") {
        snap.counter("ac_errors_total")
            .with_labels(&[("operation", "get_client"), ("error_category", *sibling)])
            .assert_delta(0);
    }
}
