//! P0 Smoke Tests: MH Deployment Configuration
//!
//! Two families of assertion against the LIVE cluster, never a local render:
//!
//! 1. Story 1 Test-Plan item F (validates R-21): the deployed
//!    `MH_TERMINATION_GRACE_SECONDS` equals the pod spec's
//!    `terminationGracePeriodSeconds`. (Sections below through "Why
//!    `-n dark-tower`".)
//! 2. Story 2 configuration surface (R-19, R-22, R-23, R-24): MH's egress-budget
//!    chain and §8 policy keys are wired into BOTH instances, carry parseable
//!    values, are what the running process actually loaded, and — on Kind —
//!    size a stream ceiling that meets the demo requirement. (Section "Story 2
//!    configuration surface".)
//!
//! # What this asserts, and why it is not a tautology
//!
//! Both sides come off **one live pod object** fetched from the API server:
//!
//! - side (a) `.spec.containers[name=mh-service].env[name=MH_TERMINATION_GRACE_SECONDS].value`
//! - side (b) `.spec.terminationGracePeriodSeconds` **of that same pod**
//!
//! Nothing here runs `kustomize`. The two sides are equal only if the kustomize
//! `replacements:` block in `infra/services/mh-service/kustomization.yaml` was
//! written correctly, survived the Kind overlay build, *and* was applied to the
//! cluster. That is the "does it apply" half; the static half (is the key
//! declared at all) belongs to `dt-guard env-config`, which reads base YAML and
//! never runs kustomize.
//!
//! # Honest limitation — read this before citing the test as stronger than it is
//!
//! The strongest possible reading would be the MH **process's own** environment
//! (`printenv`, `/proc/1/environ`). That is not available: the MH image is
//! distroless with `readOnlyRootFilesystem: true` and no shell, so
//! `kubectl exec` has no binary to run. The pod spec **as stored by the API
//! server** is the strongest reading available. It is still the *deployed
//! artifact* rather than a local render, but it is not proof that the container
//! process observed the value.
//!
//! # Why `-o json` + `serde_json`, not `-o jsonpath`
//!
//! The filter expression needed here (`env[?(@.name=='...')]`) is
//! quoting-fragile in a shell argv and returns an **empty string** on a typo
//! rather than failing. An empty probe compared against an empty probe reads as
//! "equal", so a typo would make this test pass vacuously — the precise failure
//! mode it exists to prevent. Parsing structured output makes an absent field a
//! distinguishable, loud condition.
//!
//! # Why `-n dark-tower`, not `-n default`
//!
//! Every Dark Tower workload runs in `dark-tower`. Nothing runs in `default`.
//!
//! # Story 2 configuration surface
//!
//! These keys arrive via `valueFrom.configMapKeyRef`, so unlike the grace value
//! there is no literal `value` on the pod — the pod object holds only a
//! REFERENCE. The ConfigMap is content-addressed (ADR-0038 §2): its live name
//! is `mh-service-config-<hash>`, resolved FROM THAT REFERENCE
//! (`env_tests::fixtures::kube::configmap_name_for`), so every value checked is
//! the generation the pod actually runs — never the bare base name (which no
//! longer exists) and never a label selector (superseded generations are not
//! pruned). The tests therefore read three live objects and one file, and
//! each assertion proves something different. Keep (a) and (c) adjacent in
//! any rewrite: they look similar and neither covers the other.
//!
//! - **(a) Wiring proves the REFERENCE.** For both `mh-0` and `mh-1`, every key
//!   in [`STORY2_KEYS`] is present on the live pod as a `configMapKeyRef` to
//!   the ONE [`SHARED_CONFIGMAP`] generation that pod is wired to, whose `key`
//!   equals the env var's own name. This is
//!   the per-workload coverage `dt-guard env-config` cannot give yet: its rule
//!   1 only engages once the MH code task makes these reads required, and its
//!   rule 3 is satisfied if EITHER workload references a key — so "present in
//!   mh-0, missing from mh-1" is guard-green today and CrashLoops mh-1 the
//!   moment the required-read image ships. The static twin of the `key == name`
//!   half is `dt-guard env-config`'s `configmap_key_name_mismatch`; this
//!   version exists for a cluster that does not match its manifests (a hand
//!   `kubectl apply -f`, a partial rollout). Do not delete either as redundant.
//! - **(b) Parse** proves every resolved value is loadable under the MH code
//!   task's required-read image BEFORE that image ships — the point of landing
//!   manifests first.
//! - **(c) Published-gauge parity proves the READING.** For every
//!   configuration value in [`CONFIG_GAUGES`], the value the running MH
//!   PROCESS publishes (a static gauge `publish_egress_admission` sets at
//!   startup from the field enforcement reads) equals the deployed ConfigMap
//!   value of THAT instance's generation, under MH's own conversion (bits →
//!   bytes, ms → s). It is the assertion here about what the running process
//!   observed. It reads gauges, not the startup log line: a log line is lost
//!   to kubelet log rotation on a long-lived pod, and pods survive across
//!   Layer-7 runs (ADR-0038: nothing restarts an unchanged workload). A
//!   gauge is per-process and static for its lifetime, and a changed value is
//!   a new pod (content addressing), so the gauge always reflects the process
//!   that is running. It CANNOT prove wiring on its own: every one of these
//!   reads is REQUIRED (no default — a missing key refuses boot), but a literal
//!   `value:` on the workload equal to the ConfigMap's value would also match.
//!   (a) is what proves the `configMapKeyRef`, which is why it exists.
//! - **(d) Staleness — RETIRED (ADR-0038 devloop 2).** It asserted the
//!   container started after the ConfigMap last changed, because a value edit
//!   used to change a ConfigMap IN PLACE without restarting pods. Content
//!   addressing makes that failure impossible short of a hand `kubectl edit`: a
//!   changed value is a NEW ConfigMap name, hence a new pod template, hence a
//!   rollout, and Layer 7 waits for every rollout before this suite runs. The
//!   check read `managedFields` times that no longer mean "config changed".
//! - **(e) Kind sizing.** The deployed budget is the Kind overlay's value (not
//!   the base placeholder), and the stream ceiling derived from DEPLOYED values
//!   meets the demo requirement.
//!
//! ## What this does NOT prove — read before citing it
//!
//! (c) shows the process LOADED each value; it does not show every code path
//! reads the field the gauge reflects. That is pinned on the MH side:
//! `crates/mh-service/tests/stream_admission_integration.rs` asserts each gauge
//! is `.set()` from the one field enforcement reads. The retired-key refs and
//! the ratio threshold are outside (c) (the latter: `28_mh_egress_admission.rs`).
//!
//! ## Stale values after a config change
//!
//! A config change renames the content-addressed ConfigMap, which rolls the pods
//! that reference it on the next `deploy` (`infra/kind/scripts/deploy.sh`
//! applies the whole environment root and waits for every rollout). A red here
//! that says the process is not running the deployed value therefore means the
//! cluster was not converged to the tree — or was hand-edited; the failure
//! message carries the fix (`REDEPLOY_HINT`).
//!
//! # Diagnostic output policy (PII)
//!
//! Failure messages may print ONLY: pod name, namespace, label selector, env
//! var **names**, ConfigMap names and key names, the `MH_TERMINATION_GRACE_SECONDS`
//! value, `.spec.terminationGracePeriodSeconds`, the scalar VALUES of the named
//! story-2 keys (none is a credential), timestamps, and counts. Never the
//! parsed `serde_json::Value`, never the env array itself, never a ConfigMap's
//! `data` map, never raw `kubectl` stdout, and never a log line. (c) prints
//! only the named gauge's scalar and the key's deployed scalar. It never
//! scrapes a `Debug` render of a config struct, which would inherit
//! `SecretString` redaction as an assumption rather than checking it.
//!
//! This is safe *today* only because MH's sole credential (`MH_CLIENT_SECRET`)
//! arrives via `secretKeyRef`, so the pod object holds a reference rather than
//! a value, and `mh-service-config` holds no secrets — one future
//! literal-valued env var or secret-bearing ConfigMap key would turn a
//! diagnostic dump into a credential leak in CI logs. Extract the scalars
//! first, then format. The ConfigMap and gauge reads are in scope of this policy.

