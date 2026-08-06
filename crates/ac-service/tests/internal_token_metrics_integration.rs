// Every `#[sqlx::test]` is implicitly `flavor = "current_thread"`.
//
//! Component tests for `ac_token_issuance_total{grant_type=internal_meeting|internal_guest,status}`
//! per ADR-0032 Step 4 §Cluster 4.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "common/mod.rs"]
mod test_common;

use ac_service::handlers::internal_tokens::{handle_guest_token, handle_meeting_token};
use ac_service::models::{GuestTokenRequest, MeetingTokenRequest};
use ac_service::services::key_management_service;
use ac_test_utils::crypto_fixtures::test_master_key;
use axum::extract::{Extension, State};
use axum::Json;
use chrono::Utc;
use common::meeting_token::{MeetingRole, ParticipantType};
use common::observability::testing::MetricAssertion;
use sqlx::PgPool;
use uuid::Uuid;

use test_common::test_state::make_app_state;

const REQUIRED_SCOPE: &str = "internal:meeting-token";

fn make_claims(scope: &str) -> ac_service::crypto::Claims {
    let now = Utc::now().timestamp();
    ac_service::crypto::Claims {
        sub: "test-svc".to_string(),
        exp: now + 3600,
        iat: now,
        scope: scope.to_string(),
        service_type: Some("service".to_string()),
    }
}

/// Seed an org + user and return the user_id. Meeting-token issuance looks up
/// `users.display_name` and fails closed on a missing row, so a success-path
/// test must seed the subject.
async fn seed_user(pool: &PgPool, display_name: &str) -> Uuid {
    let unique = Uuid::new_v4();
    let org: (Uuid,) = sqlx::query_as(
        "INSERT INTO organizations (subdomain, display_name) VALUES ($1, $2) RETURNING org_id",
    )
    .bind(format!("org-{unique}"))
    .bind("Test Org")
    .fetch_one(pool)
    .await
    .unwrap();
    let user: (Uuid,) = sqlx::query_as(
        "INSERT INTO users (org_id, email, password_hash, display_name) \
         VALUES ($1, $2, $3, $4) RETURNING user_id",
    )
    .bind(org.0)
    .bind(format!("user-{unique}@example.com"))
    .bind("x")
    .bind(display_name)
    .fetch_one(pool)
    .await
    .unwrap();
    user.0
}

