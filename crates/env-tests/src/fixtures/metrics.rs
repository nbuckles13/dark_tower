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

    /// Fetch the RAW body of `/api/v1/rules`.
    ///
    /// Raw, not deserialised, on purpose: parsing lives in
    /// [`crate::fixtures::alert_rules_loaded::loaded_alert_names`], which is a
    /// pure kernel with FIRE fixtures in the always-on Rust lane. An extractor
    /// written inside a cluster-gated test gets zero unit coverage — that is how
    /// the escaped-JSON `/api/v1/status/config` defect this repo already shipped
    /// (see `tests/32_media_metric_hygiene.rs`) passed on every possible input.
    ///
    /// Added here rather than hand-rolled at the call site because
    /// `cluster.rs::check_prometheus` and `32_media_metric_hygiene.rs` already
    /// hand-roll `format!("{}/api/v1/...")` twice; this is the shared home, not
    /// a third copy.
    pub async fn rules(&self) -> Result<String, PrometheusError> {
        let url = format!("{}/api/v1/rules", self.base_url);
        let response = self.http_client.get(&url).send().await?;
        if !response.status().is_success() {
            return Err(PrometheusError::QueryFailed(format!(
                "/api/v1/rules returned status: {}",
                response.status()
            )));
        }
        Ok(response.text().await?)
    }

    /// Fetch the RAW body of `/api/v1/status/config` (a JSON envelope whose
    /// `data.yaml` is the resolved config as one escaped YAML string). Parsed
    /// by pure kernels ([`max_service_job_scrape_interval`]), never at the
    /// call site.
    pub async fn status_config(&self) -> Result<String, PrometheusError> {
        let url = format!("{}/api/v1/status/config", self.base_url);
        let response = self.http_client.get(&url).send().await?;
        if !response.status().is_success() {
            return Err(PrometheusError::QueryFailed(format!(
                "/api/v1/status/config returned status: {}",
                response.status()
            )));
        }
        Ok(response.text().await?)
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
/// `infra/kubernetes/observability/prometheus.yml` (the file carrying `scrape_configs`):
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

/// **THE per-instance monotonic-increase rule.** Which currently-present
/// instances exceed their OWN baseline? Returned sorted, so a diagnostic built
/// from it is deterministic across runs.
///
/// # One rule, several read shapes
///
/// This is the single home of the absent-from-baseline convention below. Every
/// other per-instance "did it rise" question in this module is expressed as a
/// read over this function's result — [`any_instance_exceeds_baseline`] is
/// `!…is_empty()`, and "did THIS instance rise" is a membership check. They are
/// wrappers that CALL this, never restatements of it: three hand-written copies
/// of a non-trivial convention means the next correction to it lands in one, two
/// or three of them and the ones it misses stay green. `any_instance_exceeds_baseline`
/// also has a live TypeScript mirror (`packages/web-app/e2e/instanceCounters.ts`),
/// whose lockstep survives only while the Rust side is a true wrapper.
///
/// Contrast [`instance_maps_equal`], which is deliberately NOT folded in here:
/// map-equality asks "did anything move at all" and this asks "which instances
/// beat their own baseline", and the two differ precisely on the absent-instance
/// convention. One rule forced to answer for both would be a false SSoT.
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
/// apart) would catch an instance reappearing BETWEEN the two reads — but not
/// one that reappears after the second, for the same reason spelled out at
/// [`poll_until_stable`]; the `mc_participant_mh_status` path and the TS specs
/// have no such stabilize at all.
///
/// **A caller that must ATTRIBUTE a rise to its own action, rather than merely
/// observe one, has to close that window itself** — see [`poll_until_stable`],
/// which closes the in-flight-increment and transient-empty-result causes.
///
/// It does **not** close all of them, and this referral must not promise more
/// than the referred-to function delivers: a target unscraped beyond
/// `query.lookback-delta` is absent from both of its reads and is NOT covered.
/// That slice needs the `up{job=...}` liveness check named at
/// [`poll_until_stable`], which is also where the derivation lives — pointed at
/// rather than restated here, so the two cannot drift.
#[must_use]
pub fn instances_exceeding_baseline(
    baseline: &InstanceCounters,
    current: &InstanceCounters,
) -> Vec<String> {
    let mut risen: Vec<String> = current
        .iter()
        .filter(|(instance, &value)| value > baseline.get(*instance).copied().unwrap_or(0.0))
        .map(|(instance, _)| instance.clone())
        .collect();
    risen.sort();
    risen
}

/// Pure per-instance monotonic-increase decision: does ANY currently-present
/// instance's value exceed its OWN baseline?
///
/// A **true wrapper** over [`instances_exceeding_baseline`], which is where the
/// rule and its residual-false-pass note live. Deliberately not a second
/// implementation that happens to agree today: see that function for why, and
/// `wrapper_and_rule_agree_*` for the tests that pin it on both arms.
#[must_use]
pub fn any_instance_exceeds_baseline(
    baseline: &InstanceCounters,
    current: &InstanceCounters,
) -> bool {
    !instances_exceeding_baseline(baseline, current).is_empty()
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

/// The settle for a stability wait over a series scraped by one of the four
/// SERVICE jobs (`ac-service`, `gc-service`, `mc-service`, `mh-service`):
/// their per-job `scrape_interval` plus [`SCRAPE_SETTLE_MARGIN`].
///
/// ANCHOR (DRY): the interval is the per-job `scrape_interval` on those four
/// jobs in `infra/kubernetes/observability/prometheus.yml`. Rust cannot import
/// YAML, so this is a COPY — which is why no caller should use it directly:
/// [`service_job_scrape_settle`] returns it only after checking it against the
/// LIVE Prometheus config, and fails loudly if it no longer exceeds the
/// deployed interval. This is the one place a numeral for the cadence belongs;
/// everywhere else cites the key.
///
/// # NOT valid for any other job class
///
/// The config carries three cadences, not one. This settle is correct ONLY for
/// the service jobs. It is WRONG — too short, so a stability wait would compare
/// two reads inside one scrape and pass having observed nothing — for:
///
/// - the `otel-collector` job (its own per-job `scrape_interval`, matched to
///   the SDK export cadence), which carries every browser `dt_client_*` series.
///   A live consumer of that class already exists:
///   `tests/32_media_metric_hygiene.rs` reads `dt_client_*` beside the
///   service jobs in one selector — the file a future author gating a client
///   series is most likely to copy from;
/// - the jobs on the GLOBAL `scrape_interval` (`prometheus`,
///   `kube-state-metrics`, `node-exporter`, `kubelet`).
///
/// A caller gating either class must derive its own settle from that job's
/// interval.
pub const SERVICE_JOB_SCRAPE_SETTLE: Duration =
    Duration::from_secs(SERVICE_JOB_SCRAPE_INTERVAL_SECS + SCRAPE_SETTLE_MARGIN_SECS);

/// The service jobs' per-job `scrape_interval`, in seconds — the copy
/// [`SERVICE_JOB_SCRAPE_SETTLE`] is derived from (see its ANCHOR).
pub const SERVICE_JOB_SCRAPE_INTERVAL_SECS: u64 = 5;

/// The margin added to one scrape interval, in seconds.
///
/// Its basis is ABSOLUTE: the time one scrape takes plus Prometheus ingestion
/// latency, so the second read is guaranteed to come from a later scrape than
/// the first. Neither shrinks when the interval does. **Do not scale this down
/// with the interval** — the live-config precondition compares the settle
/// against the CONFIGURED interval and structurally cannot see scrape
/// duration, so a margin scaled into the noise would pass that check while the
/// wait silently went vacuous.
pub const SCRAPE_SETTLE_MARGIN_SECS: u64 = 1;

/// [`SCRAPE_SETTLE_MARGIN_SECS`] as a `Duration`.
pub const SCRAPE_SETTLE_MARGIN: Duration = Duration::from_secs(SCRAPE_SETTLE_MARGIN_SECS);

/// Greppable token: the live config could not be read or did not have the
/// expected shape. Deliberately distinct from [`SETTLE_NOT_ABOVE_INTERVAL`] —
/// if the two looked alike, a parse failure would be triaged as a tuning bug
/// and "fixed" by relaxing the extractor.
pub const SCRAPE_CONFIG_UNREADABLE: &str = "scrape-config-unreadable";

/// Greppable token: the deployed service-job scrape interval is at or above
/// [`SERVICE_JOB_SCRAPE_SETTLE`], so every stability wait using it would be
/// vacuous.
pub const SETTLE_NOT_ABOVE_INTERVAL: &str = "settle-not-above-scrape-interval";

/// Why the live scrape interval of the service jobs could not be determined.
/// Never an empty success: every arm is a loud failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScrapeIntervalError {
    /// The body was not `/api/v1/status/config`'s JSON envelope with a YAML
    /// string at `data.yaml`, or that YAML did not parse.
    NotPrometheusConfig,
    /// A service job is absent from the config.
    ServiceJobMissing(String),
    /// A service job's effective interval (per-job, else global) is absent or
    /// not a Prometheus duration.
    IntervalUnparseable(String),
}

/// The LARGEST effective `scrape_interval` over the four service jobs, from a
/// `/api/v1/status/config` body. A per-job value wins over `global`; Prometheus
/// marshals the resolved config, so per-job values are normally present.
///
/// Pure, so its FIRE fixtures run in the always-on unit lane (the
/// escaped-JSON defect this repo shipped once passed on every live input).
///
/// # Errors
///
/// Every [`ScrapeIntervalError`] arm; never a default.
pub fn max_service_job_scrape_interval(body: &str) -> Result<Duration, ScrapeIntervalError> {
    use crate::fixtures::metric_hygiene::SERVICE_JOBS;

    let envelope: serde_json::Value =
        serde_json::from_str(body).map_err(|_| ScrapeIntervalError::NotPrometheusConfig)?;
    let yaml = envelope
        .get("data")
        .and_then(|d| d.get("yaml"))
        .and_then(serde_json::Value::as_str)
        .ok_or(ScrapeIntervalError::NotPrometheusConfig)?;
    let config: serde_norway::Value =
        serde_norway::from_str(yaml).map_err(|_| ScrapeIntervalError::NotPrometheusConfig)?;

    let global = config
        .get("global")
        .and_then(|g| g.get("scrape_interval"))
        .and_then(serde_norway::Value::as_str);
    let jobs = config
        .get("scrape_configs")
        .and_then(serde_norway::Value::as_sequence)
        .ok_or(ScrapeIntervalError::NotPrometheusConfig)?;

    let mut max = Duration::ZERO;
    for name in SERVICE_JOBS {
        let job = jobs
            .iter()
            .find(|j| j.get("job_name").and_then(serde_norway::Value::as_str) == Some(name))
            .ok_or_else(|| ScrapeIntervalError::ServiceJobMissing((*name).to_string()))?;
        let raw = job
            .get("scrape_interval")
            .and_then(serde_norway::Value::as_str)
            .or(global)
            .ok_or_else(|| ScrapeIntervalError::IntervalUnparseable((*name).to_string()))?;
        let interval = parse_prometheus_duration(raw)
            .ok_or_else(|| ScrapeIntervalError::IntervalUnparseable((*name).to_string()))?;
        max = max.max(interval);
    }
    Ok(max)
}

/// Parse the Prometheus duration forms a scrape interval uses (`500ms`, `5s`,
/// `1m`, `1h`, and concatenations such as `1m30s`). `None` for anything else
/// or for zero — never a guess.
fn parse_prometheus_duration(raw: &str) -> Option<Duration> {
    let mut total = Duration::ZERO;
    let mut rest = raw.trim();
    if rest.is_empty() {
        return None;
    }
    while !rest.is_empty() {
        let digits = rest.chars().take_while(char::is_ascii_digit).count();
        if digits == 0 {
            return None;
        }
        let value: u64 = rest.get(..digits)?.parse().ok()?;
        rest = rest.get(digits..)?;
        let (unit, len) = if rest.starts_with("ms") {
            (Duration::from_millis(1), 2)
        } else if rest.starts_with('s') {
            (Duration::from_secs(1), 1)
        } else if rest.starts_with('m') {
            (Duration::from_secs(60), 1)
        } else if rest.starts_with('h') {
            (Duration::from_secs(3_600), 1)
        } else {
            return None;
        };
        total += unit.checked_mul(u32::try_from(value).ok()?)?;
        rest = rest.get(len..)?;
    }
    (!total.is_zero()).then_some(total)
}

static LIVE_SERVICE_JOB_INTERVAL: tokio::sync::OnceCell<Duration> =
    tokio::sync::OnceCell::const_new();

/// [`SERVICE_JOB_SCRAPE_SETTLE`], returned ONLY after the live Prometheus
/// config proves it exceeds the deployed service-job scrape interval. The
/// live config is read once per process.
///
/// This is the fail-CLOSED binding of the in-tree copy. A cluster still
/// scraping at an older, slower cadence (an in-tree config change not yet
/// applied) makes every stability wait compare two reads inside one scrape —
/// passing, faster, having observed nothing. This turns that into a loud
/// failure instead.
///
/// # Panics
///
/// With [`SCRAPE_CONFIG_UNREADABLE`] if the config cannot be read or parsed,
/// and with [`SETTLE_NOT_ABOVE_INTERVAL`] if the settle does not exceed the
/// live interval. Never with a message that suggests shortening the settle.
pub async fn service_job_scrape_settle(prom: &PrometheusClient) -> Duration {
    let live = *LIVE_SERVICE_JOB_INTERVAL
        .get_or_init(|| async {
            let body = prom.status_config().await.unwrap_or_else(|e| {
                panic!(
                    "{SCRAPE_CONFIG_UNREADABLE}: could not read Prometheus /api/v1/status/config \
                     ({e}). Stability waits cannot be proved non-vacuous without the live \
                     scrape interval; this is an environment/config fault, not a test failure."
                )
            });
            max_service_job_scrape_interval(&body).unwrap_or_else(|e| {
                panic!(
                    "{SCRAPE_CONFIG_UNREADABLE}: the live Prometheus config has no readable \
                     scrape interval for the service jobs ({e:?}). Do not relax the extractor \
                     to make this pass: a parse that finds nothing is exactly the vacuous case."
                )
            })
        })
        .await;
    assert!(
        SERVICE_JOB_SCRAPE_SETTLE > live,
        "{SETTLE_NOT_ABOVE_INTERVAL}: SERVICE_JOB_SCRAPE_SETTLE ({SERVICE_JOB_SCRAPE_SETTLE:?}) \
         does not exceed the LIVE service-job scrape interval ({live:?}), so every stability \
         wait would compare two reads inside one scrape and pass having observed nothing. The \
         cluster is most likely running a Prometheus config older than the tree: apply it with \
         `kubectl apply -k infra/kubernetes/overlays/kind/observability/` (the generated \
         ConfigMap hash rolls the pod). If the interval was raised on purpose, raise \
         SERVICE_JOB_SCRAPE_INTERVAL_SECS to match. NEVER shorten the settle to make this pass."
    );
    SERVICE_JOB_SCRAPE_SETTLE
}

/// Poll until two consecutive per-instance reads, `settle` apart, are identical.
///
/// # This is a PRECONDITION, not a delta gate
///
/// It answers "has everything **that Prometheus has seen** settled?", so that a
/// LATER rise is attributable to the caller's own action. Note the qualifier: the
/// property delivered is attribution that is **sound for everything scraped
/// within the window**, not sound simply — see the two causes below for the
/// slice it does not reach. Without it, the
/// residual false-pass window documented on [`instances_exceeding_baseline`]
/// makes an unearned rise reachable — a pre-existing instance merely missing
/// from the baseline read reappears with its full prior value and looks like it
/// rose. That is reachable **single-threaded**; it is not only a concurrency
/// hazard, and no amount of test serialisation closes it.
///
/// # WHICH of the two causes this closes — it is one, not both
///
/// [`instances_exceeding_baseline`] names two ways an instance can be missing
/// from a baseline read, and this loop closes exactly one of them:
///
/// - **A transient empty result — CLOSED.** The instance reappears inside the
///   settle window, the two maps differ, and the routine non-equal round retries
///   until it is present in both.
/// - **A target unscraped beyond `query.lookback-delta` — NOT CLOSED, and this
///   loop structurally cannot close it.** Such a target is absent from *both*
///   reads, so the maps compare equal and this returns "stable" having never
///   observed that instance at all.
///
///   Stated at full size rather than rounded down to "narrow": nothing sets
///   `--query.lookback-delta` (the container args at
///   `infra/kubernetes/observability/prometheus-config.yaml` set only
///   `--config.file` and `--storage.tsdb.path`), so the gap is the **5m default**
///   against [`SERVICE_JOB_SCRAPE_SETTLE`] — about **fifty times** the window.
///   (It was about twenty times against the former 16s settle: shortening the
///   settle when the service jobs' scrape cadence dropped is what moved the
///   ratio. The residual itself is unchanged and structural — see below — so
///   do not read the larger figure as a new defect.) An overstatement on a
///   weak control gets caught; an overstatement on a strong one is what
///   survives, and this control is otherwise good.
///
///   **Do not reach for a longer settle — it cannot work.** The settle is
///   FLOORED by the scrape interval (below it, both reads come from one scrape
///   and this is vacuous) and would have to EXCEED 5m to span the gap, which no
///   caller can pay. The residual is structural, not a tuning error.
///
/// A caller that needs the stronger property — "every instance that EXISTS was
/// observed", not merely "everything observed has settled" — must establish it
/// itself, e.g. by checking the stabilised instance set against
/// `count(up{job="..."} == 1)`, which distinguishes *absent because gone* from
/// *absent because unscraped*. That is deliberately not done here: it would put a
/// Prometheus job-label coupling into a general-purpose loop on behalf of one
/// caller.
///
/// # `settle` is a CORRECTNESS parameter, not a budget
///
/// Pass the scrape interval of the job that produces `promql`'s series, plus a
/// margin. For the ac/gc/mc/mh service jobs that value is
/// [`SERVICE_JOB_SCRAPE_SETTLE`], obtained through
/// [`service_job_scrape_settle`], which first checks it against the LIVE
/// config. **A value at or below one scrape interval makes this VACUOUS rather than
/// merely fast**: both reads then come from the same scrape, are trivially
/// identical, and the loop returns having proved nothing. That failure is
/// invisible on the page, because [`instance_maps_equal`] looks correct at any
/// interval. Shortening this to "make the test faster" silently removes the
/// control.
///
/// # A non-equal round is ROUTINE, not evidence of interference
///
/// Counters are monotonic, so the other cause of two reads differing is an
/// instance *appearing* between them — which is this function's own success
/// path: the stale-scrape case it exists to catch is absent from read one and
/// present in read two, producing exactly one non-equal round **by
/// construction**. Pod rollover mid-test is the same shape. So it fires on
/// FAILURE TO CONVERGE ACROSS ROUNDS, never on a single inequality — budget
/// accordingly, and do not read "an idle cluster converges immediately" as
/// meaning the budget is padding. The margin is what the normal case consumes.
///
/// # `settle` and `timeout` are required parameters with NO DEFAULT, deliberately
///
/// A default would let a caller adopt a window it never reasoned about, and the
/// window is the whole correctness content of a stability wait: **too short and
/// the two reads fall inside one scrape interval, so they compare equal and this
/// loop returns "stable" having observed nothing.** Every downstream baseline is
/// then taken on an unstabilised snapshot. That is review-protocol §Assertion
/// Vacuity mechanism 1 — the check ran over input that could not disagree — and
/// it is precisely what a `DEFAULT_SETTLE` hoisted from an unrelated caller
/// would cause. Callers passing the same pair of values is exactly the adjacency
/// that makes such a hoist look like an obvious win.
///
/// **A SHARED VALUE PASSED EXPLICITLY IS NOT A DEFAULT.**
/// [`SERVICE_JOB_SCRAPE_SETTLE`] is one reasoned value, derived from the one
/// cadence every current caller's series is scraped at, and each caller still
/// names it at its own call site next to its own why-it-waits paragraph. What
/// this paragraph forbids is a value a caller ADOPTS BY OMISSION; a caller that
/// passes the service-job settle has chosen it, and a caller gating a series
/// from a different job class must choose differently (see that constant).
///
/// (Deliberately no count of those callers here. A restated count is the only
/// part of this paragraph that can rot, and it rots at the first addition — the
/// argument does not need it.)
///
/// Whether any two callers' values agree, and whether they must move together,
/// is knowable only at the call sites. **This loop makes no claim about it** —
/// stated rather than left silent, so a future reader does not fill the silence
/// in either direction.
///
/// Per-instance map comparison rather than a cluster-wide scalar is also
/// churn-robust: if a stale old-pod series expires between the two reads the maps
/// differ and we retry, and once the expired series is gone from both reads they
/// match. (A cluster-wide scalar would self-heal the same way, but re-introduces
/// the cross-pod `sum` the per-instance helpers exist to remove.) Fail-loud on a
/// Prometheus query error is inherited from `instance_counter_map`.
///
/// # `timeout` is a floor on the LAST round, not a ceiling on total time
///
/// The deadline is evaluated *after* a completed round, so a round that starts
/// just under the deadline runs to completion: the effective ceiling is
/// `timeout + settle + two query round trips`. Budget against that number, not
/// against `timeout` — with a 90s budget and [`SERVICE_JOB_SCRAPE_SETTLE`] the
/// real ceiling is about 96s (timeout + one settle + two query round trips). The two
/// delta polls in this module have the same post-round shape but overshoot only
/// by their poll interval, which is why it is called out here and not there.
///
/// Panics on timeout, inside this function, for the same reason as
/// [`poll_until_any_instance_above`]: a gate that hands its verdict to the call
/// site becomes a no-op the first time someone drops it.
#[must_use = "the returned map is the STABILISED snapshot and is what the caller must use as its \
              baseline; discarding it and re-reading separately yields a possibly-unstabilised \
              baseline, which is the exact hazard this function exists to prevent"]
pub async fn poll_until_stable(
    prom: &PrometheusClient,
    promql: &str,
    settle: Duration,
    timeout: Duration,
    timeout_message: impl Fn(&InstanceCounters, &InstanceCounters) -> String,
) -> InstanceCounters {
    let deadline = Instant::now() + timeout;
    loop {
        let first = prom.instance_counter_map(promql).await;
        tokio::time::sleep(settle).await;
        let second = prom.instance_counter_map(promql).await;
        if instance_maps_equal(&first, &second) {
            return second;
        }
        if Instant::now() > deadline {
            panic!("{}", timeout_message(&first, &second));
        }
    }
}

/// Read an EAGERLY-REGISTERED gauge per `instance`, polling until the series is
/// present on at least `min_instances` instances or `timeout` elapses.
///
/// For gauges a service publishes from process start (the MH egress-admission
/// gauges, for example): ABSENCE IS A FAILURE, never zero. That is the
/// difference from [`PrometheusClient::instance_counter_map`], where an empty
/// vector legitimately means "not observed yet". A query error panics (via
/// `instance_counter_map`), and a non-finite sample panics, so the three
/// readings — error, absent, value — never collapse.
///
/// The poll is a scrape-convergence bound (a new pod is scraped within one
/// interval), not a wall-clock assertion about the service.
///
/// # Panics
///
/// When fewer than `min_instances` instances carry the series by `timeout`.
pub async fn gauge_by_instance_present(
    prom: &PrometheusClient,
    metric: &str,
    min_instances: usize,
    timeout: Duration,
) -> InstanceCounters {
    let promql = format!("max by (instance) ({metric})");
    let deadline = Instant::now() + timeout;
    loop {
        let current = prom.instance_counter_map(&promql).await;
        if current.len() >= min_instances {
            return current;
        }
        assert!(
            Instant::now() < deadline,
            "gauge `{metric}` is present on {} instance(s), expected at least \
             {min_instances} within {timeout:?}. It is published eagerly at process \
             start, so absence means the pod is not up, not scraped, or the publish \
             path ran before the metrics recorder was installed — not 'zero'.",
            current.len()
        );
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

/// The `instance` entry that belongs to the pod at `pod_ip`, if present.
///
/// The metrics scrape's `instance` is `pod IP:port` (see [`InstanceCounters`]),
/// so a pod is matched on the `"<ip>:"` PREFIX — the colon is load-bearing:
/// without it `10.0.0.1` would also match `10.0.0.10:8083`, pinning the wrong
/// pod.
///
/// # Panics
///
/// When more than one instance matches: one pod exposes one metrics port, so
/// two matches mean the pin is not identifying a single handler.
#[must_use]
pub fn instance_for_pod_ip<'a>(
    map: &'a InstanceCounters,
    pod_ip: &str,
) -> Option<(&'a String, f64)> {
    let prefix = format!("{pod_ip}:");
    let mut matches = map
        .iter()
        .filter(|(instance, _)| instance.starts_with(&prefix));
    let first = matches.next().map(|(instance, value)| (instance, *value));
    let extra: Vec<&String> = matches.map(|(instance, _)| instance).collect();
    assert!(
        extra.is_empty(),
        "more than one instance matches pod IP {pod_ip}: {:?} and {extra:?} — the pin \
         does not identify one handler",
        first.map(|(instance, _)| instance)
    );
    first
}

/// Poll ONE pinned instance — the pod at `pod_ip` — until `predicate` holds on
/// its value, and return that value.
///
/// # Why pinned, and not [`poll_until_any_instance_above`]
///
/// "Some instance rose" is the right question when the test does not control
/// which handler acts. When the test CALLED a specific pod, it is the wrong
/// one: any-instance lets a rise on one handler and a fall on the other satisfy
/// a single pod's before/after, or lets another suite's activity satisfy it.
///
/// # Every read is a sampled, eventually-consistent value
///
/// Prometheus scrapes on an interval, so the value may lag the service. The
/// poll is a scrape-convergence bound, never a wall-clock assertion about the
/// service.
///
/// # Only for values no other suite can move against the predicate
///
/// Pinning isolates the POD, not the test: suites share pods. Use this for
/// presence, for monotonic counters (`>= own baseline + n`), and for values
/// published once from config. Never assert the VALUE of a pod-wide occupancy
/// gauge (e.g. `mh_media_registered_meetings`, `mh_media_egress_edges`):
/// concurrent suites move it in both directions, so a rise or fall may never be
/// observed. Prove per-meeting state from the service's direct replies instead,
/// and put exact gauge accounting in an in-process test that owns the recorder.
///
/// # Panics
///
/// On timeout, with `phase` first (a distinct token per call site, so a
/// never-observed rise triages differently from a missing fall), then the
/// expectation, the pinned instance's last value — or ABSENT — and every
/// instance present. A query error panics via
/// [`PrometheusClient::instance_counter_map`].
#[allow(clippy::too_many_arguments)]
pub async fn poll_until_pinned_instance(
    prom: &PrometheusClient,
    promql: &str,
    pod_ip: &str,
    timeout: Duration,
    interval: Duration,
    phase: &str,
    expectation: &str,
    predicate: impl Fn(f64) -> bool,
) -> f64 {
    let deadline = Instant::now() + timeout;
    loop {
        let current = prom.instance_counter_map(promql).await;
        let pinned = instance_for_pod_ip(&current, pod_ip).map(|(_, value)| value);
        if let Some(value) = pinned {
            if predicate(value) {
                return value;
            }
        }
        if Instant::now() > deadline {
            let last = pinned.map_or_else(|| "ABSENT".to_string(), |v| v.to_string());
            panic!(
                "{phase}: `{promql}` on the pod at {pod_ip} did not reach {expectation} within \
                 {timeout:?}; last pinned value {last}; all instances {}",
                format_instance_map(&current)
            );
        }
        tokio::time::sleep(interval).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- live scrape-interval precondition --------------------------------

    fn status_config_body(yaml: &str) -> String {
        serde_json::json!({ "status": "success", "data": { "yaml": yaml } }).to_string()
    }

    const PER_JOB: &str = "global:\n  scrape_interval: 15s\nscrape_configs:\n\
        - job_name: ac-service\n  scrape_interval: 5s\n\
        - job_name: gc-service\n  scrape_interval: 5s\n\
        - job_name: mc-service\n  scrape_interval: 5s\n\
        - job_name: mh-service\n  scrape_interval: 5s\n\
        - job_name: otel-collector\n  scrape_interval: 10s\n\
        - job_name: kubelet\n  scrape_interval: 15s\n";

    #[test]
    fn service_job_interval_is_the_per_job_value_not_global_nor_other_jobs() {
        assert_eq!(
            max_service_job_scrape_interval(&status_config_body(PER_JOB)),
            Ok(Duration::from_secs(5)),
            "the 10s otel-collector and 15s global/kubelet classes must not leak in"
        );
    }

    #[test]
    fn service_job_interval_falls_back_to_global_and_takes_the_max() {
        let yaml = "global:\n  scrape_interval: 15s\nscrape_configs:\n\
            - job_name: ac-service\n\
            - job_name: gc-service\n  scrape_interval: 5s\n\
            - job_name: mc-service\n  scrape_interval: 5s\n\
            - job_name: mh-service\n  scrape_interval: 5s\n";
        assert_eq!(
            max_service_job_scrape_interval(&status_config_body(yaml)),
            Ok(Duration::from_secs(15))
        );
    }

    /// FIRE: the stale-cluster case the precondition exists for.
    #[test]
    fn a_cluster_still_scraping_at_15s_is_not_below_the_settle() {
        let yaml = PER_JOB.replace("scrape_interval: 5s", "scrape_interval: 15s");
        let live = max_service_job_scrape_interval(&status_config_body(&yaml)).unwrap();
        assert!(SERVICE_JOB_SCRAPE_SETTLE <= live, "would panic, as it must");
    }

    #[test]
    fn the_settle_exceeds_the_in_tree_interval_by_the_margin() {
        assert_eq!(
            SERVICE_JOB_SCRAPE_SETTLE,
            Duration::from_secs(SERVICE_JOB_SCRAPE_INTERVAL_SECS) + SCRAPE_SETTLE_MARGIN
        );
        assert!(
            SCRAPE_SETTLE_MARGIN >= Duration::from_secs(1),
            "margin is absolute"
        );
    }

    #[test]
    fn unreadable_configs_fail_closed_with_a_distinct_error() {
        assert_eq!(
            max_service_job_scrape_interval("not json"),
            Err(ScrapeIntervalError::NotPrometheusConfig)
        );
        // Escaped-YAML-read-as-lines trap: the envelope WITHOUT data.yaml.
        assert_eq!(
            max_service_job_scrape_interval(r#"{"status":"success","data":{}}"#),
            Err(ScrapeIntervalError::NotPrometheusConfig)
        );
        let missing = PER_JOB.replace("mh-service", "mh-renamed");
        assert_eq!(
            max_service_job_scrape_interval(&status_config_body(&missing)),
            Err(ScrapeIntervalError::ServiceJobMissing(
                "mh-service".to_string()
            ))
        );
        let garbage = PER_JOB.replacen("scrape_interval: 5s", "scrape_interval: soon", 1);
        assert_eq!(
            max_service_job_scrape_interval(&status_config_body(&garbage)),
            Err(ScrapeIntervalError::IntervalUnparseable(
                "ac-service".to_string()
            ))
        );
        let no_interval_anywhere = "scrape_configs:\n- job_name: ac-service\n\
            - job_name: gc-service\n- job_name: mc-service\n- job_name: mh-service\n";
        assert_eq!(
            max_service_job_scrape_interval(&status_config_body(no_interval_anywhere)),
            Err(ScrapeIntervalError::IntervalUnparseable(
                "ac-service".to_string()
            ))
        );
    }

    #[test]
    fn prometheus_durations_parse_and_reject() {
        assert_eq!(
            parse_prometheus_duration("5s"),
            Some(Duration::from_secs(5))
        );
        assert_eq!(
            parse_prometheus_duration("1m30s"),
            Some(Duration::from_secs(90))
        );
        assert_eq!(
            parse_prometheus_duration("500ms"),
            Some(Duration::from_millis(500))
        );
        for bad in ["", "5", "s", "5x", "0s", "-5s"] {
            assert_eq!(parse_prometheus_duration(bad), None, "{bad:?}");
        }
    }

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

    /// The wrapper AGREES with the rule on the rollover case — the `true` arm.
    ///
    /// # Why this case, specifically
    ///
    /// The plausible wrong re-derivation iterates the BASELINE's keys and looks
    /// each up in `current`. That misses a brand-new instance entirely and
    /// returns `false` where the rule returns `true` — and a test built on two
    /// stable instances passes against exactly that bug. The rollover case is the
    /// one that separates them.
    ///
    /// This pins `any_instance_exceeds_baseline` as a genuine wrapper rather than
    /// a second implementation that happens to agree today, which is what keeps
    /// it in lockstep with its TypeScript mirror
    /// (`packages/web-app/e2e/instanceCounters.ts`) when the convention changes
    /// on one side only.
    #[test]
    fn wrapper_and_rule_agree_on_the_rollover_case() {
        let baseline =
            results_to_instance_map(TEST_PROMQL, &instance_response(&[("10.0.0.1:8081", "5")]));
        let current =
            results_to_instance_map(TEST_PROMQL, &instance_response(&[("10.0.0.2:8081", "1")]));

        let risen = instances_exceeding_baseline(&baseline, &current);
        assert_eq!(risen, vec!["10.0.0.2:8081".to_string()]);
        assert_eq!(
            any_instance_exceeds_baseline(&baseline, &current),
            !risen.is_empty()
        );
        assert!(any_instance_exceeds_baseline(&baseline, &current));
    }

    /// The wrapper agrees with the rule on the NEGATIVE case — the `false` arm.
    ///
    /// Without this arm the pair of assertions above passes against a wrapper
    /// hardcoded to `true`, so the agreement claim would be vacuous.
    #[test]
    fn wrapper_and_rule_agree_when_nothing_rose() {
        let snapshot = results_to_instance_map(
            TEST_PROMQL,
            &instance_response(&[("10.0.0.1:8081", "5"), ("10.0.0.2:8081", "3")]),
        );
        let risen = instances_exceeding_baseline(&snapshot, &snapshot);
        assert!(risen.is_empty());
        assert_eq!(
            any_instance_exceeds_baseline(&snapshot, &snapshot),
            !risen.is_empty()
        );
        assert!(!any_instance_exceeds_baseline(&snapshot, &snapshot));
    }

    /// The risen set is SORTED and names every riser, so a caller can key on a
    /// specific instance and a diagnostic is deterministic across runs.
    #[test]
    fn the_risen_set_is_sorted_and_complete() {
        let baseline = results_to_instance_map(
            TEST_PROMQL,
            &instance_response(&[("10.0.0.9:8081", "1"), ("10.0.0.1:8081", "1")]),
        );
        let current = results_to_instance_map(
            TEST_PROMQL,
            &instance_response(&[
                ("10.0.0.9:8081", "2"),
                ("10.0.0.1:8081", "2"),
                ("10.0.0.5:8081", "7"),
            ]),
        );
        assert_eq!(
            instances_exceeding_baseline(&baseline, &current),
            vec![
                "10.0.0.1:8081".to_string(),
                "10.0.0.5:8081".to_string(),
                "10.0.0.9:8081".to_string(),
            ]
        );
    }

    /// Exactly-one-rose is expressible as a membership read over the rule, which
    /// is why no separate named-instance predicate exists.
    #[test]
    fn a_named_instance_check_is_a_membership_read_over_the_one_rule() {
        let baseline = results_to_instance_map(
            TEST_PROMQL,
            &instance_response(&[("10.0.0.1:8081", "1"), ("10.0.0.2:8081", "1")]),
        );
        let current = results_to_instance_map(
            TEST_PROMQL,
            &instance_response(&[("10.0.0.1:8081", "1"), ("10.0.0.2:8081", "4")]),
        );
        let risen = instances_exceeding_baseline(&baseline, &current);
        assert_eq!(risen, vec!["10.0.0.2:8081".to_string()]);
        assert!(risen.iter().any(|i| i == "10.0.0.2:8081"));
        assert!(!risen.iter().any(|i| i == "10.0.0.1:8081"));
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

    #[test]
    fn instance_for_pod_ip_matches_on_the_ip_and_colon_only() {
        let map: InstanceCounters = std::collections::HashMap::from([
            ("10.0.0.10:8083".to_string(), 3.0),
            ("10.0.0.1:8083".to_string(), 7.0),
        ]);
        assert_eq!(
            instance_for_pod_ip(&map, "10.0.0.1").map(|(_, v)| v),
            Some(7.0),
            "10.0.0.1 must not pin 10.0.0.10's series"
        );
        assert_eq!(
            instance_for_pod_ip(&map, "10.0.0.10").map(|(_, v)| v),
            Some(3.0)
        );
        assert!(instance_for_pod_ip(&map, "10.0.0.2").is_none());
    }

    #[test]
    #[should_panic(expected = "more than one instance matches pod IP")]
    fn instance_for_pod_ip_refuses_an_ambiguous_pin() {
        let map: InstanceCounters = std::collections::HashMap::from([
            ("10.0.0.1:8083".to_string(), 1.0),
            ("10.0.0.1:9090".to_string(), 2.0),
        ]);
        let _ = instance_for_pod_ip(&map, "10.0.0.1");
    }
}
