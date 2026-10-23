//! P2 Resilience Tests
//!
//! NetworkPolicy enforcement tests using canary pods.

#![cfg(feature = "resilience")]

use env_tests::canary::{CanaryConfig, CanaryPod};
use serial_test::serial;

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
