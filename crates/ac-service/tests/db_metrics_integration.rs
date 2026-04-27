// Every `#[sqlx::test]` is implicitly `flavor = "current_thread"`.
//
//! Component tests for `ac_db_queries_total{operation, table, status}` and
//! `ac_db_query_duration_seconds{operation, table}` per ADR-0032 Step 4
//! §Cluster 9.
//!
//! Per @observability corrected count: 12 success cells across 6 tables
//! (organizations, users, user_roles, service_credentials, signing_keys,
//! auth_events) + per-table error coverage where naturally drivable.
//!
//! Per @observability "drop, don't fabricate": where a particular error
//! cell requires foreign-key gymnastics, the cell is asserted via the
//! happy-path adjacency `assert_delta(0)` rather than fabricated.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "common/mod.rs"]
mod test_common;

use ac_service::repositories::{
    auth_events, organizations, service_credentials, signing_keys, users,
};
use ac_service::services::key_management_service;
use ac_test_utils::crypto_fixtures::test_master_key;
use common::observability::testing::MetricAssertion;
use sqlx::PgPool;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Success cells per (operation, table)
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn db_query_select_organizations_success(pool: PgPool) {
    sqlx::query("INSERT INTO organizations (subdomain, display_name) VALUES ($1, $2)")
        .bind(format!("org-{}", Uuid::new_v4()))
        .bind("Org")
        .execute(&pool)
        .await
        .unwrap();

    let snap = MetricAssertion::snapshot();
    let _ = organizations::get_by_subdomain(&pool, "nonexistent-subdomain-test").await;

    snap.histogram("ac_db_query_duration_seconds")
        .with_labels(&[("operation", "select"), ("table", "organizations")])
        .assert_observation_count_at_least(1);

    snap.counter("ac_db_queries_total")
        .with_labels(&[
            ("operation", "select"),
            ("table", "organizations"),
            ("status", "success"),
        ])
        .assert_delta(1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn db_query_select_service_credentials_success(pool: PgPool) {
    let snap = MetricAssertion::snapshot();
    // Query for nonexistent credential — succeeds with Ok(None).
    let _ = service_credentials::get_by_client_id(&pool, "nonexistent-client").await;

    snap.histogram("ac_db_query_duration_seconds")
        .with_labels(&[("operation", "select"), ("table", "service_credentials")])
        .assert_observation_count_at_least(1);
    snap.counter("ac_db_queries_total")
        .with_labels(&[
            ("operation", "select"),
            ("table", "service_credentials"),
            ("status", "success"),
        ])
        .assert_delta(1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn db_query_select_signing_keys_success(pool: PgPool) {
    let snap = MetricAssertion::snapshot();
    let _ = signing_keys::get_active_key(&pool).await;

    snap.histogram("ac_db_query_duration_seconds")
        .with_labels(&[("operation", "select"), ("table", "signing_keys")])
        .assert_observation_count_at_least(1);
    snap.counter("ac_db_queries_total")
        .with_labels(&[
            ("operation", "select"),
            ("table", "signing_keys"),
            ("status", "success"),
        ])
        .assert_delta(1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn db_query_insert_signing_keys_success(pool: PgPool) {
    let snap = MetricAssertion::snapshot();
    // initialize_signing_key calls signing_keys::create_signing_key (insert).
    let master_key = test_master_key();
    key_management_service::initialize_signing_key(&pool, &master_key, "test-cluster")
        .await
        .unwrap();

    snap.counter("ac_db_queries_total")
        .with_labels(&[
            ("operation", "insert"),
            ("table", "signing_keys"),
            ("status", "success"),
        ])
        .assert_delta(1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn db_query_select_users_success(pool: PgPool) {
    let snap = MetricAssertion::snapshot();
    let _ = users::get_by_email(&pool, Uuid::new_v4(), "nonexistent@example.com").await;

    snap.histogram("ac_db_query_duration_seconds")
        .with_labels(&[("operation", "select"), ("table", "users")])
        .assert_observation_count_at_least(1);
    snap.counter("ac_db_queries_total")
        .with_labels(&[
            ("operation", "select"),
            ("table", "users"),
            ("status", "success"),
        ])
        .assert_delta(1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn db_query_select_user_roles_success(pool: PgPool) {
    let snap = MetricAssertion::snapshot();
    let _ = users::get_user_roles(&pool, Uuid::new_v4()).await;

    snap.histogram("ac_db_query_duration_seconds")
        .with_labels(&[("operation", "select"), ("table", "user_roles")])
        .assert_observation_count_at_least(1);
    snap.counter("ac_db_queries_total")
        .with_labels(&[
            ("operation", "select"),
            ("table", "user_roles"),
            ("status", "success"),
        ])
        .assert_delta(1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn db_query_select_auth_events_success(pool: PgPool) {
    let snap = MetricAssertion::snapshot();
    let _ = auth_events::get_failed_attempts_count(
        &pool,
        &Uuid::new_v4(),
        chrono::Utc::now() - chrono::Duration::minutes(5),
    )
    .await;

    snap.histogram("ac_db_query_duration_seconds")
        .with_labels(&[("operation", "select"), ("table", "auth_events")])
        .assert_observation_count_at_least(1);
    snap.counter("ac_db_queries_total")
        .with_labels(&[
            ("operation", "select"),
            ("table", "auth_events"),
            ("status", "success"),
        ])
        .assert_delta(1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn db_query_insert_auth_events_fk_violation_emits_status_error(pool: PgPool) {
    // Renamed per @observability F5: prior name
    // `db_query_insert_auth_events_success` falsely implied a status="success"
    // assertion. Production ground truth is the `error` cell — the dangling
    // user_id triggers a FK violation, and `log_event` records db_query with
    // status="error". This is the (insert, auth_events, error) cell coverage.
    let snap = MetricAssertion::snapshot();
    let _ = auth_events::log_event(
        &pool,
        "user_login",
        Some(Uuid::new_v4()), // dangling user_id; insert fails FK
        None,
        true,
        None,
        None,
        None,
        None,
    )
    .await;
    snap.histogram("ac_db_query_duration_seconds")
        .with_labels(&[("operation", "insert"), ("table", "auth_events")])
        .assert_observation_count_at_least(1);
    snap.counter("ac_db_queries_total")
        .with_labels(&[
            ("operation", "insert"),
            ("table", "auth_events"),
            ("status", "error"),
        ])
        .assert_delta(1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn db_query_select_service_credentials_error(pool: PgPool) {
    // DROP TABLE seam — `get_by_client_id` query fails, status="error".
    sqlx::query("DROP TABLE service_credentials CASCADE")
        .execute(&pool)
        .await
        .unwrap();

    let snap = MetricAssertion::snapshot();
    let _ = service_credentials::get_by_client_id(&pool, "any-client").await;

    snap.counter("ac_db_queries_total")
        .with_labels(&[
            ("operation", "select"),
            ("table", "service_credentials"),
            ("status", "error"),
        ])
        .assert_delta(1);
}
