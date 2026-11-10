//! Eventual consistency helpers for timing-dependent tests.
//!
//! This module provides retry logic with exponential backoff for tests that depend on
//! asynchronous operations like metrics scraping or log aggregation.

use std::future::Future;
use std::time::Duration;
use tokio::time::sleep;

/// Categories of eventual consistency with documented SLAs.
///
/// Each category has a maximum timeout based on the expected consistency time
/// in the cluster environment.
#[derive(Debug, Clone, Copy)]
pub enum ConsistencyCategory {
    /// Prometheus metrics scraping: 30s, a FAILURE-ONLY ceiling. It must cover
    /// two scrapes of the SLOWEST job its callers read — callers span the
    /// service jobs, `otel-collector` and the global-interval jobs, whose
    /// cadences are the per-job and global `scrape_interval` keys in
    /// `infra/kubernetes/observability/prometheus.yml` — plus ingestion lag.
    /// It is deliberately not tightened when a job's cadence drops: a ceiling
    /// is spent only when things are slow, so shrinking it buys nothing on a
    /// green run.
    MetricsScrape,

    /// Loki log aggregation (~3x 10s flush interval = 45s, generous for ingestion lag)
    LogAggregation,

    /// Cross-replica synchronization (2x 5s expected = 10s)
    ReplicaSync,

    /// Kubernetes resource updates (2x 30s expected = 60s)
    K8sResourceUpdate,
}

impl ConsistencyCategory {
    /// Get the maximum timeout for this consistency category.
    pub fn timeout(&self) -> Duration {
        match self {
            ConsistencyCategory::MetricsScrape => Duration::from_secs(30),
            ConsistencyCategory::LogAggregation => Duration::from_secs(45),
            ConsistencyCategory::ReplicaSync => Duration::from_secs(10),
            ConsistencyCategory::K8sResourceUpdate => Duration::from_secs(60),
        }
    }

    /// Get the initial retry delay for exponential backoff.
    fn initial_delay(&self) -> Duration {
        Duration::from_millis(500)
    }
}

/// Assert that a condition becomes true within the timeout for the given consistency category.
///
/// Uses exponential backoff with the following strategy:
/// - Initial delay: 500ms
/// - Exponential multiplier: 2x
/// - Maximum attempts: Until timeout is reached
///
/// # Example
///
/// ```no_run
/// use env_tests::eventual::{assert_eventually, ConsistencyCategory};
///
/// #[tokio::test]
/// async fn test_metric_appears() {
///     assert_eventually(
///         ConsistencyCategory::MetricsScrape,
///         || async {
///             // Check if metric exists
///             fetch_metric().await.is_ok()
///         }
///     ).await.expect("Metric should appear within timeout");
/// }
/// ```
pub async fn assert_eventually<F, Fut>(
    category: ConsistencyCategory,
    mut condition: F,
) -> Result<(), String>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    let timeout = category.timeout();
    poll_kernel(timeout, category.initial_delay(), None, || {
        let fut = condition();
        async move {
            if fut.await {
                Ok(())
            } else {
                Err(format!(
                    "Condition not met within {timeout:?} (category: {category:?})"
                ))
            }
        }
    })
    .await
}

/// Poll `attempt` until it returns `Ok`, within a caller-DERIVED `timeout`
/// (for ceilings computed from live config, which a [`ConsistencyCategory`]
/// cannot express). The same loop as [`assert_eventually`] (500 ms, x2),
/// capped at 5 s so a long ceiling still polls often.
///
/// # Errors
/// The LAST attempt's error, so the failure names what was still wrong.
pub async fn poll_within<T, F, Fut>(timeout: Duration, attempt: F) -> Result<T, String>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, String>>,
{
    poll_kernel(
        timeout,
        Duration::from_millis(500),
        Some(Duration::from_secs(5)),
        attempt,
    )
    .await
}

/// THE backoff loop (both helpers above): try, then sleep `delay` (x2, capped
/// at `cap` and at the time left) until `timeout`; return the last error.
async fn poll_kernel<T, F, Fut>(
    timeout: Duration,
    initial_delay: Duration,
    cap: Option<Duration>,
    mut attempt: F,
) -> Result<T, String>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, String>>,
{
    let start = std::time::Instant::now();
    let mut delay = initial_delay;
    loop {
        let last = match attempt().await {
            Ok(v) => return Ok(v),
            Err(e) => e,
        };
        let elapsed = start.elapsed();
        if elapsed >= timeout {
            return Err(last);
        }
        sleep(delay.min(timeout.saturating_sub(elapsed))).await;
        delay *= 2;
        if let Some(cap) = cap {
            delay = delay.min(cap);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_consistency_category_timeouts() {
        assert_eq!(
            ConsistencyCategory::MetricsScrape.timeout(),
            Duration::from_secs(30)
        );
        assert_eq!(
            ConsistencyCategory::LogAggregation.timeout(),
            Duration::from_secs(45)
        );
        assert_eq!(
            ConsistencyCategory::ReplicaSync.timeout(),
            Duration::from_secs(10)
        );
        assert_eq!(
            ConsistencyCategory::K8sResourceUpdate.timeout(),
            Duration::from_secs(60)
        );
    }

    #[tokio::test]
    async fn test_assert_eventually_succeeds_immediately() {
        let result = assert_eventually(ConsistencyCategory::ReplicaSync, || async { true }).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_assert_eventually_succeeds_after_retry() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_clone = attempts.clone();

        let result = assert_eventually(ConsistencyCategory::ReplicaSync, move || {
            let attempts = attempts_clone.clone();
            async move {
                let count = attempts.fetch_add(1, Ordering::SeqCst) + 1;
                count >= 2
            }
        })
        .await;
        assert!(result.is_ok());
        assert!(attempts.load(Ordering::SeqCst) >= 2);
    }

    #[tokio::test]
    async fn test_assert_eventually_fails_on_timeout() {
        let result = assert_eventually(ConsistencyCategory::ReplicaSync, || async { false }).await;
        let err = result.expect_err("Should return error on timeout");
        assert!(err.contains("not met within"));
    }
}
