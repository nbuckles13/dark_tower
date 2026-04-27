// Every `#[sqlx::test]` is implicitly `flavor = "current_thread"` per the
// sqlx-test macro contract, and the pinning is LOAD-BEARING — `MetricAssertion`
// binds a per-thread recorder. See
// `crates/common/src/observability/testing.rs:60-72` for the isolation model.
//
//! Component tests for `gc_db_queries_total{operation,status}` and
//! `gc_db_query_duration_seconds{operation}` per ADR-0032 Step 5 §Cluster 3.
//!
//! # Per-op drivability classification (per @test F1 finding 2026-04-27)
//!
//! 6 success drives + 2 driven errors + 9 wrapper-Cat-C error stubs (3 orphan
//! recording sites cover both success + error wiring proof; the 3 Participants
//! ops below collapse).
//!
//! | Op                          | Disposition  | Driven via |
//! |-----------------------------|--------------|------------|
//! | `count_active_participants` | wrapper-Cat-C (orphan recording site) | `record_db_query` direct call |
//! | `add_participant`           | wrapper-Cat-C (orphan recording site) | `record_db_query` direct call |
//! | `remove_participant`        | wrapper-Cat-C (orphan recording site) | `record_db_query` direct call |
//! | `create_meeting`            | driven error | unique-collision: insert duplicate meeting_code |
//! | `log_audit_event`           | driven error | FK violation: insert against non-existent org_id |
//! | `activate_meeting`          | wrapper-Cat-C error (no business-error branch) | UPDATE no-match returns Ok(None) |
//! | `register_mh`               | wrapper-Cat-C error (no business-error branch) | Idempotent UPSERT |
//! | `update_load_report`        | wrapper-Cat-C error (no business-error branch) | UPDATE no-match returns Ok(rows_affected=0) |
//! | `mark_stale_mh_unhealthy`   | wrapper-Cat-C error (no business-error branch) | UPDATE no-match returns Ok(rows_affected=0) |
//!
//! Two wrapper-Cat-C variants in this file:
//!   1. **No-business-error-branch** (4 ops: `activate_meeting`, `register_mh`,
//!      `update_load_report`, `mark_stale_mh_unhealthy`): error path only
//!      reachable via `?`-propagated sqlx errors against in-process Postgres;
//!      not drivable without a fault-injection harness. Standard canonical
//!      comment block.
//!   2. **Orphan recording site** (3 ops: `count_active_participants`,
//!      `add_participant`, `remove_participant`): the entire
//!      `ParticipantsRepository` has no caller in `crates/gc-service/src/` as
//!      of `feature/mh-quic-mh-tests` (`crates/gc-service/src/repositories/mod.rs:21`
//!      "will be used in meeting join handler"). Both success and error
//!      wrapper drives prove wiring rather than behavior, since production
//!      never invokes any of these recording sites. Distinct canonical
//!      comment block calling out the orphan classification + handler-
//!      integration gap.
//!
//! Real-recording-site error coverage of the 7 wrapper-Cat-C ops is deferred
//! per ADR-0032 Step 5; the orphan recording site additionally requires
//! production handler-integration work before fault-injection becomes
//! meaningful. Both deferrals tracked under `docs/TODO.md` §Observability Debt.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use ::common::observability::testing::MetricAssertion;
use gc_service::observability::metrics::record_db_query;
use gc_service::repositories::{
    HealthStatus, MediaHandlersRepository, MeetingsRepository, ParticipantsRepository,
};
use sqlx::PgPool;
use uuid::Uuid;

// ============================================================================
// Test fixtures (DB seeding helpers)
// ============================================================================

async fn insert_test_org(pool: &PgPool) -> Uuid {
    let org_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO organizations (org_id, subdomain, display_name, plan_tier, is_active)
        VALUES ($1, $2, 'DB Metrics Test Org', 'free', true)
        "#,
    )
    .bind(org_id)
    .bind(format!("db-test-{}", &org_id.to_string()[..8]))
    .execute(pool)
    .await
    .expect("Failed to insert test org");
    org_id
}

async fn insert_test_user(pool: &PgPool, org_id: Uuid) -> Uuid {
    let user_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO users (user_id, org_id, email, password_hash, display_name, is_active)
        VALUES ($1, $2, $3, 'hashed', 'DB Test User', true)
        "#,
    )
    .bind(user_id)
    .bind(org_id)
    .bind(format!("db-{}@example.com", &user_id.to_string()[..8]))
    .execute(pool)
    .await
    .expect("Failed to insert test user");
    user_id
}

