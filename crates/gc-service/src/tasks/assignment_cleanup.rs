//! Assignment cleanup background task.
//!
//! Periodically cleans up meeting assignments:
//! 1. BACKSTOP: soft-ends assignments MC never reported ended -- rows whose MC
//!    is no longer healthy (heartbeat stale) and that are older than
//!    `GC_INACTIVITY_HOURS`. MC itself ends a meeting and notifies GC
//!    (`NotifyMeetingEnded`) when its last participant leaves or when no
//!    participant connects within MC's join window (ADR-0010 §3); this step
//!    only covers an MC that died before it could. GC has no participant data,
//!    so it never ends a meeting for inactivity. The safety property is the
//!    MC-liveness predicate: a row whose MC still heartbeats is never ended
//!    here, so no live meeting can be re-assigned to a second MC by this job.
//!    NOT covered: a notify lost while MC stays healthy (docs/TODO.md,
//!    "`NotifyMeetingEnded` is not incarnation-safe", residual 1).
//! 2. Hard-deletes ended assignments older than `GC_RETENTION_DAYS` -- the
//!    only remover of soft-ended rows.
//!
//! # Graceful Shutdown
//!
//! The task supports graceful shutdown via a cancellation token. When the token
//! is cancelled, the task completes its current iteration and exits cleanly.

use crate::config::ConfigError;
use crate::repositories::MeetingAssignmentsRepository;
use sqlx::PgPool;
use std::collections::HashMap;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::{info, instrument, warn};

/// Default cleanup check interval in seconds (1 hour).
const DEFAULT_CHECK_INTERVAL_SECONDS: u64 = 3600;

/// Default age floor (hours) before the backstop may end an unhealthy MC's row.
const DEFAULT_INACTIVITY_HOURS: i32 = 1;

/// Default retention period in days before hard-deleting assignments.
const DEFAULT_RETENTION_DAYS: i32 = 7;

/// Configuration for assignment cleanup task.
#[derive(Debug, Clone)]
pub struct AssignmentCleanupConfig {
    /// Cleanup check interval in seconds.
    pub check_interval_seconds: u64,
    /// Minimum assignment age (hours) before the backstop may end a row whose
    /// MC is unhealthy. Not an inactivity timer: GC has no participant data.
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
    /// Load config from the process environment. See [`Self::from_vars`].
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_vars(&std::env::vars().collect())
    }

    /// Load config from a variable map.
    ///
    /// - `GC_CLEANUP_INTERVAL_SECONDS` - check interval, >= 1 (default: 3600)
    /// - `GC_INACTIVITY_HOURS` - backstop age floor, >= 1 (default: 1)
    /// - `GC_RETENTION_DAYS` - retention before hard delete, >= 1 (default: 7)
    ///
    /// An UNSET variable takes its default. A SET variable that does not parse,
    /// or is below its minimum, is a startup error: falling back to the default
    /// would mask an operator's typo (CLAUDE.md "fail loudly"). The minimums are
    /// not the backstop's safety property (that is the MC-liveness predicate in
    /// `end_stale_assignments`); they keep the age floor and the interval from
    /// being configured away, and a zero interval would panic
    /// `tokio::time::interval`.
    pub fn from_vars(vars: &HashMap<String, String>) -> Result<Self, ConfigError> {
        Ok(Self {
            check_interval_seconds: parse_at_least(
                vars,
                "GC_CLEANUP_INTERVAL_SECONDS",
                DEFAULT_CHECK_INTERVAL_SECONDS,
                1,
            )?,
            inactivity_hours: parse_at_least(
                vars,
                "GC_INACTIVITY_HOURS",
                DEFAULT_INACTIVITY_HOURS,
                1,
            )?,
            retention_days: parse_at_least(vars, "GC_RETENTION_DAYS", DEFAULT_RETENTION_DAYS, 1)?,
        })
    }
}

