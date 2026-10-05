//! P0 Smoke Tests: Cluster Health
//!
//! These tests validate that the local kind cluster and port-forwards are running
//! correctly. All other tests depend on these passing.

#![cfg(feature = "smoke")]

use env_tests::cluster::ClusterConnection;
use env_tests::eventual::{assert_eventually, ConsistencyCategory};
use env_tests::fixtures::auth_client::{TokenRequest, DEV_TEST_CLIENT_ID, DEV_TEST_CLIENT_SECRET};
use env_tests::fixtures::collector::{
    assert_collector_log_has_no_secrets, assert_log_has_no_secret_fields, collector_log,
    OTEL_COLLECTOR_SELECTOR,
};
use env_tests::NAMESPACE;
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

// NAMESPACE is imported from the crate root, not redefined here: both this file
// and 01_mh_deployment_config.rs assert against it, and a per-file copy is the
// exact duplication these probes exist to catch.
//
// Historical note, so the fix is not undone: both credential-leak probes below
// queried `-n default` for as long as this file existed. Nothing has ever run
// there, so `kubectl` returned an empty string, both probes asserted
// `!"".contains("password")`, and the suite's only two runtime credential-leak
// controls passed unconditionally. A control that cannot fail is not a control.

/// Label selector for the AC pods the credential-leak probes below sample.
///
/// A const, not four literals. `assert_pods_exist` only keeps a probe honest if
/// the guard and the probe name the **same** pods, and that has to be the same
/// expression rather than the same spelling. With separate literals the drift is
/// asymmetric: a stale guard queries nothing and panics (safe), but a stale
/// *probe* leaves the guard passing on the old selector while the probe samples
/// an empty set and asserts `!"".contains("password")` — a vacuous green, which
/// is the exact defect `assert_pods_exist` was added to prevent.
const AC_SELECTOR: &str = "app=ac-service";

/// Panics unless the selector matches at least one pod.
///
/// This is what keeps the probes below honest. Correcting the namespace makes
/// them green *today*; this guard is what stops the next namespace or label
/// drift from silently re-disarming them. Without it, "no pods matched" and
/// "pods matched and were clean" are the same green — which is exactly how the
/// `-n default` defect survived undetected.
fn assert_pods_exist(selector: &str) {
    let output = Command::new("kubectl")
        .args([
            "get",
            "pods",
            "-n",
            NAMESPACE,
            "-l",
            selector,
            "-o",
            "jsonpath={.items[*].metadata.name}",
        ])
        .output()
        .unwrap_or_else(|e| {
            panic!(
                "kubectl not available - cannot verify secret leak protection. \
                 env-tests require kubectl to be installed and configured: {}",
                e
            )
        });

    assert!(
        output.status.success(),
        "kubectl failed listing pods -n {} -l {} - cannot verify secret leak \
         protection: {}",
        NAMESPACE,
        selector,
        String::from_utf8_lossy(&output.stderr)
    );

    let names = String::from_utf8_lossy(&output.stdout);
    assert!(
        !names.trim().is_empty(),
        "no pods matched -n {} -l {}: the credential-leak probe would assert \
         against an empty result and pass vacuously. Either the namespace or \
         the label selector has drifted, or the workload is not deployed - \
         fix the selector, do not delete this guard.",
        NAMESPACE,
        selector
    );
}

