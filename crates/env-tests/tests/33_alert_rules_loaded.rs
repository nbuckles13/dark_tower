//! Every alert rule on disk is actually LOADED by the running Prometheus.
//!
//! # WHAT A GREEN HERE PROVES — AND WHAT IT DOES NOT
//!
//! **It proves that the rules are loaded and evaluating.** It proves **nothing**
//! about whether any rule's expression can ever match.
//!
//! That distinction is not pedantry. `MCMediaGenerationDivergence` and
//! `MHMediaEgressQueueOverflowRate` read metrics Prometheus scrapes.
//! `MCMediaMissingKeyMaterial` reads `dt_client_media_frames_dropped_total`,
//! and no `dt_client_*` metric reaches Prometheus in this deployment at all:
//! the OTLP collector's metrics pipeline exports to `debug` — its own container
//! log — and no scrape job targets the collector. That rule is loaded,
//! evaluating, and **incapable of ever matching**. It will appear in
//! `/api/v1/rules` and this test will pass on it. Nothing here contradicts the
//! premise-unmet marking it carries in the rules file and in
//! `docs/observability/alerts.md`; the two statements are about different
//! things and both are true.
//!
//! Written out because the conflation is the exact defect `docs/TODO.md`
//! recorded against the alerting chain: *"the alerts landed" must not be read as
//! "the alerts fire"*. A test named `alert_rules_loaded` is one careless reading
//! away from being cited as evidence that alerting works.
//!
//! # Why this test exists at all
//!
//! Until 2026-09-09 nothing in this tree evaluated an alert rule: `rule_files:`
//! was commented out in the compose config and absent from the cluster config,
//! while `dt-guard alert-rules-policy` reported clean over rule *syntax*. Static
//! validation cannot see whether a process reads the files it validates. Two
//! failures in particular are invisible to config inspection and visible here:
//! a **tolerated parse failure** (Prometheus logs and continues), and a **mount
//! misconfiguration** (the ConfigMap is generated but not mounted, or mounted at
//! the wrong path).
//!
//! # Where the expected set comes from
//!
//! From the kustomize `configMapGenerator` list — the artifact that decides
//! which rule files are in the pod being queried — not from a predicate written
//! in this crate. `dt-guard alert-rules-policy` asserts set equality between
//! that list, the on-disk loadable-rules predicate, and both `rule_files` globs,
//! failing in either direction, so a file added to a glob but not the ConfigMap
//! fails Layer 3 before this test could silently skip it.
//!
//! # No skip-on-absent
//!
//! There is deliberately no `is_available` early return. "The rules are not
//! loaded" is precisely the state this test exists to go red on, so a self-skip
//! would be a check that ran where it could observe nothing and reported clean.

#![cfg(feature = "observability")]

use env_tests::cluster::ClusterConnection;
use env_tests::fixtures::alert_rules_loaded::{
    alert_names_in_yaml, loaded_alert_names, mounted_rules_files, RULES_DIR, RULES_KUSTOMIZATION,
};
use env_tests::fixtures::PrometheusClient;
use std::collections::BTreeSet;

/// Alerts this task shipped. Named individually, and separately from the
/// "non-empty" control, because a non-empty on-disk set proves only that the
/// extractor ran: the pre-existing gc/mc/mh alerts would carry the pass while a
/// NEW rule group silently failed to parse and Prometheus tolerated it. Tying
/// the assertion to this task's own deliverable closes that gap.
const NEW_ALERTS_THIS_TASK: &[&str] = &[
    "MCMediaGenerationDivergence",
    "MCMediaMissingKeyMaterial",
    "MHMediaEgressQueueOverflowRate",
];

fn repo_root() -> std::path::PathBuf {
    // CARGO_MANIFEST_DIR is `<repo>/crates/env-tests`.
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("repo root is two levels above the crate manifest")
        .to_path_buf()
}

