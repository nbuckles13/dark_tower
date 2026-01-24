//! Assignment cleanup background task.
//!
//! Periodically cleans up meeting assignments:
//! 1. Soft-deletes stale assignments (no activity for configurable hours)
//! 2. Hard-deletes old ended assignments (older than configurable days)
//!
//! # Graceful Shutdown
//!
//! The task supports graceful shutdown via a cancellation token. When the token
//! is cancelled, the task completes its current iteration and exits cleanly.

use crate::repositories::MeetingAssignmentsRepository;
use sqlx::PgPool;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::{info, instrument, warn};

/// Default cleanup check interval in seconds (1 hour).
const DEFAULT_CHECK_INTERVAL_SECONDS: u64 = 3600;

/// Default inactivity threshold in hours before soft-deleting assignments.
const DEFAULT_INACTIVITY_HOURS: i32 = 1;

/// Default retention period in days before hard-deleting assignments.
const DEFAULT_RETENTION_DAYS: i32 = 7;

/// Configuration for assignment cleanup task.
#[derive(Debug, Clone)]
pub struct AssignmentCleanupConfig {
    /// Cleanup check interval in seconds.
    pub check_interval_seconds: u64,
    /// Hours of inactivity before soft-deleting assignments.
    pub inactivity_hours: i32,
    /// Days to retain ended assignments before hard-deleting.
    pub retention_days: i32,
}

impl Default for AssignmentCleanupConfig {
    fn default() -> Self {
        Self {
            check_interval_seconds: DEFAULT_CHECK_INTERVAL_SECONDS,
            inactivity_hours: DEFAULT_INACTIVITY_HOURS,
            retention_days: DEFAULT_RETENTION_DAYS,
        }
    }
}

