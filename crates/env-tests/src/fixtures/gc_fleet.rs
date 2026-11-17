//! The live GC replica set and GC's deployed config keys: the ONE home for both,
//! shared by `tests/03_gc_deployment_config.rs` (config wiring, config gauges,
//! availability) and `tests/44_gc_mh_registration.rs` (the RegisterMH rule
//! against real slot addresses).
//!
//! GC has one Deployment with N replicas and no `instance` label, so pods are
//! found by [`GC_SELECTOR`], and every serving replica is checked on its own.
//! One pod never stands in for all of them.

use std::time::Duration;

use serde_json::Value;

use crate::eventual::ConsistencyCategory;
use crate::fixtures::deployment_config::{cm_value, parse_positive, LivePod};
use crate::fixtures::kube::{
    is_terminating, max_termination_grace, pods_json, resolve_generation, settled_serving_pods,
    REDEPLOY_HINT,
};
use crate::fixtures::metrics::{
    agreed_value, format_instance_map, instance_for_pod_ip, PrometheusClient,
};

/// The label every GC pod carries (`infra/services/gc-service/deployment.yaml`).
pub const GC_SELECTOR: &str = "app=gc-service";
/// The GC container name in the pod spec.
pub const GC_CONTAINER: &str = "gc-service";
/// Generator base name. The live name carries a hash suffix (ADR-0038 §2) and
/// is resolved from the pod specs.
pub const GC_CONFIGMAP: &str = "gc-service-config";

/// The RegisterMH endpoint-allowlist keys (story 4 GC-2, R-25; ADR-0039 §7
/// arm 0). All are REQUIRED in GC.
///
/// This is a MIRROR of `gc_service::config::MH_ENDPOINT_ALLOWLIST_KEYS`. The
/// suite links no service crate (ADR-0028), so the list cannot be imported.
/// The hermetic test
/// `tests::allowlist_keys_match_gc_config` reads the reader's source and fails
/// on drift in either direction.
pub const MH_ENDPOINT_ALLOWLIST_KEYS: [&str; 5] = [
    "GC_MH_WT_ALLOWED_HOSTS",
    "GC_MH_WT_ALLOWED_PORTS",
    "GC_MH_GRPC_ALLOWED_SCHEMES",
    "GC_MH_GRPC_ALLOWED_CIDRS",
    "GC_MH_GRPC_ALLOWED_PORTS",
];

/// The REQUIRED MH registry keys (story 4 GC-1 R-24/R-26, plus GC-3's
/// `GC_MH_MAX_EXCLUDE_HANDLER_IDS`, R-23). Each is a positive integer.
///
/// This is a MIRROR of `gc_service::config::MH_REGISTRY_KEYS` (the suite links
/// no service crate, ADR-0028). The hermetic test
/// `tests::registry_keys_match_gc_config` reads the reader's source and fails
/// on drift in either direction.
pub const MH_REGISTRY_KEYS: [&str; 5] = [
    INTERVAL_KEY,
    MULTIPLIER_KEY,
    "GC_MH_REGISTRY_RETENTION_SECONDS",
    INITIAL_HANDLERS_KEY,
    "GC_MH_MAX_EXCLUDE_HANDLER_IDS",
];

/// GC-1's load-report interval key (ms).
pub const INTERVAL_KEY: &str = "GC_MH_LOAD_REPORT_INTERVAL_MS";
/// GC-1's staleness multiplier key: `B = k × I`.
pub const MULTIPLIER_KEY: &str = "GC_MH_STALENESS_MULTIPLIER";
/// GC's per-meeting initial handler count key (story 4 MC-B2 placement).
pub const INITIAL_HANDLERS_KEY: &str = "GC_MH_INITIAL_HANDLERS_PER_MEETING";

/// Margin over the deployed termination grace. After SIGKILL at the end of the
/// grace, the kubelet still has to report the container gone and the API
/// server has to remove the pod object. This is a failure-only ceiling: it is
/// spent only when a pod is genuinely slow to go.
pub const POD_REMOVAL_MARGIN: Duration = Duration::from_secs(15);

/// A settled GC pod set: the live (non-terminating) pod documents, their
/// serving view, and the ONE `gc-service-config` generation they all
/// reference. All three come from ONE listing, so the specs checked, the IPs
/// pinned and the ConfigMap read belong to the same pods.
pub struct GcPods {
    pub items: Vec<Value>,
    pub serving: Vec<LivePod>,
    pub configmap: String,
}

impl GcPods {
    /// The serving pod names, sorted. This is the replica-set identity two
    /// listings are compared by.
    #[must_use]
    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.serving.iter().map(|p| p.name.clone()).collect();
        names.sort();
        names
    }
}

