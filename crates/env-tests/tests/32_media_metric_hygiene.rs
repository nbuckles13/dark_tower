//! ADR-0036 §11 metric hygiene, asserted against the REAL scrape.
//!
//! The *applies* half of the control whose *fires* half lives in
//! `env_tests::fixtures::metric_hygiene`'s unit fixtures. A bad label cannot be
//! planted on a live cluster, so the two halves are split: the kernel's FIRE
//! fixtures run in the always-on Rust lane, and this feeds real stored series
//! into that same kernel.
//!
//! # Read this before reading a green
//!
//! **A scrape sees only series that were emitted.** A meeting-labelled metric on
//! a rare or error-only path is invisible to this suite, and so is any metric
//! from a pod that was not up. This control's own "does it apply" answer is
//! therefore bounded, and it is stated here so a green is never read as more
//! than it supports. It is complemented, not replaced, by
//! `PII_PREFIX_DENYLIST` at the Rust source level — which is itself bypassable
//! by `# pii-safe:` and blind to trailing compounds.
//!
//! # What this adds over the source guard
//!
//! Containment rather than `starts_with`. `PII_PREFIX_DENYLIST` cannot see a
//! TRAILING compound (`x_meeting_id`), and `docs/observability/label-taxonomy.md`
//! marks that case `[reviewer-only]` for exactly that reason. This closes it at
//! the artifact. **Do not read that as this suite being redundant with the
//! guard, or as the guard being redundant with this** — they are stronger on
//! different axes, and this one is narrow on the overclaim axis (scraped Rust
//! service metric labels only; not dashboards, not docs, not log fields).
//!
//! # MH gates, MC is recorded — do NOT "fix" this into symmetry
//!
//! `resolve_media_handles()` runs unconditionally at MH startup and resolves the
//! full label cross-product, so every `mh_media_*` series exists at zero on any
//! running MH pod with no media flowing. Absence there is a genuine fault and is
//! gated.
//!
//! MC's media metrics are created lazily on first emission
//! (`init_metrics_recorder()` sets histogram buckets only), so on an idle cluster
//! their absence is the EXPECTED state. MC is therefore in the absence selector
//! but **not** in the presence anchor. An MC-shaped presence gate would red on
//! every idle run and be muted within weeks — which is how a control dies, and
//! is the failure mode this whole suite exists to demonstrate against. The
//! asymmetry is deliberate. It is stated here and in the runbook because it
//! reads as an oversight.

#![cfg(feature = "observability")]

use env_tests::cluster::ClusterConnection;
use env_tests::eventual::{assert_eventually, ConsistencyCategory};
use env_tests::fixtures::metric_hygiene::{
    check_series, scrape_reachable_client_series, service_jobs_with_metric_relabeling, Series,
};
use env_tests::fixtures::PrometheusClient;
use std::collections::BTreeMap;

/// Rust service jobs. Deliberately ALL four, not just the media pair: R1 is
/// "no meeting identifier on any metric anywhere in this design", and narrowing
/// to media metrics would leave the broader rule unasserted.
const JOBS: &str = "ac-service|gc-service|mc-service|mh-service";

/// The presence anchor. Eagerly registered at MH startup, so its absence means
/// MH is not up, not scraped, or its registration path broke.
const MH_ANCHOR_METRIC: &str = "mh_media_frames_forwarded_total";

/// Distinct triage strings. **Keyed by the runbook §8 rows** — if either side is
/// reworded the row silently stops matching, which is the extraction-vacuity
/// failure with a human as the extractor. The constants are named here and the
/// §8 rows name the constants, so a reworder meets the obligation from whichever
/// side they arrive at.
const TRIAGE_SCRAPE: &str = "Triage MH scrape/metric-registration";
const TRIAGE_VIOLATION: &str = "Triage metric-label policy violation";
const TRIAGE_RELABEL: &str = "Triage Prometheus relabel configuration";

async fn cluster() -> ClusterConnection {
    ClusterConnection::new()
        .await
        .expect("Failed to connect to cluster - ensure port-forwards are running")
}