#![cfg(feature = "smoke")]

use env_tests::fixtures::kube::{configmap_name_for, configmap_name_for_all, REDEPLOY_HINT};
use env_tests::fixtures::mh_config::{gauge_verdict, Conversion, GaugeVerdict};
use env_tests::{repo_root, NAMESPACE};
use serde_json::Value;
use std::process::Command;

const CONTAINER: &str = "mh-service";
const GRACE_ENV: &str = "MH_TERMINATION_GRACE_SECONDS";

/// Fetches the single pod matching `instance=<instance>` as parsed JSON.
///
/// Never skips. Missing `kubectl`, a failed call, or an empty pod set are all
/// hard failures with distinct messages — a probe that can return "nothing" and
/// be read as "equal" is what this test exists to rule out.
fn fetch_pod(instance: &str) -> Value {
    let selector = format!("instance={instance}");
    let args = [
        "get",
        "pods",
        "-n",
        NAMESPACE,
        "-l",
        selector.as_str(),
        "-o",
        "json",
    ];

    let output = Command::new("kubectl")
        .args(args)
        .output()
        .unwrap_or_else(|e| {
            panic!(
                "kubectl not available - cannot verify the deployed drain window \
                 for {instance}. env-tests require kubectl installed and \
                 configured: {e}"
            )
        });

    assert!(
        output.status.success(),
        "`kubectl {}` failed for {instance} (exit status {}): {}",
        args.join(" "),
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    let parsed: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!("could not parse `kubectl get pods -o json` output for {instance}: {e}")
    });

    let items = parsed
        .get("items")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("kubectl output for {instance} has no `items` array"));

    assert!(
        !items.is_empty(),
        "no pods matched -n {NAMESPACE} -l instance={instance}. The drain-window \
         assertion would have nothing to compare and would pass vacuously. \
         Either MH is not deployed or the label selector has drifted - fix the \
         selector, do not delete this assertion."
    );

    // Ignore pods that are already terminating. During a rolling update both the
    // old and the new ReplicaSet's pods match the `instance=` selector, and the
    // OLD pod predates this manifest change - it carries no MH_TERMINATION_GRACE_SECONDS
    // at all. Taking items[0] unfiltered would fail against a pod that is on its
    // way out, which is a false negative on the Layer-7 retry path rather than a
    // real defect. Filtering on deletionTimestamp (not on phase) is the precise
    // test: it means "the API server has accepted this pod's deletion".
    let live: Vec<&Value> = items
        .iter()
        .filter(|p| p.pointer("/metadata/deletionTimestamp").is_none())
        .collect();

    assert!(
        !live.is_empty(),
        "every pod matching -n {NAMESPACE} -l instance={instance} is terminating \
         ({} total). Nothing is serving this instance - the assertion is not \
         meaningful and must not pass vacuously.",
        items.len()
    );

    // Pick the NEWEST live pod, not an arbitrary one.
    //
    // `deletionTimestamp` alone is not sufficient. These are 1-replica
    // Deployments with the default RollingUpdate strategy, which resolves to
    // maxSurge=1 / maxUnavailable=0 — so the replacement pod is created and must
    // become Ready BEFORE the outgoing pod is marked for deletion. That leaves a
    // window where two pods match `instance=mh-N` and NEITHER has a
    // `deletionTimestamp`: the old one (which predates this manifest change and
    // carries no MH_TERMINATION_GRACE_SECONDS at all) and the new one. Taking an
    // arbitrary element would panic against a pod on its way out — a false
    // negative on the Layer-7 retry path, not a real defect.
    //
    // The surged pod is always the newer, and it is the one carrying the value
    // under test. RFC 3339 timestamps sort lexicographically, so a string
    // comparison is the correct ordering here.
    let newest = live
        .iter()
        .max_by_key(|p| {
            p.pointer("/metadata/creationTimestamp")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        })
        .expect("live is non-empty, asserted above");

    (*newest).clone()
}

