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
    /// Prometheus metrics scraping (2x 15s scrape interval = 30s)
    MetricsScrape,

    /// Loki log aggregation (2x 10s flush interval = 20s)
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
            ConsistencyCategory::LogAggregation => Duration::from_secs(20),
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
    let mut delay = category.initial_delay();
    let start = std::time::Instant::now();

    loop {
        if condition().await {
            return Ok(());
        }

        let elapsed = start.elapsed();
        if elapsed >= timeout {
            return Err(format!(
                "Condition not met within {:?} (category: {:?})",
                timeout, category
            ));
        }

        // Sleep for current delay
        sleep(delay).await;

        // Exponential backoff with 2x multiplier
        delay *= 2;

        // Cap delay at remaining time
        let remaining = timeout.saturating_sub(elapsed);
        if delay > remaining {
            delay = remaining;
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
            Duration::from_secs(20)
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
