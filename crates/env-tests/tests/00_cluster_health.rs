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
