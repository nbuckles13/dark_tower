//! P1 Observability Tests
//!
//! Tests for Prometheus scraping and Loki log aggregation across all deployed services.
//! Loki tests auto-skip if the log aggregation stack is not available.

#![cfg(feature = "observability")]

use env_tests::cluster::ClusterConnection;
use env_tests::eventual::{assert_eventually, ConsistencyCategory};
use env_tests::fixtures::PrometheusClient;
use std::time::{SystemTime, UNIX_EPOCH};

/// Helper to create a cluster connection for tests.
async fn cluster() -> ClusterConnection {
    ClusterConnection::new()
        .await
        .expect("Failed to connect to cluster - ensure port-forwards are running")
}

/// All services that must be scraped by Prometheus and have logs in Loki.
/// If a service is deployed but missing from this list, these tests won't catch it.
/// If a service is in this list but not deployed, the test will fail — which is correct.
const EXPECTED_SERVICES: &[(&str, &str)] = &[
    ("ac-service", "ac_"),
    ("gc-service", "gc_"),
    ("mc-service", "mc_"),
    ("mh-service", "mh_"),
];

#[tokio::test]
async fn test_all_services_scraped_by_prometheus() {
    let cluster = cluster().await;
    let prometheus_client = PrometheusClient::new(&cluster.prometheus_base_url);

    // Verify every expected service appears in Prometheus up{} metric.
    let jobs_regex = EXPECTED_SERVICES
        .iter()
        .map(|(job, _)| *job)
        .collect::<Vec<_>>()
        .join("|");

    let up_response = prometheus_client
        .query_promql(&format!(r#"up{{job=~"{}"}}"#, jobs_regex))
        .await
        .expect("Prometheus up{} query should succeed");

    let discovered_jobs: Vec<String> = up_response
        .data
        .result
        .iter()
        .filter_map(|r| r.metric.get("job").cloned())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    eprintln!(
        "Discovered {} services in Prometheus: {:?}",
        discovered_jobs.len(),
        discovered_jobs
    );

    // Assert ALL expected services are present, not just "at least one".
    for (job, _prefix) in EXPECTED_SERVICES {
        assert!(
            discovered_jobs.iter().any(|j| j == job),
            "Expected service '{}' not found in Prometheus up{{}} metric. \
             Discovered: {:?}. Check Prometheus scrape config.",
            job,
            discovered_jobs
        );
    }

    // For each expected service, verify at least one metric with the correct prefix exists.
    for (job, prefix) in EXPECTED_SERVICES {
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

    // Loki must be available for this test.
    assert!(
        cluster.is_loki_available().await,
        "Loki must be available for log aggregation tests - ensure observability stack is running"
    );

    let loki_url = cluster
        .loki_base_url
        .as_ref()
        .expect("Loki URL should be set");

    // Verify every expected service has logs in Loki.
    // Uses a 29-day window to catch any logs from the cluster's lifetime while
    // staying within Loki's max_query_length limit (30d1h).
    // limit=1 keeps the query cheap regardless of the wide time window.
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("System clock before UNIX epoch");
    let end_ns = now.as_nanos();
    let twenty_nine_days_ns: u128 = 29 * 24 * 3600 * 1_000_000_000;
    let start_ns = end_ns.saturating_sub(twenty_nine_days_ns);

    let expected_apps: Vec<&str> = EXPECTED_SERVICES.iter().map(|(job, _)| *job).collect();

    eprintln!(
        "Checking Loki logs for {} services: {:?}",
        expected_apps.len(),
        expected_apps
    );

    for app in &expected_apps {
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
                "Loki should have logs from service '{}'. \
                 Check Promtail pipeline for this service.",
                app
            )
        });
    }
}

// test_logs_have_trace_ids removed: was aspirational (never asserted anything).
// TODO: Re-add when OpenTelemetry trace ID propagation is implemented (ADR-0011).
