//! Assignment cleanup background task.
//!
//! Periodically cleans up meeting assignments:
//! 1. BACKSTOP: soft-ends assignments MC never reported ended -- rows whose MC
//!    is no longer alive (heartbeat older than `MC_STALENESS_THRESHOLD_SECONDS`)
//!    and that are older than `GC_INACTIVITY_HOURS`. MC itself ends a meeting and notifies GC
//!    (`NotifyMeetingEnded`) when its last participant leaves or when no
//!    participant connects within MC's join window (ADR-0010 §3); this step
//!    only covers an MC that died before it could. GC has no participant data,
//!    so it never ends a meeting for inactivity. The safety property is the
//!    MC-liveness predicate: a row whose MC still heartbeats is never ended
//!    here, WHATEVER its `health_status` (draining, degraded, re-registered),
//!    so no live meeting can be re-assigned to a second MC by this job. A
//!    notify lost while MC stays alive is not this job's case either: the
//!    notify is incarnation-fenced, so MC retries it until GC answers
//!    (ADR-0010 §3).
//! 2. Hard-deletes ended assignments older than `GC_RETENTION_DAYS` -- the
//!    only remover of soft-ended rows.
//!
//! # Graceful Shutdown
//!
//! The task supports graceful shutdown via a cancellation token. When the token
//! is cancelled, the task completes its current iteration and exits cleanly.

pub use crate::config::AssignmentCleanupConfig;
use crate::repositories::MeetingAssignmentsRepository;
use sqlx::PgPool;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::{info, instrument, warn};

/// Start the assignment cleanup background task.
///
/// This task runs in a loop, performing cleanup operations at the configured interval.
/// It will exit gracefully when the cancellation token is triggered.
///
/// # Arguments
///
/// * `pool` - Database connection pool
/// * `config` - Cleanup configuration
/// * `mc_staleness` - MC liveness threshold (`MC_STALENESS_THRESHOLD_SECONDS`)
/// * `cancel_token` - Token for graceful shutdown
///
/// # Operations
///
/// Each iteration performs:
/// 1. Backstop: end rows whose MC is no longer alive, older than `inactivity_hours`
/// 2. Hard-delete old ended assignments (older than `retention_days`)
///
/// # Returns
///
/// Returns when the cancellation token is triggered.
#[instrument(skip_all, name = "gc.task.assignment_cleanup")]
pub async fn start_assignment_cleanup(
    pool: PgPool,
    config: AssignmentCleanupConfig,
    mc_staleness: Duration,
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
                run_cleanup(&pool, &config, mc_staleness).await;
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
pub(crate) async fn run_cleanup(
    pool: &PgPool,
    config: &AssignmentCleanupConfig,
    mc_staleness: Duration,
) {
    // Step 1: Backstop -- end rows a no-longer-alive MC never reported ended
    // Uses default batch size (None) to let repository use its default
    match MeetingAssignmentsRepository::end_stale_assignments(
        pool,
        config.inactivity_hours,
        mc_staleness,
        None,
    )
    .await
    {
        Ok(count) => {
            if count > 0 {
                warn!(
                    target: "gc.task.assignment_cleanup",
                    stale_count = count,
                    inactivity_hours = config.inactivity_hours,
                    mc_staleness_seconds = mc_staleness.as_secs(),
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

/// Integration tests for assignment cleanup task requiring database.
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::repositories::MeetingControllersRepository;
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
        let handle = tokio::spawn(start_assignment_cleanup(
            pool,
            config,
            Duration::from_secs(30),
            cancel_token,
        ));

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

    /// Register an MC with the given status and heartbeat age, and give it one
    /// 2-hour-old live assignment (well past the 1-hour age floor).
    async fn mc_with_old_row(pool: &PgPool, id: &str, status: &str, heartbeat_age_secs: i64) {
        MeetingControllersRepository::register_mc(
            pool,
            id,
            "us-east-1",
            "https://mc:50051",
            Some("https://mc:443"),
            100,
            1000,
        )
        .await
        .expect("Failed to register controller");
        sqlx::query(
            "UPDATE meeting_controllers SET health_status = $2, \
             last_heartbeat_at = NOW() - ($3 || ' seconds')::INTERVAL WHERE controller_id = $1",
        )
        .bind(id)
        .bind(status)
        .bind(heartbeat_age_secs.to_string())
        .execute(pool)
        .await
        .expect("Failed to set MC status/heartbeat");
        sqlx::query(
            r#"
            INSERT INTO meeting_assignments
                (meeting_id, meeting_controller_id, region, assigned_by_gc_id, assigned_at, assignment_id)
            VALUES ($1, $1, 'us-east-1', 'gc-test', NOW() - INTERVAL '2 hours', gen_random_uuid())
            "#,
        )
        .bind(id)
        .execute(pool)
        .await
        .expect("Failed to create assignment");
    }

    /// THE BACKSTOP'S SAFETY PROPERTY: it ends a row iff the row's MC has
    /// stopped heartbeating, whatever the MC's `health_status`. A draining,
    /// degraded, re-registered (`pending`) or even self-reported-unhealthy MC
    /// that still heartbeats holds its meetings; ending its row would let the
    /// next join re-assign a live meeting to a second MC (split meeting). A
    /// stale MC is ended even if its last status read `healthy` (or
    /// `draining`, which `mark_stale_controllers_unhealthy` never re-marks).
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_backstop_ends_iff_mc_heartbeat_is_stale(pool: PgPool) {
        let staleness = Duration::from_secs(30);
        let statuses = ["healthy", "degraded", "draining", "pending", "unhealthy"];
        for status in statuses {
            mc_with_old_row(&pool, &format!("live-{status}"), status, 1).await;
            mc_with_old_row(&pool, &format!("dead-{status}"), status, 100).await;
        }

        let config = AssignmentCleanupConfig {
            check_interval_seconds: 3600,
            inactivity_hours: 1,
            retention_days: 7,
        };
        run_cleanup(&pool, &config, staleness).await;

        for status in statuses {
            for (prefix, want_ended) in [("live", false), ("dead", true)] {
                let id = format!("{prefix}-{status}");
                let ended: (Option<chrono::DateTime<chrono::Utc>>,) = sqlx::query_as(
                    "SELECT ended_at FROM meeting_assignments WHERE meeting_id = $1",
                )
                .bind(&id)
                .fetch_one(&pool)
                .await
                .expect("Failed to get assignment");
                assert_eq!(
                    ended.0.is_some(),
                    want_ended,
                    "{id}: backstop must end the row iff the MC's heartbeat is stale"
                );
            }
        }
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
            INSERT INTO meeting_assignments (meeting_id, meeting_controller_id, region, assigned_by_gc_id, assigned_at, ended_at, assignment_id)
            VALUES ($1, $2, $3, $4, NOW() - INTERVAL '10 days', NOW() - INTERVAL '10 days', gen_random_uuid())
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

        run_cleanup(&pool, &config, Duration::from_secs(30)).await;

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
            INSERT INTO meeting_assignments (meeting_id, meeting_controller_id, region, assigned_by_gc_id, assigned_at, ended_at, assignment_id)
            VALUES ($1, $2, $3, $4, NOW() - INTERVAL '1 day', NOW() - INTERVAL '1 day', gen_random_uuid())
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

        run_cleanup(&pool, &config, Duration::from_secs(30)).await;

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