/// Asserts one instance's deployed env value equals its own pod spec's grace.
fn assert_grace_matches_pod_spec(instance: &str) {
    let pod = fetch_pod(instance);

    // Extract every scalar BEFORE formatting anything, so no `Value` and no env
    // array can reach a panic message. See the PII policy in the module doc.
    let pod_name = pod
        .pointer("/metadata/name")
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("pod matched by instance={instance} has no metadata.name"))
        .to_string();

    let spec_grace = pod
        .pointer("/spec/terminationGracePeriodSeconds")
        .and_then(Value::as_i64)
        .unwrap_or_else(|| {
            panic!(
                "pod {pod_name} (-n {NAMESPACE}) has no readable \
                 .spec.terminationGracePeriodSeconds. This is the SOURCE side of \
                 the kustomize replacement; without it the derivation has no \
                 single source of truth."
            )
        });

    let containers = pod
        .pointer("/spec/containers")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("pod {pod_name} has no .spec.containers array"));

    let container = containers
        .iter()
        .find(|c| c.get("name").and_then(Value::as_str) == Some(CONTAINER))
        .unwrap_or_else(|| {
            panic!("pod {pod_name} has no container named {CONTAINER:?}");
        });

    let env = container
        .get("env")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("container {CONTAINER} in pod {pod_name} has no `env` array"));

    let env_entry = env
        .iter()
        .find(|e| e.get("name").and_then(Value::as_str) == Some(GRACE_ENV));

    let Some(env_entry) = env_entry else {
        // Names only - never the values. See the PII policy in the module doc.
        let names: Vec<&str> = env
            .iter()
            .filter_map(|e| e.get("name").and_then(Value::as_str))
            .collect();
        panic!(
            "pod {pod_name} has no {GRACE_ENV} env entry. This is the TARGET side \
             of the kustomize replacement. Present env names: {names:?}"
        );
    };

    let raw = env_entry
        .get("value")
        .and_then(Value::as_str)
        .unwrap_or_else(|| {
            panic!("pod {pod_name}: {GRACE_ENV} has no literal `value` (is it a valueFrom?)")
        });

    // Branch 1: the sentinel survived. This is a BUILD-PATH failure, not a
    // drifted value, and "expected 35, got 0" would send a responder to the
    // wrong file - so it gets its own message.
    assert_ne!(
        raw, "0",
        "pod {pod_name}: {GRACE_ENV} is still the unreplaced sentinel \"0\". The \
         kustomize `replacements:` block in \
         infra/services/mh-service/kustomization.yaml did NOT run - these \
         manifests were applied without kustomize (`kubectl apply -f` bypasses \
         it). Redeploy through the Kind overlay. This is a build-path problem, \
         not a wrong grace period."
    );

    let env_grace: i64 = raw.parse().unwrap_or_else(|e| {
        panic!("pod {pod_name}: {GRACE_ENV} value {raw:?} is not an integer: {e}")
    });

    // Branch 2: both sides present and parseable, but they disagree.
    assert_eq!(
        env_grace, spec_grace,
        "pod {pod_name} (-n {NAMESPACE}): {GRACE_ENV}={env_grace} does not match \
         its own .spec.terminationGracePeriodSeconds={spec_grace}. These must be \
         equal by construction - the env value is DERIVED from the pod spec by \
         the kustomize replacement (R-21). A mismatch means the replacement \
         targeted the wrong Deployment, or the value was hand-edited."
    );
}

#[tokio::test]
async fn test_mh_0_termination_grace_matches_pod_spec() {
    assert_grace_matches_pod_spec("mh-0");
}

#[tokio::test]
async fn test_mh_1_termination_grace_matches_pod_spec() {
    assert_grace_matches_pod_spec("mh-1");
}

// =============================================================================
// Story 2 configuration surface (R-19, R-22, R-23, R-24)
//
// See the module doc, "Story 2 configuration surface", for what each assertion
// proves and — as importantly — what it does not.
// =============================================================================

/// The GENERATOR BASE NAME of the ConfigMap every story-2 key is read from.
/// The live name is content-addressed (`mh-service-config-<hash>`, ADR-0038
/// §2); resolve it from the pod spec with [`shared_configmap_for`] /
/// [`shared_configmap`] — never use this bare name as a live object name.
const SHARED_CONFIGMAP: &str = "mh-service-config";

/// Both MH instances. Every per-instance assertion runs over this one list.
const MH_INSTANCES: &[&str] = &["mh-0", "mh-1"];

/// The live `mh-service-config` generation `instance`'s pod references.
fn shared_configmap_for(instance: &str) -> String {
    configmap_name_for(instance, SHARED_CONFIGMAP)
}

/// The live `mh-service-config` generation BOTH MH instances reference — for
/// the cluster-wide assertions. The instances sharing one generation is itself
/// asserted (by the fixture): two generations means they run different config.
fn shared_configmap() -> String {
    configmap_name_for_all(MH_INSTANCES, SHARED_CONFIGMAP)
}

/// How a resolved value must parse. Mirrors what the MH code task's required
/// reads will accept, so a value that fails here would refuse to boot there.
#[derive(Clone, Copy, Debug)]
enum ValueShape {
    /// A count, bit rate or duration: an unsigned integer strictly above zero.
    PositiveInteger,
    /// A ratio in `0.0..=1.0`.
    UnitRatio,
}