#[sqlx::test(migrations = "../../migrations")]
async fn handle_meeting_token_success_emits_grant_type_internal_meeting_status_success(
    pool: PgPool,
) {
    let master_key = test_master_key();
    key_management_service::initialize_signing_key(&pool, &master_key, "test-cluster")
        .await
        .unwrap();
    let subject_user_id = seed_user(&pool, "Alice Example").await;
    let state = make_app_state(pool);

    let snap = MetricAssertion::snapshot();
    let _ = handle_meeting_token(
        State(state),
        Extension(make_claims(REQUIRED_SCOPE)),
        Json(MeetingTokenRequest {
            subject_user_id,
            meeting_id: Uuid::new_v4(),
            home_org_id: Uuid::new_v4(),
            meeting_org_id: Uuid::new_v4(),
            participant_type: ParticipantType::Member,
            role: MeetingRole::Participant,
            capabilities: vec!["video".to_string()],
            ttl_seconds: 600,
        }),
    )
    .await
    .unwrap();

    snap.histogram("ac_token_issuance_duration_seconds")
        .with_labels(&[("grant_type", "internal_meeting"), ("status", "success")])
        .assert_observation_count(1);

    snap.counter("ac_token_issuance_total")
        .with_labels(&[("grant_type", "internal_meeting"), ("status", "success")])
        .assert_delta(1);

    // Production-path coverage for the display-name lookup outcome counter.
    snap.counter("ac_meeting_token_display_name_total")
        .with_labels(&[("outcome", "resolved")])
        .assert_delta(1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn handle_meeting_token_missing_user_emits_outcome_user_not_found(pool: PgPool) {
    let master_key = test_master_key();
    key_management_service::initialize_signing_key(&pool, &master_key, "test-cluster")
        .await
        .unwrap();
    // Deliberately do NOT seed the subject user -> fail-closed (missing row) path.
    let state = make_app_state(pool);

    let snap = MetricAssertion::snapshot();
    let err = handle_meeting_token(
        State(state),
        Extension(make_claims(REQUIRED_SCOPE)),
        Json(MeetingTokenRequest {
            subject_user_id: Uuid::new_v4(),
            meeting_id: Uuid::new_v4(),
            home_org_id: Uuid::new_v4(),
            meeting_org_id: Uuid::new_v4(),
            participant_type: ParticipantType::Member,
            role: MeetingRole::Participant,
            capabilities: vec!["video".to_string()],
            ttl_seconds: 600,
        }),
    )
    .await
    .unwrap_err();

    // Fail-closed surfaces as HTTP 404.
    assert_eq!(
        err.status_code(),
        404,
        "missing users row must fail closed 404"
    );

    // The alerting-critical failure label MUST actually be emitted. Without this
    // assertion a label typo (e.g. "user_notfound") would compile, still 404,
    // pass every behavior test, and silently emit nothing to the SLO signal.
    snap.counter("ac_meeting_token_display_name_total")
        .with_labels(&[("outcome", "user_not_found")])
        .assert_delta(1);

    // NOTE: the `lookup_error` label is intentionally NOT asserted here.
    // Inducing a real DB error (broken/closed pool) inside the sqlx::test
    // harness is disproportionate and fragile. The emission mechanism and
    // label-spelling pattern are already locked by this `user_not_found`
    // assertion, the `resolved` assertion in the success test, the pure
    // `resolve_meeting_display_name` unit tests, and the 404 behavior test.
    // (Accepted with @observability.)
}

#[sqlx::test(migrations = "../../migrations")]
async fn handle_meeting_token_missing_scope_emits_grant_type_internal_meeting_status_error(
    pool: PgPool,
) {
    let master_key = test_master_key();
    key_management_service::initialize_signing_key(&pool, &master_key, "test-cluster")
        .await
        .unwrap();
    let state = make_app_state(pool);

    let snap = MetricAssertion::snapshot();
    let _ = handle_meeting_token(
        State(state),
        Extension(make_claims("wrong:scope")), // missing internal:meeting-token
        Json(MeetingTokenRequest {
            subject_user_id: Uuid::new_v4(),
            meeting_id: Uuid::new_v4(),
            home_org_id: Uuid::new_v4(),
            meeting_org_id: Uuid::new_v4(),
            participant_type: ParticipantType::Member,
            role: MeetingRole::Participant,
            capabilities: vec![],
            ttl_seconds: 600,
        }),
    )
    .await
    .unwrap_err();

    snap.histogram("ac_token_issuance_duration_seconds")
        .with_labels(&[("grant_type", "internal_meeting"), ("status", "error")])
        .assert_observation_count(1);

    snap.counter("ac_token_issuance_total")
        .with_labels(&[("grant_type", "internal_meeting"), ("status", "error")])
        .assert_delta(1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn handle_guest_token_success_emits_grant_type_internal_guest_status_success(pool: PgPool) {
    let master_key = test_master_key();
    key_management_service::initialize_signing_key(&pool, &master_key, "test-cluster")
        .await
        .unwrap();
    let state = make_app_state(pool);

    let snap = MetricAssertion::snapshot();
    let _ = handle_guest_token(
        State(state),
        Extension(make_claims(REQUIRED_SCOPE)),
        Json(GuestTokenRequest {
            guest_id: Uuid::new_v4(),
            display_name: "Alice".to_string(),
            meeting_id: Uuid::new_v4(),
            meeting_org_id: Uuid::new_v4(),
            waiting_room: false,
            ttl_seconds: 300,
        }),
    )
    .await
    .unwrap();

    snap.histogram("ac_token_issuance_duration_seconds")
        .with_labels(&[("grant_type", "internal_guest"), ("status", "success")])
        .assert_observation_count(1);

    snap.counter("ac_token_issuance_total")
        .with_labels(&[("grant_type", "internal_guest"), ("status", "success")])
        .assert_delta(1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn handle_guest_token_missing_scope_emits_grant_type_internal_guest_status_error(
    pool: PgPool,
) {
    let master_key = test_master_key();
    key_management_service::initialize_signing_key(&pool, &master_key, "test-cluster")
        .await
        .unwrap();
    let state = make_app_state(pool);

    let snap = MetricAssertion::snapshot();
    let _ = handle_guest_token(
        State(state),
        Extension(make_claims("wrong:scope")),
        Json(GuestTokenRequest {
            guest_id: Uuid::new_v4(),
            display_name: "Alice".to_string(),
            meeting_id: Uuid::new_v4(),
            meeting_org_id: Uuid::new_v4(),
            waiting_room: false,
            ttl_seconds: 300,
        }),
    )
    .await
    .unwrap_err();

    snap.histogram("ac_token_issuance_duration_seconds")
        .with_labels(&[("grant_type", "internal_guest"), ("status", "error")])
        .assert_observation_count(1);

    snap.counter("ac_token_issuance_total")
        .with_labels(&[("grant_type", "internal_guest"), ("status", "error")])
        .assert_delta(1);
}
