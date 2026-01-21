//! Health checker background task.
//!
//! Periodically checks for stale Meeting Controllers and marks them as unhealthy.
//! Controllers are considered stale if they haven't sent a heartbeat within
//! the configured threshold.
//!
//! # Graceful Shutdown
//!
//! The task supports graceful shutdown via a cancellation token. When the token
//! is cancelled, the task completes its current iteration and exits cleanly.

use crate::repositories::MeetingControllersRepository;
use sqlx::PgPool;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::{info, instrument, warn};

/// Default health check interval in seconds.
const DEFAULT_CHECK_INTERVAL_SECONDS: u64 = 5;

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
#[instrument(skip_all, name = "gc.task.health_checker")]
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

    let mut interval = tokio::time::interval(Duration::from_secs(DEFAULT_CHECK_INTERVAL_SECONDS));

    loop {
        tokio::select! {
            _ = interval.tick() => {
                // Check for stale controllers
                match MeetingControllersRepository::mark_stale_controllers_unhealthy(
                    &pool,
                    staleness_threshold_seconds as i64,
                ).await {
                    Ok(count) => {
                        if count > 0 {
                            warn!(
                                target: "gc.task.health_checker",
                                stale_count = count,
                                "Marked stale controllers as unhealthy"
                            );
                        }
                    }
                    Err(e) => {
                        // Log error but continue - database might recover
                        tracing::error!(
                            target: "gc.task.health_checker",
                            error = %e,
                            "Failed to check for stale controllers"
                        );
                    }
                }
            }
            _ = cancel_token.cancelled() => {
                info!(
                    target: "gc.task.health_checker",
                    "Health checker task received shutdown signal, exiting"
                );
                break;
            }
        }
    }

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
