// Every `#[sqlx::test]` is implicitly `flavor = "current_thread"`.
//
//! MH health-checker tick gates (ADR-0039 §10): the spawned loop, the sweep's
//! four arms (draining included), millisecond binding, and tick order
//! (sweep → retention → availability).

#[path = "common/mod.rs"]
mod test_common;

use gc_service::repositories::{HealthStatus, MediaHandlersRepository};
use gc_service::tasks::mh_health_checker::{mh_registry_tick, start_mh_health_checker};
use sqlx::PgPool;
use std::time::Duration;
use test_common::mh_registry::{freshness_registry, ms, seed, status_of};
use tokio_util::sync::CancellationToken;

/// Test that the MH health checker task starts and stops gracefully.
#[sqlx::test(migrations = "../../migrations")]
async fn test_mh_health_checker_starts_and_stops(pool: PgPool) {
    let cancel_token = CancellationToken::new();
    let cancel_clone = cancel_token.clone();

    let handle = tokio::spawn(start_mh_health_checker(
        pool,
        freshness_registry(2),
        "test-region".to_string(),
        cancel_token,
    ));

    tokio::time::sleep(Duration::from_millis(100)).await;
    cancel_clone.cancel();

    let result = tokio::time::timeout(Duration::from_secs(2), handle).await;
    assert!(
        result.is_ok(),
        "MH health checker should stop within 2 seconds after cancellation"
    );
    result.unwrap().expect("Task should not panic");
}

/// The spawned loop drives the real tick: a stale row flips to unhealthy.
#[sqlx::test(migrations = "../../migrations")]
async fn test_mh_health_checker_loop_marks_stale_handlers(pool: PgPool) {
    let bound = freshness_registry(2).staleness_bound();
    seed(
        &pool,
        "stale-mh-001",
        "test-region",
        "healthy",
        ms(bound) * 2,
    )
    .await;

    let cancel_token = CancellationToken::new();
    let cancel_clone = cancel_token.clone();
    let handle = tokio::spawn(start_mh_health_checker(
        pool.clone(),
        freshness_registry(2),
        "test-region".to_string(),
        cancel_token,
    ));

    // The interval's first tick fires immediately; poll for the flip (the
    // ceiling only absorbs scheduling delay under parallel test load).
    let polled = tokio::time::timeout(Duration::from_secs(10), async {
        while status_of(&pool, "stale-mh-001").await != Some(HealthStatus::Unhealthy) {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;

    cancel_clone.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;

    assert!(
        polled.is_ok(),
        "Stale handler should be marked unhealthy by the health checker loop within 10s"
    );
}

/// Sweep, all four arms in one test so a no-op or draining-skipping
/// implementation fails: stale draining → unhealthy (ADR-0039 §10: an
/// exited draining MH must not stay draining forever); fresh draining →
/// draining; stale healthy → unhealthy; fresh healthy → healthy.
#[sqlx::test(migrations = "../../migrations")]
async fn test_sweep_marks_every_stale_row_including_draining(pool: PgPool) {
    let bound = freshness_registry(2).staleness_bound();
    let stale = ms(bound) * 2;
    let fresh = ms(bound) / 4;
    seed(&pool, "stale-draining", "test-region", "draining", stale).await;
    seed(&pool, "fresh-draining", "test-region", "draining", fresh).await;
    seed(&pool, "stale-healthy", "test-region", "healthy", stale).await;
    seed(&pool, "fresh-healthy", "test-region", "healthy", fresh).await;

    let marked = MediaHandlersRepository::mark_stale_handlers_unhealthy(&pool, bound)
        .await
        .unwrap();

    assert_eq!(marked, 2, "exactly the two stale rows transition");
    assert_eq!(
        status_of(&pool, "stale-draining").await,
        Some(HealthStatus::Unhealthy)
    );
    assert_eq!(
        status_of(&pool, "fresh-draining").await,
        Some(HealthStatus::Draining)
    );
    assert_eq!(
        status_of(&pool, "stale-healthy").await,
        Some(HealthStatus::Unhealthy)
    );
    assert_eq!(
        status_of(&pool, "fresh-healthy").await,
        Some(HealthStatus::Healthy)
    );

    // Already-unhealthy rows are not re-counted.
    let again = MediaHandlersRepository::mark_stale_handlers_unhealthy(&pool, bound)
        .await
        .unwrap();
    assert_eq!(again, 0, "only transitions are counted");
}

/// The bound is bound in MILLISECONDS: B = 3750 ms. A row at −3200 ms is
/// fresh (a bound truncated to 3 s would sweep it); a row at −3900 ms is
/// stale (a bound rounded to 4 s would keep it). Rows only age between the
/// backdating UPDATE and the sweep, so the fresh row's 550 ms slack is the
/// only margin that matters; the UPDATE runs immediately before the sweep.
#[sqlx::test(migrations = "../../migrations")]
async fn test_sweep_bound_is_milliseconds(pool: PgPool) {
    let bound = freshness_registry(2).staleness_bound();
    assert_eq!(bound, Duration::from_millis(3750));
    seed(&pool, "ms-stale", "test-region", "healthy", 3900).await;
    seed(&pool, "ms-fresh", "test-region", "healthy", 3200).await;

    MediaHandlersRepository::mark_stale_handlers_unhealthy(&pool, bound)
        .await
        .unwrap();

    assert_eq!(
        status_of(&pool, "ms-fresh").await,
        Some(HealthStatus::Healthy)
    );
    assert_eq!(
        status_of(&pool, "ms-stale").await,
        Some(HealthStatus::Unhealthy)
    );
}

/// Tick order: in ONE tick a row older than retention is deleted (and so
/// absent from the gauge input), and a stale row younger than retention is
/// swept and counted unhealthy.
#[sqlx::test(migrations = "../../migrations")]
async fn test_tick_sweeps_then_deletes_then_counts(pool: PgPool) {
    let reg = freshness_registry(2);
    seed(
        &pool,
        "expired",
        "test-region",
        "healthy",
        ms(reg.retention()) + 5_000,
    )
    .await;
    seed(
        &pool,
        "stale",
        "test-region",
        "healthy",
        ms(reg.staleness_bound()) * 2,
    )
    .await;
    seed(&pool, "fresh", "test-region", "healthy", 0).await;

    let swept = mh_registry_tick(&pool, &reg, "test-region").await.unwrap();
    assert_eq!(
        swept, 2,
        "expired and stale rows were both stale at sweep time"
    );

    assert_eq!(
        status_of(&pool, "expired").await,
        None,
        "expired row deleted"
    );
    assert_eq!(
        status_of(&pool, "stale").await,
        Some(HealthStatus::Unhealthy)
    );
    assert_eq!(status_of(&pool, "fresh").await, Some(HealthStatus::Healthy));

    let rows = MediaHandlersRepository::count_available_handlers(
        &pool,
        reg.staleness_bound(),
        "test-region",
    )
    .await
    .unwrap();
    let total: u64 = rows.iter().map(|r| r.count).sum();
    assert_eq!(total, 2, "deleted row is in no bucket");
}