/// Wait out a GC rollout's tail, then read the pod set.
///
/// `rollout status` returns while an old ReplicaSet's pod is still inside its
/// termination grace, so the first run after every GC deploy can see a
/// Terminating pod. The wait is bounded by the largest
/// `terminationGracePeriodSeconds` on the live pods (including the Terminating
/// one), read from the API server and never retyped, plus
/// [`POD_REMOVAL_MARGIN`]. Past the bound, a pod still Terminating or not Ready
/// is a loud PRECONDITION (`gc-pods-unserving`); it is never dropped.
///
/// `resolve_generation` then refuses a bare name, zero generations, or more
/// than one. The serving replicas must run ONE config generation.
///
/// # Panics
/// PRECONDITION, as above.
pub async fn live_gc_pods() -> GcPods {
    let first = tokio::task::spawn_blocking(|| pods_json(GC_SELECTOR))
        .await
        .expect("kubectl listing task");
    let grace = max_termination_grace(&first, GC_SELECTOR)
        .unwrap_or_else(|e| panic!("PRECONDITION (gc-pods-unserving): {e}. {REDEPLOY_HINT}."));
    let (doc, serving) =
        settled_serving_pods(GC_SELECTOR, grace + POD_REMOVAL_MARGIN, "gc-pods-unserving").await;
    let items: Vec<Value> = doc
        .get("items")
        .and_then(Value::as_array)
        .expect("partition_serving accepted the document, so it has `items`")
        .iter()
        .filter(|p| !is_terminating(p))
        .cloned()
        .collect();
    assert_eq!(
        items.len(),
        serving.len(),
        "PRECONDITION: the settled GC listing holds a pod that is neither serving nor \
         terminating"
    );
    let configmap = resolve_generation(&doc, GC_SELECTOR, GC_CONFIGMAP)
        .unwrap_or_else(|e| panic!("PRECONDITION: {e}"));
    GcPods {
        items,
        serving,
        configmap,
    }
}

/// A deployed GC-1 value, strictly positive, read from the live generation.
///
/// # Panics
/// The value is absent or not a positive integer.
#[must_use]
pub fn deployed(cm: &Value, key: &str) -> u64 {
    parse_positive(key, &cm_value(cm, GC_CONFIGMAP, key), GC_CONFIGMAP)
        .unwrap_or_else(|e| panic!("{e}"))
}

/// `(B in ms, I in ms, k)` from the live generation: `B = k × I`, the
/// expression `MhRegistryConfig::new` uses. Overflow is a loud failure, not a
/// wrap.
///
/// # Panics
/// A key is invalid, or `k × I` overflows.
#[must_use]
pub fn deployed_bound_and_interval_ms(cm: &Value) -> (u64, u64, u64) {
    let interval_ms = deployed(cm, INTERVAL_KEY);
    let k = deployed(cm, MULTIPLIER_KEY);
    let bound_ms = interval_ms.checked_mul(k).unwrap_or_else(|| {
        panic!("{MULTIPLIER_KEY}={k} × {INTERVAL_KEY}={interval_ms} overflows u64")
    });
    (bound_ms, interval_ms, k)
}

/// Per-replica healthy MH count (summed over `region` WITHIN a replica).
pub const HEALTHY_PER_REPLICA_PROMQL: &str =
    "sum by (instance) (gc_mh_available{state=\"healthy\"})";

// ANCHOR (DRY): the removed-MH settle predicate has two homes that cannot share
// code: this fixture (Rust, through Prometheus) and
// `infra/kind/scripts/deploy.sh:wait_removed_mh_unselectable` (bash, through the
// apiserver proxy). The tokens both must use are the `SETTLE_*` constants below.
// `tests::settle_predicate_matches_deploy_sh` reads deploy.sh and fails on drift.

/// The registry-count series.
pub const SETTLE_HEALTHY_SERIES: &str = "gc_mh_available";
/// The `gc_mh_available` states a FRESH row can hold while it is, or can
/// become, selectable. `pending` must be included: a just-registered
/// replacement sits in `pending` until its first load report, and counting only
/// `healthy` would let a removed MH's still-healthy dead row hide behind it
/// (healthy == live while the dead row is selectable). `draining` is never
/// selectable (and after MH-3 it is what a removed MH reports); `unhealthy` is
/// a row past B, or one marked unhealthy.
pub const SETTLE_STATES: &str = "pending|healthy|degraded";
/// The positive-control series: a SUCCESSFUL `gc_mh_available` recompute.
pub const SETTLE_RECOMPUTE_SERIES: &str = "gc_db_queries_total";
pub const SETTLE_RECOMPUTE_OPERATION: &str = "count_available_mh";
pub const SETTLE_RECOMPUTE_SUCCESS: &str = "status=\"success\"";
/// The `mh.sh list` slot state counted as LIVE. Only `serving`: MH is Ready
/// (= Available) only after GC accepted its registration, so serving is the
/// population that can be healthy. A `starting` MH can't be healthy yet, and
/// counting it would let a still-selectable removed row hide behind it.
pub const SETTLE_LIVE_STATE: &str = "serving";