/// Every key this story wires into BOTH MH workloads. ONE list, iterated for
/// every instance — never a second copy per instance or per test.
///
/// `MH_MAX_STREAMS` is RETIRED but deliberately listed: its two refs must
/// survive one deploy so `kubectl rollout undo` does not produce
/// `CreateContainerConfigError`, and once the code read is gone nothing else in
/// the tree asserts those refs still exist.
const STORY2_KEYS: &[(&str, ValueShape)] = &[
    // Egress-budget chain (R-19, R-23).
    ("MH_EGRESS_BUDGET_BPS", ValueShape::PositiveInteger),
    ("MH_STREAM_COST_AUDIO_BPS", ValueShape::PositiveInteger),
    ("MH_STREAM_COST_VIDEO_BPS", ValueShape::PositiveInteger),
    ("MH_EGRESS_REJECTION_RATIO_THRESHOLD", ValueShape::UnitRatio),
    ("MH_MAX_REGISTERED_MEETINGS", ValueShape::PositiveInteger),
    (
        "MH_MAX_MUTED_SOURCES_PER_MEETING",
        ValueShape::PositiveInteger,
    ),
    // ADR-0036 §8 policy bounds (R-22).
    (
        "MH_MAX_EGRESS_STREAMS_PER_MEETING",
        ValueShape::PositiveInteger,
    ),
    (
        "MH_MAX_CANDIDATE_SOURCES_PER_EGRESS",
        ValueShape::PositiveInteger,
    ),
    ("MH_MAX_TOTAL_EGRESS_EDGES", ValueShape::PositiveInteger),
    ("MH_POLICY_APPLY_TIMEOUT_MS", ValueShape::PositiveInteger),
    // Retired, refs retained for rollback safety (R-24).
    ("MH_MAX_STREAMS", ValueShape::PositiveInteger),
];

/// How long a MH gauge series may take to appear for a pod (its first scrape
/// after process start). ONE bound for every presence poll in this file.
const GAUGE_PRESENT_BOUND: std::time::Duration = std::time::Duration::from_secs(60);

/// `(ConfigMap key, published gauge, Conversion)` for every configuration value
/// the running MH process loaded. ONE table, and the gauges are published by
/// `crates/mh-service/src/observability/metrics.rs:publish_egress_admission`
/// from the ONE field enforcement reads. The derived stream ceiling is not a
/// ConfigMap key (its parity is
/// [`test_running_mh_publishes_the_deployed_stream_ceiling`]); the ratio
/// threshold is a float, checked against its gauge by `28_mh_egress_admission.rs`.
const CONFIG_GAUGES: &[(&str, &str, Conversion)] = &[
    (
        "MH_MAX_EGRESS_STREAMS_PER_MEETING",
        "mh_media_egress_streams_per_meeting_limit",
        Conversion::Identity,
    ),
    (
        "MH_MAX_CANDIDATE_SOURCES_PER_EGRESS",
        "mh_media_candidate_sources_per_egress_limit",
        Conversion::Identity,
    ),
    (
        "MH_MAX_TOTAL_EGRESS_EDGES",
        "mh_media_egress_edges_limit",
        Conversion::Identity,
    ),
    (
        "MH_POLICY_APPLY_TIMEOUT_MS",
        "mh_media_policy_apply_timeout_seconds",
        Conversion::MsToSeconds,
    ),
    (
        "MH_MAX_REGISTERED_MEETINGS",
        "mh_media_registered_meetings_limit",
        Conversion::Identity,
    ),
    (
        "MH_MAX_MUTED_SOURCES_PER_MEETING",
        "mh_media_muted_sources_per_meeting_limit",
        Conversion::Identity,
    ),
    (
        "MH_EGRESS_BUDGET_BPS",
        "mh_media_egress_budget_bytes_per_second",
        Conversion::BitsToBytesFloor,
    ),
    (
        "MH_STREAM_COST_AUDIO_BPS",
        "mh_media_stream_cost_audio_bytes_per_second",
        Conversion::BitsToBytesCeil,
    ),
    (
        "MH_STREAM_COST_VIDEO_BPS",
        "mh_media_stream_cost_video_bytes_per_second",
        Conversion::BitsToBytesCeil,
    ),
];

/// The demo requirement the Kind budget is sized against: N+1 participants
/// all hearing each other on one handler. This is the REQUIREMENT being
/// asserted, stated once here; the Kind patch comment states it in prose and
/// points at this test.
const DEMO_N: u64 = 5;

/// Egress streams for N+1 participants all hearing each other: every one of
/// the N+1 subscribers receives each of the other N senders.
const fn required_edges(n: u64) -> u64 {
    (n + 1) * n
}

/// Fetches the named ConfigMap from the live cluster as parsed JSON.
///
/// Never skips; same discipline as [`fetch_pod`]. Reading the cluster's copy —
/// not a kustomize render — is the point: a render cannot tell you whether the
/// overlay was applied, or whether someone edited the live object by hand.
fn fetch_configmap(name: &str) -> Value {
    // `name` is the LIVE (hash-suffixed) name, resolved from the pod spec.
    let args = ["get", "configmap", name, "-n", NAMESPACE, "-o", "json"];
    let output = Command::new("kubectl")
        .args(args)
        .output()
        .unwrap_or_else(|e| {
            panic!(
                "kubectl not available - cannot read ConfigMap {name}. env-tests \
                 require kubectl installed and configured: {e}"
            )
        });
    assert!(
        output.status.success(),
        "`kubectl {}` failed (exit status {}): {}",
        args.join(" "),
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!("could not parse `kubectl get configmap {name} -o json` output: {e}")
    })
}

/// The pod's `metadata.name`, extracted before anything is formatted.
fn pod_name_of(pod: &Value, instance: &str) -> String {
    pod.pointer("/metadata/name")
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("pod matched by instance={instance} has no metadata.name"))
        .to_string()
}

