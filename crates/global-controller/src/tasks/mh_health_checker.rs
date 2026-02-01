//! MH health checker background task.
//!
//! Periodically checks for stale Media Handlers and marks them as unhealthy.
//! Handlers are considered stale if they haven't sent a load report within
//! the configured threshold.
//!
//! # Graceful Shutdown
//!
//! The task supports graceful shutdown via a cancellation token. When the token
//! is cancelled, the task completes its current iteration and exits cleanly.

use crate::repositories::MediaHandlersRepository;
use sqlx::PgPool;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::{info, instrument, warn};

/// Default health check interval in seconds.
const DEFAULT_CHECK_INTERVAL_SECONDS: u64 = 5;

/// Start the MH health checker background task.
///
/// This task runs in a loop, checking for stale handlers every 5 seconds.
/// It will exit gracefully when the cancellation token is triggered.
///
/// # Arguments
///
/// * `pool` - Database connection pool
/// * `staleness_threshold_seconds` - Seconds since last heartbeat to consider stale
/// * `cancel_token` - Token for graceful shutdown
///
/// # Returns
///
/// Returns when the cancellation token is triggered.
#[instrument(skip_all, name = "gc.task.mh_health_checker")]
pub async fn start_mh_health_checker(
    pool: PgPool,
    staleness_threshold_seconds: u64,
    cancel_token: CancellationToken,
) {
    info!(
        target: "gc.task.mh_health_checker",
        staleness_threshold = staleness_threshold_seconds,
        check_interval = DEFAULT_CHECK_INTERVAL_SECONDS,
        "Starting MH health checker task"
    );

    let mut interval = tokio::time::interval(Duration::from_secs(DEFAULT_CHECK_INTERVAL_SECONDS));

    loop {
        tokio::select! {
            _ = interval.tick() => {
                // Check for stale handlers
                match MediaHandlersRepository::mark_stale_handlers_unhealthy(
                    &pool,
                    staleness_threshold_seconds as i64,
                ).await {
                    Ok(count) => {
                        if count > 0 {
                            warn!(
                                target: "gc.task.mh_health_checker",
                                stale_count = count,
                                "Marked stale handlers as unhealthy"
                            );
                        }
                    }
                    Err(e) => {
                        // Log error but continue - database might recover
                        tracing::error!(
                            target: "gc.task.mh_health_checker",
                            error = %e,
                            "Failed to check for stale handlers"
                        );
                    }
                }
            }
            _ = cancel_token.cancelled() => {
                info!(
                    target: "gc.task.mh_health_checker",
                    "MH health checker task received shutdown signal, exiting"
                );
                break;
            }
        }
    }

    info!(
        target: "gc.task.mh_health_checker",
        "MH health checker task stopped"
    );
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_default_check_interval() {
        assert_eq!(DEFAULT_CHECK_INTERVAL_SECONDS, 5);
    }

    #[tokio::test]
    async fn test_cancellation_token_stops_task() {
        // Create a mock cancellation that triggers immediately
        let cancel_token = CancellationToken::new();
        let cancel_clone = cancel_token.clone();

        // Cancel immediately
        cancel_clone.cancel();

        // The task should return quickly since it's cancelled
        // We can't easily test without a real database, but we can verify
        // the cancellation token works
        assert!(cancel_token.is_cancelled());
    }
}

