//! MH health checker background task.
//!
//! Every tick (ADR-0039 §10), in this order:
//! 1. **sweep** — rows with no load report within the MH staleness bound,
//!    draining included, are marked unhealthy;
//! 2. **retention** — rows with no load report within the registry retention
//!    are deleted (rows are per incarnation, so this bounds the table);
//! 3. **availability** — `gc_mh_available{region, state}` is recomputed and set
//!    from the registry, so a row just swept reads `unhealthy` and a row just
//!    deleted is gone from every bucket.
//!
//! Each step runs regardless of the others' outcome. A retention or
//! availability failure is logged here at error with a literal `step` field; a
//! sweep failure is returned and logged once by the generic loop. Every failing
//! step's `gc_db_queries_total` error series is recorded by the repository.
//!
//! The loop itself is the generic health checker; the MH closure captures the
//! `MhRegistryConfig` so the bound never passes through a unit-less parameter.
//!
//! # Graceful Shutdown
//!
//! The task supports graceful shutdown via a cancellation token. When the token
//! is cancelled, the task completes its current iteration and exits cleanly.

use crate::config::MhRegistryConfig;
use crate::errors::GcError;
use crate::observability::metrics;
use crate::repositories::MediaHandlersRepository;
use crate::tasks::generic_health_checker::{
    start_generic_health_checker, DEFAULT_CHECK_INTERVAL_SECONDS,
};
use sqlx::PgPool;
use tokio_util::sync::CancellationToken;
use tracing::{info, Instrument};

/// Run one MH registry tick: sweep, then retention, then availability.
///
/// Returns the sweep's result (the number of rows marked unhealthy) for the
/// generic loop's logging; retention and availability failures are logged
/// here and do not short-circuit.
pub async fn mh_registry_tick(
    pool: &PgPool,
    registry: &MhRegistryConfig,
    own_region: &str,
) -> Result<u64, GcError> {
    // A sweep failure is returned (not logged here): the generic loop logs
    // every `Err` once ("Failed to check for stale handlers").
    let swept =
        MediaHandlersRepository::mark_stale_handlers_unhealthy(pool, registry.staleness_bound())
            .await;

    if let Err(e) =
        MediaHandlersRepository::delete_expired_handlers(pool, registry.retention()).await
    {
        tracing::error!(
            target: "gc.task.mh_health_checker",
            step = "retention",
            error = %e,
            "MH registry tick step failed"
        );
    }

    match MediaHandlersRepository::count_available_handlers(
        pool,
        registry.staleness_bound(),
        own_region,
    )
    .await
    {
        Ok(rows) => metrics::set_mh_available(own_region, &rows),
        Err(e) => {
            // The gauge keeps its previous values; this error log and
            // gc_db_queries_total{operation="count_available_mh",status="error"}
            // are the staleness signal.
            tracing::error!(
                target: "gc.task.mh_health_checker",
                step = "availability",
                error = %e,
                "MH registry tick step failed; gc_mh_available not refreshed"
            );
        }
    }

    swept
}

/// Start the MH health checker background task.
///
/// Runs [`mh_registry_tick`] every 5 seconds until the cancellation token is
/// triggered.
///
/// # Arguments
///
/// * `pool` - Database connection pool
/// * `registry` - MH registry config (staleness bound, retention)
/// * `own_region` - GC's configured region (the `gc_mh_available` own-region bucket)
/// * `cancel_token` - Token for graceful shutdown
pub async fn start_mh_health_checker(
    pool: PgPool,
    registry: MhRegistryConfig,
    own_region: String,
    cancel_token: CancellationToken,
) {
    info!(
        target: "gc.task.mh_health_checker",
        staleness_bound = ?registry.staleness_bound(),
        retention = ?registry.retention(),
        check_interval = DEFAULT_CHECK_INTERVAL_SECONDS,
        "Starting MH health checker task"
    );

    start_generic_health_checker(pool, cancel_token, "handlers", move |pool| {
        let own_region = own_region.clone();
        async move { mh_registry_tick(&pool, &registry, &own_region).await }
    })
    .instrument(tracing::info_span!("gc.task.mh_health_checker"))
    .await;

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
}

// DB-backed tick/sweep tests live in `tests/mh_health_checker_tests.rs`, next
// to the shared fixtures in `tests/common/mh_registry.rs`.