/// The `mh-service` container of `pod`.
fn mh_container<'a>(pod: &'a Value, pod_name: &str) -> &'a Value {
    pod.pointer("/spec/containers")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("pod {pod_name} has no .spec.containers array"))
        .iter()
        .find(|c| c.get("name").and_then(Value::as_str) == Some(CONTAINER))
        .unwrap_or_else(|| panic!("pod {pod_name} has no container named {CONTAINER:?}"))
}

/// `data[key]` of a fetched ConfigMap. An absent key is its own failure: it is
/// the `CreateContainerConfigError` case, not a wrong value.
fn configmap_value(cm: &Value, key: &str) -> String {
    cm.pointer("/data")
        .and_then(|d| d.get(key))
        .and_then(Value::as_str)
        .unwrap_or_else(|| {
            panic!(
                "live ConfigMap {SHARED_CONFIGMAP} (-n {NAMESPACE}) has no key {key}. \
                 Any pod holding a hard configMapKeyRef to it cannot start \
                 (CreateContainerConfigError, empty logs). Either the manifests \
                 were not applied, or the key was removed ahead of its refs — \
                 which is the rollback trap the two-deploy retirement exists to avoid."
            )
        })
        .to_string()
}

/// Parses `raw` as a strictly-positive integer, failing with the key's name.
fn parse_positive(key: &str, raw: &str) -> u64 {
    let v: u64 = raw.parse().unwrap_or_else(|e| {
        panic!(
            "{key}={raw:?} in {SHARED_CONFIGMAP} is not an unsigned integer ({e}). \
             The MH code task reads it as REQUIRED and would refuse to start."
        )
    });
    assert!(
        v > 0,
        "{key}=0 in {SHARED_CONFIGMAP}: must be strictly positive. The MH code task \
         would refuse to start (and a zero cost would divide by zero in the ceiling)."
    );
    v
}

/// Asserts `raw` parses as `shape` would under the MH code task's required read.
fn assert_parses(key: &str, raw: &str, shape: ValueShape) {
    match shape {
        ValueShape::PositiveInteger => {
            parse_positive(key, raw);
        }
        ValueShape::UnitRatio => {
            let v: f64 = raw.parse().unwrap_or_else(|e| {
                panic!("{key}={raw:?} in {SHARED_CONFIGMAP} is not a number ({e}).")
            });
            assert!(
                v.is_finite() && (0.0..=1.0).contains(&v),
                "{key}={raw:?} in {SHARED_CONFIGMAP} must lie in 0.0..=1.0. The MH \
                 code task reads it as REQUIRED and would refuse to start."
            );
        }
    }
}

/// (a) wiring + (b) parse, for one instance.
fn assert_story2_keys_wired(instance: &str) {
    // The generation THIS instance's pod references — so the values checked
    // below are the config in effect for this pod, not a superseded one.
    let live_cm = shared_configmap_for(instance);
    let cm = fetch_configmap(&live_cm);
    let pod = fetch_pod(instance);
    let pod_name = pod_name_of(&pod, instance);
    let env = mh_container(&pod, &pod_name)
        .get("env")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("container {CONTAINER} in pod {pod_name} has no `env` array"));

    for &(key, shape) in STORY2_KEYS {
        let Some(entry) = env
            .iter()
            .find(|e| e.get("name").and_then(Value::as_str) == Some(key))
        else {
            panic!(
                "pod {pod_name} (-n {NAMESPACE}) has no env entry {key}. Every \
                 story-2 key must be referenced from BOTH mh-0-deployment.yaml and \
                 mh-1-deployment.yaml — per workload, never union. Once the MH \
                 code task makes this read required, this pod CrashLoops alone \
                 while its sibling stays healthy."
            );
        };

        let cm_ref = entry
            .pointer("/valueFrom/configMapKeyRef")
            .unwrap_or_else(|| {
                panic!(
                    "pod {pod_name}: env {key} is not a configMapKeyRef (a literal \
                     value or another source). {SHARED_CONFIGMAP} is the single \
                     home for its number; a literal here is a second encoding."
                )
            });
        let ref_name = cm_ref.get("name").and_then(Value::as_str);
        let ref_key = cm_ref.get("key").and_then(Value::as_str);

        assert_eq!(
            ref_name,
            Some(live_cm.as_str()),
            "pod {pod_name}: env {key} references ConfigMap {ref_name:?}, expected \
             {live_cm:?} (the one {SHARED_CONFIGMAP} generation this pod is wired to)."
        );
        assert_eq!(
            ref_key,
            Some(key),
            "pod {pod_name}: env {key} reads key {ref_key:?} — a CROSSED reference. \
             The pod starts and passes readiness while running another variable's \
             value. The key must equal the env var name. (dt-guard env-config's \
             configmap_key_name_mismatch catches this in the manifests; reaching \
             this assertion means the live cluster does not match them.)"
        );
        assert_ne!(
            cm_ref.get("optional").and_then(Value::as_bool),
            Some(true),
            "pod {pod_name}: env {key} is `optional: true`. It must be a HARD \
             reference: a missing key must be a loud CreateContainerConfigError, \
             never a pod that starts on a value nobody chose."
        );

        let raw = configmap_value(&cm, key);
        assert_parses(key, &raw, shape);
    }

    // The muted-source bound's safety claim is a RELATIONSHIP between two
    // deployed values, so it is asserted here rather than left to the comment
    // that states it.
    //
    // `MH_MAX_MUTED_SOURCES_PER_MEETING` is documented as a bound that can
    // never falsely reject a legitimate snapshot. That holds only while it is
    // at least `MH_MAX_EGRESS_STREAMS_PER_MEETING`: MC sends a handler only the
    // muted senders that SOURCE at least one of its egress streams, so a
    // legitimate snapshot names at most as many muted senders as it has
    // streams (derivation: `infra/services/mh-service/config.env`).
    //
    // Since story 2 task 10 MH also REFUSES TO BOOT on the inversion
    // (`ConfigError::MutedSourceBoundBelowEgressBound`), so a violation here
    // would additionally show as a crash-looping pod; this assertion names the
    // cause in seconds.
    //
    // DIRECTION: raising the egress bound above the muted bound breaks the
    // claim; lowering it does not. Without this assertion a one-character edit
    // to either key silently turns server mute off for large meetings — the
    // registration is rejected whole, which is not a mute-shaped symptom.
    let muted = parse_positive(
        "MH_MAX_MUTED_SOURCES_PER_MEETING",
        &configmap_value(&cm, "MH_MAX_MUTED_SOURCES_PER_MEETING"),
    );
    let egress = parse_positive(
        "MH_MAX_EGRESS_STREAMS_PER_MEETING",
        &configmap_value(&cm, "MH_MAX_EGRESS_STREAMS_PER_MEETING"),
    );
    assert!(
        muted >= egress,
        "MH_MAX_MUTED_SOURCES_PER_MEETING={muted} is BELOW \
         MH_MAX_EGRESS_STREAMS_PER_MEETING={egress} in {SHARED_CONFIGMAP}. The \
         muted bound is documented as never able to falsely reject a legitimate \
         snapshot, and that derivation requires muted >= egress. As deployed, a \
         legitimate registration naming more muted senders than the muted bound \
         admits is rejected WHOLE — server mute stops working for large meetings, \
         and the symptom is a rejected registration rather than anything \
         mute-shaped. Fix: raise MH_MAX_MUTED_SOURCES_PER_MEETING to at least \
         MH_MAX_EGRESS_STREAMS_PER_MEETING, or lower the egress bound."
    );
}

