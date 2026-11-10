//! Shared MH-registry fixtures for the freshness integration tests
//! (`mh_registry_tests.rs`, `mh_health_checker_tests.rs`,
//! `mh_registry_metrics_integration.rs`). The canonical home: do not copy.

use gc_service::config::MhRegistryConfig;
use gc_service::repositories::{
    HealthStatus, MediaHandlersRepository, MhRegistrationDecision, NewMhRegistration,
};
use sqlx::PgPool;
use std::time::Duration;

/// Test-local registry for tests ABOUT freshness: 1250 ms × k 3 = a 3750 ms
/// bound (deliberately not a whole number of seconds), 60 s retention.
/// The exclude cap is not what these tests are about: 16 (the [4, 64] range
/// and the > initial-count rule hold for every N in [1, 4]).
pub fn freshness_registry(initial_handlers_per_meeting: u64) -> MhRegistryConfig {
    MhRegistryConfig::new(1250, 3, 60, initial_handlers_per_meeting, 16)
        .expect("valid test registry config")
}

/// A span as whole milliseconds, for backdating offsets.
pub fn ms(d: Duration) -> i64 {
    i64::try_from(d.as_millis()).expect("test span fits i64 ms")
}

/// Register `id` in `region`, then force its status and its heartbeat age.
///
/// Rows only age after this runs (NOW() is not frozen), so callers seed a
/// near-the-bound fresh row immediately before the query under test.
pub async fn seed(pool: &PgPool, id: &str, region: &str, status: &str, age_ms: i64) {
    seed_with_endpoints(
        pool,
        id,
        region,
        &format!("https://{id}:443"),
        &format!("grpc://{id}:50051"),
        status,
        age_ms,
    )
    .await;
}

/// [`seed`] with explicit endpoints (the RegisterMH rule tests). The
/// registration must be a plain `Registered` (no other row matched), so a
/// fixture can never silently supersede another fixture.
pub async fn seed_with_endpoints(
    pool: &PgPool,
    id: &str,
    region: &str,
    webtransport_endpoint: &str,
    grpc_endpoint: &str,
    status: &str,
    age_ms: i64,
) {
    let decision = MediaHandlersRepository::register_mh(
        pool,
        &NewMhRegistration {
            handler_id: id,
            region,
            webtransport_endpoint,
            grpc_endpoint,
            max_streams: 1000,
        },
        gc_test_utils::reference_mh_registry().staleness_bound(),
    )
    .await
    .expect("register");
    assert_eq!(
        decision,
        MhRegistrationDecision::Registered,
        "fixture {id} must not match another row"
    );
    sqlx::query(
        "UPDATE media_handlers SET health_status = $2, \
         last_heartbeat_at = NOW() - $3 * INTERVAL '1 millisecond' WHERE handler_id = $1",
    )
    .bind(id)
    .bind(status)
    .bind(age_ms)
    .execute(pool)
    .await
    .expect("seed");
}

/// A row exactly as stored: (wt, grpc, status, last_heartbeat_at), or `None`.
pub async fn stored_row(
    pool: &PgPool,
    id: &str,
) -> Option<(String, String, String, chrono::DateTime<chrono::Utc>)> {
    sqlx::query_as(
        "SELECT webtransport_endpoint, grpc_endpoint, health_status, last_heartbeat_at \
         FROM media_handlers WHERE handler_id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .expect("stored_row")
}

/// Total registry rows.
pub async fn row_count(pool: &PgPool) -> i64 {
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM media_handlers")
        .fetch_one(pool)
        .await
        .expect("row_count");
    n
}

/// The row's stored status, or `None` if the row does not exist.
pub async fn status_of(pool: &PgPool, id: &str) -> Option<HealthStatus> {
    MediaHandlersRepository::get_handler(pool, id)
        .await
        .expect("get_handler")
        .map(|h| h.health_status)
}