impl AssignmentCleanupConfig {
    /// Create config from environment variables.
    ///
    /// Environment variables:
    /// - `GC_CLEANUP_INTERVAL_SECONDS` - Cleanup check interval (default: 3600)
    /// - `GC_INACTIVITY_HOURS` - Inactivity threshold (default: 1)
    /// - `GC_RETENTION_DAYS` - Retention period (default: 7)
    pub fn from_env() -> Self {
        let check_interval_seconds = std::env::var("GC_CLEANUP_INTERVAL_SECONDS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_CHECK_INTERVAL_SECONDS);

        let inactivity_hours = std::env::var("GC_INACTIVITY_HOURS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_INACTIVITY_HOURS);

        let retention_days = std::env::var("GC_RETENTION_DAYS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_RETENTION_DAYS);

        Self {
            check_interval_seconds,
            inactivity_hours,
            retention_days,
        }
    }
}

/// Start the assignment cleanup background task.
///
/// This task runs in a loop, performing cleanup operations at the configured interval.
/// It will exit gracefully when the cancellation token is triggered.
///
/// # Arguments
///
/// * `pool` - Database connection pool
/// * `config` - Cleanup configuration
/// * `cancel_token` - Token for graceful shutdown
///
/// # Operations
///
/// Each iteration performs:
/// 1. Soft-delete stale assignments (no activity for `inactivity_hours`)
/// 2. Hard-delete old ended assignments (older than `retention_days`)
///
/// # Returns
///
/// Returns when the cancellation token is triggered.
#[instrument(skip_all, name = "gc.task.assignment_cleanup")]
pub async fn start_assignment_cleanup(
    pool: PgPool,
    config: AssignmentCleanupConfig,
    cancel_token: CancellationToken,
) {
    info!(
        target: "gc.task.assignment_cleanup",
        check_interval_seconds = config.check_interval_seconds,
        inactivity_hours = config.inactivity_hours,
        retention_days = config.retention_days,
        "Starting assignment cleanup task"
    );

    let mut interval = tokio::time::interval(Duration::from_secs(config.check_interval_seconds));

    loop {
        tokio::select! {
            _ = interval.tick() => {
                // Run cleanup operations
                run_cleanup(&pool, &config).await;
            }
            _ = cancel_token.cancelled() => {
                info!(
                    target: "gc.task.assignment_cleanup",
                    "Assignment cleanup task received shutdown signal, exiting"
                );
                break;
            }
        }
    }

    info!(
        target: "gc.task.assignment_cleanup",
        "Assignment cleanup task stopped"
    );
}

/// Run a single cleanup iteration.
///
/// This is separated from the main loop to allow direct testing.
/// Made public within the crate for testing access.
pub(crate) async fn run_cleanup(pool: &PgPool, config: &AssignmentCleanupConfig) {
    // Step 1: Soft-delete stale assignments
    // Uses default batch size (None) to let repository use its default
    match MeetingAssignmentsRepository::end_stale_assignments(pool, config.inactivity_hours, None)
        .await
    {
        Ok(count) => {
            if count > 0 {
                warn!(
                    target: "gc.task.assignment_cleanup",
                    stale_count = count,
                    inactivity_hours = config.inactivity_hours,
                    "Soft-deleted stale assignments"
                );
            }
        }
        Err(e) => {
            tracing::error!(
                target: "gc.task.assignment_cleanup",
                error = %e,
                "Failed to end stale assignments"
            );
        }
    }

    // Step 2: Hard-delete old ended assignments
    // Uses default batch size (None) to let repository use its default
    match MeetingAssignmentsRepository::cleanup_old_assignments(pool, config.retention_days, None)
        .await
    {
        Ok(count) => {
            if count > 0 {
                info!(
                    target: "gc.task.assignment_cleanup",
                    deleted_count = count,
                    retention_days = config.retention_days,
                    "Hard-deleted old assignments"
                );
            }
        }
        Err(e) => {
            tracing::error!(
                target: "gc.task.assignment_cleanup",
                error = %e,
                "Failed to cleanup old assignments"
            );
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Mutex to ensure env var tests don't run in parallel
    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn test_default_config() {
        let config = AssignmentCleanupConfig::default();
        assert_eq!(
            config.check_interval_seconds,
            DEFAULT_CHECK_INTERVAL_SECONDS
        );
        assert_eq!(config.inactivity_hours, DEFAULT_INACTIVITY_HOURS);
        assert_eq!(config.retention_days, DEFAULT_RETENTION_DAYS);
    }

    #[test]
    fn test_default_check_interval() {
        assert_eq!(DEFAULT_CHECK_INTERVAL_SECONDS, 3600);
    }

    #[test]
    fn test_default_inactivity_hours() {
        assert_eq!(DEFAULT_INACTIVITY_HOURS, 1);
    }

    #[test]
    fn test_default_retention_days() {
        assert_eq!(DEFAULT_RETENTION_DAYS, 7);
    }

    #[test]
    fn test_from_env_with_valid_values() {
        let _guard = ENV_MUTEX.lock().unwrap();

        // Set environment variables
        std::env::set_var("GC_CLEANUP_INTERVAL_SECONDS", "7200");
        std::env::set_var("GC_INACTIVITY_HOURS", "2");
        std::env::set_var("GC_RETENTION_DAYS", "14");

        let config = AssignmentCleanupConfig::from_env();

        // Clean up
        std::env::remove_var("GC_CLEANUP_INTERVAL_SECONDS");
        std::env::remove_var("GC_INACTIVITY_HOURS");
        std::env::remove_var("GC_RETENTION_DAYS");

        assert_eq!(config.check_interval_seconds, 7200);
        assert_eq!(config.inactivity_hours, 2);
        assert_eq!(config.retention_days, 14);
    }

    #[test]
    fn test_from_env_with_invalid_values_uses_defaults() {
        let _guard = ENV_MUTEX.lock().unwrap();

        // Set invalid (non-numeric) environment variables
        std::env::set_var("GC_CLEANUP_INTERVAL_SECONDS", "not-a-number");
        std::env::set_var("GC_INACTIVITY_HOURS", "invalid");
        std::env::set_var("GC_RETENTION_DAYS", "");

        let config = AssignmentCleanupConfig::from_env();

        // Clean up
        std::env::remove_var("GC_CLEANUP_INTERVAL_SECONDS");
        std::env::remove_var("GC_INACTIVITY_HOURS");
        std::env::remove_var("GC_RETENTION_DAYS");

        // Should fall back to defaults
        assert_eq!(
            config.check_interval_seconds,
            DEFAULT_CHECK_INTERVAL_SECONDS
        );
        assert_eq!(config.inactivity_hours, DEFAULT_INACTIVITY_HOURS);
        assert_eq!(config.retention_days, DEFAULT_RETENTION_DAYS);
    }

    #[test]
    fn test_from_env_with_missing_vars_uses_defaults() {
        let _guard = ENV_MUTEX.lock().unwrap();

        // Ensure variables are not set
        std::env::remove_var("GC_CLEANUP_INTERVAL_SECONDS");
        std::env::remove_var("GC_INACTIVITY_HOURS");
        std::env::remove_var("GC_RETENTION_DAYS");

        let config = AssignmentCleanupConfig::from_env();

        assert_eq!(
            config.check_interval_seconds,
            DEFAULT_CHECK_INTERVAL_SECONDS
        );
        assert_eq!(config.inactivity_hours, DEFAULT_INACTIVITY_HOURS);
        assert_eq!(config.retention_days, DEFAULT_RETENTION_DAYS);
    }
}

/// Integration tests for assignment cleanup task requiring database.
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::repositories::{HealthStatus, MeetingControllersRepository};
    use sqlx::PgPool;
    use std::time::Duration;

    /// Test that the cleanup task starts and stops gracefully.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_assignment_cleanup_starts_and_stops(pool: PgPool) {
        let cancel_token = CancellationToken::new();
        let cancel_clone = cancel_token.clone();

        // Use a short interval for testing
        let config = AssignmentCleanupConfig {
            check_interval_seconds: 1,
            inactivity_hours: 1,
            retention_days: 7,
        };

        // Spawn the cleanup task
        let handle = tokio::spawn(start_assignment_cleanup(pool, config, cancel_token));

        // Let it run briefly
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Cancel the task
        cancel_clone.cancel();

        // Task should complete within a reasonable time
        let result = tokio::time::timeout(Duration::from_secs(2), handle).await;
        assert!(
            result.is_ok(),
            "Assignment cleanup should stop within 2 seconds after cancellation"
        );
        result.unwrap().expect("Task should not panic");
    }

    /// Test that the cleanup task soft-deletes stale assignments.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_assignment_cleanup_ends_stale_assignments(pool: PgPool) {
        // Register an unhealthy MC
        MeetingControllersRepository::register_mc(
            &pool,
            "stale-mc-001",
            "us-east-1",
            "https://stale-mc:50051",
            Some("https://stale-mc:443"),
            100,
            1000,
        )
        .await
        .expect("Failed to register controller");

        // Mark it as unhealthy
        sqlx::query(
            "UPDATE meeting_controllers SET health_status = 'unhealthy' WHERE controller_id = $1",
        )
        .bind("stale-mc-001")
        .execute(&pool)
        .await
        .expect("Failed to set unhealthy status");

        // Create an old assignment (2 hours old)
        sqlx::query(
            r#"
            INSERT INTO meeting_assignments (meeting_id, meeting_controller_id, region, assigned_by_gc_id, assigned_at)
            VALUES ($1, $2, $3, $4, NOW() - INTERVAL '2 hours')
            "#,
        )
        .bind("stale-meeting-001")
        .bind("stale-mc-001")
        .bind("us-east-1")
        .bind("gc-test")
        .execute(&pool)
        .await
        .expect("Failed to create assignment");

        // Run cleanup directly
        let config = AssignmentCleanupConfig {
            check_interval_seconds: 3600,
            inactivity_hours: 1, // 1 hour threshold
            retention_days: 7,
        };

        run_cleanup(&pool, &config).await;

        // Verify the assignment was soft-deleted
        let ended: (Option<chrono::DateTime<chrono::Utc>>,) =
            sqlx::query_as("SELECT ended_at FROM meeting_assignments WHERE meeting_id = $1")
                .bind("stale-meeting-001")
                .fetch_one(&pool)
                .await
                .expect("Failed to get assignment");

        assert!(
            ended.0.is_some(),
            "Stale assignment should be soft-deleted (ended_at should be set)"
        );
    }

    /// Test that active assignments with healthy MCs are not deleted.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_assignment_cleanup_preserves_healthy_assignments(pool: PgPool) {
        // Register a healthy MC
        MeetingControllersRepository::register_mc(
            &pool,
            "healthy-mc-001",
            "us-east-1",
            "https://healthy-mc:50051",
            Some("https://healthy-mc:443"),
            100,
            1000,
        )
        .await
        .expect("Failed to register controller");

        // Update to healthy status with recent heartbeat
        MeetingControllersRepository::update_heartbeat(
            &pool,
            "healthy-mc-001",
            10,
            50,
            HealthStatus::Healthy,
        )
        .await
        .expect("Failed to update heartbeat");

        // Create an old assignment (2 hours old) but MC is healthy
        sqlx::query(
            r#"
            INSERT INTO meeting_assignments (meeting_id, meeting_controller_id, region, assigned_by_gc_id, assigned_at)
            VALUES ($1, $2, $3, $4, NOW() - INTERVAL '2 hours')
            "#,
        )
        .bind("active-meeting-001")
        .bind("healthy-mc-001")
        .bind("us-east-1")
        .bind("gc-test")
        .execute(&pool)
        .await
        .expect("Failed to create assignment");

        // Run cleanup directly
        let config = AssignmentCleanupConfig {
            check_interval_seconds: 3600,
            inactivity_hours: 1,
            retention_days: 7,
        };

        run_cleanup(&pool, &config).await;

        // Verify the assignment was NOT soft-deleted (MC is healthy)
        let ended: (Option<chrono::DateTime<chrono::Utc>>,) =
            sqlx::query_as("SELECT ended_at FROM meeting_assignments WHERE meeting_id = $1")
                .bind("active-meeting-001")
                .fetch_one(&pool)
                .await
                .expect("Failed to get assignment");

        assert!(
            ended.0.is_none(),
            "Assignment with healthy MC should not be soft-deleted"
        );
    }

    /// Test that the cleanup task hard-deletes old ended assignments.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_assignment_cleanup_hard_deletes_old_assignments(pool: PgPool) {
        // Register an MC
        MeetingControllersRepository::register_mc(
            &pool,
            "old-mc-001",
            "us-east-1",
            "https://old-mc:50051",
            None,
            100,
            1000,
        )
        .await
        .expect("Failed to register controller");

        // Create an old ended assignment (10 days old, ended 10 days ago)
        sqlx::query(
            r#"
            INSERT INTO meeting_assignments (meeting_id, meeting_controller_id, region, assigned_by_gc_id, assigned_at, ended_at)
            VALUES ($1, $2, $3, $4, NOW() - INTERVAL '10 days', NOW() - INTERVAL '10 days')
            "#,
        )
        .bind("old-meeting-001")
        .bind("old-mc-001")
        .bind("us-east-1")
        .bind("gc-test")
        .execute(&pool)
        .await
        .expect("Failed to create assignment");

        // Run cleanup directly with 7 day retention
        let config = AssignmentCleanupConfig {
            check_interval_seconds: 3600,
            inactivity_hours: 1,
            retention_days: 7,
        };

        run_cleanup(&pool, &config).await;

        // Verify the assignment was hard-deleted
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM meeting_assignments WHERE meeting_id = $1")
                .bind("old-meeting-001")
                .fetch_one(&pool)
                .await
                .expect("Failed to count assignments");

        assert_eq!(count.0, 0, "Old ended assignment should be hard-deleted");
    }

