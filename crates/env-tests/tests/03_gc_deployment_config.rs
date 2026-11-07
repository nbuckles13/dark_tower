//! P0 Smoke Tests: GC Deployment Configuration (story 4 GC-1: R-24, R-26)
//!
//! The GC counterpart of `02_mc_deployment_config.rs`, run against the LIVE
//! cluster, never a local render. GC has one Deployment with N replicas and no
//! `instance` label, so pods are found by [`GC_SELECTOR`] and EVERY serving
//! replica is checked (per pod, never one pod standing in for all):
//!
//! - **Wiring.** Each key in [`MH_REGISTRY_KEYS`] is a HARD
//!   `configMapKeyRef`, with `key` == env name, to the ONE live generation of
//!   `gc-service-config` that every GC pod resolves, and its value parses. For
//!   retention and the initial handler count this is the only in-cluster
//!   evidence (they have no gauge).
//! - **Config gauges.** What the RUNNING process loaded equals that
//!   generation: `gc_mh_load_report_interval_seconds` equals
//!   `GC_MH_LOAD_REPORT_INTERVAL_MS`, and `gc_mh_staleness_threshold_seconds`
//!   equals deployed `GC_MH_STALENESS_MULTIPLIER` × deployed interval. Both
//!   expectations are READ from the live ConfigMap; no deployed number appears
//!   in this file.
//! - **Availability.** Every GC replica's `gc_mh_available{state="healthy"}`,
//!   summed over regions, is at least the number of live MH pods. It is
//!   checked per replica, NEVER summed across replicas: each replica
//!   recomputes the same fleet count from the shared registry, so a sum would
//!   double-count, and a replica whose tick is broken would hide behind the
//!   other.
//!
//! # Vacuity controls
//!
//! The key table is non-empty. The GC pod set is non-empty
//! (`require_all_serving` refuses zero pods). The live MH count is asserted
//! `>= 1` with its own reason token (`no-live-mh`), so `0 >= 0` cannot pass.
//! Gauge absence, gauge shape, and value mismatch fail with three distinct
//! texts, as in 01(c)/02.
//!
//! # Bounds
//!
//! Every test first waits out a GC rollout's tail (`live_gc_pods`): an old
//! ReplicaSet's pod still inside its termination grace. The wait is bounded by
//! the live pods' largest `terminationGracePeriodSeconds` plus a removal
//! margin; past it, a Terminating or not-Ready GC pod fails loudly.
//!
//! The availability wait is a FAILURE-ONLY ceiling derived from deployed
//! config: the staleness bound `B = k × I` (a reporting MH is counted healthy
//! within B of its first report) plus the `MetricsScrape` ceiling. The scrape
//! ceiling also absorbs GC's health-checker tick, which is a code constant
//! (`DEFAULT_CHECK_INTERVAL_SECONDS`), not config, so this test can't read it.
//! On a green run the poll returns at the first sample that holds.
//!
//! # Diagnostic output policy (PII)
//!
//! Pod names, namespace, key/ConfigMap NAMES and the scalar values of named
//! configuration keys only (`gc-service-config` holds no secrets; GC's
//! credentials arrive via `secretKeyRef`). Never a parsed `Value` or raw
//! `kubectl` output.
//!
//! Edge-sum (concurrent Σ P×(N+M)): 0

#![cfg(feature = "smoke")]

use std::time::Duration;

use env_tests::cluster::ClusterConnection;
use env_tests::eventual::ConsistencyCategory;
use env_tests::fixtures::deployment_config::{
    check_key_ref, check_shape, cm_value, fetch_configmap, parse_positive, LivePod, RefKind,
    ValueShape,
};
use env_tests::fixtures::kube::{
    is_terminating, live_mh_pods, max_termination_grace, pods_json, resolve_generation,
    settled_serving_pods, REDEPLOY_HINT,
};
use env_tests::fixtures::metrics::poll_until_pinned_instance;
use env_tests::fixtures::mh_config::{gauge_verdict, Conversion, GaugeVerdict};
use env_tests::fixtures::PrometheusClient;
use serde_json::Value;

/// The label every GC pod carries (`infra/services/gc-service/deployment.yaml`).
const GC_SELECTOR: &str = "app=gc-service";
const CONTAINER: &str = "gc-service";
/// Generator base name; the live name is hash-suffixed (ADR-0038 §2) and
/// resolved from the pod specs.
const GC_CONFIGMAP: &str = "gc-service-config";