/// Successful recomputes the wait must observe past its first read. GC records
/// the success counter BEFORE it sets the gauge, so one recompute can be in
/// flight at the baseline read; +2 guarantees a WHOLE recompute that started
/// after the baseline.
pub const SETTLE_RECOMPUTES: f64 = 2.0;

/// Per-replica count of rows in [`SETTLE_STATES`] (summed over `region` within a
/// replica).
#[must_use]
pub fn selectable_per_replica_promql() -> String {
    format!("sum by (instance) ({SETTLE_HEALTHY_SERIES}{{state=~\"{SETTLE_STATES}\"}})")
}

/// Per-replica count of successful `gc_mh_available` recomputes.
#[must_use]
pub fn recompute_success_per_replica_promql() -> String {
    format!(
        "sum by (instance) ({SETTLE_RECOMPUTE_SERIES}{{operation=\"{SETTLE_RECOMPUTE_OPERATION}\",{SETTLE_RECOMPUTE_SUCCESS}}})"
    )
}

/// PURE: the baseline to compare against this poll. A counter BELOW its
/// baseline means the GC process restarted (Prometheus counters reset to ~0),
/// so the baseline becomes 0 and the restarted process must itself recompute
/// [`SETTLE_RECOMPUTES`] times. Without this a container restart mid-wait is a
/// false red until the deadline. An unseen baseline adopts the first present
/// sample (never 0: the counter is a lifetime total).
#[must_use]
pub fn rebaseline(base: Option<f64>, now: Option<f64>) -> Option<f64> {
    match (base, now) {
        (Some(b), Some(n)) if n < b => Some(0.0),
        (None, n) => n,
        (b, _) => b,
    }
}

/// The staleness bound each running GC publishes (the process value; 03 pins
/// it equal to the live ConfigMap's k x I).
pub const STALENESS_PER_REPLICA_PROMQL: &str =
    "max by (instance) (gc_mh_staleness_threshold_seconds)";

/// GC's health-checker tick: its ONE home is this source constant (the GC
/// image is built from this tree; deploy.sh reads the same line).
pub const GC_HEALTH_TICK_SRC: &str = "crates/gc-service/src/tasks/generic_health_checker.rs";

const SETTLE_POLL_INTERVAL: Duration = Duration::from_secs(2);

/// PURE: `DEFAULT_CHECK_INTERVAL_SECONDS` from the health checker's source.
///
/// # Errors
/// The declaration is absent, or not a positive integer.
pub fn gc_health_tick(source: &str) -> Result<Duration, String> {
    source
        .lines()
        .find_map(|l| {
            l.trim()
                .strip_prefix("pub const DEFAULT_CHECK_INTERVAL_SECONDS: u64 = ")?
                .strip_suffix(';')
        })
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .map(Duration::from_secs)
        .ok_or_else(|| {
            format!(
                "no `pub const DEFAULT_CHECK_INTERVAL_SECONDS: u64 = <n>;` in {GC_HEALTH_TICK_SRC}"
            )
        })
}

/// One replica's view at one poll.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReplicaSettle {
    /// `gc_mh_available` over [`SETTLE_STATES`], summed over regions, or `None`
    /// if absent.
    pub selectable: Option<f64>,
    /// Successful recomputes at the first read of this replica's series, or
    /// `None` if that series has not been seen yet. Never defaulted to 0: the
    /// counter is a lifetime total, so a 0 baseline would pass on the first
    /// present sample with no recompute after the teardown.
    pub recompute_baseline: Option<f64>,
    /// Successful recomputes now, or `None` if absent.
    pub recompute_now: Option<f64>,
}

/// PURE: is this replica settled? It must have run a WHOLE recompute since the
/// wait started: a success counter at least [`SETTLE_RECOMPUTES`] past its
/// baseline. That is the positive control; without it, a frozen zero-init
/// gauge (0 <= live) would pass having observed nothing. And its count of
/// [`SETTLE_STATES`] rows must be at most `live`.
#[must_use]
pub fn replica_settled(r: ReplicaSettle, live: f64) -> bool {
    let recomputed = r
        .recompute_now
        .zip(r.recompute_baseline)
        .is_some_and(|(now, base)| now > 0.0 && now >= base + SETTLE_RECOMPUTES);
    recomputed && r.selectable.is_some_and(|n| n <= live)
}

/// The FAILURE-ONLY budget for a pool change to settle end to end, as GC
/// sees it: B (the largest `gc_mh_staleness_threshold_seconds` the live GC
/// replicas publish) + two GC health-checker ticks (read from
/// [`GC_HEALTH_TICK_SRC`]) + the MetricsScrape ceiling. ONE home: the
/// removal-settle wait below and `mh_pool::MhPool::assert_baseline` both use
/// it, so neither re-derives GC's staleness.
///
/// # Errors
/// The tick source is unreadable, or a GC replica publishes no staleness
/// bound.
pub async fn removal_settle_budget(prom: &PrometheusClient) -> Result<Duration, String> {
    removal_settle_budget_for(prom, &live_gc_pods().await).await
}