#[tokio::test]
async fn test_secrets_not_in_env_vars() {
    // Use kubectl to check pod environment variables don't contain secrets
    assert_pods_exist(AC_SELECTOR);

    let output = Command::new("kubectl")
        .args([
            "get",
            "pods",
            "-n",
            NAMESPACE,
            "-l",
            AC_SELECTOR,
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
    // Drive secret- and token-bearing traffic FIRST (dev client credentials →
    // AC, its service token → GC), so both scans below cover it.
    let traffic = drive_authenticated_traffic().await;

    // AC service logs: the same shared secret-field + secret-literal scan as
    // the collector log (one detector, so the two cannot drift), over the
    // window that holds the traffic above.
    assert_pods_exist(AC_SELECTOR);
    let output = Command::new("kubectl")
        .args([
            "logs",
            "-n",
            NAMESPACE,
            "-l",
            AC_SELECTOR,
            "--tail=-1",
            &format!("--since={COLLECTOR_LOG_WINDOW}"),
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
    // In-window positive control: the scan must cover the client-credentials
    // grant driven above, or it passes vacuously on an empty / wrong-selector /
    // out-of-window log. AC logs `SERVICE_TOKEN_ISSUED_MESSAGE`
    // (`ac-service/src/services/token_service.rs`) once per issuance, inside the
    // request span whose `endpoint` is the service-token route. env-tests does
    // not depend on ac-service, so the string is restated here; a rename fails
    // this assertion loudly rather than silently.
    assert!(
        logs.lines()
            .any(|line| line.contains("Service token issued")
                && line.contains("/api/v1/auth/service/token")),
        "positive control: the AC log window has no service-token issuance line for the \
         client-credentials request just driven, so the AC leak scan would scan nothing \
         ({} lines in window)",
        logs.lines().count()
    );
    assert_log_has_no_secret_fields("AC service", &logs, &[DEV_TEST_CLIENT_SECRET]);

    // JWT tokens are expected in some log contexts (e.g., "issued token"),
    // but shouldn't appear as raw bearer tokens
    let bearer_count = logs.matches("Bearer ey").count();
    assert!(
        bearer_count == 0,
        "Logs should not contain raw Bearer tokens (found {} instances)",
        bearer_count
    );

    assert_collector_log_has_no_secrets_after_authenticated_traffic(&traffic).await;
}

/// Token-bearing traffic driven for the leak scans: the fresh trace_id sent on
/// it and the AC-minted service token.
struct AuthenticatedTraffic {
    trace_id: String,
    token: String,
}

/// Authenticate the dev client with its client secret (AC's client-credentials
/// path), then send the AC-minted service token to GC `/api/v1/me` — which
/// validates service tokens — with a fresh `traceparent`. Must get a 2xx so
/// that request's spans are exported into the scanned window.
async fn drive_authenticated_traffic() -> AuthenticatedTraffic {
    let cluster = cluster().await;

    let auth = env_tests::fixtures::AuthClient::new(&cluster.ac_base_url);
    let token = auth
        .issue_token(TokenRequest::client_credentials(
            DEV_TEST_CLIENT_ID,
            DEV_TEST_CLIENT_SECRET,
            "test:all",
        ))
        .await
        .expect("AC should issue the dev client a service token")
        .access_token;

    let trace_id = uuid::Uuid::new_v4().simple().to_string();
    let parent_span_id: String = uuid::Uuid::new_v4()
        .simple()
        .to_string()
        .chars()
        .take(16)
        .collect();
    let gc = env_tests::fixtures::GcClient::new(&cluster.gc_base_url);
    let response = gc
        .http_client()
        .get(format!("{}/api/v1/me", gc.base_url()))
        .header("Authorization", format!("Bearer {token}"))
        .header("traceparent", format!("00-{trace_id}-{parent_span_id}-01"))
        .send()
        .await
        .expect("authenticated GC request should send");
    assert!(
        response.status().is_success(),
        "authenticated GC request should succeed, got {}",
        response.status()
    );
    AuthenticatedTraffic { trace_id, token }
}

/// The collector log is a secondary sink for anything a service puts on a span
/// (see `env_tests::fixtures::collector`), so it gets the same leak scan as the
/// service logs — but only AFTER secret- and token-bearing traffic has flowed,
/// or the scan proves nothing.
///
/// Waits until the traffic's trace_id is in the collector log, then runs the
/// shared collector scan over that window (trace_id presence, the JWT regex
/// matching the real token, and the dev client secret literal are its
/// controls).
async fn assert_collector_log_has_no_secrets_after_authenticated_traffic(
    traffic: &AuthenticatedTraffic,
) {
    assert_pods_exist(OTEL_COLLECTOR_SELECTOR);

    let mut log = String::new();
    let _ = assert_eventually(ConsistencyCategory::LogAggregation, || {
        log = collector_log(COLLECTOR_LOG_WINDOW);
        let present = log.contains(&traffic.trace_id);
        async move { present }
    })
    .await;
    assert_collector_log_has_no_secrets(
        &log,
        &traffic.trace_id,
        &traffic.token,
        &[DEV_TEST_CLIENT_SECRET],
    );
}

/// Window for the collector-log scan; the trace_id is fresh per run, so the
/// window only bounds output.
const COLLECTOR_LOG_WINDOW: &str = "5m";

/// The dev OTel collector (R-59) must be Ready. This asserts the cluster-setup
/// readiness gate is correct: the `app=otel-collector` selector here MUST match
/// (1) the Deployment pod-template label and (2) the `kubectl wait` selector in
/// `deploy_otel_collector()` (infra/kind/scripts/deploy.sh). The three-way match
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
/// false-fail even though deploy.sh has already Ready-gated the collector.
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
            OTEL_COLLECTOR_SELECTOR,
            "-n",
            NAMESPACE,
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
        "OTel collector pod (-l {OTEL_COLLECTOR_SELECTOR} -n {NAMESPACE}) is not Ready - \
         the deploy_otel_collector readiness gate may be misconfigured (selector/namespace \
         mismatch) or the collector failed to start: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