/// Parse an OPTIONAL variable: unset -> `default`; set -> must parse and be
/// `>= min`, else `ConfigError::InvalidAssignmentCleanupConfig` naming the key.
fn parse_at_least<T>(
    vars: &HashMap<String, String>,
    key: &str,
    default: T,
    min: T,
) -> Result<T, ConfigError>
where
    T: std::str::FromStr + PartialOrd + std::fmt::Display + Copy,
    T::Err: std::fmt::Display,
{
    let Some(raw) = vars.get(key) else {
        return Ok(default);
    };
    let value = raw.trim().parse::<T>().map_err(|e| {
        ConfigError::InvalidAssignmentCleanupConfig(format!(
            "{key} must be an integer >= {min}, got '{raw}': {e}"
        ))
    })?;
    if value < min {
        return Err(ConfigError::InvalidAssignmentCleanupConfig(format!(
            "{key} must be >= {min}, got {value}"
        )));
    }
    Ok(value)
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
/// 1. Backstop: end rows whose MC is unhealthy, older than `inactivity_hours`
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
    // Step 1: Backstop -- end rows an unhealthy MC never reported ended
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

    fn vars(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    /// The rejection must be the cleanup variant AND name the offending key,
    /// so the operator sees which variable to fix.
    fn assert_rejected(pairs: &[(&str, &str)], key: &str) {
        let err = AssignmentCleanupConfig::from_vars(&vars(pairs))
            .expect_err("invalid value must be rejected, not defaulted");
        assert!(
            matches!(&err, ConfigError::InvalidAssignmentCleanupConfig(msg) if msg.contains(key)),
            "expected InvalidAssignmentCleanupConfig naming {key}, got {err:?}"
        );
    }

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
    fn test_from_vars_with_valid_values() {
        let config = AssignmentCleanupConfig::from_vars(&vars(&[
            ("GC_CLEANUP_INTERVAL_SECONDS", "7200"),
            ("GC_INACTIVITY_HOURS", "2"),
            ("GC_RETENTION_DAYS", "14"),
        ]))
        .unwrap();
        assert_eq!(config.check_interval_seconds, 7200);
        assert_eq!(config.inactivity_hours, 2);
        assert_eq!(config.retention_days, 14);
    }

    #[test]
    fn test_from_vars_accepts_minimums() {
        let config = AssignmentCleanupConfig::from_vars(&vars(&[
            ("GC_CLEANUP_INTERVAL_SECONDS", "1"),
            ("GC_INACTIVITY_HOURS", "1"),
            ("GC_RETENTION_DAYS", "1"),
        ]))
        .unwrap();
        assert_eq!(config.check_interval_seconds, 1);
        assert_eq!(config.inactivity_hours, 1);
        assert_eq!(config.retention_days, 1);
    }

    #[test]
    fn test_from_vars_with_missing_vars_uses_defaults() {
        let config = AssignmentCleanupConfig::from_vars(&HashMap::new()).unwrap();
        assert_eq!(
            config.check_interval_seconds,
            DEFAULT_CHECK_INTERVAL_SECONDS
        );
        assert_eq!(config.inactivity_hours, DEFAULT_INACTIVITY_HOURS);
        assert_eq!(config.retention_days, DEFAULT_RETENTION_DAYS);
    }

    #[test]
    fn test_from_vars_rejects_unparseable_interval() {
        assert_rejected(
            &[("GC_CLEANUP_INTERVAL_SECONDS", "not-a-number")],
            "GC_CLEANUP_INTERVAL_SECONDS",
        );
    }

    #[test]
    fn test_from_vars_rejects_zero_interval() {
        assert_rejected(
            &[("GC_CLEANUP_INTERVAL_SECONDS", "0")],
            "GC_CLEANUP_INTERVAL_SECONDS",
        );
    }

    #[test]
    fn test_from_vars_rejects_unparseable_inactivity_hours() {
        assert_rejected(&[("GC_INACTIVITY_HOURS", "invalid")], "GC_INACTIVITY_HOURS");
    }

    #[test]
    fn test_from_vars_rejects_zero_inactivity_hours() {
        assert_rejected(&[("GC_INACTIVITY_HOURS", "0")], "GC_INACTIVITY_HOURS");
    }

    #[test]
    fn test_from_vars_rejects_negative_inactivity_hours() {
        assert_rejected(&[("GC_INACTIVITY_HOURS", "-1")], "GC_INACTIVITY_HOURS");
    }

    #[test]
    fn test_from_vars_rejects_empty_retention_days() {
        assert_rejected(&[("GC_RETENTION_DAYS", "")], "GC_RETENTION_DAYS");
    }

    #[test]
    fn test_from_vars_rejects_zero_retention_days() {
        assert_rejected(&[("GC_RETENTION_DAYS", "0")], "GC_RETENTION_DAYS");
    }

    /// One bad key fails the whole load even when the others are valid.
    #[test]
    fn test_from_vars_one_bad_key_fails_whole_load() {
        assert_rejected(
            &[
                ("GC_CLEANUP_INTERVAL_SECONDS", "60"),
                ("GC_INACTIVITY_HOURS", "0"),
                ("GC_RETENTION_DAYS", "7"),
            ],
            "GC_INACTIVITY_HOURS",
        );
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

    /// Backstop: a row MC never reported ended, whose MC is no longer healthy
    /// and which is older than the age floor, is ended (soft delete).
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_backstop_ends_rows_of_unhealthy_mc(pool: PgPool) {
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
            "Backstop should end an unhealthy MC's row (ended_at should be set)"
        );
    }

    /// THE BACKSTOP'S SAFETY PROPERTY: a row whose MC is healthy is never
    /// ended, however old. Ending it would let the next join re-assign a live
    /// meeting to a second MC (split meeting). Liveness, not row age, is what
    /// protects it -- the row here is well past the age floor.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_backstop_never_ends_a_healthy_mcs_row(pool: PgPool) {
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
            "Backstop must never end a healthy MC's row"
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
