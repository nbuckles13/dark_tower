//! Resilience tests: NetworkPolicy enforcement (canary pods), and the MH
//! pool / process-injector smoke legs that prove the story 4 resilience
//! fixture end to end (ADR-0039 §9, R-39).
//!
//! # Isolation convention (EVERY test in this binary is `#[serial]`)
//!
//! Story 4 has no per-test isolation of the MH pool; it relies on
//! serialization. Every destructive or pool-mutating env-test (kill, freeze,
//! drain, up) lives in THIS binary, and every test here carries `#[serial]`,
//! because `serial_test` only serializes MARKED tests: one unmarked test could
//! run beside a frozen or killed MH. `cargo test` runs binaries one at a time
//! and Playwright runs `workers: 1`, so within one Layer-7 run a killed,
//! frozen or drained MH affects only the test that did it. The rule is checked
//! at Layer 4 by `env_tests::resilience_hygiene` (token-level, for this file
//! AND any other binary that calls a pool-mutating fixture API, e.g. 44).
//!
//! Concurrent runs sharing one cluster are NOT isolated: they fail loudly with
//! `foreign-mh-activity` (`MhPool::assert_baseline`, before and after every
//! leg), never pass falsely.
//!
//! Every pool leg uses only FIXTURE-OWNED MHs (owner = the run token), tears
//! down explicitly and asserts it (`MhPool::run_leg`: `teardown_and_settle`,
//! which also checks nothing is left owned by the run), and re-asserts the
//! baseline. A
//! panicking leg's `MhPool` Drop prints the exact reclaim command.
//!
//! Edge-sum (concurrent Σ P×(N+M)): 0

#![cfg(feature = "resilience")]

use env_tests::canary::{CanaryConfig, CanaryPod};
use env_tests::fixtures::mh_address::assert_addresses_unique;
use env_tests::fixtures::mh_pool::MhPool;
use env_tests::fixtures::PrometheusClient;
use serial_test::serial;

// ============================================================================
// MH pool fixture + injector smoke legs (T-1)
// ============================================================================

/// The pool fixture end to end on the live cluster: `up` (returns at MH
/// /ready) -> Prometheus scrapes it by pod IP -> explicit, asserted `drain`
/// -> owner teardown leaves nothing owned by the run -> baseline restored.
/// No assertion reads the exiting MH's series (drain outcomes are MH-free).
#[tokio::test]
#[serial]
async fn mh_pool_fixture_up_scrape_drain_restores_baseline() {
    MhPool::run_leg(|pool, prom_url| async move {
        let prom = PrometheusClient::new(&prom_url);
        let slot = pool.up_scraped(&prom).await;
        let listed = {
            let s = slot.clone();
            tokio::task::spawn_blocking(move || s.entry())
                .await
                .expect("list task")
        };
        assert_eq!(
            listed.owner.as_deref(),
            Some(pool.token()),
            "up's MH is run-owned"
        );
        assert_eq!(listed.pod_uid.as_deref(), Some(slot.pod_uid.as_str()));
        tokio::task::spawn_blocking(move || slot.drain())
            .await
            .unwrap_or_else(|e| std::panic::resume_unwind(e.into_panic()));
    })
    .await;
}