#[tokio::test]
async fn test_story2_keys_wired_into_every_mh_instance() {
    for instance in MH_INSTANCES {
        assert_story2_keys_wired(instance);
    }
}

/// (c) What the RUNNING process loaded equals the deployed ConfigMap, for every
/// row of [`CONFIG_GAUGES`], per MH instance against ITS OWN ConfigMap
/// generation. Read from the static gauges MH publishes at startup — not the
/// startup log line, which log rotation loses on a long-lived pod. Three
/// distinct outcomes per row: no data (the gauge is absent for this pod), shape
/// (the publisher changed what it publishes), mismatch (the process runs a value
/// other than the deployed one).
#[tokio::test]
async fn test_policy_bounds_reported_by_running_mh_match_configmap() {
    use env_tests::cluster::ClusterConnection;
    use env_tests::fixtures::metrics::poll_until_pinned_instance;
    use env_tests::fixtures::PrometheusClient;
    use std::time::Duration;

    let cluster = ClusterConnection::new()
        .await
        .expect("Failed to connect to cluster - ensure port-forwards are running");
    let prom = PrometheusClient::new(&cluster.prometheus_base_url);

    // Per instance: its own generation and its own pod IP (the Prometheus
    // `instance` label is pod IP:port).
    let pods: Vec<(&str, String, String, Value)> = MH_INSTANCES
        .iter()
        .map(|&instance| {
            let pod = fetch_pod(instance);
            let pod_name = pod_name_of(&pod, instance);
            let pod_ip = pod
                .pointer("/status/podIP")
                .and_then(Value::as_str)
                .unwrap_or_else(|| panic!("pod {pod_name} has no status.podIP"))
                .to_string();
            let cm = fetch_configmap(&shared_configmap_for(instance));
            (instance, pod_name, pod_ip, cm)
        })
        .collect();

    for &(key, gauge, conversion) in CONFIG_GAUGES {
        for (instance, pod_name, pod_ip, cm) in &pods {
            let deployed = parse_positive(key, &configmap_value(cm, key));
            // Branch: no data for THIS pod. Polled PER POD, pinned to its IP: right after
            // a deploy rolls MH, the terminating predecessor is still a scrape target for
            // its drain window, so "enough instances have the series" can be satisfied
            // before the NEW pod's first scrape. The poll's timeout panic is the no-data
            // branch (its phase/expectation carry the explanation).
            let promql = format!("max by (instance) ({gauge})");
            let value = poll_until_pinned_instance(
                &prom,
                &promql,
                pod_ip,
                GAUGE_PRESENT_BOUND,
                Duration::from_secs(2),
                &format!(
                    "PRESENCE ({instance}, pod {pod_name}): MH publishes {gauge} at process \
                     start, so absence is a scrape/startup problem (the pod is not scraped, \
                     or the publish ran before the recorder) — NOT a configuration mismatch"
                ),
                "present",
                |_| true,
            )
            .await;
            let series = format!("instance at {pod_ip}");
            match gauge_verdict(conversion, deployed, value) {
                GaugeVerdict::Match => {}
                // Branch: the publisher's shape changed.
                GaugeVerdict::Shape => panic!(
                    "gauge {gauge} on {instance} ({series}) is not a value its {conversion:?} \
                     conversion can produce ({value}): the publisher's shape changed in \
                     crates/mh-service/src/observability/metrics.rs — update CONFIG_GAUGES \
                     with it. This is not a configuration mismatch."
                ),
                // Branch: the running process holds another value.
                GaugeVerdict::Mismatch(expected) => panic!(
                    "{instance} (pod {pod_name}) runs {gauge}={value}, but its ConfigMap \
                     ({SHARED_CONFIGMAP} generation) carries {key}={deployed}, which MH's \
                     {conversion:?} conversion makes {expected}. The process is not running \
                     the deployed value: the key is not wired into this workload, or the pod \
                     predates the config; {REDEPLOY_HINT}."
                ),
            }
        }
    }
}