const INTERVAL_KEY: &str = "GC_MH_LOAD_REPORT_INTERVAL_MS";
const MULTIPLIER_KEY: &str = "GC_MH_STALENESS_MULTIPLIER";

/// Every GC-1 key, wired into the GC container as a HARD ref. All four are
/// REQUIRED reads with a lower bound of at least 1, so the shape is
/// PositiveInteger. Ranges stay with the reader (`crates/gc-service/src/config.rs`).
const MH_REGISTRY_KEYS: &[&str] = &[
    INTERVAL_KEY,
    MULTIPLIER_KEY,
    "GC_MH_REGISTRY_RETENTION_SECONDS",
    "GC_MH_INITIAL_HANDLERS_PER_MEETING",
];

/// How long a startup gauge may take to be scraped for a pod.
const GAUGE_PRESENT_BOUND: Duration = Duration::from_secs(60);
const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// Margin over the deployed termination grace: after SIGKILL at the end of the
/// grace, the kubelet still has to report the container gone and the API
/// server remove the pod object. A failure-only ceiling, spent only when a pod
/// is genuinely slow to go.
const POD_REMOVAL_MARGIN: Duration = Duration::from_secs(15);

/// A settled GC pod set: the live (non-terminating) pod documents, their
/// serving view, and the ONE `gc-service-config` generation they all
/// reference — all from ONE listing, so the specs checked, the IPs pinned and
/// the ConfigMap read are the same pods.
struct GcPods {
    items: Vec<Value>,
    serving: Vec<LivePod>,
    configmap: String,
}