async fn removal_settle_budget_for(
    prom: &PrometheusClient,
    gc: &GcPods,
) -> Result<Duration, String> {
    let tick_path = crate::repo_root().join(GC_HEALTH_TICK_SRC);
    let tick = std::fs::read_to_string(&tick_path)
        .map_err(|e| {
            format!(
                "SETTLE (removed-mh-still-selectable): read {}: {e}",
                tick_path.display()
            )
        })
        .and_then(|src| gc_health_tick(&src))?;

    let bound_s = gc_replica_values(prom, STALENESS_PER_REPLICA_PROMQL, gc)
        .await
        .map_err(|e| format!("SETTLE (removed-mh-still-selectable): {e}"))?
        .into_iter()
        .map(|(_, b)| b)
        .fold(0.0_f64, f64::max);
    Ok(Duration::from_secs_f64(bound_s) + 2 * tick + ConsistencyCategory::MetricsScrape.timeout())
}

/// The load-report interval each running GC publishes (story 4 GC-1;
/// `GC_MH_LOAD_REPORT_INTERVAL_MS`), per replica.
pub const LOAD_REPORT_INTERVAL_PER_REPLICA_PROMQL: &str =
    "max by (instance) (gc_mh_load_report_interval_seconds)";

/// `promql` (a `max by (instance) (...)` per-replica read) for EVERY serving
/// GC replica in `gc`, as `(pod name, value)`. The ONE per-replica GC reader:
/// [`removal_settle_budget_for`] takes the max of it, and
/// [`gc_config_gauge_agreed`] requires agreement.
///
/// # Errors
/// A serving replica publishes no series for `promql` (a scrape/startup fault,
/// never a default), or no replica serves.
pub async fn gc_replica_values(
    prom: &PrometheusClient,
    promql: &str,
    gc: &GcPods,
) -> Result<Vec<(String, f64)>, String> {
    if gc.serving.is_empty() {
        return Err(format!("no serving GC replica to read `{promql}` from"));
    }
    let map = prom.instance_counter_map(promql).await;
    gc.serving
        .iter()
        .map(|p| {
            instance_for_pod_ip(&map, &p.ip)
                .map(|(_, v)| (p.name.clone(), v))
                .ok_or_else(|| {
                    format!(
                        "GC pod {} publishes no `{promql}`; instances {}",
                        p.name,
                        format_instance_map(&map)
                    )
                })
        })
        .collect()
}

/// A GC config gauge every serving replica publishes with the SAME value
/// (`promql` is the per-replica read). For a derivation that must hold on
/// every replica, never the max of a mixed fleet.
///
/// # Errors
/// As [`gc_replica_values`] and [`agreed_value`].
pub async fn gc_config_gauge_agreed(
    prom: &PrometheusClient,
    promql: &str,
    gc: &GcPods,
) -> Result<Duration, String> {
    let values = gc_replica_values(prom, promql, gc).await?;
    let v = agreed_value(promql, &values)?;
    if v <= 0.0 {
        return Err(format!("`{promql}` is not positive ({v})"));
    }
    Ok(Duration::from_secs_f64(v))
}

