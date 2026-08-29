//! Prometheus client fixture for querying metrics.

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use thiserror::Error;

/// Prometheus client errors.
#[derive(Debug, Error)]
pub enum PrometheusError {
    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("Query failed: {0}")]
    QueryFailed(String),

    #[error("JSON deserialization failed: {0}")]
    JsonError(#[from] serde_json::Error),
}

/// Prometheus query response.
#[derive(Debug, Deserialize)]
pub struct QueryResponse {
    pub status: String,
    pub data: QueryData,
}

/// Query response data.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryData {
    pub result_type: String,
    pub result: Vec<QueryResult>,
}

/// A single query result.
#[derive(Debug, Deserialize)]
pub struct QueryResult {
    pub metric: std::collections::HashMap<String, String>,
    pub value: Option<(f64, String)>,
    pub values: Option<Vec<(f64, String)>>,
}

/// Request parameters for a Prometheus query.
#[derive(Debug, Serialize)]
pub struct QueryRequest {
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time: Option<i64>,
}

impl QueryRequest {
    /// Create a new instant query.
    pub fn instant(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            time: None,
        }
    }

    /// Create a new instant query at a specific time.
    pub fn instant_at(query: impl Into<String>, time: i64) -> Self {
        Self {
            query: query.into(),
            time: Some(time),
        }
    }
}

/// Client for querying Prometheus.
pub struct PrometheusClient {
    base_url: String,
    http_client: Client,
}

impl PrometheusClient {
    /// Create a new Prometheus client.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            http_client: Client::new(),
        }
    }

    /// Execute an instant query.
    pub async fn query(&self, request: QueryRequest) -> Result<QueryResponse, PrometheusError> {
        let query_url = format!("{}/api/v1/query", self.base_url);

        let response = self
            .http_client
            .get(&query_url)
            .query(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(PrometheusError::QueryFailed(format!(
                "Status: {}",
                response.status()
            )));
        }

        let query_response = response.json::<QueryResponse>().await?;

        if query_response.status != "success" {
            return Err(PrometheusError::QueryFailed(format!(
                "Query status: {}",
                query_response.status
            )));
        }

        Ok(query_response)
    }

    /// Execute a PromQL query and return the results.
    pub async fn query_promql(&self, promql: &str) -> Result<QueryResponse, PrometheusError> {
        self.query(QueryRequest::instant(promql)).await
    }

    /// Get the raw metrics from a specific endpoint.
    ///
    /// This bypasses Prometheus storage and queries the service's /metrics endpoint directly.
    pub async fn fetch_metrics(&self, metrics_url: &str) -> Result<String, PrometheusError> {
        let response = self.http_client.get(metrics_url).send().await?;

        if !response.status().is_success() {
            return Err(PrometheusError::QueryFailed(format!(
                "Metrics endpoint returned status: {}",
                response.status()
            )));
        }

        Ok(response.text().await?)
    }

    /// Get the HTTP client for custom requests.
    pub fn http_client(&self) -> &Client {
        &self.http_client
    }

    /// Read a per-instance snapshot of a counter via a `sum by (instance)(...)`
    /// PromQL query.
    ///
    /// The returned [`InstanceCounters`] maps each scrape target's `instance`
    /// label (pod IP:port) to that instance's current counter value. Use it as
    /// the baseline/current input to [`any_instance_exceeds_baseline`] for a
    /// pod-rollover-robust monotonic-increase delta check (see that function's
    /// doc for the rationale).
    ///
    /// FAIL-LOUDLY (docs/TODO.md §Env-Test Resilience, defect #2): a transport /
    /// HTTP-status / deserialization error PANICS, naming the PromQL and the
    /// underlying [`PrometheusError`], so triage lands on Prometheus rather than
    /// on the service under test. An EMPTY instant vector (a *successful* query
    /// for a series that has not been observed yet — e.g. a failure-only counter
    /// before its first occurrence) is legitimately zero and returns an EMPTY
    /// map. The empty-vector and query-error cases are kept strictly distinct;
    /// they must never collapse into the same reading.
    pub async fn instance_counter_map(&self, promql: &str) -> InstanceCounters {
        instance_map_from_query(promql, self.query_promql(promql).await)
    }
}

