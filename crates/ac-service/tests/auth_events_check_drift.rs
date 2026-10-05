// Every `#[sqlx::test]` is implicitly `flavor = "current_thread"`.
//
//! Drift guard between `AuthEventType` (the audit vocabulary SSoT,
//! `crates/ac-service/src/models/mod.rs`) and the SQL constraints on
//! `auth_events` (`valid_event_type`, `event_has_subject`).
//!
//! The CHECK cannot be derived from the enum, so this test pins them together
//! in BOTH directions: every variant inserts in its real write shape, and the
//! CHECK's value set read back from `pg_constraint` equals the enum's strings.
//! It also proves migration 20261004000002 ran (`convalidated = true`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use ac_service::models::AuthEventType;
use sqlx::PgPool;
use std::collections::BTreeSet;

/// Events written without a user or credential subject (exempted in
/// `event_has_subject`).
fn is_subjectless(event: AuthEventType) -> bool {
    matches!(
        event,
        AuthEventType::KeyGenerated
            | AuthEventType::KeyRotated
            | AuthEventType::KeyExpired
            | AuthEventType::UserRegistrationFailed
    )
}

async fn seed_credential(pool: &PgPool) -> uuid::Uuid {
    sqlx::query_scalar(
        "INSERT INTO service_credentials \
         (client_id, client_secret_hash, service_type, scopes) \
         VALUES ('drift-client', 'hash', 'global-controller', '{}') RETURNING credential_id",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn every_auth_event_type_inserts_in_its_write_shape(pool: PgPool) {
    let credential_id = seed_credential(&pool).await;
    for event in AuthEventType::ALL {
        let subject = (!is_subjectless(*event)).then_some(credential_id);
        let result = sqlx::query(
            "INSERT INTO auth_events (event_type, credential_id, success) VALUES ($1, $2, true)",
        )
        .bind(event.as_str())
        .bind(subject)
        .execute(&pool)
        .await;
        assert!(
            result.is_ok(),
            "{} must be accepted by the auth_events CHECKs (missing from a migration?): {:?}",
            event.as_str(),
            result.err()
        );
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn unknown_event_type_violates_valid_event_type(pool: PgPool) {
    let credential_id = seed_credential(&pool).await;
    let err = sqlx::query(
        "INSERT INTO auth_events (event_type, credential_id, success) VALUES ($1, $2, true)",
    )
    .bind("not_an_event")
    .bind(credential_id)
    .execute(&pool)
    .await
    .expect_err("unknown event_type must be rejected");
    let db_err = err.as_database_error().expect("database error");
    assert_eq!(db_err.code().as_deref(), Some("23514"), "check_violation");
    assert_eq!(db_err.constraint(), Some("valid_event_type"));
}

/// Negative control: the subject exemption covers only the listed types.
#[sqlx::test(migrations = "../../migrations")]
async fn subjectless_user_login_failed_violates_event_has_subject(pool: PgPool) {
    let err = sqlx::query("INSERT INTO auth_events (event_type, success) VALUES ($1, false)")
        .bind(AuthEventType::UserLoginFailed.as_str())
        .execute(&pool)
        .await
        .expect_err("subjectless user_login_failed must be rejected");
    let db_err = err.as_database_error().expect("database error");
    assert_eq!(db_err.code().as_deref(), Some("23514"), "check_violation");
    assert_eq!(db_err.constraint(), Some("event_has_subject"));
}

/// The CHECK's literal set equals `AuthEventType::ALL` exactly — a variant
/// without a CHECK value, or a CHECK value without a variant, fails here.
#[sqlx::test(migrations = "../../migrations")]
async fn valid_event_type_matches_enum_in_both_directions(pool: PgPool) {
    // Definition looks like: CHECK (((event_type)::text = ANY ((ARRAY['a'::character
    // varying, ...])::text[]))).
    let (def, in_check) = constraint_literals(&pool, "valid_event_type").await;
    let in_enum: BTreeSet<String> = AuthEventType::ALL
        .iter()
        .map(|v| v.as_str().to_string())
        .collect();
    assert_eq!(in_check, in_enum, "constraint def: {def}");
}

/// Read a constraint's definition and extract every quoted literal.
async fn constraint_literals(pool: &PgPool, name: &str) -> (String, BTreeSet<String>) {
    let def: String = sqlx::query_scalar(
        "SELECT pg_get_constraintdef(oid) FROM pg_constraint \
         WHERE conrelid = 'auth_events'::regclass AND conname = $1",
    )
    .bind(name)
    .fetch_one(pool)
    .await
    .unwrap();
    let literals = def
        .split('\'')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect();
    (def, literals)
}

/// The `event_has_subject` exemption list equals `ALL.filter(is_subjectless)`
/// exactly — so the write-shape loop above cannot pass against a CHECK that
/// exempts a type the test does not know about (or vice versa).
#[sqlx::test(migrations = "../../migrations")]
async fn event_has_subject_exemptions_match_enum_in_both_directions(pool: PgPool) {
    let (def, in_check) = constraint_literals(&pool, "event_has_subject").await;
    let in_enum: BTreeSet<String> = AuthEventType::ALL
        .iter()
        .filter(|v| is_subjectless(**v))
        .map(|v| v.as_str().to_string())
        .collect();
    assert_eq!(in_check, in_enum, "constraint def: {def}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn both_constraints_are_validated(pool: PgPool) {
    let rows: Vec<(String, bool)> = sqlx::query_as(
        "SELECT conname::text, convalidated FROM pg_constraint \
         WHERE conrelid = 'auth_events'::regclass \
           AND conname IN ('valid_event_type', 'event_has_subject') ORDER BY conname",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        rows,
        vec![
            ("event_has_subject".to_string(), true),
            ("valid_event_type".to_string(), true),
        ]
    );
}
