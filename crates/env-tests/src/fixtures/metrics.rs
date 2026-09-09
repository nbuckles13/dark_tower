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

/// How many instances rose above their own baseline — the decision a caller makes
/// when it must ATTRIBUTE a rise to its own action rather than merely observe one.
///
/// Exhaustive on purpose. The three arms have three different owners, and
/// collapsing any two of them is how an attribution gate stops attributing:
/// [`Self::None`] is "we were not observed", [`Self::Many`] is "we cannot tell
/// which rise was ours", and only [`Self::One`] licenses naming an instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RisenOutcome {
    /// Nothing rose. The caller's action was not observed (yet, or at all).
    None,
    /// Exactly one instance rose, and it is named.
    One(String),
    /// More than one rose, sorted. **Never reduce this to its first element** —
    /// picking one would assert an attribution the observation does not support,
    /// which is the whole failure this type exists to make unrepresentable.
    Many(Vec<String>),
}

/// Classify the risen set into the attribution trichotomy.
///
/// Pure, and unit-tested on **all three arms** — the [`RisenOutcome::Many`] arm
/// especially, because it is the one whose degradation to "just take the first"
/// would rebuild a wrong-instance gate that no test in the tree would notice.
/// The async poll that consumes this ([`poll_until_exactly_one_instance_rose`])
/// is a thin loop precisely so this decision is reachable without a cluster.
#[must_use]
pub fn exactly_one_risen(baseline: &InstanceCounters, current: &InstanceCounters) -> RisenOutcome {
    let mut risen = instances_exceeding_baseline(baseline, current);
    match risen.len() {
        0 => RisenOutcome::None,
        1 => RisenOutcome::One(risen.remove(0)),
        _ => RisenOutcome::Many(risen),
    }
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

/// Poll until ONE NAMED instance's value exceeds its own `baseline` value.
///
/// The instance-keyed sibling of [`poll_until_any_instance_above`], for a caller
/// that has already established WHICH instance it is asking about. The
/// instance-agnostic form answers "somebody applied a policy"; this answers "the
/// one I am talking to did" — and those are different questions whenever the
/// fleet has more than one pod. A gate that asks the first when it means the
/// second is satisfied by a pod the client never touched, which is
/// story-task-25's defect in miniature.
///
/// # Why a sibling and not a parameterisation of the existing loop
///
/// [`poll_until_any_instance_above`] gates three live, non-`#[ignore]`d tests and
/// has no unit coverage of its own body. Re-expressing it on a shared generic
/// would make "no behaviour change" a claim to be argued rather than a fact:
/// a weakened gate does not fail, it passes EARLIER, silently. Leaving that body
/// untouched makes the preservation structural. The duplication here is ~10
/// lines of loop against that, and both consume the one comparison rule
/// ([`instances_exceeding_baseline`]), which is where drift would actually hurt.
///
/// Panics on timeout, inside this function — it never returns a value for a
/// caller to interpret or drop. A gate that returns `bool` becomes a no-op the
/// first time someone writes `let _ =`, and the tests keep passing while they
/// stop testing. Same discipline as [`instance_map_from_query`], which panics on
/// a query error rather than reading it as zero.
pub async fn poll_until_instance_above(
    prom: &PrometheusClient,
    promql: &str,
    baseline: &InstanceCounters,
    instance: &str,
    timeout: Duration,
    interval: Duration,
    timeout_message: impl Fn(&InstanceCounters) -> String,
) {
    let deadline = Instant::now() + timeout;
    loop {
        let current = prom.instance_counter_map(promql).await;
        if instances_exceeding_baseline(baseline, &current)
            .iter()
            .any(|risen| risen == instance)
        {
            return;
        }
        if Instant::now() > deadline {
            panic!("{}", timeout_message(&current));
        }
        tokio::time::sleep(interval).await;
    }
}

/// Poll until EXACTLY ONE instance has risen above its own baseline, and return
/// it.
///
/// The attribution primitive: a caller that has just taken an action and needs to
/// know **which** instance served it, rather than merely that somebody did. The
/// decision is [`exactly_one_risen`], which is pure and unit-tested on all three
/// arms; this is only the loop around it.
///
/// Fails closed on BOTH degenerate outcomes, with the caller's own message for
/// each, because they have different owners: nothing risen within `timeout` is
/// usually an environment or scrape problem, while more than one risen is a test
/// isolation problem. **It never returns a `Many` and never reduces one** —
/// picking an element would assert an attribution the observation does not
/// support, and would silently produce a gate keyed on the wrong instance.
///
/// Panics inside this function rather than returning a `Result`, for the same
/// reason as [`poll_until_instance_above`]: a gate that hands its verdict to the
/// call site becomes a no-op the first time someone drops it.
pub async fn poll_until_exactly_one_instance_rose(
    prom: &PrometheusClient,
    promql: &str,
    baseline: &InstanceCounters,
    timeout: Duration,
    interval: Duration,
    none_message: impl Fn(&InstanceCounters) -> String,
    ambiguous_message: impl Fn(&[String], &InstanceCounters) -> String,
) -> String {
    let deadline = Instant::now() + timeout;
    loop {
        let current = prom.instance_counter_map(promql).await;
        match exactly_one_risen(baseline, &current) {
            RisenOutcome::One(instance) => return instance,
            // Ambiguity is terminal immediately: waiting cannot un-observe the
            // extra riser, and every further second makes the misattribution
            // window wider rather than narrower.
            RisenOutcome::Many(risen) => panic!("{}", ambiguous_message(&risen, &current)),
            RisenOutcome::None => {
                if Instant::now() > deadline {
                    panic!("{}", none_message(&current));
                }
            }
        }
        tokio::time::sleep(interval).await;
    }
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
///   against a 16s settle — nearly **twenty times** the window. An overstatement
///   on a weak control gets caught; an overstatement on a strong one is what
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
/// Pass `Prometheus scrape_interval + margin` — 16s against the cluster's 15s
/// `scrape_interval` (`infra/kubernetes/observability/prometheus-config.yaml`).
/// **A value at or below one scrape interval makes this VACUOUS rather than
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
/// against `timeout` — with the 16s/90s pair the real ceiling is ~106s. The two
/// delta polls in this module have the same post-round shape but overshoot only
/// by their poll interval, which is why it is called out here and not there.
///
/// Panics on timeout, inside this function, for the same reason as
/// [`poll_until_instance_above`].
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

    /// The attribution trichotomy, `None` arm: nothing rose.
    #[test]
    fn exactly_one_risen_reports_none_when_nothing_rose() {
        let snapshot = results_to_instance_map(
            TEST_PROMQL,
            &instance_response(&[("10.0.0.1:8081", "5"), ("10.0.0.2:8081", "3")]),
        );
        assert_eq!(
            exactly_one_risen(&snapshot, &snapshot),
            RisenOutcome::None,
            "no instance beat its own baseline, so nothing may be attributed"
        );
    }

    /// The `One` arm: exactly one rose, and it is the one named — not merely
    /// "some instance".
    #[test]
    fn exactly_one_risen_names_the_single_riser() {
        let baseline = results_to_instance_map(
            TEST_PROMQL,
            &instance_response(&[("10.0.0.1:8081", "1"), ("10.0.0.2:8081", "1")]),
        );
        let current = results_to_instance_map(
            TEST_PROMQL,
            &instance_response(&[("10.0.0.1:8081", "1"), ("10.0.0.2:8081", "4")]),
        );
        assert_eq!(
            exactly_one_risen(&baseline, &current),
            RisenOutcome::One("10.0.0.2:8081".to_string())
        );
    }

    /// The `Many` arm — **the one that must never degrade to "take the first"**.
    ///
    /// This is the arm whose collapse rebuilds a wrong-instance gate: picking an
    /// element asserts an attribution the observation does not support, and the
    /// resulting gate would be keyed on a pod the caller never touched while
    /// looking entirely correct. Asserted as the whole sorted set, so an
    /// implementation that returned `One(first)` fails here rather than passing.
    #[test]
    fn exactly_one_risen_reports_all_risers_and_never_picks_one() {
        let baseline = results_to_instance_map(
            TEST_PROMQL,
            &instance_response(&[("10.0.0.9:8081", "1"), ("10.0.0.1:8081", "1")]),
        );
        let current = results_to_instance_map(
            TEST_PROMQL,
            &instance_response(&[("10.0.0.9:8081", "2"), ("10.0.0.1:8081", "2")]),
        );
        let outcome = exactly_one_risen(&baseline, &current);
        assert_eq!(
            outcome,
            RisenOutcome::Many(vec![
                "10.0.0.1:8081".to_string(),
                "10.0.0.9:8081".to_string(),
            ])
        );
        // Stated as its own assertion: `Many` is not `One` of anything, however
        // tempting the first element looks.
        assert!(!matches!(outcome, RisenOutcome::One(_)));
    }

    /// A brand-new instance (absent from baseline) counts as a riser here, the
    /// same way it does in the rule — the trichotomy adds no second convention.
    #[test]
    fn exactly_one_risen_follows_the_rules_absent_from_baseline_convention() {
        let baseline =
            results_to_instance_map(TEST_PROMQL, &instance_response(&[("10.0.0.1:8081", "5")]));
        let current =
            results_to_instance_map(TEST_PROMQL, &instance_response(&[("10.0.0.2:8081", "1")]));
        assert_eq!(
            exactly_one_risen(&baseline, &current),
            RisenOutcome::One("10.0.0.2:8081".to_string())
        );
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
