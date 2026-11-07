//! Shared MH-registry fixtures for the freshness integration tests
//! (`mh_registry_tests.rs`, `mh_health_checker_tests.rs`,
//! `mh_registry_metrics_integration.rs`). The canonical home: do not copy.

use gc_service::config::MhRegistryConfig;
use gc_service::repositories::{HealthStatus, MediaHandlersRepository};
use sqlx::PgPool;
use std::time::Duration;

/// Test-local registry for tests ABOUT freshness: 1250 ms × k 3 = a 3750 ms
/// bound (deliberately not a whole number of seconds), 60 s retention.
pub fn freshness_registry(initial_handlers_per_meeting: u64) -> MhRegistryConfig {
    MhRegistryConfig::new(1250, 3, 60, initial_handlers_per_meeting)
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
    MediaHandlersRepository::register_mh(
        pool,
        id,
        region,
        &format!("https://{id}:443"),
        &format!("grpc://{id}:50051"),
        1000,
    )
    .await
    .expect("register");
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

/// The row's stored status, or `None` if the row does not exist.
pub async fn status_of(pool: &PgPool, id: &str) -> Option<HealthStatus> {
    MediaHandlersRepository::get_handler(pool, id)
        .await
        .expect("get_handler")
        .map(|h| h.health_status)
}