/// The injector end to end on a fixture-owned MH: freeze (mh.sh observes
/// `/proc/<pid>/stat` = T: the freeze TOOK EFFECT) -> resume inside the live
/// liveness budget with restart_count and pod_uid unchanged -> kill with an
/// in-place restart (same pod_uid, restart_count + 1) -> scrape re-waited ->
/// teardown -> the R-28 address invariant still holds.
#[tokio::test]
#[serial]
async fn mh_injector_freeze_resume_then_kill_restarts_in_place() {
    MhPool::run_leg(|pool, prom_url| async move {
        let prom = PrometheusClient::new(&prom_url);
        let slot = pool.up_scraped(&prom).await;
        let before = slot.restart_count;

        let frozen = tokio::task::spawn_blocking(move || slot.freeze())
            .await
            .unwrap_or_else(|e| std::panic::resume_unwind(e.into_panic()));
        // The CONT call costs about what the STOP call did (same mechanism),
        // so resuming in time needs 2 x stop_call inside the budget.
        assert!(
            2 * frozen.stop_call() < frozen.budget(),
            "the STOP call took {:?}; with a CONT of similar cost the {:?} liveness budget \
             cannot be met",
            frozen.stop_call(),
            frozen.budget()
        );
        let mut slot = frozen.resume(&prom).await;
        assert_eq!(slot.restart_count, before, "freeze/resume never restarts");

        let uid = slot.pod_uid.clone();
        slot.kill(&prom).await;
        assert_eq!(
            slot.restart_count,
            before + 1,
            "KILL restarts the container in place"
        );
        assert_eq!(
            slot.pod_uid, uid,
            "same pod: an in-place restart, never a pod delete"
        );
        // R-28 on the RESTARTED incarnation (its fresh startup log and its slot
        // address) while it is still in the pool; the gather settles all pods.
        assert_addresses_unique().await;
    })
    .await;
}

// ============================================================================
// NetworkPolicy Tests
// ============================================================================

/// Test that pods with allowed labels can reach the AC service.
/// This is a POSITIVE test to verify NetworkPolicy allows expected traffic.
///
/// The canary uses `app=gc-service` label to match the NetworkPolicy's
/// ingress rules, which allow traffic from gc-service pods.
///
/// If this test fails, it indicates either:
/// 1. The AC service is not running or not healthy
/// 2. The NetworkPolicy ingress rules don't allow gc-service traffic
/// 3. The canary pod deployment failed
#[tokio::test]
#[serial]
async fn test_same_namespace_connectivity() {
    // Deploy a canary pod with labels that match the NetworkPolicy ingress rules
    // The AC NetworkPolicy allows traffic from pods with app=gc-service
    let config = CanaryConfig {
        labels: "app=gc-service,test=network-policy".to_string(),
        ..CanaryConfig::default()
    };

    let canary = CanaryPod::deploy_with_config("dark-tower", config)
        .await
        .expect("Failed to deploy canary pod in dark-tower namespace");

    // Test connectivity to AC service health endpoint
    // Using service DNS name which resolves within the cluster
    let target_url = "http://ac-service:8082/health";
    let can_reach = canary.can_reach(target_url).await;

    // Cleanup before asserting (ensure cleanup even on failure)
    let cleanup_result = canary.cleanup().await;

    // Now assert
    assert!(
        can_reach,
        "Canary pod with app=gc-service label should be able to reach AC service at {}. \
         This validates NetworkPolicy allows expected ingress traffic. \
         If this fails, check: 1) AC service is running, 2) NetworkPolicy ingress rules, \
         3) Pod labels match NetworkPolicy selectors.",
        target_url
    );

    // A failed cleanup leaks a canary pod into the next run: fail loudly,
    // after the primary assertion so it never hides that result.
    if let Err(e) = cleanup_result {
        panic!("canary cleanup failed (pod leaked into dark-tower): {e}");
    }
}