/// After a test REMOVED an MH (owner teardown), wait until no GC replica still
/// counts it as selectable, so a later suite is never handed a dead handler.
///
/// # Why this wait exists
///
/// An MH reports DRAINING to GC on SIGTERM (story 4 MH-3), which excludes its
/// row from selection at once, so this wait is normally about one
/// health-checker tick plus a scrape. It still bounds the case where that
/// report never lands (an MH killed before it could send it): GC's row then
/// stays `healthy` with a fresh heartbeat, and `get_candidate_mhs` still
/// selects it, until the heartbeat ages past `B = k × I`. The window is the
/// ADR-0039 design, not a bug, but a test that removes an MH must not leak it
/// to the next suite.
///
/// # Evidence (GC's own view, never a re-derivation of its selection rule)
///
/// On EVERY live GC replica, never summed across replicas ([`replica_settled`]):
/// - a whole successful `gc_mh_available` recompute ran after the wait started
///   (the positive control, [`SETTLE_RECOMPUTES`]);
/// - the count of fresh rows in [`SETTLE_STATES`] is at most the number of
///   [`SETTLE_LIVE_STATE`] slots in `mh.sh list`.
///
/// Residual: an unhealthy serving MH could hide the removed row. 03's
/// `healthy >= live` check covers that state.
///
/// # Bound (FAILURE-ONLY)
///
/// B, the largest `gc_mh_staleness_threshold_seconds` the live replicas publish,
/// plus two GC health-checker ticks (read from [`GC_HEALTH_TICK_SRC`]), plus
/// the MetricsScrape ceiling, because this reads through Prometheus.
///
/// # Errors
/// `removed-mh-still-selectable`, naming each replica's state.
pub async fn wait_removed_mh_unselectable(prom: &PrometheusClient) -> Result<(), String> {
    let gc = live_gc_pods().await;
    let budget = removal_settle_budget_for(prom, &gc).await?;

    let live_slots = tokio::task::spawn_blocking(crate::fixtures::mh_pool::list)
        .await
        .map_err(|e| format!("SETTLE (removed-mh-still-selectable): mh.sh list task: {e}"))?;
    let live = live_slots
        .slots
        .iter()
        .filter(|s| s.state == crate::fixtures::mh_pool::SlotState::Serving)
        .count();
    #[expect(
        clippy::cast_precision_loss,
        reason = "an MH slot count is far below 2^53"
    )]
    let ceiling = live as f64;

    let selectable_promql = selectable_per_replica_promql();
    let recompute_promql = recompute_success_per_replica_promql();
    let baseline = prom.instance_counter_map(&recompute_promql).await;
    // Per replica, the first PRESENT sample is the baseline. A series absent at
    // wait start (scrape gap, not yet scraped) is baselined at its first
    // appearance, never at 0, and still needs +SETTLE_RECOMPUTES past that.
    let mut bases: Vec<Option<f64>> = gc
        .serving
        .iter()
        .map(|p| instance_for_pod_ip(&baseline, &p.ip).map(|(_, v)| v))
        .collect();

    let deadline = std::time::Instant::now() + budget;
    loop {
        let selectable = prom.instance_counter_map(&selectable_promql).await;
        let recompute = prom.instance_counter_map(&recompute_promql).await;
        let views: Vec<(&str, ReplicaSettle)> = gc
            .serving
            .iter()
            .zip(bases.iter_mut())
            .map(|(p, base)| {
                let now = instance_for_pod_ip(&recompute, &p.ip).map(|(_, v)| v);
                // A reset counter (process restart) re-baselines at 0 BEFORE
                // the comparison; an unseen baseline is adopted AFTER it, so
                // the first present sample never settles on its own.
                let seen = base.is_some();
                if seen {
                    *base = rebaseline(*base, now);
                }
                let view = ReplicaSettle {
                    selectable: instance_for_pod_ip(&selectable, &p.ip).map(|(_, v)| v),
                    recompute_baseline: *base,
                    recompute_now: now,
                };
                if !seen {
                    *base = rebaseline(None, now);
                }
                (p.name.as_str(), view)
            })
            .collect();
        let pending: Vec<String> = views
            .iter()
            .filter(|(_, v)| !replica_settled(*v, ceiling))
            .map(|(name, v)| {
                format!(
                    "{name}: {SETTLE_STATES}={} recompute_success={}->{}",
                    v.selectable
                        .map_or_else(|| "ABSENT".to_string(), |h| h.to_string()),
                    v.recompute_baseline
                        .map_or_else(|| "ABSENT".to_string(), |b| b.to_string()),
                    v.recompute_now
                        .map_or_else(|| "ABSENT".to_string(), |r| r.to_string()),
                )
            })
            .collect();
        if pending.is_empty() {
            return Ok(());
        }
        if std::time::Instant::now() > deadline {
            return Err(format!(
                "SETTLE (removed-mh-still-selectable): after the owner teardown, within \
                 {budget:?} (removal_settle_budget: B + 2 GC health ticks + the scrape ceiling), GC replica(s) either \
                 still count more {SETTLE_STATES} rows than the {live} {SETTLE_LIVE_STATE} MH \
                 slot(s), or never completed a gc_mh_available recompute: [{}]. A removed MH may still be \
                 selectable, so the next meeting may land on a dead handler. A recompute counter \
                 that never advanced means a frozen gauge: check \
                 gc_db_queries_total{{operation=\"count_available_mh\",status=\"error\"}} and GC's \
                 `MH registry tick step failed` log lines.",
                pending.join("; "),
            ));
        }
        tokio::time::sleep(SETTLE_POLL_INTERVAL).await;
    }
}

/// PURE: the string literals of the `pub const <name>: [&str; N] = [ ... ];`
/// array in `source`, in order.
///
/// # Errors
/// The declaration is absent or unterminated.
pub fn const_str_array(source: &str, name: &str) -> Result<Vec<String>, String> {
    let decl = format!("pub const {name}:");
    let start = source
        .find(&decl)
        .ok_or_else(|| format!("no `{decl}` declaration"))?;
    let body = &source[start..];
    let open = body
        .find('[')
        .and_then(|i| body[i + 1..].find('[').map(|j| i + 1 + j))
        .ok_or_else(|| format!("`{name}` has no array initialiser"))?;
    let close = body[open..]
        .find("];")
        .map(|i| open + i)
        .ok_or_else(|| format!("`{name}`'s array is not terminated by `];`"))?;
    Ok(body[open + 1..close]
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.starts_with("//"))
        .map(|s| s.trim_matches('"').to_string())
        .collect())
}