/// Every `alert:` name across the loadable rules files on disk.
///
/// Panics with a COULD-NOT-EVALUATE reason on any read or parse problem. That
/// token is deliberately distinct from the content-mismatch token below: a
/// broken extractor and a genuinely-unloaded rule are different faults, and
/// collapsing them is how a broken extractor gets "fixed" by relaxing it.
fn on_disk_alert_names() -> BTreeSet<String> {
    let root = repo_root();
    // DERIVED FROM THE MOUNT DECISION, NOT FROM A PREDICATE WRITTEN HERE.
    // The `configMapGenerator` list is what puts files in the pod this test
    // queries, so it is the artifact under test. An earlier version hand-wrote
    // a mirror of the `rule_files` glob and its header claimed dt-guard kept
    // the two honest; it did not, and they already disagreed. See the module
    // rustdoc.
    let kust_path = root.join(RULES_KUSTOMIZATION);
    let kust = std::fs::read_to_string(&kust_path).unwrap_or_else(|e| {
        panic!(
            "COULD-NOT-EVALUATE: cannot read {}: {e}",
            kust_path.display()
        )
    });
    let mounted = mounted_rules_files(&kust).unwrap_or_else(|| {
        panic!(
            "COULD-NOT-EVALUATE: no `configMapGenerator` rule-file entries found in \
             {RULES_KUSTOMIZATION}. An empty mounted set makes every assertion below pass \
             vacuously, so this is a broken-extraction failure, NOT a clean result."
        )
    });

    let dir = root.join(RULES_DIR);
    let mut names = BTreeSet::new();
    for file_name in &mounted {
        let path = dir.join(file_name);
        let content = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "COULD-NOT-EVALUATE: {RULES_KUSTOMIZATION} mounts {file_name}, but it cannot \
                 be read at {}: {e}",
                path.display()
            )
        });
        let parsed = alert_names_in_yaml(&content).unwrap_or_else(|| {
            panic!("COULD-NOT-EVALUATE: {file_name} did not parse as alert-rules YAML")
        });
        names.extend(parsed);
    }
    names
}

#[tokio::test]
async fn every_on_disk_alert_rule_is_loaded_by_prometheus() {
    let cluster = ClusterConnection::new()
        .await
        .expect("Failed to connect to cluster - ensure port-forwards are running");
    let prometheus = PrometheusClient::new(&cluster.prometheus_base_url);

    let expected = on_disk_alert_names();
    // POSITIVE CONTROL #1 — the extraction produced something.
    assert!(
        !expected.is_empty(),
        "COULD-NOT-EVALUATE: extracted zero alert names from {RULES_DIR}"
    );

    let body = prometheus
        .rules()
        .await
        .expect("COULD-NOT-EVALUATE: GET /api/v1/rules failed");
    let loaded = loaded_alert_names(&body).unwrap_or_else(|| {
        panic!(
            "COULD-NOT-EVALUATE: /api/v1/rules did not return a successful, well-formed \
             rules envelope. Body starts: {}",
            &body.chars().take(200).collect::<String>()
        )
    });

    // POSITIVE CONTROL #2 — this task's OWN rules loaded, not merely somebody's.
    // Checked before the subset assertion so a new group that failed to parse is
    // reported as itself rather than as one name among many.
    for name in NEW_ALERTS_THIS_TASK {
        assert!(
            loaded.contains(*name),
            "RULE-NOT-LOADED: {name} is on disk but absent from Prometheus's loaded rule \
             groups. Prometheus TOLERATES a rule-file parse error by logging and continuing, \
             so a syntax error in a new group looks exactly like this. Check the Prometheus \
             pod logs, then that the rules ConfigMap is mounted at /etc/prometheus/rules and \
             matched by the `rule_files` glob."
        );
    }

    // Direction: on-disk SUBSET-OF loaded. Never the reverse — "every loaded
    // rule is on disk" is trivially true when nothing loaded at all.
    let missing: Vec<&String> = expected.difference(&loaded).collect();
    assert!(
        missing.is_empty(),
        "RULE-NOT-LOADED: {} of {} on-disk alert rules are absent from Prometheus's loaded \
         rule groups: {missing:?}. Causes this catches that config inspection cannot: a \
         tolerated rule-file parse failure, and a ConfigMap that is generated but not mounted \
         (or mounted at a path the `rule_files` glob does not cover).",
        missing.len(),
        expected.len()
    );
}
