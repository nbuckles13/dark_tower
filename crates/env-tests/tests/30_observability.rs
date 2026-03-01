//! P1 Observability Tests
//!
//! Tests for metrics exposure, Prometheus scraping, and log aggregation.
//! Loki tests auto-skip if the log aggregation stack is not available.

#![cfg(feature = "observability")]

use env_tests::cluster::ClusterConnection;
use env_tests::eventual::{assert_eventually, ConsistencyCategory};
use env_tests::fixtures::auth_client::TokenRequest;
use env_tests::fixtures::{AuthClient, PrometheusClient};
use std::time::{SystemTime, UNIX_EPOCH};

/// Helper to create a cluster connection for tests.
async fn cluster() -> ClusterConnection {
    ClusterConnection::new()
        .await
        .expect("Failed to connect to cluster - ensure port-forwards are running")
}

#[tokio::test]
async fn test_ac_metrics_exposed() {
    let cluster = cluster().await;

    // Query the AC service /metrics endpoint directly (bypass Prometheus storage)
    let metrics_url = format!("{}/metrics", cluster.ac_base_url);

    let response = cluster
        .http_client()
        .get(&metrics_url)
        .send()
        .await
        .expect("Metrics endpoint should be reachable");

    assert!(
        response.status().is_success(),
        "Metrics endpoint should return 200 OK"
    );

    let metrics_text = response.text().await.expect("Should read metrics body");

    // Verify Prometheus exposition format
    // Note: The metrics library may not emit HELP comments for all metrics
    assert!(
        metrics_text.contains("# TYPE"),
        "Metrics should contain TYPE comments"
    );

    // Verify key AC metrics are present
    assert!(
        metrics_text.contains("ac_token_issuance_total"),
        "Should expose token issuance counter"
    );
}

#[tokio::test]
async fn test_metrics_have_expected_labels() {
    let cluster = cluster().await;
    let metrics_url = format!("{}/metrics", cluster.ac_base_url);

    let response = cluster
        .http_client()
        .get(&metrics_url)
        .send()
        .await
        .expect("Metrics endpoint should be reachable");

    let metrics_text = response.text().await.expect("Should read metrics body");

    // Look for token issuance metrics with labels
    let issuance_lines: Vec<&str> = metrics_text
        .lines()
        .filter(|line| line.contains("ac_token_issuance_total"))
        .filter(|line| !line.starts_with('#'))
        .collect();

    assert!(
        !issuance_lines.is_empty(),
        "Should have at least one token issuance metric"
    );

    // Verify label structure (should have grant_type and status labels)
    for line in issuance_lines {
        // Actual format: ac_token_issuance_total{grant_type="client_credentials",status="success"} <value>
        assert!(
            line.contains("grant_type="),
            "Token issuance metric should have grant_type label: {}",
            line
        );
        assert!(
            line.contains("status="),
            "Token issuance metric should have status label: {}",
            line
        );
    }
}

#[tokio::test]
async fn test_token_counter_increments_after_issuance() {
    let cluster = cluster().await;
    let auth_client = AuthClient::new(&cluster.ac_base_url);
    let prometheus_client = PrometheusClient::new(&cluster.prometheus_base_url);

    // Query current counter value from Prometheus
    // AC service metrics use labels: grant_type, status (per ADR-0011 cardinality bounds)
    let query_before = prometheus_client
        .query_promql(
            r#"ac_token_issuance_total{grant_type="client_credentials",status="success"}"#,
        )
        .await
        .expect("Prometheus query should succeed");

    let count_before = if query_before.data.result.is_empty() {
        0.0
    } else {
        query_before.data.result[0]
            .value
            .as_ref()
            .map(|(_, v)| v.parse::<f64>().unwrap_or(0.0))
            .unwrap_or(0.0)
    };

    // Issue a token
    let request =
        TokenRequest::client_credentials("test-client", "test-client-secret-dev-999", "test:all");

    auth_client
        .issue_token(request)
        .await
        .expect("Token issuance should succeed");

    // Wait for Prometheus to scrape the new metric value
    assert_eventually(ConsistencyCategory::MetricsScrape, || async {
        let query_after = match prometheus_client
            .query_promql(
                r#"ac_token_issuance_total{grant_type="client_credentials",status="success"}"#,
            )
            .await
        {
            Ok(q) => q,
            Err(_) => return false,
        };

        if query_after.data.result.is_empty() {
            return false;
        }

        let count_after = query_after.data.result[0]
            .value
            .as_ref()
            .map(|(_, v)| v.parse::<f64>().unwrap_or(0.0))
            .unwrap_or(0.0);

        count_after > count_before
    })
    .await
    .expect("Counter should increment after token issuance");
}

