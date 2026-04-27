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

#[sqlx::test(migrations = "../../migrations")]
async fn handle_meeting_token_success_emits_grant_type_internal_meeting_status_success(
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
    .unwrap();

    snap.histogram("ac_token_issuance_duration_seconds")
        .with_labels(&[("grant_type", "internal_meeting"), ("status", "success")])
        .assert_observation_count(1);

    snap.counter("ac_token_issuance_total")
        .with_labels(&[("grant_type", "internal_meeting"), ("status", "success")])
        .assert_delta(1);
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