/// A per-instance snapshot of a counter: `instance` label (pod IP:port) →
/// current value.
///
/// # Why `instance` (and not `pod`) — @paired-observability, confirmed against
/// `infra/kubernetes/observability/prometheus-config.yaml`:
/// the mc/mh-service scrape jobs use `kubernetes_sd_configs role: pod` with
/// relabel_configs that only `keep` on the app label + container port; NO
/// relabel emits a `pod`/`pod_name` target_label. So Prometheus's default
/// applies: `instance` = `__address__` = pod IP:port, which is fresh on every
/// pod rollover (a new pod gets a new IP → a new `instance` → a series absent
/// from any pre-rollover baseline).
///
/// # Two traps this identity depends on (do NOT "fix" `instance` → `pod`):
/// 1. A `pod` target_label DOES exist — but only in
///    `infra/kubernetes/observability/promtail-config.yaml` (the logs/Loki
///    pipeline), NEVER in the metrics scrape path. For metrics the per-pod
///    identity is `instance`.
/// 2. The mc/mh Deployments carry a STABLE pod *metadata* label
///    `instance: mc-0` / `mc-1` that is not currently propagated into series
///    (no `labelmap` in the mc/mh jobs). Adding
///    `labelmap __meta_kubernetes_pod_label_(.+)` to the metrics scrape config
///    later would OVERRIDE the default `instance` with that stable value and
///    SILENTLY break this fix — a rolled-over pod would keep `instance=mc-0`,
///    so no "new instance above zero" would ever appear. Do not add such a
///    labelmap without revisiting these helpers.
///
/// # Staleness/cardinality: every rollover mints a new `instance` series; the
/// old one persists up to Prometheus's staleness window (~5min, the
/// `query.lookback-delta` default) then vanishes. During an assertion window
/// both old and new instance series may be transiently present — the
/// per-instance decision tolerates this by construction.
pub type InstanceCounters = std::collections::HashMap<String, f64>;

/// Build an [`InstanceCounters`] map from a Prometheus instant-query response.
///
/// Pure (no I/O): parses an already-`Ok` [`QueryResponse`] produced by a
/// `sum by (instance)(...)` query. An empty `result` yields an empty map
/// (the legitimately-zero, series-not-yet-observed case).
///
/// Two anomalies PANIC rather than being silently absorbed (`promql` is named
/// in each so triage lands on the query):
/// - a non-finite / non-numeric sample value. This rejects BOTH a value that
///   fails to parse (`"5abc"`, `""`) AND the finite-`f64`-but-not-really tokens
///   Prometheus renders as `"NaN"` / `"+Inf"` / `"-Inf"` — `f64::from_str`
///   accepts those, so without the `is_finite()` guard a `"NaN"` sample would
///   silently read as "no increment" (`NaN > baseline` is always false) and a
///   `"+Inf"` sample would silently FALSE-PASS (`inf > baseline` is always
///   true). The TS mirror (`Number(raw)` + empty-string guard + `isFinite`)
///   rejects the same set; keeping the two in lockstep is what the parity unit
///   tests pin.
/// - a result row with NO `instance` label. Defaulting such a row to the empty
///   key would silently bucket every unlabeled series together — and if a
///   future edit ever dropped `by (instance)` from the PromQL, EVERY pod would
///   collapse into that one bucket holding the cluster-wide sum, reverting this
///   fix to `sum(...) > baseline` with nothing failing. We refuse the grouping
///   key we cannot form, the same way we refuse a non-finite sample.
pub fn results_to_instance_map(promql: &str, response: &QueryResponse) -> InstanceCounters {
    response
        .data
        .result
        .iter()
        .filter_map(|r| {
            // Rows without an instant `value` (e.g. a range-only result) carry
            // nothing to compare; skip them rather than inventing a zero.
            let (_, raw) = r.value.as_ref()?;
            let parsed = raw
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite())
                .unwrap_or_else(|| {
                    panic!(
                        "Prometheus returned a non-finite/non-numeric sample {raw:?} for query {promql:?}"
                    )
                });
            let instance = r.metric.get("instance").cloned().unwrap_or_else(|| {
                panic!(
                    "Prometheus result row for query {promql:?} has no `instance` label — the \
                     per-instance grouping key cannot be formed. A `sum by (instance)(...)` query \
                     must attach `instance`; a row without it means the grouping was lost (e.g. \
                     `by (instance)` was dropped from the PromQL), which would silently revert the \
                     per-instance comparison to a cluster-wide sum."
                )
            });
            Some((instance, parsed))
        })
        .collect()
}