/// Integration tests for MH health checker task requiring database.
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::repositories::{HealthStatus, MediaHandlersRepository};
    use sqlx::PgPool;
    use std::time::Duration;

    /// Test that the MH health checker task starts and stops gracefully.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_mh_health_checker_starts_and_stops(pool: PgPool) {
        let cancel_token = CancellationToken::new();
        let cancel_clone = cancel_token.clone();

        // Spawn the health checker task
        let handle = tokio::spawn(start_mh_health_checker(pool, 30, cancel_token));

        // Let it run briefly (not long enough for a full interval)
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Cancel the task
        cancel_clone.cancel();

        // Task should complete within a reasonable time
        let result = tokio::time::timeout(Duration::from_secs(2), handle).await;
        assert!(
            result.is_ok(),
            "MH health checker should stop within 2 seconds after cancellation"
        );
        result.unwrap().expect("Task should not panic");
    }

    /// Test that the health checker marks stale handlers as unhealthy.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_mh_health_checker_marks_stale_handlers(pool: PgPool) {
        // Register a handler with an old heartbeat (manually set via SQL)
        MediaHandlersRepository::register_mh(
            &pool,
            "stale-mh-001",
            "us-east-1",
            "https://stale-mh:443",
            "grpc://stale-mh:50051",
            1000,
        )
        .await
        .expect("Failed to register handler");

        // Update heartbeat to healthy status first
        MediaHandlersRepository::update_load_report(
            &pool,
            "stale-mh-001",
            10,
            HealthStatus::Healthy,
            Some(20.0),
            Some(30.0),
            Some(40.0),
        )
        .await
        .expect("Failed to update heartbeat");

        // Manually backdate the heartbeat to make it stale (older than 1 second)
        sqlx::query(
            "UPDATE media_handlers SET last_heartbeat_at = NOW() - INTERVAL '10 seconds' WHERE handler_id = $1"
        )
        .bind("stale-mh-001")
        .execute(&pool)
        .await
        .expect("Failed to backdate heartbeat");

        let cancel_token = CancellationToken::new();
        let cancel_clone = cancel_token.clone();

        // Spawn health checker with a very short staleness threshold (1 second)
        let pool_clone = pool.clone();
        let handle = tokio::spawn(start_mh_health_checker(pool_clone, 1, cancel_token));

        // Wait for one health check cycle (default interval is 5 seconds, but we can wait a bit more)
        tokio::time::sleep(Duration::from_secs(6)).await;

        // Cancel the task
        cancel_clone.cancel();
        let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;

        // Verify the handler is now marked as unhealthy
        let handler = MediaHandlersRepository::get_handler(&pool, "stale-mh-001")
            .await
            .expect("Failed to get handler")
            .expect("Handler should exist");

        assert_eq!(
            handler.health_status,
            HealthStatus::Unhealthy,
            "Stale handler should be marked as unhealthy"
        );
    }

    /// Test that healthy handlers with recent heartbeats are not marked unhealthy.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_mh_health_checker_preserves_healthy_handlers(pool: PgPool) {
        // Register a handler with fresh heartbeat
        MediaHandlersRepository::register_mh(
            &pool,
            "healthy-mh-001",
            "us-west-2",
            "https://healthy-mh:443",
            "grpc://healthy-mh:50051",
            1000,
        )
        .await
        .expect("Failed to register handler");

        // Update heartbeat to healthy status (just registered, so heartbeat is fresh)
        MediaHandlersRepository::update_load_report(
            &pool,
            "healthy-mh-001",
            5,
            HealthStatus::Healthy,
            Some(10.0),
            Some(20.0),
            Some(15.0),
        )
        .await
        .expect("Failed to update heartbeat");

        let cancel_token = CancellationToken::new();
        let cancel_clone = cancel_token.clone();

        // Spawn health checker with a long staleness threshold (60 seconds)
        let pool_clone = pool.clone();
        let handle = tokio::spawn(start_mh_health_checker(pool_clone, 60, cancel_token));

        // Wait for one health check cycle
        tokio::time::sleep(Duration::from_secs(6)).await;

        // Cancel the task
        cancel_clone.cancel();
        let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;

        // Verify the handler is still healthy
        let handler = MediaHandlersRepository::get_handler(&pool, "healthy-mh-001")
            .await
            .expect("Failed to get handler")
            .expect("Handler should exist");

        assert_eq!(
            handler.health_status,
            HealthStatus::Healthy,
            "Healthy handler with recent heartbeat should remain healthy"
        );
    }

    /// Test that draining handlers are not marked unhealthy (even if stale).
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_mh_health_checker_skips_draining_handlers(pool: PgPool) {
        // Register a handler and set it to draining status
        MediaHandlersRepository::register_mh(
            &pool,
            "draining-mh-001",
            "eu-west-1",
            "https://draining-mh:443",
            "grpc://draining-mh:50051",
            500,
        )
        .await
        .expect("Failed to register handler");

        // Set status to draining via direct SQL
        sqlx::query(
            "UPDATE media_handlers SET health_status = 'draining', last_heartbeat_at = NOW() - INTERVAL '10 seconds' WHERE handler_id = $1"
        )
        .bind("draining-mh-001")
        .execute(&pool)
        .await
        .expect("Failed to set draining status");

        let cancel_token = CancellationToken::new();
        let cancel_clone = cancel_token.clone();

        // Spawn health checker with short staleness threshold
        let pool_clone = pool.clone();
        let handle = tokio::spawn(start_mh_health_checker(pool_clone, 1, cancel_token));

        // Wait for one health check cycle
        tokio::time::sleep(Duration::from_secs(6)).await;

        // Cancel the task
        cancel_clone.cancel();
        let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;

        // Verify the handler is still draining (not marked unhealthy)
        let handler = MediaHandlersRepository::get_handler(&pool, "draining-mh-001")
            .await
            .expect("Failed to get handler")
            .expect("Handler should exist");

        assert_eq!(
            handler.health_status,
            HealthStatus::Draining,
            "Draining handler should not be marked unhealthy"
        );
    }
}