// test_logs_appear_in_loki removed: superseded by test_all_services_have_logs_in_loki
// which dynamically discovers services rather than hardcoding ac-service (which may
// not have logs if AC startup logs age out of Loki's retention window).

#[tokio::test]
async fn test_all_services_scraped_by_prometheus() {
    let cluster = cluster().await;
    let prometheus_client = PrometheusClient::new(&cluster.prometheus_base_url);

    // Discover which services Prometheus is scraping via the `up` metric.
    // Scrape jobs are named ac-service, gc-service, mc-service.
    let up_response = prometheus_client
        .query_promql(r#"up{job=~"ac-service|gc-service|mc-service"}"#)
        .await
        .expect("Prometheus up{} query should succeed");

    assert!(
        !up_response.data.result.is_empty(),
        "Prometheus should be scraping at least one service (ac-service, gc-service, or mc-service)"
    );

    // Build list of discovered services with their expected metric prefixes.
    let prefix_map: std::collections::HashMap<&str, &str> = [
        ("ac-service", "ac_"),
        ("gc-service", "gc_"),
        ("mc-service", "mc_"),
    ]
    .into_iter()
    .collect();

    let mut discovered_services: Vec<(String, String)> = Vec::new();
    for result in &up_response.data.result {
        if let Some(job) = result.metric.get("job") {
            if let Some(&prefix) = prefix_map.get(job.as_str()) {
                let entry = (job.clone(), prefix.to_string());
                if !discovered_services.contains(&entry) {
                    discovered_services.push(entry);
                }
            }
        }
    }

    assert!(
        !discovered_services.is_empty(),
        "Should discover at least one service from Prometheus up{{}} metric"
    );

    eprintln!(
        "Discovered {} services in Prometheus: {:?}",
        discovered_services.len(),
        discovered_services
            .iter()
            .map(|(job, _)| job.as_str())
            .collect::<Vec<_>>()
    );

    // For each discovered service, verify at least one metric with the correct prefix exists.
    for (job, prefix) in &discovered_services {
        // Query for any metric from this job using the expected prefix.
        // Use count to verify at least one metric exists.
        let query = format!(
            r#"count({{job="{job}",__name__=~"{prefix}.*"}})"#,
            job = job,
            prefix = prefix
        );

        assert_eventually(ConsistencyCategory::MetricsScrape, || {
            let prometheus_client = PrometheusClient::new(&cluster.prometheus_base_url);
            let query = query.clone();
            async move {
                let response = match prometheus_client.query_promql(&query).await {
                    Ok(r) => r,
                    Err(_) => return false,
                };

                if response.data.result.is_empty() {
                    return false;
                }

                // count() returns a single result with the count as value
                response.data.result[0]
                    .value
                    .as_ref()
                    .map(|(_, v)| v.parse::<f64>().unwrap_or(0.0) > 0.0)
                    .unwrap_or(false)
            }
        })
        .await
        .unwrap_or_else(|_| {
            panic!(
                "Service '{}' should have at least one metric with prefix '{}' in Prometheus",
                job, prefix
            )
        });
    }
}

#[tokio::test]
async fn test_all_services_have_logs_in_loki() {
    let cluster = cluster().await;
    let prometheus_client = PrometheusClient::new(&cluster.prometheus_base_url);

    // Loki must be available for this test.
    assert!(
        cluster.is_loki_available().await,
        "Loki must be available for log aggregation tests - ensure observability stack is running"
    );

    let loki_url = cluster
        .loki_base_url
        .as_ref()
        .expect("Loki URL should be set");

    // Discover running services from Prometheus (source of truth for "what's deployed").
    // This is the same discovery mechanism as test_all_services_scraped_by_prometheus,
    // ensuring we cross-reference: every service Prometheus scrapes should also have
    // logs in Loki. If a service is running but Promtail isn't collecting its logs,
    // this test catches the gap.
    let job_to_app: std::collections::HashMap<&str, &str> = [
        ("ac-service", "ac-service"),
        ("gc-service", "gc-service"),
        ("mc-service", "mc-service"),
    ]
    .into_iter()
    .collect();

    let up_response = prometheus_client
        .query_promql(r#"up{job=~"ac-service|gc-service|mc-service"}"#)
        .await
        .expect("Prometheus up{} query should succeed");

    let mut running_services: Vec<String> = Vec::new();
    for result in &up_response.data.result {
        if let Some(job) = result.metric.get("job") {
            if let Some(&app) = job_to_app.get(job.as_str()) {
                if !running_services.contains(&app.to_string()) {
                    running_services.push(app.to_string());
                }
            }
        }
    }

    assert!(
        !running_services.is_empty(),
        "Prometheus should show at least one running service"
    );

    eprintln!(
        "Prometheus reports {} running services: {:?}",
        running_services.len(),
        running_services
    );

    // For each running service, verify Loki has at least one log entry.
    // Uses a 29-day window to catch any logs from the cluster's lifetime while
    // staying within Loki's max_query_length limit (30d1h).
    // limit=1 keeps the query cheap regardless of the wide time window.
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("System clock before UNIX epoch");
    let end_ns = now.as_nanos();
    let twenty_nine_days_ns: u128 = 29 * 24 * 3600 * 1_000_000_000;
    let start_ns = end_ns.saturating_sub(twenty_nine_days_ns);

    for app in &running_services {
        let query = format!("{{app=\"{}\"}}", app);

        assert_eventually(ConsistencyCategory::LogAggregation, || {
            let loki_url = loki_url.clone();
            let query = query.clone();
            let http_client = cluster.http_client().clone();
            async move {
                let url = format!("{}/loki/api/v1/query_range", loki_url);

                let response = match http_client
                    .get(&url)
                    .query(&[
                        ("query", query.as_str()),
                        ("start", &start_ns.to_string()),
                        ("end", &end_ns.to_string()),
                        ("limit", "1"),
                    ])
                    .send()
                    .await
                {
                    Ok(r) => r,
                    Err(_) => return false,
                };

                if !response.status().is_success() {
                    return false;
                }

                let body = match response.text().await {
                    Ok(b) => b,
                    Err(_) => return false,
                };

                let json: serde_json::Value = match serde_json::from_str(&body) {
                    Ok(v) => v,
                    Err(_) => return false,
                };

                let status_ok = json.get("status").and_then(|s| s.as_str()) == Some("success");
                let has_results = json
                    .get("data")
                    .and_then(|d| d.get("result"))
                    .and_then(|r| r.as_array())
                    .map(|a| !a.is_empty())
                    .unwrap_or(false);

                status_ok && has_results
            }
        })
        .await
        .unwrap_or_else(|_| {
            panic!(
                "Loki should have logs from service '{}' — Prometheus shows it is running \
                 but no logs found. Check Promtail pipeline for this service.",
                app
            )
        });
    }
}

// test_logs_have_trace_ids removed: was aspirational (never asserted anything).
// TODO: Re-add when OpenTelemetry trace ID propagation is implemented (ADR-0011).
