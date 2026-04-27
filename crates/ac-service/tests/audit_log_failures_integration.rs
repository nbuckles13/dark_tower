// Every `#[sqlx::test]` is implicitly `flavor = "current_thread"`.
//
//! Component tests for `ac_audit_log_failures_total{event_type, reason}`
//! per ADR-0032 Step 4 §Cluster 11.
//!
//! # Real-recording-site drives via DROP TABLE seam
//!
//! Per @test reviewer T2 + @observability concur: each of the production
//! audit-log emission sites in `ac-service/src/services/` is driven by a
//! `#[sqlx::test]` that DROPs the `auth_events` table BEFORE invoking the
//! production fn. The audit-log INSERT then fails, the `if let Err(_)`
//! branch fires, the wrapper records, and the snapshot observes.
//!
//! Production fn returns success (audit-log fail is a non-fatal side-effect
//! per the existing `if let Err(e) = ...` pattern in every site). The test
//! asserts the production reachable (event_type, "db_write_failed") combo.
//!
//! # Cross-cluster emission note (per @test R1)
//!
//! Each test path also exercises `ac_db_queries_total{table=auth_events,
//! status=error}` and `ac_db_query_duration_seconds{table=auth_events}`
//! because `auth_events::log_event` calls `record_db_query` regardless of
//! status. Primary assertion in this file is the audit failure counter;
//! db_query adjacency is load-bearing in `tests/db_metrics_integration.rs`
//! (Cluster 9), not here. Histogram-first ordering mandatory in any
//! mixed-kind snapshot to avoid drain-on-read interaction.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "common/mod.rs"]
mod test_common;

use ac_service::services::{key_management_service, registration_service, user_service};
use ac_test_utils::crypto_fixtures::test_master_key;
use common::observability::testing::{MetricAssertion, MetricSnapshot};
use sqlx::PgPool;

/// Every event_type production sites pass to `record_audit_log_failure`.
/// Authoritative production-site mapping (one per callsite); per @test T-F4,
/// the constant must match production exactly so a future maintainer adding
/// a wrapper call also updates this constant (CR catches the omission):
///
///   service_registered      → registration_service.rs:78
///   scopes_updated          → registration_service.rs:124
///   service_deactivated     → registration_service.rs:158
///   key_generated           → key_management_service.rs:96
///   key_rotated             → key_management_service.rs:166
///   key_expired             → key_management_service.rs:384
///   user_registered         → user_service.rs:144
///   service_token_failed    → token_service.rs:105
///   service_token_issued    → token_service.rs:172
///   user_login              → token_service.rs:362 (success=true branch)
///   user_login_failed       → token_service.rs:362 (success=false branch)
///
/// Used for `assert_delta(0)` adjacency on every sibling under the same
/// `reason="db_write_failed"` filter (label-swap-bug catcher per ADR-0032
/// §Pattern #3).
const ALL_EVENT_TYPES: &[&str] = &[
    "service_registered",
    "scopes_updated",
    "service_deactivated",
    "key_generated",
    "key_rotated",
    "key_expired",
    "user_registered",
    "service_token_issued",
    "service_token_failed",
    "user_login",
    "user_login_failed",
];

/// Force `auth_events::log_event` INSERT failures while still allowing
/// SELECT queries (used by `issue_service_token`'s rate-check at
/// `token_service.rs:54-59` before the audit-log emission site fires).
///
/// Adds a CHECK constraint that always rejects new rows. Existing rows
/// remain queryable; new INSERTs fail with constraint violation. This is
/// the surgical fault-injection seam @test reviewer T2 requested for
/// production-path drives, and it's surgical-enough that even tests
/// driving fns that pre-query auth_events (token_service.rs:54) still
/// hit the audit-log emission site.
async fn break_auth_events_inserts(pool: &PgPool) {
    // NOT VALID — Postgres skips re-validation of existing rows, so the
    // constraint applies only to new INSERTs (and UPDATEs of constrained
    // columns). The seeded credential's prior `service_registered` event,
    // and the seed signing key's `key_generated` event, both stay in the
    // table without re-validation failure.
    sqlx::query(
        "ALTER TABLE auth_events ADD CONSTRAINT block_inserts \
         CHECK (event_type = 'IMPOSSIBLE_NEVER_MATCH_ANY_REAL_EVENT') NOT VALID",
    )
    .execute(pool)
    .await
    .unwrap();
}

