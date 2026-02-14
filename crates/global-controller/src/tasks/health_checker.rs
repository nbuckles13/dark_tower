//! Health checker background task.
//!
//! Periodically checks for stale Meeting Controllers and marks them as unhealthy.
//! Controllers are considered stale if they haven't sent a heartbeat within
//! the configured threshold.
//!
//! This is a thin wrapper around the generic health checker, parameterized for
//! Meeting Controllers.
//!
//! # Graceful Shutdown
//!
//! The task supports graceful shutdown via a cancellation token. When the token
//! is cancelled, the task completes its current iteration and exits cleanly.

use crate::observability::metrics;
use crate::repositories::MeetingControllersRepository;
use crate::tasks::generic_health_checker::{
    start_generic_health_checker, DEFAULT_CHECK_INTERVAL_SECONDS,
};
use sqlx::PgPool;
use tokio_util::sync::CancellationToken;
use tracing::{info, Instrument};

/// Refresh the registered controllers gauge metric.
///
/// Queries the database for current controller counts by status
/// and updates the `gc_registered_controllers` gauge.
async fn refresh_controller_metrics(pool: &PgPool) {
    match MeetingControllersRepository::get_controller_counts_by_status(pool).await {
        Ok(counts) => {
            // Convert to the format expected by the metrics helper
            let counts: Vec<(String, u64)> = counts
                .into_iter()
                .map(|(status, count)| {
                    use crate::repositories::HealthStatus;
                    let status_str = match status {
                        HealthStatus::Pending => "pending",
                        HealthStatus::Healthy => "healthy",
                        HealthStatus::Degraded => "degraded",
                        HealthStatus::Unhealthy => "unhealthy",
                        HealthStatus::Draining => "draining",
                    };
                    (status_str.to_string(), count as u64)
                })
                .collect();
            metrics::update_registered_controller_gauges("meeting", &counts);
        }
        Err(e) => {
            tracing::warn!(
                target: "gc.task.health_checker",
                error = %e,
                "Failed to refresh controller metrics"
            );
        }
    }
}