/// Test that NetworkPolicy blocks cross-namespace traffic to AC service.
/// This is a NEGATIVE test to verify NetworkPolicy enforcement.
///
/// Test interpretation:
/// - If ONLY this test passes (positive fails): NetworkPolicy is blocking ALL traffic (misconfigured)
/// - If BOTH tests pass: NetworkPolicy is NOT enforced (security gap!)
/// - If positive passes, negative fails: NetworkPolicy is working correctly
#[tokio::test]
#[serial]
async fn test_network_policy_blocks_cross_namespace() {
    // Use a unique test namespace to ensure isolation
    let test_namespace = "canary-test-isolated";

    // Deploy a canary pod in a DIFFERENT namespace from AC service
    let canary = CanaryPod::deploy(test_namespace).await.unwrap_or_else(|e| {
        panic!(
            "Failed to deploy canary pod in {}: {}. \
                 Ensure kubectl is configured and has permissions to create namespaces/pods.",
            test_namespace, e
        )
    });

    // Try to reach AC service from the different namespace
    // Using fully-qualified service DNS: <service>.<namespace>.svc.cluster.local
    let target_url = "http://ac-service.dark-tower.svc.cluster.local:8082/health";
    let can_reach = canary.can_reach(target_url).await;

    // Cleanup before asserting (ensure cleanup even on failure)
    let cleanup_result = canary.cleanup().await;
    let namespace_result = cleanup_test_namespace(test_namespace);

    // Assert that cross-namespace traffic is blocked
    assert!(
        !can_reach,
        "Canary pod in '{}' namespace should NOT be able to reach AC service at {}. \
         This indicates NetworkPolicy is NOT enforced - SECURITY GAP! \
         Ensure NetworkPolicy is deployed in the dark-tower namespace to restrict ingress. \
         Expected: Connection blocked/timeout. Got: Connection succeeded.",
        test_namespace, target_url
    );

    // A failed cleanup leaks a pod or namespace into the next run, which then
    // fails confusingly: fail loudly, after the primary assertion so it never
    // hides that result.
    if let Err(e) = cleanup_result {
        panic!("canary cleanup failed (pod leaked into {test_namespace}): {e}");
    }
    if let Err(e) = namespace_result {
        panic!("test namespace cleanup failed ({test_namespace} leaked): {e}");
    }
}

/// Test that NetworkPolicy blocks cross-namespace traffic to GC's HTTP port.
///
/// GC's HTTP port is externally reachable (the ingress gateway in production,
/// the NodePort in Kind), which once tempted an admit-all `from: []` rule on
/// it, and that admitted every pod in every namespace too. The Kind overlay's
/// NodePort rule admits only NON-pod sources, so a pod in another namespace
/// must still be blocked. GC's allowed external path is exercised by every
/// env-test that calls GC through `ENV_TEST_GC_URL` (the NodePort).
#[tokio::test]
#[serial]
async fn test_network_policy_blocks_cross_namespace_to_gc() {
    let test_namespace = "canary-test-isolated";

    let canary = CanaryPod::deploy(test_namespace).await.unwrap_or_else(|e| {
        panic!(
            "Failed to deploy canary pod in {}: {}. \
                 Ensure kubectl is configured and has permissions to create namespaces/pods.",
            test_namespace, e
        )
    });

    let target_url = "http://gc-service.dark-tower.svc.cluster.local:8080/health";
    let can_reach = canary.can_reach(target_url).await;

    // Cleanup before asserting (ensure cleanup even on failure)
    let cleanup_result = canary.cleanup().await;
    let namespace_result = cleanup_test_namespace(test_namespace);

    assert!(
        !can_reach,
        "Canary pod in '{}' namespace should NOT be able to reach GC service at {}. \
         GC's NetworkPolicy admits external clients only via the ingress gateway (prod) \
         or non-pod sources (Kind overlay); a pod in another namespace reaching it is a \
         SECURITY GAP. Expected: Connection blocked/timeout. Got: Connection succeeded.",
        test_namespace, target_url
    );

    // A failed cleanup leaks a pod or namespace into the next run: fail
    // loudly, after the primary assertion so it never hides that result.
    if let Err(e) = cleanup_result {
        panic!("canary cleanup failed (pod leaked into {test_namespace}): {e}");
    }
    if let Err(e) = namespace_result {
        panic!("test namespace cleanup failed ({test_namespace} leaked): {e}");
    }
}

/// Delete a test namespace created by a NetworkPolicy test. Errors are
/// returned, never discarded: a leaked namespace breaks the next run.
fn cleanup_test_namespace(namespace: &str) -> Result<(), String> {
    use std::process::Command;

    // Only cleanup namespaces we created for testing
    if !namespace.starts_with("canary-test") {
        return Err(format!(
            "refusing to delete '{namespace}': not a canary-test namespace"
        ));
    }

    let output = Command::new("kubectl")
        .args(["delete", "namespace", namespace, "--ignore-not-found=true"])
        .output()
        .map_err(|e| format!("kubectl could not run: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "kubectl delete namespace exited {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}
