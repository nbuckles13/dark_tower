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
    self, check_series, scrape_reachable_client_series, service_jobs_with_metric_relabeling, Series,
};
use env_tests::fixtures::PrometheusClient;

/// Rust service jobs. Deliberately ALL four, not just the media pair: R1 is
/// "no meeting identifier on any metric anywhere in this design", and narrowing
/// to media metrics would leave the broader rule unasserted.
/// `otel-collector` IS INCLUDED, and it is the whole reason R1 has anything to
/// say about client series. With `honor_labels: false` the browser `dt_client_*`
/// series arrive under `job="otel-collector"`, so a selector listing only the
/// four Rust services would never see them — the absence checks would keep
/// passing while examining an empty set, which is a control that has been
/// blinded rather than satisfied.
///
/// NOT `otel-collector-telemetry`: that job carries the collector's own
/// `otelcol_*` counters, which are neither client nor service application
/// metrics and are not what these rules are about.
const JOBS: &str = "ac-service|gc-service|mc-service|mh-service|otel-collector";

/// The presence anchor. Eagerly registered at MH startup, so its absence means
/// MH is not up, not scraped, or its registration path broke.
const MH_ANCHOR_METRIC: &str = "mh_media_frames_forwarded_total";

/// The collector-leg positive control.
///
/// `up` for a scrape target is synthesised by Prometheus itself on every scrape,
/// so it is present the moment the job exists and the target is reachable —
/// INCLUDING on a completely idle cluster where no browser has ever emitted.
/// That is precisely what makes it usable here: it proves this suite reached a
/// verdict over a real scrape of the collector, rather than reading empty
/// because the selector was wrong, the job was missing, or the query path was
/// broken.
///
/// A client-series presence anchor would NOT do this job. Client series only
/// exist while a browser has emitted within `metric_expiration`, so gating on
/// one would red on every idle run — the identical trap this file already
/// reasons about for MC's lazily-created metrics, and how a control dies.
const COLLECTOR_UP_QUERY: &str = r#"up{job="otel-collector"}"#;

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
///
/// **That argument lives HERE, at the call site, and not on the kernel
/// function.** `metric_hygiene::fetch_all_series` returns a `Vec` and cannot make
/// any caller use one fetch for two assertions; stating the guarantee there would
/// read as though it held for everyone, while a caller fetching twice made it hold
/// nowhere. `30_observability.rs` makes the same argument separately, for its own
/// assertion pair — two call sites, two facts, not one duplicated fact.
///
/// The mechanics (response → `Vec<Series>`, and the loud refusal of a series with
/// no `__name__`) were hoisted to `crates/env-tests/src/fixtures/metric_hygiene.rs`
/// so they run in the always-on Rust lane. They previously sat in this file, which
/// `cargo test` cannot reach — the same root cause as the escaped-JSON relabel bug
/// documented below, and the reason a `"<unnamed>"` fallback survived here
/// undetected while silently defeating the anchor assertion.
async fn fetch_all_series(client: &PrometheusClient) -> Vec<Series> {
    metric_hygiene::fetch_all_series(client, &format!(r#"{{job=~"{JOBS}"}}"#), TRIAGE_SCRAPE).await
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

/// The client-series leg of R1, now that client metrics ARE scrape-reachable.
///
/// # This test replaced a premise pin that this very change would have blinded
///
/// It used to assert that `dt_client_*` was NOT scrape-reachable, and instructed
/// a future reader to extend the suite if that ever changed. That change is
/// this one. Left as written it would not have failed and prompted the
/// extension — it would have kept passing, because the old job selector listed
/// only the four Rust services while client series arrive under
/// `job="otel-collector"`. A control whose subject moves out of its selector
/// reports clean forever, having observed nothing.
///
/// # Why the §11 exception still has zero members — for a NEW reason
///
/// The old reasoning was structural non-reachability: the collector had one
/// `debug` exporter and nothing scraped it. Both facts are now false. The
/// conclusion survives on a better premise: the collector's metric-name
/// allowlist EXCLUDES all five grandfathered join-flow metrics
/// (`infra/services/otel-collector/configmap.yaml`), so no
/// `meeting_id_hash`-carrying series is exported at all. Had we instead exported
/// them and stripped the hash, this empty allowlist would have had to become a
/// real carve-out — which is an independent reason the name list is curated the
/// way it is.
///
/// # Vacuity, stated rather than hidden
///
/// On an idle cluster no browser has emitted within the collector's
/// `metric_expiration`, so there are no client series and the R1 leg over them
/// is ACKNOWLEDGED-VACUOUS: it passes having examined nothing. The positive
/// control below keeps that honest by proving the scrape path itself is live, so
/// a vacuous pass is distinguishable from a broken query. The NON-vacuous proof
/// that `keep_keys` actually drops `meeting_id_hash` lives in the client-owned
/// browser read-back, which asserts presence AND absence in the same fetch, in a
/// run where a browser is guaranteed to have emitted. Do not read this leg as
/// that proof.
#[tokio::test]
async fn client_series_carry_no_meeting_identifier_when_present() {
    let cluster = cluster().await;
    let client = PrometheusClient::new(&cluster.prometheus_base_url);

    // POSITIVE CONTROL FIRST. Without it, every assertion below is satisfiable
    // by a wholesale-empty result.
    let up = client
        .query(env_tests::fixtures::metrics::QueryRequest::instant(
            COLLECTOR_UP_QUERY,
        ))
        .await
        .unwrap_or_else(|e| panic!("{TRIAGE_SCRAPE}: querying {COLLECTOR_UP_QUERY} failed: {e}"));
    assert!(
        up.data.result.iter().any(|s| s
            .value
            .as_ref()
            .and_then(|(_, v)| v.parse::<f64>().ok())
            .is_some_and(|v| v >= 1.0)),
        "{TRIAGE_SCRAPE}: `{COLLECTOR_UP_QUERY}` is not 1.\n\n\
         The otel-collector scrape target is not up, so this suite cannot see \
         client series AT ALL and every absence assertion below would pass \
         vacuously. Check the `otel-collector` job in \
         `infra/kubernetes/observability/prometheus.yml`, the :8889 container \
         port, and the Prometheus ingress rule in the collector NetworkPolicy. \
         This is an environment failure, not a policy violation."
    );

    let series = fetch_all_series(&client).await;
    // One home for "what counts as a client series" — the same predicate the
    // fixture's own unit tests exercise, rather than a second inline prefix test.
    let client_names = scrape_reachable_client_series(&series);
    let owned: Vec<Series> = series
        .iter()
        .filter(|s| client_names.contains(&s.name))
        .cloned()
        .collect();

    // Not an anchor: see the vacuity paragraph above. Reported so a reader of a
    // green run knows which case they got.
    if owned.is_empty() {
        eprintln!(
            "NOTE: no `dt_client_*` series present — no browser has emitted within \
             the collector's metric_expiration. The R1 leg over client series is \
             vacuous for this run; the non-vacuous proof is the browser read-back."
        );
    }

    let violations = check_series(&owned);
    assert!(
        violations.is_empty(),
        "{TRIAGE_VIOLATION}: client series reached Prometheus carrying a meeting \
         identifier or an overclaim token:\n{}\n\n\
         The metrics-path `keep_keys` in \
         `infra/services/otel-collector/configmap.yaml` is the control that should \
         have prevented this — check it before looking anywhere else.",
        violations
            .iter()
            .map(|v| format!("  - [{:?}] {} :: {}", v.rule, v.series, v.detail))
            .collect::<Vec<_>>()
            .join("\n")
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
