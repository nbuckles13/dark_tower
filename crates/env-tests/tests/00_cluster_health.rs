//! P0 Smoke Tests: Cluster Health
//!
//! These tests validate that the local kind cluster and port-forwards are running
//! correctly. All other tests depend on these passing.

#![cfg(feature = "smoke")]

use env_tests::cluster::ClusterConnection;
use std::process::Command;

/// Helper to create a cluster connection for tests.
async fn cluster() -> ClusterConnection {
    ClusterConnection::new()
        .await
        .expect("Failed to connect to cluster - ensure port-forwards are running")
}

#[tokio::test]
async fn test_ac_health_endpoint() {
    let cluster = cluster().await;

    cluster
        .check_ac_health()
        .await
        .expect("AC /health endpoint should respond with 200 OK");
}

#[tokio::test]
async fn test_ac_ready_endpoint() {
    let cluster = cluster().await;

    cluster
        .check_ac_ready()
        .await
        .expect("AC /ready endpoint should respond with 200 OK");
}

#[tokio::test]
async fn test_prometheus_reachable() {
    let cluster = cluster().await;

    cluster
        .check_prometheus()
        .await
        .expect("Prometheus should be reachable on localhost:9090");
}

#[tokio::test]
async fn test_grafana_reachable() {
    let cluster = cluster().await;

    cluster
        .check_grafana()
        .await
        .expect("Grafana should be reachable on localhost:3000");
}

#[tokio::test]
async fn test_secrets_not_in_env_vars() {
    // Use kubectl to check pod environment variables don't contain secrets
    let output = Command::new("kubectl")
        .args([
            "get",
            "pods",
            "-n",
            "default",
            "-l",
            "app=ac-service",
            "-o",
            "jsonpath={.items[*].spec.containers[*].env[*].value}",
        ])
        .output();

    let output = output.unwrap_or_else(|e| {
        panic!(
            "kubectl not available - cannot verify secret leak protection. \
             env-tests require kubectl to be installed and configured: {}",
            e
        )
    });

    assert!(
        output.status.success(),
        "kubectl command failed - cannot verify secret leak protection: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let env_vars = String::from_utf8_lossy(&output.stdout);

    // Check for common secret patterns
    assert!(
        !env_vars.contains("password"),
        "Environment variables should not contain plaintext passwords"
    );
    assert!(
        !env_vars.contains("secret"),
        "Environment variables should not contain plaintext secrets"
    );
    assert!(
        !env_vars.contains("DATABASE_URL=postgresql://"),
        "Environment variables should not contain connection strings with credentials"
    );
}

#[tokio::test]
async fn test_secrets_not_in_logs() {
    // Use kubectl to sample recent logs and check for leaked credentials
    let output = Command::new("kubectl")
        .args([
            "logs",
            "-n",
            "default",
            "-l",
            "app=ac-service",
            "--tail=100",
        ])
        .output();

    let output = output.unwrap_or_else(|e| {
        panic!(
            "kubectl not available - cannot verify secret leak protection. \
             env-tests require kubectl to be installed and configured: {}",
            e
        )
    });

    assert!(
        output.status.success(),
        "kubectl logs command failed - cannot verify secret leak protection: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let logs = String::from_utf8_lossy(&output.stdout);

    // Check for common credential patterns
    // Note: These patterns are heuristic - they may have false positives/negatives
    assert!(
        !logs.contains("client_secret"),
        "Logs should not contain client_secret field values"
    );
    assert!(
        !logs.contains("password="),
        "Logs should not contain password= patterns"
    );

    // JWT tokens are expected in some log contexts (e.g., "issued token"),
    // but shouldn't appear as raw bearer tokens
    let bearer_count = logs.matches("Bearer ey").count();
    assert!(
        bearer_count == 0,
        "Logs should not contain raw Bearer tokens (found {} instances)",
        bearer_count
    );
}

/// The dev OTel collector (R-59) must be Ready. This asserts the cluster-setup
/// readiness gate is correct: the `app=otel-collector` selector here MUST match
/// (1) the Deployment pod-template label and (2) the `kubectl wait` selector in
/// `deploy_otel_collector()` (infra/kind/scripts/setup.sh). The three-way match
/// is the invariant — if any one is typo'd/renamed, this test fails rather than
/// the gate silently passing on zero pods and the breakage surfacing later as
/// CrashLooping services once R-55 makes the four services depend on the
/// collector.
///
/// `kubectl wait` on a selector matching ZERO pods returns "no matching
/// resources found" immediately (regardless of `--timeout`), so a stale/typo'd
/// selector is still caught here, not vacuously passed. The `--timeout=10s` only
/// governs how long an EXISTING matched pod is given to reach Ready — a small
/// tolerance matching this suite's other health checks (cluster.rs uses 5-10s
/// probe timeouts), so a brief readiness blip when smoke tests start doesn't
/// false-fail even though setup.sh has already Ready-gated the collector.
///
/// Namespace is `dark-tower` (where the collector actually runs) — NOT `default`.
#[tokio::test]
async fn test_otel_collector_ready() {
    let output = Command::new("kubectl")
        .args([
            "wait",
            "--for=condition=Ready",
            "pod",
            "-l",
            "app=otel-collector",
            "-n",
            "dark-tower",
            "--timeout=10s",
        ])
        .output();

    let output = output.unwrap_or_else(|e| {
        panic!(
            "kubectl not available - cannot verify OTel collector readiness. \
             env-tests require kubectl to be installed and configured: {}",
            e
        )
    });

    assert!(
        output.status.success(),
        "OTel collector pod (-l app=otel-collector -n dark-tower) is not Ready - \
         the deploy_otel_collector readiness gate may be misconfigured (selector/namespace \
         mismatch) or the collector failed to start: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
