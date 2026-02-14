//! Generic health checker background task.
//!
//! Provides a reusable health checker loop that periodically calls a staleness-check
//! function and marks stale entities as unhealthy. Used by both the MC health checker
//! and MH health checker via thin wrapper functions.
//!
//! # Graceful Shutdown
//!
//! The task supports graceful shutdown via a cancellation token. When the token
//! is cancelled, the task completes its current iteration and exits cleanly.

use crate::errors::GcError;
use sqlx::PgPool;
use std::future::Future;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

/// Default health check interval in seconds.
pub const DEFAULT_CHECK_INTERVAL_SECONDS: u64 = 5;

/// Run the generic health checker loop.
///
/// Periodically calls `mark_stale_fn` to check for stale entities and mark them
/// as unhealthy. Exits when the cancellation token is triggered.
///
/// Callers should chain `.instrument(tracing::info_span!(...))` on the returned
/// future to set the tracing span name, and emit startup/shutdown lifecycle logs
/// with literal `target:` values.
///
/// # Arguments
///
/// * `pool` - Database connection pool
/// * `staleness_threshold_seconds` - Seconds since last heartbeat to consider stale
/// * `cancel_token` - Token for graceful shutdown
/// * `entity_name` - Human-readable entity name for log messages (e.g., "controllers", "handlers")
/// * `mark_stale_fn` - Closure that marks stale entities as unhealthy.
///   Takes an owned `PgPool` (cheap to clone) and staleness threshold.
pub async fn start_generic_health_checker<F, Fut>(
    pool: PgPool,
    staleness_threshold_seconds: u64,
    cancel_token: CancellationToken,
    entity_name: &'static str,
    mark_stale_fn: F,
) where
    F: Fn(PgPool, i64) -> Fut + Send,
    Fut: Future<Output = Result<u64, GcError>> + Send,
{
    let mut interval = tokio::time::interval(Duration::from_secs(DEFAULT_CHECK_INTERVAL_SECONDS));

    loop {
        tokio::select! {
            _ = interval.tick() => {
                match mark_stale_fn(pool.clone(), staleness_threshold_seconds as i64).await {
                    Ok(count) => {
                        if count > 0 {
                            warn!(
                                entity = entity_name,
                                stale_count = count,
                                "Marked stale {} as unhealthy", entity_name
                            );
                        }
                    }
                    Err(e) => {
                        tracing::error!(
                            entity = entity_name,
                            error = %e,
                            "Failed to check for stale {}", entity_name
                        );
                    }
                }
            }
            _ = cancel_token.cancelled() => {
                info!(
                    entity = entity_name,
                    "Health checker task received shutdown signal, exiting"
                );
                break;
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_default_check_interval() {
        assert_eq!(DEFAULT_CHECK_INTERVAL_SECONDS, 5);
    }
}