/// Heavier hammer: full DROP for fns that do not pre-query auth_events.
/// (initialize_signing_key, register_service do not pre-query.)
async fn break_auth_events_table(pool: &PgPool) {
    sqlx::query("DROP TABLE auth_events CASCADE")
        .execute(pool)
        .await
        .unwrap();
}

fn assert_only_event_type(snap: &MetricSnapshot, expected_event_type: &str) {
    snap.counter("ac_audit_log_failures_total")
        .with_labels(&[
            ("event_type", expected_event_type),
            ("reason", "db_write_failed"),
        ])
        .assert_delta(1);

    for sibling in ALL_EVENT_TYPES
        .iter()
        .filter(|e| **e != expected_event_type)
    {
        snap.counter("ac_audit_log_failures_total")
            .with_labels(&[("event_type", *sibling), ("reason", "db_write_failed")])
            .assert_delta(0);
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn audit_log_failure_emits_event_type_key_generated(pool: PgPool) {
    // Drive `key_management_service::initialize_signing_key` (key_management_service.rs:96).
    break_auth_events_table(&pool).await;
    let snap = MetricAssertion::snapshot();
    let master_key = test_master_key();
    // Production fn returns Ok even though audit log write fails (non-fatal).
    let _ =
        key_management_service::initialize_signing_key(&pool, &master_key, "test-cluster").await;
    assert_only_event_type(&snap, "key_generated");
}

#[sqlx::test(migrations = "../../migrations")]
async fn audit_log_failure_emits_event_type_service_registered(pool: PgPool) {
    // Drive `registration_service::register_service` (registration_service.rs:78).
    break_auth_events_table(&pool).await;
    let snap = MetricAssertion::snapshot();
    let _ = registration_service::register_service(
        &pool,
        "global-controller",
        None,
        ac_service::config::MIN_BCRYPT_COST,
    )
    .await;
    assert_only_event_type(&snap, "service_registered");
}

#[sqlx::test(migrations = "../../migrations")]
async fn audit_log_failure_emits_event_type_user_registered(pool: PgPool) {
    // Drive `user_service::register_user` (user_service.rs:144).
    let master_key = test_master_key();
    key_management_service::initialize_signing_key(&pool, &master_key, "test-cluster")
        .await
        .unwrap();
    let org_id = sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO organizations (subdomain, display_name) VALUES ($1, $2) RETURNING org_id",
    )
    .bind(format!("audit-{}", uuid::Uuid::new_v4()))
    .bind("Audit test")
    .fetch_one(&pool)
    .await
    .unwrap();

    break_auth_events_table(&pool).await;
    let snap = MetricAssertion::snapshot();
    let _ = user_service::register_user(
        &pool,
        &master_key,
        &master_key,
        org_id,
        user_service::RegistrationRequest {
            email: "audit-user@example.com".to_string(),
            password: "test-password-12345".to_string(),
            display_name: "Audit User".to_string(),
        },
        None,
        None,
        ac_service::config::MIN_BCRYPT_COST,
        ac_service::config::DEFAULT_REGISTRATION_RATE_LIMIT_WINDOW_MINUTES,
        ac_service::config::DEFAULT_REGISTRATION_RATE_LIMIT_MAX_ATTEMPTS,
        ac_service::config::DEFAULT_RATE_LIMIT_WINDOW_MINUTES,
        ac_service::config::DEFAULT_RATE_LIMIT_MAX_ATTEMPTS,
    )
    .await;

    // `register_user` emits BOTH `user_registered` (user_service.rs:144) AND
    // chains to `issue_user_token` (auto-login) which emits `user_login` via
    // the parameterized site (token_service.rs:362). Both are real production
    // behavior on this path with auth_events broken; both must fire.
    snap.counter("ac_audit_log_failures_total")
        .with_labels(&[
            ("event_type", "user_registered"),
            ("reason", "db_write_failed"),
        ])
        .assert_delta(1);
    snap.counter("ac_audit_log_failures_total")
        .with_labels(&[("event_type", "user_login"), ("reason", "db_write_failed")])
        .assert_delta(1);

    // Adjacency: every OTHER event_type stays at 0 (label-swap-bug catcher
    // for the 9 sibling cells).
    for sibling in ALL_EVENT_TYPES
        .iter()
        .filter(|e| **e != "user_registered" && **e != "user_login")
    {
        snap.counter("ac_audit_log_failures_total")
            .with_labels(&[("event_type", *sibling), ("reason", "db_write_failed")])
            .assert_delta(0);
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn audit_log_failure_emits_event_type_service_token_failed(pool: PgPool) {
    // Drive `token_service::issue_service_token` failure path
    // (token_service.rs:105 — fires when audit-log INSERT fails inside the
    // bad-credentials branch).
    use test_common::test_state::seed_service_credential;

    let master_key = test_master_key();
    key_management_service::initialize_signing_key(&pool, &master_key, "test-cluster")
        .await
        .unwrap();
    seed_service_credential(&pool, "audit-svc-fail", &["service.write"])
        .await
        .unwrap();

    // Use the surgical break (CHECK-constraint reject on INSERT, allow SELECT)
    // because `issue_service_token` queries auth_events first for the
    // rate-check at token_service.rs:54-59.
    break_auth_events_inserts(&pool).await;
    let snap = MetricAssertion::snapshot();
    let _ = ac_service::services::token_service::issue_service_token(
        &pool,
        &master_key,
        &master_key,
        "audit-svc-fail",
        "wrong-secret", // <- triggers failure branch
        "client_credentials",
        None,
        None,
        None,
        ac_service::config::DEFAULT_RATE_LIMIT_WINDOW_MINUTES,
        ac_service::config::DEFAULT_RATE_LIMIT_MAX_ATTEMPTS,
    )
    .await;

    assert_only_event_type(&snap, "service_token_failed");
}

#[sqlx::test(migrations = "../../migrations")]
async fn audit_log_failure_emits_event_type_service_token_issued(pool: PgPool) {
    // Drive `token_service::issue_service_token` success path
    // (token_service.rs:172 — fires when audit-log INSERT fails inside the
    // success branch; primary token issuance still completes).
    use test_common::test_state::{seed_service_credential, TEST_CLIENT_SECRET};

    let master_key = test_master_key();
    key_management_service::initialize_signing_key(&pool, &master_key, "test-cluster")
        .await
        .unwrap();
    seed_service_credential(&pool, "audit-svc-issue", &["service.write"])
        .await
        .unwrap();

    // Surgical break — issue_service_token success path also pre-reads
    // auth_events for the rate-check.
    break_auth_events_inserts(&pool).await;
    let snap = MetricAssertion::snapshot();
    let _ = ac_service::services::token_service::issue_service_token(
        &pool,
        &master_key,
        &master_key,
        "audit-svc-issue",
        TEST_CLIENT_SECRET,
        "client_credentials",
        None,
        None,
        None,
        ac_service::config::DEFAULT_RATE_LIMIT_WINDOW_MINUTES,
        ac_service::config::DEFAULT_RATE_LIMIT_MAX_ATTEMPTS,
    )
    .await;

    assert_only_event_type(&snap, "service_token_issued");
}

// ===========================================================================
// 6 NEW production-driven tests for the deferred event_types per @team-lead
// scope-fidelity ask. Each uses the appropriate seam:
// - `break_auth_events_table` (DROP) for fns that don't pre-query auth_events.
// - `break_auth_events_inserts` (CHECK NOT VALID) for fns that DO pre-query
//   (rate-check, etc.) — preserves the SELECT path.
// All assert per-failure-class adjacency via `assert_only_event_type`
// (label-swap-bug catcher across the 11-cell event_type axis).
// ===========================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn audit_log_failure_emits_event_type_key_rotated(pool: PgPool) {
    // Drive `key_management_service::rotate_signing_key` (key_management_service.rs:166).
    // Bootstrap a key first, then break inserts (rotate doesn't pre-query
    // auth_events but a DROP would also break the signing_keys FK chain
    // in this fn — surgical CHECK is safer).
    let master_key = test_master_key();
    key_management_service::initialize_signing_key(&pool, &master_key, "test-cluster")
        .await
        .unwrap();

    break_auth_events_inserts(&pool).await;
    let snap = MetricAssertion::snapshot();
    let _ = key_management_service::rotate_signing_key(&pool, &master_key, "test-cluster").await;
    assert_only_event_type(&snap, "key_rotated");
}

#[sqlx::test(migrations = "../../migrations")]
async fn audit_log_failure_emits_event_type_key_expired(pool: PgPool) {
    // Drive `key_management_service::expire_old_keys` (key_management_service.rs:384).
    // expire_old_keys requires a key with valid_until < NOW() AND is_active=true.
    // Seed one directly via SQL (initialize_signing_key produces a fresh key
    // with future valid_until). Then break inserts so the audit-log INSERT
    // for "key_expired" fails.
    use chrono::{Duration, Utc};

    let master_key = test_master_key();
    key_management_service::initialize_signing_key(&pool, &master_key, "test-cluster")
        .await
        .unwrap();

    // Force the seeded key into the expired window. The signing_keys table
    // has a CHECK constraint requiring valid_until > valid_from, so move
    // BOTH timestamps backwards (valid_from = -2h, valid_until = -1h).
    let now = Utc::now();
    sqlx::query("UPDATE signing_keys SET valid_from = $1, valid_until = $2 WHERE is_active = true")
        .bind(now - Duration::hours(2))
        .bind(now - Duration::hours(1))
        .execute(&pool)
        .await
        .unwrap();

    break_auth_events_inserts(&pool).await;
    let snap = MetricAssertion::snapshot();
    let _ = key_management_service::expire_old_keys(&pool).await;
    assert_only_event_type(&snap, "key_expired");
}

#[sqlx::test(migrations = "../../migrations")]
async fn audit_log_failure_emits_event_type_scopes_updated(pool: PgPool) {
    // Drive `registration_service::update_service_scopes` (registration_service.rs:124).
    // Requires an existing service credential; seed via test fixture.
    use ac_service::services::registration_service;
    use test_common::test_state::seed_service_credential;

    seed_service_credential(&pool, "audit-svc-scopes", &["service.read"])
        .await
        .unwrap();

    break_auth_events_inserts(&pool).await;
    let snap = MetricAssertion::snapshot();
    let _ = registration_service::update_service_scopes(
        &pool,
        "audit-svc-scopes",
        vec!["service.write".to_string()],
    )
    .await;
    assert_only_event_type(&snap, "scopes_updated");
}

#[sqlx::test(migrations = "../../migrations")]
async fn audit_log_failure_emits_event_type_service_deactivated(pool: PgPool) {
    // Drive `registration_service::deactivate_service` (registration_service.rs:158).
    use ac_service::services::registration_service;
    use test_common::test_state::seed_service_credential;

    seed_service_credential(&pool, "audit-svc-deactivate", &["service.read"])
        .await
        .unwrap();

    break_auth_events_inserts(&pool).await;
    let snap = MetricAssertion::snapshot();
    let _ = registration_service::deactivate_service(&pool, "audit-svc-deactivate").await;
    assert_only_event_type(&snap, "service_deactivated");
}

#[sqlx::test(migrations = "../../migrations")]
async fn audit_log_failure_emits_event_type_user_login(pool: PgPool) {
    // Drive `token_service::issue_user_token` success path
    // (parameterized site at token_service.rs:362, where `event_type =
    // AuthEventType::UserLogin.as_str()` because `success = true`).
    // Bootstrap user fixture, then break inserts — the audit-log INSERT
    // for "user_login" fails AFTER the JWT is signed (non-fatal per
    // token_service.rs:319).
    use ac_service::services::{token_service, user_service};

    let master_key = test_master_key();
    key_management_service::initialize_signing_key(&pool, &master_key, "test-cluster")
        .await
        .unwrap();

    let org_id = sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO organizations (subdomain, display_name) VALUES ($1, $2) RETURNING org_id",
    )
    .bind(format!("audit-{}", uuid::Uuid::new_v4()))
    .bind("Audit test")
    .fetch_one(&pool)
    .await
    .unwrap();

    user_service::register_user(
        &pool,
        &master_key,
        &master_key,
        org_id,
        user_service::RegistrationRequest {
            email: "audit-login@example.com".to_string(),
            password: "test-password-12345".to_string(),
            display_name: "Audit Login".to_string(),
        },
        Some("198.51.100.1"),
        None,
        ac_service::config::MIN_BCRYPT_COST,
        ac_service::config::DEFAULT_REGISTRATION_RATE_LIMIT_WINDOW_MINUTES,
        ac_service::config::DEFAULT_REGISTRATION_RATE_LIMIT_MAX_ATTEMPTS,
        ac_service::config::DEFAULT_RATE_LIMIT_WINDOW_MINUTES,
        ac_service::config::DEFAULT_RATE_LIMIT_MAX_ATTEMPTS,
    )
    .await
    .unwrap();

    break_auth_events_inserts(&pool).await;
    let snap = MetricAssertion::snapshot();
    let _ = token_service::issue_user_token(
        &pool,
        &master_key,
        &master_key,
        org_id,
        "audit-login@example.com",
        "test-password-12345", // <- valid creds → success branch → user_login
        Some("198.51.100.1"),
        None,
        ac_service::config::DEFAULT_RATE_LIMIT_WINDOW_MINUTES,
        ac_service::config::DEFAULT_RATE_LIMIT_MAX_ATTEMPTS,
    )
    .await;
    assert_only_event_type(&snap, "user_login");
}

#[sqlx::test(migrations = "../../migrations")]
async fn audit_log_failure_emits_event_type_user_login_failed(pool: PgPool) {
    // Drive `token_service::issue_user_token` failure path
    // (parameterized site at token_service.rs:362, where `event_type =
    // AuthEventType::UserLoginFailed.as_str()` because `success = false`).
    use ac_service::services::{token_service, user_service};

    let master_key = test_master_key();
    key_management_service::initialize_signing_key(&pool, &master_key, "test-cluster")
        .await
        .unwrap();

    let org_id = sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO organizations (subdomain, display_name) VALUES ($1, $2) RETURNING org_id",
    )
    .bind(format!("audit-{}", uuid::Uuid::new_v4()))
    .bind("Audit test")
    .fetch_one(&pool)
    .await
    .unwrap();

    user_service::register_user(
        &pool,
        &master_key,
        &master_key,
        org_id,
        user_service::RegistrationRequest {
            email: "audit-login-fail@example.com".to_string(),
            password: "correct-password-12345".to_string(),
            display_name: "Audit LoginFail".to_string(),
        },
        Some("198.51.100.2"),
        None,
        ac_service::config::MIN_BCRYPT_COST,
        ac_service::config::DEFAULT_REGISTRATION_RATE_LIMIT_WINDOW_MINUTES,
        ac_service::config::DEFAULT_REGISTRATION_RATE_LIMIT_MAX_ATTEMPTS,
        ac_service::config::DEFAULT_RATE_LIMIT_WINDOW_MINUTES,
        ac_service::config::DEFAULT_RATE_LIMIT_MAX_ATTEMPTS,
    )
    .await
    .unwrap();

    break_auth_events_inserts(&pool).await;
    let snap = MetricAssertion::snapshot();
    let _ = token_service::issue_user_token(
        &pool,
        &master_key,
        &master_key,
        org_id,
        "audit-login-fail@example.com",
        "WRONG-password-99999", // <- bad creds → failure branch → user_login_failed
        Some("198.51.100.2"),
        None,
        ac_service::config::DEFAULT_RATE_LIMIT_WINDOW_MINUTES,
        ac_service::config::DEFAULT_RATE_LIMIT_MAX_ATTEMPTS,
    )
    .await;
    assert_only_event_type(&snap, "user_login_failed");
}