/// PURE: the value of the ONE `const <name>: Duration = Duration::from_secs(<n>);`
/// (optionally `pub`) declaration in `source`. The home a derivation reads a
/// service's source const from when that const has no gauge (env-tests cannot
/// import service crates, ADR-0028).
///
/// # Errors
/// FAIL-CLOSED, as `SOURCE-CONST-MISSING` (distinct from any content verdict):
/// no such declaration, more than one, or any other initialiser form
/// (`from_millis`, an expression, a non-integer) — never a default and never a
/// unit guess.
pub fn const_duration_secs(source: &str, name: &str) -> Result<Duration, String> {
    let decls: Vec<&str> = source
        .lines()
        .map(str::trim)
        .filter(|l| {
            let l = l.strip_prefix("pub ").unwrap_or(l);
            l.strip_prefix("const ")
                .and_then(|rest| rest.strip_prefix(name))
                .is_some_and(|rest| rest.trim_start().starts_with(':'))
        })
        .collect();
    let [decl] = decls.as_slice() else {
        return Err(format!(
            "SOURCE-CONST-MISSING: expected exactly one `const {name}: Duration` declaration, \
             found {}",
            decls.len()
        ));
    };
    decl.split_once('=')
        .map(|(_, init)| init.trim())
        .and_then(|init| init.strip_prefix("Duration::from_secs("))
        .and_then(|rest| rest.strip_suffix(");"))
        .and_then(|n| n.trim().parse::<u64>().ok())
        .filter(|n| *n > 0)
        .map(Duration::from_secs)
        .ok_or_else(|| {
            format!(
                "SOURCE-CONST-MISSING: `{name}` is not `Duration::from_secs(<positive integer>);` \
                 (found `{decl}`); re-derive the reader rather than guessing a unit"
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn const_duration_secs_reads_one_from_secs_and_fails_closed() {
        let src = "const BACKOFF_BASE: Duration = Duration::from_secs(1);\n\
                   /// docs\n\
                   const BACKOFF_MAX: Duration = Duration::from_secs(30);\n";
        assert_eq!(
            const_duration_secs(src, "BACKOFF_MAX").unwrap(),
            Duration::from_secs(30)
        );
        // `pub` and a prefix-sharing sibling are both handled.
        let src_pub = "pub const ADMISSION_WINDOW_BUCKET: Duration = Duration::from_secs(10);\n\
                       pub const ADMISSION_WINDOW_BUCKETS: usize = 30;\n";
        assert_eq!(
            const_duration_secs(src_pub, "ADMISSION_WINDOW_BUCKET").unwrap(),
            Duration::from_secs(10)
        );
        // Renamed const: absent, never a default.
        let renamed = src.replace("BACKOFF_MAX", "BACKOFF_CAP");
        let e = const_duration_secs(&renamed, "BACKOFF_MAX").unwrap_err();
        assert!(e.starts_with("SOURCE-CONST-MISSING"), "{e}");
        // from_millis must NOT be read as seconds.
        let millis = "const BACKOFF_MAX: Duration = Duration::from_millis(30000);";
        let e = const_duration_secs(millis, "BACKOFF_MAX").unwrap_err();
        assert!(e.starts_with("SOURCE-CONST-MISSING"), "{e}");
        // Two declarations: ambiguous.
        let twice = format!("{src}const BACKOFF_MAX: Duration = Duration::from_secs(31);");
        let e = const_duration_secs(&twice, "BACKOFF_MAX").unwrap_err();
        assert!(e.contains("found 2"), "{e}");
        // Zero is not a bound.
        assert!(const_duration_secs("const X: Duration = Duration::from_secs(0);", "X").is_err());
    }

    #[test]
    fn rebaseline_handles_counter_reset_and_unseen_baseline() {
        assert_eq!(rebaseline(Some(40.0), Some(3.0)), Some(0.0), "reset -> 0");
        assert_eq!(
            rebaseline(Some(40.0), Some(41.0)),
            Some(40.0),
            "advance keeps base"
        );
        assert_eq!(
            rebaseline(Some(40.0), None),
            Some(40.0),
            "absent keeps base"
        );
        assert_eq!(
            rebaseline(None, Some(7.0)),
            Some(7.0),
            "unseen adopts first sample"
        );
        assert_eq!(rebaseline(None, None), None);
        // After a reset, the restarted process must itself recompute twice.
        let base = rebaseline(Some(40.0), Some(1.0));
        let at = |now| ReplicaSettle {
            selectable: Some(1.0),
            recompute_baseline: base,
            recompute_now: Some(now),
        };
        assert!(!replica_settled(at(1.0), 2.0));
        assert!(replica_settled(at(2.0), 2.0));
    }

    #[test]
    fn recompute_query_is_built_from_the_pinned_constants() {
        let q = recompute_success_per_replica_promql();
        assert_eq!(
            q,
            "sum by (instance) (gc_db_queries_total{operation=\"count_available_mh\",status=\"success\"})"
        );
    }

    /// [`MH_ENDPOINT_ALLOWLIST_KEYS`] == the reader's list in
    /// `crates/gc-service/src/config.rs` (its ONE home), read from the tree.
    #[test]
    fn allowlist_keys_match_gc_config() {
        let path = crate::repo_root().join("crates/gc-service/src/config.rs");
        let text = std::fs::read_to_string(&path).expect("read gc-service config.rs");
        let keys = const_str_array(&text, "MH_ENDPOINT_ALLOWLIST_KEYS")
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        // Vacuity control: an extraction that went wrong (empty, or literals
        // carrying quotes/whitespace) must not compare equal to anything.
        assert_eq!(keys.len(), 5, "extraction produced {keys:?}");
        assert!(
            keys.iter()
                .all(|k| k.starts_with("GC_MH_") && !k.contains(['"', ' '])),
            "malformed extraction: {keys:?}"
        );
        assert_eq!(
            keys, MH_ENDPOINT_ALLOWLIST_KEYS,
            "GC's MH_ENDPOINT_ALLOWLIST_KEYS drifted from this mirror"
        );
    }

    /// [`MH_REGISTRY_KEYS`] == the reader's list in
    /// `crates/gc-service/src/config.rs`, read from the tree; the named
    /// interval/multiplier constants are members of it.
    #[test]
    fn registry_keys_match_gc_config() {
        let path = crate::repo_root().join("crates/gc-service/src/config.rs");
        let text = std::fs::read_to_string(&path).expect("read gc-service config.rs");
        let keys = const_str_array(&text, "MH_REGISTRY_KEYS")
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(
            keys.len(),
            MH_REGISTRY_KEYS.len(),
            "extraction produced {keys:?}"
        );
        assert!(
            keys.iter()
                .all(|k| k.starts_with("GC_MH_") && !k.contains(['"', ' '])),
            "malformed extraction: {keys:?}"
        );
        assert_eq!(
            keys, MH_REGISTRY_KEYS,
            "GC's MH_REGISTRY_KEYS drifted from this mirror"
        );
        assert!(MH_REGISTRY_KEYS.contains(&INTERVAL_KEY));
        assert!(MH_REGISTRY_KEYS.contains(&MULTIPLIER_KEY));
        assert!(MH_REGISTRY_KEYS.contains(&INITIAL_HANDLERS_KEY));
    }

    /// Adverse input for the drift check: a reader source missing one key (or
    /// carrying an extra one) does NOT compare equal to the mirror.
    #[test]
    fn registry_key_drift_is_detected() {
        let path = crate::repo_root().join("crates/gc-service/src/config.rs");
        let text = std::fs::read_to_string(&path).expect("read gc-service config.rs");
        let missing = text.replacen("    \"GC_MH_MAX_EXCLUDE_HANDLER_IDS\",\n", "", 1);
        assert_ne!(missing, text, "fixture edit must apply");
        let keys = const_str_array(&missing, "MH_REGISTRY_KEYS").unwrap();
        assert_ne!(keys, MH_REGISTRY_KEYS, "a missing key must be detected");
        let extra = text.replacen(
            "    \"GC_MH_MAX_EXCLUDE_HANDLER_IDS\",\n",
            "    \"GC_MH_MAX_EXCLUDE_HANDLER_IDS\",\n    \"GC_MH_NEW_KEY\",\n",
            1,
        );
        let keys = const_str_array(&extra, "MH_REGISTRY_KEYS").unwrap();
        assert_ne!(keys, MH_REGISTRY_KEYS, "an extra key must be detected");
    }

    #[test]
    fn replica_settled_needs_a_recompute_and_a_bounded_count() {
        let r = |selectable, base: f64, now| ReplicaSettle {
            selectable,
            recompute_baseline: Some(base),
            recompute_now: now,
        };
        // Positive: a whole recompute since the start (+2), count <= live.
        assert!(replica_settled(r(Some(2.0), 5.0, Some(7.0)), 2.0));
        assert!(replica_settled(r(Some(1.0), 0.0, Some(2.0)), 2.0));
        // +1 only: that recompute may have been in flight at the baseline read.
        assert!(!replica_settled(r(Some(2.0), 5.0, Some(6.0)), 2.0));
        // A frozen zero-init gauge with no recompute must NOT pass (vacuity).
        assert!(!replica_settled(r(Some(0.0), 5.0, Some(5.0)), 2.0));
        assert!(!replica_settled(r(Some(0.0), 0.0, None), 2.0));
        assert!(!replica_settled(r(Some(0.0), 0.0, Some(0.0)), 2.0));
        // The dead row still fresh: a just-registered PENDING replacement plus
        // the removed MH's HEALTHY row = live + 1 must NOT settle, though
        // `healthy` alone would equal live.
        assert!(!replica_settled(r(Some(3.0), 5.0, Some(9.0)), 2.0));
        // Absent series.
        assert!(!replica_settled(r(None, 5.0, Some(9.0)), 2.0));
        // Absent BASELINE: a lifetime total of 1000 must not pass as "+1000".
        let no_base = ReplicaSettle {
            selectable: Some(0.0),
            recompute_baseline: None,
            recompute_now: Some(1000.0),
        };
        assert!(!replica_settled(no_base, 2.0));
    }

    #[test]
    fn selectable_promql_counts_exactly_the_settle_states() {
        let q = selectable_per_replica_promql();
        assert_eq!(
            q,
            "sum by (instance) (gc_mh_available{state=~\"pending|healthy|degraded\"})"
        );
        for state in ["pending", "healthy", "degraded"] {
            assert!(SETTLE_STATES.split('|').any(|s| s == state), "{state}");
        }
        for state in ["draining", "unhealthy"] {
            assert!(!SETTLE_STATES.split('|').any(|s| s == state), "{state}");
        }
    }

    #[test]
    fn gc_health_tick_reads_the_source_constant() {
        let src = std::fs::read_to_string(crate::repo_root().join(GC_HEALTH_TICK_SRC))
            .expect("read the health checker source");
        let tick = gc_health_tick(&src).expect("the live constant parses");
        assert!(tick > Duration::ZERO);
        assert!(gc_health_tick("pub const OTHER: u64 = 5;")
            .unwrap_err()
            .contains("DEFAULT_CHECK_INTERVAL_SECONDS"));
        assert!(gc_health_tick("pub const DEFAULT_CHECK_INTERVAL_SECONDS: u64 = 0;").is_err());
    }

    /// The two settle waits (this fixture, deploy.sh) use ONE predicate: every
    /// shared token appears in deploy.sh's `wait_removed_mh_unselectable` and its
    /// caller, and deploy.sh counts exactly [`SETTLE_LIVE_STATE`] as live.
    #[test]
    fn settle_predicate_matches_deploy_sh() {
        let path = crate::repo_root().join("infra/kind/scripts/deploy.sh");
        let text = std::fs::read_to_string(&path).expect("read deploy.sh");
        // Vacuity control: the wait exists, so the token checks below look at it.
        assert!(
            text.contains("wait_removed_mh_unselectable()"),
            "deploy.sh has no wait_removed_mh_unselectable(); the guard would check nothing"
        );
        for token in [
            SETTLE_HEALTHY_SERIES,
            SETTLE_RECOMPUTE_SERIES,
            SETTLE_RECOMPUTE_OPERATION,
            SETTLE_RECOMPUTE_SUCCESS,
            GC_HEALTH_TICK_SRC,
        ] {
            assert!(
                text.contains(token),
                "deploy.sh's settle wait lacks {token:?}"
            );
        }
        let live_decl = text
            .lines()
            .find_map(|l| l.trim().strip_prefix("readonly MH_SETTLE_LIVE_STATE="))
            .expect("deploy.sh declares `readonly MH_SETTLE_LIVE_STATE=...` (one named home)");
        assert_eq!(
            live_decl.trim_matches('"'),
            SETTLE_LIVE_STATE,
            "deploy.sh counts a different slot state as live"
        );
        let states_decl = text
            .lines()
            .find_map(|l| l.trim().strip_prefix("readonly MH_SETTLE_STATES="))
            .expect("deploy.sh declares `readonly MH_SETTLE_STATES=...` (one named home)");
        assert_eq!(
            states_decl.trim_matches(['\'', '"']),
            SETTLE_STATES,
            "deploy.sh counts a different gc_mh_available state set as selectable"
        );
        let recomputes_decl = text
            .lines()
            .find_map(|l| l.trim().strip_prefix("readonly MH_SETTLE_RECOMPUTES="))
            .expect("deploy.sh declares `readonly MH_SETTLE_RECOMPUTES=...` (one named home)");
        let recomputes: f64 = recomputes_decl
            .trim_matches(['\'', '"'])
            .parse()
            .expect("MH_SETTLE_RECOMPUTES is a number");
        assert!(
            (recomputes - SETTLE_RECOMPUTES).abs() < f64::EPSILON,
            "deploy.sh requires a different number of recomputes ({recomputes}) than {SETTLE_RECOMPUTES}"
        );
    }

    #[test]
    fn const_str_array_reads_literals_and_refuses_absence() {
        let src = "x\npub const K: [&str; 2] = [\n    \"A_B\",\n    \"C\", // trailing\n];\n";
        assert_eq!(const_str_array(src, "K").unwrap(), vec!["A_B", "C"]);
        assert!(const_str_array(src, "Z")
            .unwrap_err()
            .contains("no `pub const Z:`"));
        assert!(const_str_array("pub const K: [&str; 1] = [\"A\"", "K")
            .unwrap_err()
            .contains("not terminated"));
    }
}