/// Reads `data.MH_EGRESS_BUDGET_BPS` from a ConfigMap manifest in the tree.
fn tree_budget(rel_path: &str) -> u64 {
    let path = repo_root().join(rel_path);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    // The base is a configMapGenerator env file (ADR-0038 §2: `KEY=VALUE`, taken
    // literally); the Kind overlay's patch is still a strategic-merge YAML patch.
    if rel_path.ends_with(".env") {
        let raw = text
            .lines()
            .find_map(|l| l.strip_prefix("MH_EGRESS_BUDGET_BPS="))
            .unwrap_or_else(|| panic!("{} has no MH_EGRESS_BUDGET_BPS= line", path.display()));
        return parse_positive("MH_EGRESS_BUDGET_BPS", raw);
    }
    let doc: serde_norway::Value = serde_norway::from_str(&text)
        .unwrap_or_else(|e| panic!("{} is not valid YAML: {e}", path.display()));
    let raw = doc
        .get("data")
        .and_then(|d| d.get("MH_EGRESS_BUDGET_BPS"))
        .and_then(serde_norway::Value::as_str)
        .unwrap_or_else(|| panic!("{} has no string data.MH_EGRESS_BUDGET_BPS", path.display()));
    parse_positive("MH_EGRESS_BUDGET_BPS", raw)
}

/// (e) The deployed Kind stream ceiling meets the demo requirement.
///
/// Cited by path::fn from the Kind overlay patch comment — keep the name stable.
#[tokio::test]
async fn test_kind_egress_stream_ceiling_meets_demo_requirement() {
    const PATCH: &str =
        "infra/kubernetes/overlays/kind/services/mh-service/configmap-egress-budget-patch.yaml";
    const BASE: &str = "infra/services/mh-service/config.env";

    let cm = fetch_configmap(&shared_configmap());

    // Precondition, with its own message: this sizing requirement is the Kind
    // cluster's. On any other cluster a "ceiling too low" would be the wrong
    // diagnosis, so say what is actually wrong.
    assert_eq!(
        cm.pointer("/metadata/labels/environment")
            .and_then(Value::as_str),
        Some("kind"),
        "ConfigMap {SHARED_CONFIGMAP} does not carry label environment=kind, which \
         the Kind overlay adds. This test sizes the KIND budget and is not \
         meaningful against another cluster - this is not a sizing failure."
    );

    let deployed = parse_positive(
        "MH_EGRESS_BUDGET_BPS",
        &configmap_value(&cm, "MH_EGRESS_BUDGET_BPS"),
    );
    let patch_value = tree_budget(PATCH);
    let base_value = tree_budget(BASE);

    assert_ne!(
        patch_value, base_value,
        "{PATCH} sets MH_EGRESS_BUDGET_BPS equal to the base placeholder in {BASE}: \
         the Kind override is a no-op. Fix the tree."
    );

    // Branch 1: the overlay did not apply. A BUILD-PATH failure, not a sizing
    // one — "ceiling too low" would send a responder to re-size a budget whose
    // override never ran.
    assert_ne!(
        deployed, base_value,
        "deployed MH_EGRESS_BUDGET_BPS is the BASE placeholder: the Kind overlay \
         patch ({PATCH}) did NOT apply. {REDEPLOY_HINT} (never a plain `kubectl \
         apply -k` of an overlay — the bases carry image placeholders only \
         deploy.sh renders); if the base key was renamed, the strategic merge now \
         adds a dead key and the patch must follow the rename. This is a \
         build-path problem, not a budget-sizing one."
    );
    assert_eq!(
        deployed, patch_value,
        "deployed MH_EGRESS_BUDGET_BPS={deployed} is neither the base placeholder \
         nor the Kind patch value ({patch_value}): the live ConfigMap was edited \
         by hand, or the cluster predates the current patch; {REDEPLOY_HINT}."
    );

    // Branch 2: the patch applied; is the value it set big enough? Computed
    // from DEPLOYED values, never a literal.
    //
    // A second encoding of MH's derivation (budget / max(cost_audio,
    // cost_video), floored), and now a CHECKED one:
    // `test_running_mh_publishes_the_deployed_stream_ceiling` compares MH's
    // published `mh_media_egress_stream_ceiling` against the same deployed
    // values, so this formula cannot silently disagree with the enforced one.
    //
    // CONSERVATIVE OVER BOTH CONVERSION ORDERS. The keys are in bits; the MH
    // code task converts to bytes once at load. With integer division,
    // dividing in bits and dividing in bytes can floor to different ceilings
    // whenever a value is not a multiple of 8. Rather than assert a property of
    // today's values in a comment, compute both orders and hold the requirement
    // against the SMALLER — so the check is true whichever order MH uses, and
    // imposes no multiple-of-8 rule MH itself does not impose.
    let audio = parse_positive(
        "MH_STREAM_COST_AUDIO_BPS",
        &configmap_value(&cm, "MH_STREAM_COST_AUDIO_BPS"),
    );
    let video = parse_positive(
        "MH_STREAM_COST_VIDEO_BPS",
        &configmap_value(&cm, "MH_STREAM_COST_VIDEO_BPS"),
    );
    let bits_ceiling = deployed / audio.max(video);
    let byte_cost = (audio / 8).max(video / 8);
    assert!(
        byte_cost > 0,
        "both stream costs (MH_STREAM_COST_AUDIO_BPS={audio}, \
         MH_STREAM_COST_VIDEO_BPS={video}) are below 8 bits/s, so each converts to \
         0 bytes/s and the byte-order ceiling divides by zero. MH would refuse or \
         misbehave on these values; they are not real stream costs."
    );
    let bytes_ceiling = (deployed / 8) / byte_cost;
    let ceiling = bits_ceiling.min(bytes_ceiling);
    let required = required_edges(DEMO_N);

    // Upper side (story 2 task 10): the S9 exhaustion test must still be able
    // to drive a registration PAST this ceiling within its economic
    // participant limit. Checked here, from the live ConfigMaps, in seconds —
    // rather than discovered as a PRECONDITION panic deep in the Layer-7 flows
    // run. The limit and the pigeonhole rule have ONE home, shared with test 28.
    let slot_cap = env_tests::fixtures::kube::configmap_u64(
        &configmap_name_for_all(&["mc-0", "mc-1"], "mc-service-config"),
        "MC_MAX_RECEIVE_SLOTS",
    );
    let feasible = env_tests::fixtures::egress_admission::max_s9_feasible_ceiling(slot_cap);
    let s9_ceiling = bits_ceiling.max(bytes_ceiling);
    assert!(
        s9_ceiling <= feasible,
        "Kind stream ceiling {s9_ceiling} is ABOVE the largest ceiling the S9 exhaustion test \
         (28_mh_egress_admission.rs) can exceed: {feasible}, i.e. P x S > 2 x ceiling with at \
         most {} participants and MC_MAX_RECEIVE_SLOTS={slot_cap}. Both bounds on the Kind \
         budget: ceiling >= {required} (demo) and ceiling <= {feasible} (S9). Fix: LOWER \
         MH_EGRESS_BUDGET_BPS in {PATCH}; raising MAX_S9_PARTICIPANTS instead is a decision \
         about test cost, not a sizing fix.",
        env_tests::fixtures::egress_admission::MAX_S9_PARTICIPANTS
    );

    assert!(
        ceiling >= required,
        "Kind stream ceiling {ceiling} (the smaller of {bits_ceiling} dividing in \
         bits and {bytes_ceiling} dividing in bytes) is BELOW the demo requirement \
         of {required} egress streams (N={DEMO_N}: (N+1) x N). Derived from the deployed \
         MH_EGRESS_BUDGET_BPS={deployed} / max(MH_STREAM_COST_AUDIO_BPS={audio}, \
         MH_STREAM_COST_VIDEO_BPS={video}). Under-sizing makes admission reject \
         partway through the largest scenario, which looks exactly like a routing \
         bug. Fix: RAISE the budget in {PATCH}. Do not lower the costs and do not \
         lower N."
    );
}