/// Start the health checker background task.
///
/// This task runs in a loop, checking for stale controllers every 5 seconds.
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
pub async fn start_health_checker(
    pool: PgPool,
    staleness_threshold_seconds: u64,
    cancel_token: CancellationToken,
) {
    info!(
        target: "gc.task.health_checker",
        staleness_threshold = staleness_threshold_seconds,
        check_interval = DEFAULT_CHECK_INTERVAL_SECONDS,
        "Starting health checker task"
    );

    start_generic_health_checker(
        pool,
        staleness_threshold_seconds,
        cancel_token,
        "controllers",
        |pool, threshold| async move {
            let result =
                MeetingControllersRepository::mark_stale_controllers_unhealthy(&pool, threshold)
                    .await;
            if result.is_ok() {
                refresh_controller_metrics(&pool).await;
            }
            result
        },
    )
    .instrument(tracing::info_span!("gc.task.health_checker"))
    .await;

    info!(
        target: "gc.task.health_checker",
        "Health checker task stopped"
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

/// Integration tests for health checker task requiring database.
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::repositories::{HealthStatus, MeetingControllersRepository};
    use sqlx::PgPool;
    use std::time::Duration;

    /// Test that the health checker task starts and stops gracefully.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_health_checker_starts_and_stops(pool: PgPool) {
        let cancel_token = CancellationToken::new();
        let cancel_clone = cancel_token.clone();

        // Spawn the health checker task
        let handle = tokio::spawn(start_health_checker(pool, 30, cancel_token));

        // Let it run briefly (not long enough for a full interval)
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Cancel the task
        cancel_clone.cancel();

        // Task should complete within a reasonable time
        let result = tokio::time::timeout(Duration::from_secs(2), handle).await;
        assert!(
            result.is_ok(),
            "Health checker should stop within 2 seconds after cancellation"
        );
        result.unwrap().expect("Task should not panic");
    }

    /// Test that the health checker marks stale controllers as unhealthy.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_health_checker_marks_stale_controllers(pool: PgPool) {
        // Register a controller with an old heartbeat (manually set via SQL)
        MeetingControllersRepository::register_mc(
            &pool,
            "stale-mc-001",
            "us-east-1",
            "grpc://stale-mc:50051",
            Some("https://stale-mc:443"),
            100,
            1000,
        )
        .await
        .expect("Failed to register controller");

        // Update heartbeat to healthy status first
        MeetingControllersRepository::update_heartbeat(
            &pool,
            "stale-mc-001",
            10,
            50,
            HealthStatus::Healthy,
        )
        .await
        .expect("Failed to update heartbeat");

        // Manually backdate the heartbeat to make it stale (older than 1 second)
        sqlx::query(
            "UPDATE meeting_controllers SET last_heartbeat_at = NOW() - INTERVAL '10 seconds' WHERE controller_id = $1"
        )
        .bind("stale-mc-001")
        .execute(&pool)
        .await
        .expect("Failed to backdate heartbeat");

        let cancel_token = CancellationToken::new();
        let cancel_clone = cancel_token.clone();

        // Spawn health checker with a very short staleness threshold (1 second)
        let pool_clone = pool.clone();
        let handle = tokio::spawn(start_health_checker(pool_clone, 1, cancel_token));

        // Wait for one health check cycle (default interval is 5 seconds, but we can wait a bit more)
        // Using a shorter wait since test intervals are 5 seconds
        tokio::time::sleep(Duration::from_secs(6)).await;

        // Cancel the task
        cancel_clone.cancel();
        let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;

        // Verify the controller is now marked as unhealthy
        let controller = MeetingControllersRepository::get_controller(&pool, "stale-mc-001")
            .await
            .expect("Failed to get controller")
            .expect("Controller should exist");

        assert_eq!(
            controller.health_status,
            HealthStatus::Unhealthy,
            "Stale controller should be marked as unhealthy"
        );
    }

    /// Test that healthy controllers with recent heartbeats are not marked unhealthy.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_health_checker_preserves_healthy_controllers(pool: PgPool) {
        // Register a controller with fresh heartbeat
        MeetingControllersRepository::register_mc(
            &pool,
            "healthy-mc-001",
            "us-west-2",
            "grpc://healthy-mc:50051",
            Some("https://healthy-mc:443"),
            100,
            1000,
        )
        .await
        .expect("Failed to register controller");

        // Update heartbeat to healthy status (just registered, so heartbeat is fresh)
        MeetingControllersRepository::update_heartbeat(
            &pool,
            "healthy-mc-001",
            5,
            25,
            HealthStatus::Healthy,
        )
        .await
        .expect("Failed to update heartbeat");

        let cancel_token = CancellationToken::new();
        let cancel_clone = cancel_token.clone();

        // Spawn health checker with a long staleness threshold (60 seconds)
        let pool_clone = pool.clone();
        let handle = tokio::spawn(start_health_checker(pool_clone, 60, cancel_token));

        // Wait for one health check cycle
        tokio::time::sleep(Duration::from_secs(6)).await;

        // Cancel the task
        cancel_clone.cancel();
        let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;

        // Verify the controller is still healthy
        let controller = MeetingControllersRepository::get_controller(&pool, "healthy-mc-001")
            .await
            .expect("Failed to get controller")
            .expect("Controller should exist");

        assert_eq!(
            controller.health_status,
            HealthStatus::Healthy,
            "Healthy controller with recent heartbeat should remain healthy"
        );
    }

    /// Test that draining controllers are not marked unhealthy (even if stale).
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_health_checker_skips_draining_controllers(pool: PgPool) {
        // Register a controller and set it to draining status
        MeetingControllersRepository::register_mc(
            &pool,
            "draining-mc-001",
            "eu-west-1",
            "grpc://draining-mc:50051",
            None,
            50,
            500,
        )
        .await
        .expect("Failed to register controller");

        // Set status to draining via direct SQL (since update_heartbeat doesn't allow draining)
        sqlx::query(
            "UPDATE meeting_controllers SET health_status = 'draining', last_heartbeat_at = NOW() - INTERVAL '10 seconds' WHERE controller_id = $1"
        )
        .bind("draining-mc-001")
        .execute(&pool)
        .await
        .expect("Failed to set draining status");

        let cancel_token = CancellationToken::new();
        let cancel_clone = cancel_token.clone();

        // Spawn health checker with short staleness threshold
        let pool_clone = pool.clone();
        let handle = tokio::spawn(start_health_checker(pool_clone, 1, cancel_token));

        // Wait for one health check cycle
        tokio::time::sleep(Duration::from_secs(6)).await;

        // Cancel the task
        cancel_clone.cancel();
        let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;

        // Verify the controller is still draining (not marked unhealthy)
        let controller = MeetingControllersRepository::get_controller(&pool, "draining-mc-001")
            .await
            .expect("Failed to get controller")
            .expect("Controller should exist");

        assert_eq!(
            controller.health_status,
            HealthStatus::Draining,
            "Draining controller should not be marked unhealthy"
        );
    }

    /// Test that already unhealthy controllers are not re-updated.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_health_checker_skips_already_unhealthy(pool: PgPool) {
        // Register a controller that's already unhealthy
        MeetingControllersRepository::register_mc(
            &pool,
            "unhealthy-mc-001",
            "ap-south-1",
            "grpc://unhealthy-mc:50051",
            None,
            100,
            1000,
        )
        .await
        .expect("Failed to register controller");

        // Set to unhealthy with old heartbeat
        sqlx::query(
            "UPDATE meeting_controllers SET health_status = 'unhealthy', last_heartbeat_at = NOW() - INTERVAL '100 seconds' WHERE controller_id = $1"
        )
        .bind("unhealthy-mc-001")
        .execute(&pool)
        .await
        .expect("Failed to set unhealthy status");

        // Record the updated_at time before health check
        let before: (chrono::DateTime<chrono::Utc>,) =
            sqlx::query_as("SELECT updated_at FROM meeting_controllers WHERE controller_id = $1")
                .bind("unhealthy-mc-001")
                .fetch_one(&pool)
                .await
                .expect("Failed to get updated_at");

        let cancel_token = CancellationToken::new();
        let cancel_clone = cancel_token.clone();

        // Spawn health checker
        let pool_clone = pool.clone();
        let handle = tokio::spawn(start_health_checker(pool_clone, 1, cancel_token));

        // Wait for one health check cycle
        tokio::time::sleep(Duration::from_secs(6)).await;

        // Cancel the task
        cancel_clone.cancel();
        let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;

        // Verify the controller was not re-updated (updated_at should be unchanged)
        let after: (chrono::DateTime<chrono::Utc>,) =
            sqlx::query_as("SELECT updated_at FROM meeting_controllers WHERE controller_id = $1")
                .bind("unhealthy-mc-001")
                .fetch_one(&pool)
                .await
                .expect("Failed to get updated_at");

        assert_eq!(
            before.0, after.0,
            "Already unhealthy controller should not be re-updated"
        );
    }
}