/// Fetch every series for the Rust service jobs, ONCE.
///
/// **One fetch, both assertions — this is structural, not tidiness.** If the
/// anchor and the absence assertion were separate queries, a typo, a wrong job
/// name or a non-matching selector would leave the anchor passing (it queries
/// the MH family by name) while the absence assertion inspected zero series and
/// reported clean: a green suite with a satisfied precondition and no input.
/// Fetching once and asserting both over the same `Vec` makes the anchor
/// *guarantee* the predicate had input, because there is only one input.
async fn fetch_all_series(client: &PrometheusClient) -> Vec<Series> {
    let response = client
        .query_promql(&format!(r#"{{job=~"{JOBS}"}}"#))
        .await
        .unwrap_or_else(|e| {
            panic!(
                "{TRIAGE_SCRAPE}: Prometheus instant query failed: {e}. \
                    The cluster or the port-forward is the fault here, not the diff."
            )
        });

    response
        .data
        .result
        .iter()
        .map(|r| {
            let mut labels: BTreeMap<String, String> = r
                .metric
                .iter()
                .filter(|(k, _)| k.as_str() != "__name__")
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            let name = r
                .metric
                .get("__name__")
                .cloned()
                .unwrap_or_else(|| "<unnamed>".to_string());
            labels.remove("__name__");
            Series { name, labels }
        })
        .collect()
}

#[tokio::test]
async fn media_metric_labels_carry_no_meeting_identifier_and_no_overclaim() {
    let cluster = cluster().await;
    let client = PrometheusClient::new(&cluster.prometheus_base_url);

    // Scrape lag is real; the anchor is polled through the established category
    // rather than a hand-rolled sleep. ADR-0028 zero-retry is about test logic,
    // not infrastructure convergence.
    assert_eventually(ConsistencyCategory::MetricsScrape, || {
        let client = PrometheusClient::new(&cluster.prometheus_base_url);
        async move {
            client
                .query_promql(&format!("count({MH_ANCHOR_METRIC})"))
                .await
                .ok()
                .filter(|r| !r.data.result.is_empty())
                .is_some()
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!(
            "{TRIAGE_SCRAPE}: `{MH_ANCHOR_METRIC}` is absent from Prometheus. \
             MH resolves the full media metric label cross-product unconditionally \
             at startup, so those series exist at zero on any running MH pod with \
             no media flowing. Absence means MH is not up, not scraped, or its \
             metric registration path is broken — investigate the pod and the \
             scrape target. DO NOT satisfy this check by deleting it: without the \
             anchor every assertion below passes vacuously."
        )
    });

    let series = fetch_all_series(&client).await;

    // Non-vacuity, over the SAME set the predicate will see.
    assert!(
        !series.is_empty(),
        "{TRIAGE_SCRAPE}: the scrape returned zero series for jobs `{JOBS}`. \
         The absence assertions below would pass having inspected nothing."
    );
    assert!(
        series.iter().any(|s| s.name == MH_ANCHOR_METRIC),
        "{TRIAGE_SCRAPE}: `{MH_ANCHOR_METRIC}` was not in the fetched series set \
         even though the anchor query found it — the job selector `{JOBS}` does \
         not cover the MH job, so this suite is inspecting the wrong surface."
    );

    // MC is RECORDED, not gated. Its media metrics are lazily created, so
    // absence on an idle cluster is expected and must never fail the run.
    let mc_media_present = series.iter().any(|s| s.name.starts_with("mc_media_"));
    eprintln!(
        "metric-hygiene: inspected {} series across jobs [{JOBS}]; \
         mc_media_* present: {mc_media_present}",
        series.len()
    );

    let violations = check_series(&series);
    assert!(
        violations.is_empty(),
        "{TRIAGE_VIOLATION}: {} ADR-0036 §11 violation(s) in scraped metrics.\n{}\n\n\
         This is a POLICY violation, not a test defect — the assertion has no \
         carve-outs by design, so there is no legitimate series it can red on. \
         Fix the METRIC, never the assertion: remove the label from the emitting \
         `metrics.rs`, or change its value.\n\n\
         Two named non-fixes. (1) Do NOT add a grandfather allowlist: ADR-0036 \
         §11's grandfathered set is the client SDK's ADR-0028 join-flow metrics, \
         which cannot reach a Prometheus scrape at all — the OTel collector has \
         only a `debug` exporter and there is no otel-collector scrape job — so \
         over these four jobs the exception has ZERO members and an allowlist \
         would be dead on the day it was written. (2) Do NOT narrow the job \
         selector: if a carve-out seems necessary, the real diagnosis is that the \
         selector was widened past the Rust services.",
        violations.len(),
        violations
            .iter()
            .map(|v| format!("  - [{:?}] {} :: {}", v.rule, v.series, v.detail))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// Premise pin: client media metrics are not scrape-reachable.
///
/// Trivially true today and that is the point — it *documents* the
/// non-reachability at the artifact rather than in a comment that can rot. The
/// client surface is covered at the node tier by task 19
/// (`packages/sdk-core/src/media/setup/__tests__/mediaMetrics.test.ts` and
/// `ingress.attribution.test.ts`); no second assertion of it is added here.
#[tokio::test]
async fn client_media_metrics_are_not_scrape_reachable() {
    let cluster = cluster().await;
    let client = PrometheusClient::new(&cluster.prometheus_base_url);
    let series = fetch_all_series(&client).await;

    let reachable = scrape_reachable_client_series(&series);
    assert!(
        reachable.is_empty(),
        "Client SDK metrics are now scrape-reachable: {reachable:?}\n\n\
         THIS IS NOT A LEAK and must not be triaged as one. It means someone \
         added a Prometheus exporter to the OTel collector, or an \
         otel-collector scrape job. The obligation it creates is to EXTEND this \
         suite: the client join-flow metrics legitimately carry \
         `meeting_id_hash` as ADR-0036 §11's grandfathered set, so once they are \
         scraped the empty-allowlist reasoning above no longer holds and the \
         grandfathered roster in `docs/observability/metrics/client.md` has to \
         become a real carve-out here."
    );
}

/// The PromQL view's own premise, checked rather than commented.
///
/// Evaluating stored series equals evaluating what the pod emits ONLY because
/// nothing strips labels between scrape and storage. That holds today — zero
/// `metric_relabel_configs` in `prometheus-config.yaml` — but it is a
/// configuration fact, not a property. If one is ever added, this suite goes
/// blind to a label the pod still emits and narrows silently while staying
/// green, which is the exact failure class this task is about.
///
/// Scoped to `metric_relabel_configs` DELIBERATELY. The service jobs do carry
/// `relabel_configs`, but those are `keep`-only target *discovery*, not sample
/// rewriting. `metric_relabel_configs` is the mechanism that could silently HIDE
/// a violation by dropping or replacing a label before storage; a
/// `relabel_configs` addition would surface as a new label this suite CATCHES.
/// Widening this to `relabel_configs` would add noise and close nothing.
#[tokio::test]
async fn no_metric_relabeling_narrows_this_suite_silently() {
    let cluster = cluster().await;
    let url = format!("{}/api/v1/status/config", cluster.prometheus_base_url);
    let body = cluster
        .http_client()
        .get(&url)
        .send()
        .await
        .unwrap_or_else(|e| panic!("{TRIAGE_RELABEL}: cannot read Prometheus config: {e}"))
        .text()
        .await
        .unwrap_or_else(|e| panic!("{TRIAGE_RELABEL}: cannot read Prometheus config body: {e}"));

    // The parse, the scoping and the vacuity guard all live in the KERNEL
    // (`service_jobs_with_metric_relabeling`), where they get FIRE fixtures in the
    // always-on Rust lane. This function only supplies real input and renders the
    // failure.
    //
    // That split is not tidiness — it is the fix for a dead control. An earlier
    // version scoped the jobs inline HERE, splitting the raw body on `- job_name:`
    // and calling `.lines()`. But `/api/v1/status/config` returns JSON with the
    // config as an ESCAPED string, so the body holds zero real newlines and every
    // job-name extraction returned the whole remainder of the config, matched
    // nothing, and passed on every possible input including a real violation.
    // It had no unit coverage precisely because it sat in this cluster-gated file,
    // which `cargo test` cannot reach (@observability).
    let offending_jobs = service_jobs_with_metric_relabeling(&body).unwrap_or_else(|e| {
        panic!(
            "{TRIAGE_RELABEL}: could not scan the Prometheus config for \
             `metric_relabel_configs`: {e:?}. This is a COULD-NOT-EVALUATE, not a \
             clean result — an empty offender list from a failed scan reads exactly \
             like a passing one, which is how the previous version of this check \
             passed on every input. Do NOT 'fix' it by treating the error as empty."
        )
    });

    assert!(
        offending_jobs.is_empty(),
        "{TRIAGE_RELABEL}: `metric_relabel_configs` is now applied to service \
         job(s) {offending_jobs:?}. This suite evaluates STORED series, so a \
         drop/replace applied before storage makes it blind to a label the pod \
         still emits — it would keep passing while covering less. Read the labels \
         AT THE POD for those jobs. Do NOT widen this suite's selector and do NOT \
         delete this assertion.\n\n\
         Scoped to the four Rust service jobs deliberately: a relabel on \
         `prometheus`, `kube-state-metrics`, `node-exporter` or `kubelet` is \
         ordinary cardinality management, does not affect this suite, and is NOT \
         what this assertion is about."
    );
}