/// Fail-loud seam between the async query and the pure map-building: on a
/// query error PANIC (naming the PromQL + [`PrometheusError`]); on success
/// delegate to [`results_to_instance_map`] (empty result → empty map).
///
/// Factored out of the async reader so the empty-vs-error split is unit-testable
/// without a cluster.
pub fn instance_map_from_query(
    promql: &str,
    result: Result<QueryResponse, PrometheusError>,
) -> InstanceCounters {
    match result {
        Ok(response) => results_to_instance_map(promql, &response),
        Err(e) => panic!(
            "Prometheus query {promql:?} failed: {e}. Global-setup proved Prometheus \
             healthy, so a query error here is a real problem — NOT a zero reading. \
             Triage Prometheus/port-forward, not the service under test."
        ),
    }
}

/// Pure per-instance monotonic-increase decision: does ANY currently-present
/// instance's value exceed its OWN baseline?
///
/// An instance absent from `baseline` (a fresh pod that appeared after a
/// rollover) is treated as baseline `0.0`, so it passes as soon as its value
/// exceeds zero. A stale instance that expired between baseline and now is
/// simply absent from `current` and cannot inflate anything — because no other
/// pod shares its `instance` value, per-instance comparison never sums across
/// pods. This is the whole point of the fix: the old cluster-wide `sum(...)`
/// baseline was inflated by soon-to-expire old-pod series, so fresh pods could
/// never beat it (docs/TODO.md §Env-Test Resilience, defect #1).
///
/// # Residual false-pass window (@security, Gate 3 — NOT a "by construction"
/// guarantee). Treating an absent-from-baseline instance as baseline `0.0` is
/// sound for the intended case (a fresh post-rollover pod whose counter really
/// starts at 0). It is NOT sound for a *pre-existing* pod that was merely
/// MISSING from the baseline read (its target unscraped beyond
/// `query.lookback-delta`, or a transient empty result) and then reappears
/// carrying its full prior value: that value exceeds 0 without the test having
/// caused any increment → a narrow false pass. The old cluster-wide form could
/// not produce this (its churn failure mode was one-directional, false-negative
/// only); rollover-robustness was traded for it. The window is narrow and the
/// failure mode it replaces is worse, so it is documented rather than designed
/// out — there is no cheap way to distinguish a new pod from a returning one
/// without pod start time. Note the asymmetry: the notification path's
/// `wait_for_notification_counter_stable` (two identical per-instance maps 16s
/// apart) would catch a reappearing instance; the `mc_participant_mh_status`
/// path and the TS specs have no such stabilize.
pub fn any_instance_exceeds_baseline(
    baseline: &InstanceCounters,
    current: &InstanceCounters,
) -> bool {
    current
        .iter()
        .any(|(instance, &value)| value > baseline.get(instance).copied().unwrap_or(0.0))
}

/// Two per-instance snapshots are equal when they cover the same instances with
/// (float-)equal values. Counter values are whole numbers, but we compare with
/// an epsilon for float-safety. The TS families have no stabilize counterpart
/// today, so this predicate is Rust-only — kept here (beside
/// [`any_instance_exceeds_baseline`]) so both pure `InstanceCounters` decisions
/// share one home with unit coverage rather than one of them living in a test
/// binary.
pub fn instance_maps_equal(a: &InstanceCounters, b: &InstanceCounters) -> bool {
    a.len() == b.len()
        && a.iter()
            .all(|(k, v)| b.get(k).is_some_and(|w| (v - w).abs() < f64::EPSILON))
}