async fn insert_test_meeting(pool: &PgPool, org_id: Uuid, user_id: Uuid, code: &str) -> Uuid {
    let meeting_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO meetings (meeting_id, org_id, created_by_user_id, display_name,
                              meeting_code, join_token_secret, max_participants, status)
        VALUES ($1, $2, $3, 'DB Metrics Test Meeting', $4, 'secret-db-test', 10, 'active')
        "#,
    )
    .bind(meeting_id)
    .bind(org_id)
    .bind(user_id)
    .bind(code)
    .execute(pool)
    .await
    .expect("Failed to insert test meeting");
    meeting_id
}

// ============================================================================
// Driven success path — every operation
// ============================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn db_query_create_meeting_success_emits_operation_create_meeting(pool: PgPool) {
    let org_id = insert_test_org(&pool).await;
    let user_id = insert_test_user(&pool, org_id).await;

    let snap = MetricAssertion::snapshot();
    let _ = MeetingsRepository::create_meeting_with_limit_check(
        &pool,
        org_id,
        user_id,
        "DB Metrics",
        "DBMETRICSXX1",
        "secret",
        10,
        true,
        true,
        false,
        false,
        false,
        true,
        None,
    )
    .await
    .unwrap();

    snap.histogram("gc_db_query_duration_seconds")
        .with_labels(&[("operation", "create_meeting")])
        .assert_observation_count_at_least(1);
    snap.counter("gc_db_queries_total")
        .with_labels(&[("operation", "create_meeting"), ("status", "success")])
        .assert_delta(1);
    snap.counter("gc_db_queries_total")
        .with_labels(&[("operation", "create_meeting"), ("status", "error")])
        .assert_delta(0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn db_query_log_audit_event_success_emits_operation_log_audit_event(pool: PgPool) {
    let org_id = insert_test_org(&pool).await;
    let user_id = insert_test_user(&pool, org_id).await;
    let meeting_id = insert_test_meeting(&pool, org_id, user_id, "AUDITLOGOK1").await;

    let snap = MetricAssertion::snapshot();
    MeetingsRepository::log_audit_event(&pool, org_id, Some(user_id), meeting_id, "test_action")
        .await
        .unwrap();

    snap.histogram("gc_db_query_duration_seconds")
        .with_labels(&[("operation", "log_audit_event")])
        .assert_observation_count_at_least(1);
    snap.counter("gc_db_queries_total")
        .with_labels(&[("operation", "log_audit_event"), ("status", "success")])
        .assert_delta(1);
    // Adjacency: error sibling silent on this drive.
    snap.counter("gc_db_queries_total")
        .with_labels(&[("operation", "log_audit_event"), ("status", "error")])
        .assert_delta(0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn db_query_activate_meeting_success_emits_operation_activate_meeting(pool: PgPool) {
    let org_id = insert_test_org(&pool).await;
    let user_id = insert_test_user(&pool, org_id).await;
    // Insert a `scheduled` meeting so activate_meeting performs the transition.
    let meeting_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO meetings (meeting_id, org_id, created_by_user_id, display_name,
                              meeting_code, join_token_secret, max_participants, status)
        VALUES ($1, $2, $3, 'Activate Test', 'ACTIVATE001', 'secret', 10, 'scheduled')
        "#,
    )
    .bind(meeting_id)
    .bind(org_id)
    .bind(user_id)
    .execute(&pool)
    .await
    .unwrap();

    let snap = MetricAssertion::snapshot();
    let _ = MeetingsRepository::activate_meeting(&pool, meeting_id)
        .await
        .unwrap();

    snap.histogram("gc_db_query_duration_seconds")
        .with_labels(&[("operation", "activate_meeting")])
        .assert_observation_count_at_least(1);
    snap.counter("gc_db_queries_total")
        .with_labels(&[("operation", "activate_meeting"), ("status", "success")])
        .assert_delta(1);
    // Adjacency: error sibling silent on this drive.
    snap.counter("gc_db_queries_total")
        .with_labels(&[("operation", "activate_meeting"), ("status", "error")])
        .assert_delta(0);
}

// WRAPPER-CAT-C (orphan recording site): `count_active_participants` has no
// production caller as of feature/mh-quic-mh-tests; metric emission site is
// dormant. See `crates/gc-service/src/repositories/mod.rs:21` ("will be used
// in meeting join handler"). This wrapper invocation proves metric wiring
// (name + label set + recorder registration). It does NOT prove that
// production code emits it under any real fault — that requires the fault-
// injection harness in TODO §Observability Debt, AND the production caller
// must exist first.
#[sqlx::test(migrations = "../../migrations")]
async fn db_query_count_active_participants_success_wrapper_cat_c_orphan(pool: PgPool) {
    let org_id = insert_test_org(&pool).await;
    let user_id = insert_test_user(&pool, org_id).await;
    let meeting_id = insert_test_meeting(&pool, org_id, user_id, "COUNTPARTAA").await;

    let snap = MetricAssertion::snapshot();
    let _ = ParticipantsRepository::count_active_participants(&pool, meeting_id)
        .await
        .unwrap();

    snap.counter("gc_db_queries_total")
        .with_labels(&[
            ("operation", "count_active_participants"),
            ("status", "success"),
        ])
        .assert_delta(1);
    // Adjacency: error sibling silent on this drive.
    snap.counter("gc_db_queries_total")
        .with_labels(&[
            ("operation", "count_active_participants"),
            ("status", "error"),
        ])
        .assert_delta(0);
}

// WRAPPER-CAT-C (orphan recording site): `add_participant` has no production
// caller as of feature/mh-quic-mh-tests; metric emission site is dormant.
// See `crates/gc-service/src/repositories/mod.rs:21` ("will be used in meeting
// join handler"). This wrapper invocation proves metric wiring (name + label
// set + recorder registration). It does NOT prove that production code emits
// it under any real fault — that requires the fault-injection harness in
// TODO §Observability Debt, AND the production caller must exist first.
#[sqlx::test(migrations = "../../migrations")]
async fn db_query_add_participant_success_wrapper_cat_c_orphan(pool: PgPool) {
    let org_id = insert_test_org(&pool).await;
    let user_id = insert_test_user(&pool, org_id).await;
    let meeting_id = insert_test_meeting(&pool, org_id, user_id, "ADDPARTOK01").await;

    let snap = MetricAssertion::snapshot();
    let _ = ParticipantsRepository::add_participant(
        &pool,
        meeting_id,
        Some(user_id),
        "Test Participant",
        "member",
        "host",
    )
    .await
    .unwrap();

    snap.counter("gc_db_queries_total")
        .with_labels(&[("operation", "add_participant"), ("status", "success")])
        .assert_delta(1);
    // Adjacency: error sibling silent on this drive.
    snap.counter("gc_db_queries_total")
        .with_labels(&[("operation", "add_participant"), ("status", "error")])
        .assert_delta(0);
}

// WRAPPER-CAT-C (orphan recording site): `remove_participant` has no production
// caller as of feature/mh-quic-mh-tests; metric emission site is dormant.
// See `crates/gc-service/src/repositories/mod.rs:21` ("will be used in meeting
// join handler"). This wrapper invocation proves metric wiring (name + label
// set + recorder registration). It does NOT prove that production code emits
// it under any real fault — that requires the fault-injection harness in
// TODO §Observability Debt, AND the production caller must exist first.
#[sqlx::test(migrations = "../../migrations")]
async fn db_query_remove_participant_success_wrapper_cat_c_orphan(pool: PgPool) {
    let org_id = insert_test_org(&pool).await;
    let user_id = insert_test_user(&pool, org_id).await;
    let meeting_id = insert_test_meeting(&pool, org_id, user_id, "REMPARTOK01").await;
    ParticipantsRepository::add_participant(
        &pool,
        meeting_id,
        Some(user_id),
        "Test",
        "member",
        "host",
    )
    .await
    .unwrap();

    let snap = MetricAssertion::snapshot();
    let _ = ParticipantsRepository::remove_participant(&pool, meeting_id, user_id)
        .await
        .unwrap();

    snap.counter("gc_db_queries_total")
        .with_labels(&[("operation", "remove_participant"), ("status", "success")])
        .assert_delta(1);
    // Adjacency: error sibling silent on this drive.
    snap.counter("gc_db_queries_total")
        .with_labels(&[("operation", "remove_participant"), ("status", "error")])
        .assert_delta(0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn db_query_register_mh_success_emits_operation_register_mh(pool: PgPool) {
    let snap = MetricAssertion::snapshot();
    MediaHandlersRepository::register_mh(
        &pool,
        "test-mh-001",
        "us-east-1",
        "https://wt.example.com",
        "https://grpc.example.com",
        100,
    )
    .await
    .unwrap();

    snap.counter("gc_db_queries_total")
        .with_labels(&[("operation", "register_mh"), ("status", "success")])
        .assert_delta(1);
    // Adjacency: error sibling silent on this drive.
    snap.counter("gc_db_queries_total")
        .with_labels(&[("operation", "register_mh"), ("status", "error")])
        .assert_delta(0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn db_query_update_load_report_success_emits_op_update_load_report(pool: PgPool) {
    MediaHandlersRepository::register_mh(
        &pool,
        "test-mh-load",
        "us-east-1",
        "https://wt.example.com",
        "https://grpc.example.com",
        100,
    )
    .await
    .unwrap();

    let snap = MetricAssertion::snapshot();
    let _ = MediaHandlersRepository::update_load_report(
        &pool,
        "test-mh-load",
        25,
        HealthStatus::Healthy,
        Some(0.5),
        Some(0.3),
        Some(0.2),
    )
    .await
    .unwrap();

    snap.counter("gc_db_queries_total")
        .with_labels(&[("operation", "update_load_report"), ("status", "success")])
        .assert_delta(1);
    // Adjacency: error sibling silent on this drive.
    snap.counter("gc_db_queries_total")
        .with_labels(&[("operation", "update_load_report"), ("status", "error")])
        .assert_delta(0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn db_query_mark_stale_mh_unhealthy_success_emits_op_mark_stale_mh_unhealthy(pool: PgPool) {
    let snap = MetricAssertion::snapshot();
    let _ = MediaHandlersRepository::mark_stale_handlers_unhealthy(&pool, 60)
        .await
        .unwrap();

    snap.counter("gc_db_queries_total")
        .with_labels(&[
            ("operation", "mark_stale_mh_unhealthy"),
            ("status", "success"),
        ])
        .assert_delta(1);
    // Adjacency: error sibling silent on this drive.
    snap.counter("gc_db_queries_total")
        .with_labels(&[
            ("operation", "mark_stale_mh_unhealthy"),
            ("status", "error"),
        ])
        .assert_delta(0);
}

// ============================================================================
// Driven error paths (2 ops with reachable error branches in production)
// ============================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn db_query_create_meeting_unique_collision_emits_status_error(pool: PgPool) {
    let org_id = insert_test_org(&pool).await;
    let user_id = insert_test_user(&pool, org_id).await;

    // First insert: succeeds and creates a meeting with code "DUPECODE001A".
    MeetingsRepository::create_meeting_with_limit_check(
        &pool,
        org_id,
        user_id,
        "First",
        "DUPECODE001A",
        "secret",
        10,
        true,
        true,
        false,
        false,
        false,
        true,
        None,
    )
    .await
    .unwrap();

    // Second insert with same code: triggers unique-constraint violation.
    let snap = MetricAssertion::snapshot();
    let result = MeetingsRepository::create_meeting_with_limit_check(
        &pool,
        org_id,
        user_id,
        "Second",
        "DUPECODE001A",
        "secret",
        10,
        true,
        true,
        false,
        false,
        false,
        true,
        None,
    )
    .await;
    assert!(result.is_err(), "Expected unique-constraint collision");

    snap.counter("gc_db_queries_total")
        .with_labels(&[("operation", "create_meeting"), ("status", "error")])
        .assert_delta(1);
    // Adjacency: log_audit_event sibling silent (different op label).
    snap.counter("gc_db_queries_total")
        .with_labels(&[("operation", "log_audit_event"), ("status", "error")])
        .assert_delta(0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn db_query_log_audit_event_fk_violation_emits_status_error(pool: PgPool) {
    // FK violation source: `audit_logs.org_id REFERENCES organizations(org_id)`
    // (per `migrations/20250118000001_initial_schema.sql:CREATE TABLE audit_logs`).
    // `resource_id` (the meeting_id binding) has NO FK constraint, so a
    // bogus meeting_id alone won't trip the error branch — bogus org_id will.
    let nonexistent_org = Uuid::new_v4();
    let nonexistent_meeting = Uuid::new_v4();

    let snap = MetricAssertion::snapshot();
    let result = MeetingsRepository::log_audit_event(
        &pool,
        nonexistent_org,
        None,
        nonexistent_meeting,
        "test_action",
    )
    .await;
    assert!(result.is_err(), "Expected FK violation on org_id");

    snap.counter("gc_db_queries_total")
        .with_labels(&[("operation", "log_audit_event"), ("status", "error")])
        .assert_delta(1);
    snap.counter("gc_db_queries_total")
        .with_labels(&[("operation", "create_meeting"), ("status", "error")])
        .assert_delta(0);
}

// ============================================================================
// WRAPPER-CAT-C error stubs (7 ops; 6 no-business-error-branch + 1 orphan)
// ============================================================================

// This wrapper invocation proves metric wiring (name + label set + recorder
// registration). It does NOT prove that production code emits it under any
// real fault — that requires the fault-injection harness in TODO §Observability
// Debt.
#[test]
fn db_query_count_active_participants_error_wrapper_cat_c() {
    let snap = MetricAssertion::snapshot();
    record_db_query(
        "count_active_participants",
        "error",
        Duration::from_millis(5),
    );
    snap.counter("gc_db_queries_total")
        .with_labels(&[
            ("operation", "count_active_participants"),
            ("status", "error"),
        ])
        .assert_delta(1);
}

// This wrapper invocation proves metric wiring (name + label set + recorder
// registration). It does NOT prove that production code emits it under any
// real fault — that requires the fault-injection harness in TODO §Observability
// Debt.
#[test]
fn db_query_remove_participant_error_wrapper_cat_c() {
    let snap = MetricAssertion::snapshot();
    record_db_query("remove_participant", "error", Duration::from_millis(3));
    snap.counter("gc_db_queries_total")
        .with_labels(&[("operation", "remove_participant"), ("status", "error")])
        .assert_delta(1);
}

// This wrapper invocation proves metric wiring (name + label set + recorder
// registration). It does NOT prove that production code emits it under any
// real fault — that requires the fault-injection harness in TODO §Observability
// Debt.
#[test]
fn db_query_activate_meeting_error_wrapper_cat_c() {
    let snap = MetricAssertion::snapshot();
    record_db_query("activate_meeting", "error", Duration::from_millis(7));
    snap.counter("gc_db_queries_total")
        .with_labels(&[("operation", "activate_meeting"), ("status", "error")])
        .assert_delta(1);
}

// This wrapper invocation proves metric wiring (name + label set + recorder
// registration). It does NOT prove that production code emits it under any
// real fault — that requires the fault-injection harness in TODO §Observability
// Debt.
#[test]
fn db_query_register_mh_error_wrapper_cat_c() {
    let snap = MetricAssertion::snapshot();
    record_db_query("register_mh", "error", Duration::from_millis(8));
    snap.counter("gc_db_queries_total")
        .with_labels(&[("operation", "register_mh"), ("status", "error")])
        .assert_delta(1);
}

// This wrapper invocation proves metric wiring (name + label set + recorder
// registration). It does NOT prove that production code emits it under any
// real fault — that requires the fault-injection harness in TODO §Observability
// Debt.
#[test]
fn db_query_update_load_report_error_wrapper_cat_c() {
    let snap = MetricAssertion::snapshot();
    record_db_query("update_load_report", "error", Duration::from_millis(4));
    snap.counter("gc_db_queries_total")
        .with_labels(&[("operation", "update_load_report"), ("status", "error")])
        .assert_delta(1);
}

// This wrapper invocation proves metric wiring (name + label set + recorder
// registration). It does NOT prove that production code emits it under any
// real fault — that requires the fault-injection harness in TODO §Observability
// Debt.
#[test]
fn db_query_mark_stale_mh_unhealthy_error_wrapper_cat_c() {
    let snap = MetricAssertion::snapshot();
    record_db_query("mark_stale_mh_unhealthy", "error", Duration::from_millis(6));
    snap.counter("gc_db_queries_total")
        .with_labels(&[
            ("operation", "mark_stale_mh_unhealthy"),
            ("status", "error"),
        ])
        .assert_delta(1);
}

// WRAPPER-CAT-C (orphan recording site): `add_participant` has no production
// caller as of feature/mh-quic-mh-tests; metric emission site is dormant.
// See `crates/gc-service/src/repositories/mod.rs:21` ("will be used in meeting
// join handler"). This wrapper invocation proves metric wiring (name + label
// set + recorder registration). It does NOT prove that production code emits
// it under any real fault — that requires the fault-injection harness in
// TODO §Observability Debt, AND the production caller must exist first.
#[test]
fn db_query_add_participant_error_wrapper_cat_c_orphan() {
    let snap = MetricAssertion::snapshot();
    record_db_query("add_participant", "error", Duration::from_millis(7));
    snap.counter("gc_db_queries_total")
        .with_labels(&[("operation", "add_participant"), ("status", "error")])
        .assert_delta(1);
}