/// MH's rule for the ceiling, EXACTLY as `crates/mh-service/src/config.rs`
/// states it: bits converted to bytes ONCE — the budget floored, each cost
/// ceiled (both fail closed) — then budget / max(cost). Written out here so the
/// running pod's published gauge is compared against the documented rule, not
/// against itself.
fn mh_documented_ceiling(budget_bps: u64, audio_bps: u64, video_bps: u64) -> u64 {
    let max_cost_bytes = audio_bps.div_ceil(8).max(video_bps.div_ceil(8));
    assert!(max_cost_bytes > 0, "stream costs must be non-zero");
    (budget_bps / 8) / max_cost_bytes
}

/// The running MH processes publish the stream ceiling the DEPLOYED ConfigMap
/// derives, and the advisory recommended minimum this suite's own demo
/// requirement derives (story 2 task 8; closes the docs/TODO.md §Observability
/// Debt entry on the Kind ceiling test's re-derived formula).
///
/// Three encodings of one requirement meet here and are CHECKED rather than
/// anchored: MH's `EGRESS_STREAM_CEILING_RECOMMENDED_MIN` (published as a
/// gauge), this file's `required_edges(DEMO_N)`, and the Kind budget patch
/// sized against it.
#[tokio::test]
async fn test_running_mh_publishes_the_deployed_stream_ceiling() {
    use env_tests::cluster::ClusterConnection;
    use env_tests::fixtures::metrics::gauge_by_instance_present;
    use env_tests::fixtures::PrometheusClient;

    let cm = fetch_configmap(&shared_configmap());
    let budget = parse_positive(
        "MH_EGRESS_BUDGET_BPS",
        &configmap_value(&cm, "MH_EGRESS_BUDGET_BPS"),
    );
    let audio = parse_positive(
        "MH_STREAM_COST_AUDIO_BPS",
        &configmap_value(&cm, "MH_STREAM_COST_AUDIO_BPS"),
    );
    let video = parse_positive(
        "MH_STREAM_COST_VIDEO_BPS",
        &configmap_value(&cm, "MH_STREAM_COST_VIDEO_BPS"),
    );
    let expected = mh_documented_ceiling(budget, audio, video);

    let cluster = ClusterConnection::new()
        .await
        .expect("Failed to connect to cluster - ensure port-forwards are running");
    let prom = PrometheusClient::new(&cluster.prometheus_base_url);
    let instances = MH_INSTANCES.len();

    let ceilings = gauge_by_instance_present(
        &prom,
        "mh_media_egress_stream_ceiling",
        instances,
        GAUGE_PRESENT_BOUND,
    )
    .await;
    for (instance, value) in &ceilings {
        assert_eq!(
            *value, expected as f64,
            "MH instance {instance} publishes mh_media_egress_stream_ceiling={value}, but \
             the deployed ConfigMap (MH_EGRESS_BUDGET_BPS={budget}, \
             MH_STREAM_COST_AUDIO_BPS={audio}, MH_STREAM_COST_VIDEO_BPS={video}) derives \
             {expected} under MH's documented rule. Either the pod predates the config \
             ({REDEPLOY_HINT}) or MH's derivation diverged from its documentation."
        );
    }

    let mins = gauge_by_instance_present(
        &prom,
        "mh_media_egress_stream_ceiling_recommended_min",
        instances,
        GAUGE_PRESENT_BOUND,
    )
    .await;
    let required = required_edges(DEMO_N);
    for (instance, value) in &mins {
        assert_eq!(
            *value, required as f64,
            "MH instance {instance} publishes \
             mh_media_egress_stream_ceiling_recommended_min={value}, but this suite's demo \
             requirement is required_edges(DEMO_N={DEMO_N})={required}. The two encode ONE \
             requirement (one full all-hear-all meeting at N={DEMO_N}); change both together."
        );
    }
}