/// Wait out a GC rollout's tail, then read the pod set. `rollout status`
/// returns while an old ReplicaSet's pod is still inside its termination
/// grace, so the first run after every GC deploy can see one Terminating. The
/// wait is bounded by the largest `terminationGracePeriodSeconds` on the live
/// pods (the Terminating one included), read from the API server, never
/// retyped, plus [`POD_REMOVAL_MARGIN`]. Past the bound, a pod still
/// Terminating or not Ready is a loud PRECONDITION (`gc-pods-unserving`), never
/// dropped.
///
/// `resolve_generation` then refuses a bare name, zero generations, or more
/// than one: the serving replicas must run ONE config generation.
async fn live_gc_pods() -> GcPods {
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
fn deployed(cm: &Value, key: &str) -> u64 {
    parse_positive(key, &cm_value(cm, GC_CONFIGMAP, key), GC_CONFIGMAP)
        .unwrap_or_else(|e| panic!("{e}"))
}

/// `(B in ms, I in ms, k)` from the live generation: `B = k × I`, the expression
/// `MhRegistryConfig::new` uses. Overflow is a loud failure, not a wrap.
fn deployed_bound_and_interval_ms(cm: &Value) -> (u64, u64, u64) {
    let interval_ms = deployed(cm, INTERVAL_KEY);
    let k = deployed(cm, MULTIPLIER_KEY);
    let bound_ms = interval_ms.checked_mul(k).unwrap_or_else(|| {
        panic!("{MULTIPLIER_KEY}={k} × {INTERVAL_KEY}={interval_ms} overflows u64")
    });
    (bound_ms, interval_ms, k)
}

#[tokio::test]
async fn test_gc_mh_registry_keys_wired_into_every_gc_pod() {
    assert!(!MH_REGISTRY_KEYS.is_empty());
    let gc = live_gc_pods().await;
    let cm = fetch_configmap(&gc.configmap);
    for pod in &gc.items {
        for &key in MH_REGISTRY_KEYS {
            check_key_ref(pod, CONTAINER, key, &gc.configmap, RefKind::Hard)
                .unwrap_or_else(|e| panic!("{e}"));
            check_shape(
                key,
                &cm_value(&cm, GC_CONFIGMAP, key),
                ValueShape::PositiveInteger,
                GC_CONFIGMAP,
            )
            .unwrap_or_else(|e| panic!("{e}"));
        }
    }
}

/// What every RUNNING GC replica loaded equals the live generation. Three
/// distinct outcomes, as in 02: absent (scrape/startup), shape (the publisher
/// changed), mismatch (the process runs another value).
#[tokio::test]
async fn test_config_reported_by_running_gc_matches_configmap() {
    let cluster = ClusterConnection::new()
        .await
        .expect("Failed to connect to cluster - ensure port-forwards are running");
    let prom = PrometheusClient::new(&cluster.prometheus_base_url);

    let gc = live_gc_pods().await;
    let cm = fetch_configmap(&gc.configmap);
    let (bound_ms, interval_ms, k) = deployed_bound_and_interval_ms(&cm);
    // `(series, expected ms, what the expectation is made of)`. Both are
    // MsToSeconds: GC publishes `Duration::as_secs_f64()` of the SAME Durations
    // it pushes to MH and compares heartbeats against.
    let gauges: [(&str, u64, String); 2] = [
        (
            "gc_mh_load_report_interval_seconds",
            interval_ms,
            format!("{INTERVAL_KEY}={interval_ms}"),
        ),
        (
            "gc_mh_staleness_threshold_seconds",
            bound_ms,
            format!("{MULTIPLIER_KEY}={k} × {INTERVAL_KEY}={interval_ms} = {bound_ms}"),
        ),
    ];

    for pod in &gc.serving {
        for (series, expected_ms, source) in &gauges {
            let promql = format!("max by (instance) ({series})");
            let value = poll_until_pinned_instance(
                &prom,
                &promql,
                &pod.ip,
                GAUGE_PRESENT_BOUND,
                POLL_INTERVAL,
                &format!(
                    "PRESENCE (pod {}): GC publishes {series} at process start, so absence is a \
                     scrape/startup problem, NOT a configuration mismatch",
                    pod.name
                ),
                "present",
                |_| true,
            )
            .await;
            match gauge_verdict(Conversion::MsToSeconds, *expected_ms, value) {
                GaugeVerdict::Match => {}
                GaugeVerdict::Shape => panic!(
                    "{series} on pod {} is not a value MsToSeconds can produce ({value}): the \
                     publisher's shape changed in crates/gc-service/src/observability/metrics.rs. \
                     This is not a configuration mismatch.",
                    pod.name
                ),
                GaugeVerdict::Mismatch(expected) => panic!(
                    "GC pod {} runs {series}={value}, but its {GC_CONFIGMAP} generation gives \
                     {source} ms ({expected}). The process is not running the deployed value; \
                     {REDEPLOY_HINT}.",
                    pod.name
                ),
            }
        }
    }
}

/// Every GC replica counts at least every live MH as healthy (R-24): each live MH
/// keeps reporting within the staleness bound, so GC's registry recompute must
/// see it. This is the input to the fleet-level MHDown alert.
#[tokio::test]
async fn test_every_gc_replica_counts_every_live_mh_healthy() {
    let cluster = ClusterConnection::new()
        .await
        .expect("Failed to connect to cluster - ensure port-forwards are running");
    let prom = PrometheusClient::new(&cluster.prometheus_base_url);

    let live_mhs = live_mh_pods().await.len();
    assert!(
        live_mhs >= 1,
        "PRECONDITION (no-live-mh): zero live MH pods, so `healthy >= 0` would pass vacuously"
    );
    let gc = live_gc_pods().await;
    let cm = fetch_configmap(&gc.configmap);
    let (bound_ms, _, _) = deployed_bound_and_interval_ms(&cm);
    let budget = Duration::from_millis(bound_ms) + ConsistencyCategory::MetricsScrape.timeout();

    #[expect(
        clippy::cast_precision_loss,
        reason = "an MH pod count is far below 2^53"
    )]
    let floor = live_mhs as f64;
    // Summed over `region` (own region + "other") WITHIN one replica; the pin
    // then selects that replica. Never summed across replicas.
    let promql = "sum by (instance) (gc_mh_available{state=\"healthy\"})";
    for pod in &gc.serving {
        poll_until_pinned_instance(
            &prom,
            promql,
            &pod.ip,
            budget,
            POLL_INTERVAL,
            &format!(
                "AVAILABILITY (pod {}): {live_mhs} live MH pod(s), but this GC replica does not \
                 count that many as healthy within B={bound_ms} ms + the scrape ceiling. Either \
                 an MH is not registering or reporting, or this replica's health-checker tick \
                 is not recomputing gc_mh_available",
                pod.name
            ),
            &format!(">= {live_mhs}"),
            |v| v >= floor,
        )
        .await;
    }
}