    /// Test that recently ended assignments are not hard-deleted.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_assignment_cleanup_preserves_recent_ended_assignments(pool: PgPool) {
        // Register an MC
        MeetingControllersRepository::register_mc(
            &pool,
            "recent-mc-001",
            "us-east-1",
            "https://recent-mc:50051",
            None,
            100,
            1000,
        )
        .await
        .expect("Failed to register controller");

        // Create a recently ended assignment (1 day old)
        sqlx::query(
            r#"
            INSERT INTO meeting_assignments (meeting_id, meeting_controller_id, region, assigned_by_gc_id, assigned_at, ended_at)
            VALUES ($1, $2, $3, $4, NOW() - INTERVAL '1 day', NOW() - INTERVAL '1 day')
            "#,
        )
        .bind("recent-meeting-001")
        .bind("recent-mc-001")
        .bind("us-east-1")
        .bind("gc-test")
        .execute(&pool)
        .await
        .expect("Failed to create assignment");

        // Run cleanup directly with 7 day retention
        let config = AssignmentCleanupConfig {
            check_interval_seconds: 3600,
            inactivity_hours: 1,
            retention_days: 7,
        };

        run_cleanup(&pool, &config).await;

        // Verify the assignment was NOT hard-deleted
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM meeting_assignments WHERE meeting_id = $1")
                .bind("recent-meeting-001")
                .fetch_one(&pool)
                .await
                .expect("Failed to count assignments");

        assert_eq!(
            count.0, 1,
            "Recently ended assignment should not be hard-deleted"
        );
    }
}