/// Render a per-instance snapshot for a diagnostic message: `{a=1, b=2}`,
/// entries SORTED by instance so the output is deterministic across runs (a
/// `HashMap`'s iteration order is not — and comparing a failure's map across
/// Layer-7 retry attempts is exactly the triage workflow this flake class
/// generates). An empty snapshot renders self-describingly rather than as a
/// bare `{}` (which reads as a broken format placeholder at 3am) and names the
/// distinction the per-instance design created: an empty `current` means
/// Prometheus answered but has NO series for this selector — a different
/// investigation from "instances present, none incremented". Mirrors the TS
/// `formatInstanceMap`.
pub fn format_instance_map(map: &InstanceCounters) -> String {
    if map.is_empty() {
        return "{} (no instances — Prometheus returned an empty vector for this selector; \
                check the MC/MH pods are up and scraped)"
            .to_string();
    }
    let mut entries: Vec<(&String, &f64)> = map.iter().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    let body = entries
        .iter()
        .map(|(instance, value)| format!("{instance}={value}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{{{body}}}")
}

/// The ONE Rust counter-delta poll loop (mirrors the TS
/// `pollUntilAnyInstanceAbove` — the two families must not re-encode the poll
/// idiom by hand, which is the drift vector this task exists to remove). Reads
/// `promql` as a per-instance snapshot (fail-loud on query error), and returns
/// once SOME currently-present instance exceeds its OWN baseline (see
/// [`any_instance_exceeds_baseline`]); on timeout, panics with
/// `timeout_message(&current)`.
pub async fn poll_until_any_instance_above(
    prom: &PrometheusClient,
    promql: &str,
    baseline: &InstanceCounters,
    timeout: Duration,
    interval: Duration,
    timeout_message: impl Fn(&InstanceCounters) -> String,
) {
    let deadline = Instant::now() + timeout;
    loop {
        let current = prom.instance_counter_map(promql).await;
        if any_instance_exceeds_baseline(baseline, &current) {
            return;
        }
        if Instant::now() > deadline {
            panic!("{}", timeout_message(&current));
        }
        tokio::time::sleep(interval).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PromQL literal used in the pure-parse tests (only names the query in
    /// diagnostics; parsing is content-driven, not query-driven).
    const TEST_PROMQL: &str =
        "sum by (instance) (mc_participant_mh_status_total{state=\"connected\"})";

    /// Build a `QueryResponse` shaped like a `sum by (instance)(...)` instant
    /// query: one result per `(instance, value)`.
    fn instance_response(pairs: &[(&str, &str)]) -> QueryResponse {
        QueryResponse {
            status: "success".to_string(),
            data: QueryData {
                result_type: "vector".to_string(),
                result: pairs
                    .iter()
                    .map(|(instance, value)| QueryResult {
                        metric: std::collections::HashMap::from([(
                            "instance".to_string(),
                            (*instance).to_string(),
                        )]),
                        value: Some((0.0, (*value).to_string())),
                        values: None,
                    })
                    .collect(),
            },
        }
    }

    #[test]
    fn passes_when_old_instance_gone_and_new_instance_above_zero() {
        // Baseline captured before a rollover: only the old pod present at 5.
        let baseline =
            results_to_instance_map(TEST_PROMQL, &instance_response(&[("10.0.0.1:8081", "5")]));
        // After the rollover the old pod's series expired and a FRESH pod
        // (new IP → absent from baseline) is at 1. The inflated cluster-wide
        // sum would have blocked this; per-instance it must PASS.
        let current =
            results_to_instance_map(TEST_PROMQL, &instance_response(&[("10.0.0.2:8081", "1")]));
        assert!(any_instance_exceeds_baseline(&baseline, &current));
    }

    #[test]
    fn does_not_pass_when_all_present_and_none_increased() {
        let baseline = results_to_instance_map(
            TEST_PROMQL,
            &instance_response(&[("10.0.0.1:8081", "5"), ("10.0.0.2:8081", "3")]),
        );
        // Same instances, identical values — nothing incremented.
        let current = results_to_instance_map(
            TEST_PROMQL,
            &instance_response(&[("10.0.0.1:8081", "5"), ("10.0.0.2:8081", "3")]),
        );
        assert!(!any_instance_exceeds_baseline(&baseline, &current));
    }

    #[test]
    fn empty_result_is_an_empty_map_not_an_error() {
        // A successful query for a not-yet-observed series (e.g. a failure-only
        // counter) is legitimately zero, DISTINCT from a query error.
        let map = instance_map_from_query(TEST_PROMQL, Ok(instance_response(&[])));
        assert!(map.is_empty());
        // And an empty baseline vs an empty current does not pass.
        assert!(!any_instance_exceeds_baseline(&map, &map));
    }

    #[test]
    #[should_panic(expected = "no `instance` label")]
    fn missing_instance_label_fails_loudly_not_bucketed() {
        // A row with no `instance` label must PANIC (@security Gate 3): silently
        // defaulting it to the empty key would re-open cluster-wide summation if
        // `by (instance)` were ever dropped from the PromQL.
        let response = QueryResponse {
            status: "success".to_string(),
            data: QueryData {
                result_type: "vector".to_string(),
                result: vec![QueryResult {
                    metric: std::collections::HashMap::new(),
                    value: Some((0.0, "7".to_string())),
                    values: None,
                }],
            },
        };
        let _ = results_to_instance_map(TEST_PROMQL, &response);
    }

    /// Single-row response carrying `raw` as the sample value (for parse tests).
    fn sample_response(raw: &str) -> QueryResponse {
        QueryResponse {
            status: "success".to_string(),
            data: QueryData {
                result_type: "vector".to_string(),
                result: vec![QueryResult {
                    metric: std::collections::HashMap::from([(
                        "instance".to_string(),
                        "10.0.0.1:8081".to_string(),
                    )]),
                    value: Some((0.0, raw.to_string())),
                    values: None,
                }],
            },
        }
    }

    // Fail-loud parse parity with the TS mirror (@code-reviewer + @test-reviewer,
    // Gate 3): every sample that is not a finite number must PANIC, never be
    // absorbed. `"NaN"`/`"+Inf"` are the sharp ones — `f64::from_str` accepts
    // them, so without the `is_finite()` guard `"NaN"` would silently read as
    // "no increment" and `"+Inf"` would silently FALSE-PASS. Pinned on exactly
    // the inputs where the two runtimes could diverge, not just the one they
    // already agreed on.

    #[test]
    #[should_panic(expected = "non-finite/non-numeric")]
    fn nan_sample_fails_loudly_not_masked() {
        let _ = results_to_instance_map(TEST_PROMQL, &sample_response("NaN"));
    }

    #[test]
    #[should_panic(expected = "non-finite/non-numeric")]
    fn positive_inf_sample_fails_loudly_not_false_pass() {
        let _ = results_to_instance_map(TEST_PROMQL, &sample_response("+Inf"));
    }

    #[test]
    #[should_panic(expected = "non-finite/non-numeric")]
    fn trailing_garbage_sample_fails_loudly() {
        let _ = results_to_instance_map(TEST_PROMQL, &sample_response("5abc"));
    }

    #[test]
    #[should_panic(expected = "non-finite/non-numeric")]
    fn empty_sample_fails_loudly() {
        let _ = results_to_instance_map(TEST_PROMQL, &sample_response(""));
    }

    #[test]
    #[should_panic(expected = "NOT a zero reading")]
    fn query_error_fails_loudly_not_read_as_zero() {
        // A transport/query error must PANIC, never be masked as a 0 reading.
        let _ = instance_map_from_query(
            TEST_PROMQL,
            Err(PrometheusError::QueryFailed(
                "simulated Prometheus outage".to_string(),
            )),
        );
    }

    #[test]
    fn instance_maps_equal_matches_same_instances_and_values() {
        let a = results_to_instance_map(
            TEST_PROMQL,
            &instance_response(&[("10.0.0.1:8081", "5"), ("10.0.0.2:8081", "3")]),
        );
        let b = results_to_instance_map(
            TEST_PROMQL,
            &instance_response(&[("10.0.0.2:8081", "3"), ("10.0.0.1:8081", "5")]),
        );
        assert!(instance_maps_equal(&a, &b));
    }

    #[test]
    fn instance_maps_equal_rejects_same_keys_different_values() {
        let a = results_to_instance_map(TEST_PROMQL, &instance_response(&[("10.0.0.1:8081", "5")]));
        let b = results_to_instance_map(TEST_PROMQL, &instance_response(&[("10.0.0.1:8081", "6")]));
        assert!(!instance_maps_equal(&a, &b));
    }

    #[test]
    fn instance_maps_equal_rejects_when_key_set_differs() {
        // The stabilize loop relies on this to retry across a series expiry: an
        // instance present in one read and gone in the other is NOT stable.
        let a = results_to_instance_map(
            TEST_PROMQL,
            &instance_response(&[("10.0.0.1:8081", "5"), ("10.0.0.2:8081", "3")]),
        );
        let b = results_to_instance_map(TEST_PROMQL, &instance_response(&[("10.0.0.1:8081", "5")]));
        assert!(!instance_maps_equal(&a, &b));
    }

    #[test]
    fn format_instance_map_is_sorted_and_describes_empty() {
        let map = results_to_instance_map(
            TEST_PROMQL,
            &instance_response(&[("10.0.0.2:8081", "3"), ("10.0.0.1:8081", "5")]),
        );
        // Deterministic order regardless of HashMap iteration order.
        assert_eq!(
            format_instance_map(&map),
            "{10.0.0.1:8081=5, 10.0.0.2:8081=3}"
        );
        // Empty is self-describing, not a bare `{}`.
        let empty = InstanceCounters::new();
        assert!(format_instance_map(&empty).contains("no instances"));
    }
}
