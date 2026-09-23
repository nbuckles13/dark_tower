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
//! REFERENCE. The tests therefore read three live objects and one file, and
//! each assertion proves something different. Keep (a) and (c) adjacent in
//! any rewrite: they look similar and neither covers the other.
//!
//! - **(a) Wiring proves the REFERENCE.** For both `mh-0` and `mh-1`, every key
//!   in [`STORY2_KEYS`] is present on the live pod as a `configMapKeyRef` to
//!   [`SHARED_CONFIGMAP`] whose `key` equals the env var's own name. This is
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
//! - **(c) Startup-log parity proves the READING.** For the four §8 keys the
//!   current image already reads and logs, the value the MH PROCESS reported
//!   in its startup event equals the deployed ConfigMap value. This is the only
//!   assertion here about what the running process observed. It CANNOT prove
//!   wiring on its own: those four values equal their code defaults (so
//!   cutover is behaviour-neutral), and the current image still reads them
//!   optional-with-default — so parity would hold even with a ref missing.
//!   That is why (a) exists.
//! - **(d) Staleness.** A ConfigMap value change does NOT restart pods; a pod
//!   started before the change keeps the old value while (a) and (b) pass. For
//!   keys (c) cannot cover, the container's start time must be at or after the
//!   ConfigMap's last modification.
//! - **(e) Kind sizing.** The deployed budget is the Kind overlay's value (not
//!   the base placeholder), and the stream ceiling derived from DEPLOYED values
//!   meets the demo requirement.
//!
//! ## What this does NOT prove — read before citing it
//!
//! For every key except the four in (c), nothing here shows the MH PROCESS
//! observed the value — only that the deployed artifact carries it, the pod
//! references it, and the pod started after it last changed. The strengthening
//! is the MH code task's published gauges (notably
//! `mh_media_egress_stream_ceiling`): scraping the running pod's own ceiling is
//! the fail-closed "what the pod reports" check, and it retires the ANCHOR
//! (DRY) formula in (e). Tracked in `docs/TODO.md` §Observability Debt.
//!
//! ## The startup log line is a TEST CONTRACT
//!
//! (c) depends on `crates/mh-service/src/main.rs` emitting exactly one
//! JSON event with message [`CONFIG_LOADED_MESSAGE`] carrying the field names
//! in [`LOGGED_POLICY_BOUNDS`]. Renaming that message or those fields reds this
//! test with a message saying so — by design, not as config drift.
//!
//! ## Expected red on a hand-applied value change
//!
//! `infra/kind/scripts/setup.sh` restarts both MH deployments whenever its
//! `apply -k` changed an MH ConfigMap in place (and always on the devloop
//! path, which re-patches the advertise addresses). That NARROWS this window;
//! it does not close it, and (d) is not redundant with it. The script is one
//! deploy path: a hand `kubectl apply -k` against the Kind overlay — which
//! `docs/runbooks/mh-deployment.md` tells operators to run — bypasses it, and
//! the script's detector is fail-open (a missed detection is a silently stale
//! value). (d) is the fail-closed backstop for every path the script does not
//! own. DO NOT delete it on the grounds that setup.sh handles restarts.
//!
//! So a red here after a hand-applied VALUE-only change is correct: the pods
//! really are running the old value. The failure message carries the fix.
//!
//! # Diagnostic output policy (PII)
//!
//! Failure messages may print ONLY: pod name, namespace, label selector, env
//! var **names**, ConfigMap names and key names, the `MH_TERMINATION_GRACE_SECONDS`
//! value, `.spec.terminationGracePeriodSeconds`, the scalar VALUES of the named
//! story-2 keys (none is a credential), timestamps, and counts. Never the
//! parsed `serde_json::Value`, never the env array itself, never a ConfigMap's
//! `data` map, never raw `kubectl` stdout, and NEVER A LOG LINE — not even the
//! one being searched for, and not in a no-match branch. The startup event
//! carries bind and advertise addresses and other configuration; (c) extracts
//! the named numeric fields from it by JSON key and prints only those. It never
//! scrapes a `Debug` render of a config struct, which would inherit
//! `SecretString` redaction as an assumption rather than checking it.
//!
//! This is safe *today* only because MH's sole credential (`MH_CLIENT_SECRET`)
//! arrives via `secretKeyRef`, so the pod object holds a reference rather than
//! a value, and `mh-service-config` holds no secrets — one future
//! literal-valued env var or secret-bearing ConfigMap key would turn a
//! diagnostic dump into a credential leak in CI logs. Extract the scalars
//! first, then format. The ConfigMap and log reads are in scope of this policy.

#![cfg(feature = "smoke")]

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

/// The ConfigMap every story-2 key is read from.
const SHARED_CONFIGMAP: &str = "mh-service-config";

/// Both MH instances. Every per-instance assertion runs over this one list.
const MH_INSTANCES: &[&str] = &["mh-0", "mh-1"];

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

/// The message of MH's single startup configuration event, emitted by
/// `crates/mh-service/src/main.rs`. A TEST CONTRACT — see the module doc.
const CONFIG_LOADED_MESSAGE: &str = "Configuration loaded successfully";

/// `(ConfigMap key, startup-event field)` for every value the running MH
/// process reports. ONE table: the MH code task APPENDS rows here (budget,
/// costs, derived stream ceiling) once MH logs them, rather than writing a
/// second copy. Once the derived ceiling is logged, parity on it becomes the
/// checked version of the ANCHOR (DRY) formula in the Kind sizing test.
const LOGGED_POLICY_BOUNDS: &[(&str, &str)] = &[
    (
        "MH_MAX_EGRESS_STREAMS_PER_MEETING",
        "max_egress_streams_per_meeting",
    ),
    (
        "MH_MAX_CANDIDATE_SOURCES_PER_EGRESS",
        "max_candidate_sources_per_egress",
    ),
    ("MH_MAX_TOTAL_EGRESS_EDGES", "max_total_egress_edges"),
    ("MH_POLICY_APPLY_TIMEOUT_MS", "policy_apply_timeout_ms"),
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
    // `--show-managed-fields` is LOAD-BEARING for the staleness check: modern
    // kubectl strips `metadata.managedFields` from `get` output unless asked, so
    // without it the list is empty and the ConfigMap's last-change time is
    // unknowable. (Found by running the check against a live cluster — it
    // failed on every run until this flag was added.)
    let args = [
        "get",
        "configmap",
        name,
        "-n",
        NAMESPACE,
        "-o",
        "json",
        "--show-managed-fields",
    ];
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

/// Validates the one timestamp shape Kubernetes serializes `metav1.Time` in —
/// RFC 3339, second precision, UTC: `YYYY-MM-DDTHH:MM:SSZ`. Only for that fixed
/// shape is a lexicographic comparison a chronological one, so a timestamp in
/// any other shape is a hard failure, never a guess.
fn assert_k8s_timestamp(what: &str, ts: &str) {
    let b = ts.as_bytes();
    let shape_ok = b.len() == 20
        && b[4] == b'-'
        && b[7] == b'-'
        && b[10] == b'T'
        && b[13] == b':'
        && b[16] == b':'
        && b[19] == b'Z'
        && [0, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15, 17, 18]
            .iter()
            .all(|&i| b[i].is_ascii_digit());
    assert!(
        shape_ok,
        "{what} {ts:?} is not in Kubernetes' RFC 3339 second-precision UTC shape \
         (YYYY-MM-DDTHH:MM:SSZ). The staleness check orders timestamps \
         lexicographically, which is only valid for that exact shape - refusing \
         to compare rather than risk a wrong answer."
    );
}

/// (a) wiring + (b) parse, for one instance.
fn assert_story2_keys_wired(instance: &str) {
    let cm = fetch_configmap(SHARED_CONFIGMAP);
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
            Some(SHARED_CONFIGMAP),
            "pod {pod_name}: env {key} references ConfigMap {ref_name:?}, expected \
             {SHARED_CONFIGMAP:?}."
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
    // at least `MH_MAX_EGRESS_STREAMS_PER_MEETING`: a muted source must be a
    // sender, and every sender is also a subscriber holding at least one egress
    // stream, so the egress bound is the ceiling on how many muted senders a
    // legitimate snapshot can name.
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

/// (d) staleness: the container started at or after the ConfigMap last changed.
fn assert_pod_started_after_configmap_change(instance: &str) {
    let cm = fetch_configmap(SHARED_CONFIGMAP);
    let pod = fetch_pod(instance);
    let pod_name = pod_name_of(&pod, instance);

    let field_times: Vec<&str> = cm
        .pointer("/metadata/managedFields")
        .and_then(Value::as_array)
        .unwrap_or_else(|| {
            panic!(
                "ConfigMap {SHARED_CONFIGMAP} has no metadata.managedFields - cannot \
                 establish when it last changed. Unknown is NOT treated as fresh."
            )
        })
        .iter()
        .map(|f| {
            f.get("time").and_then(Value::as_str).unwrap_or_else(|| {
                panic!(
                    "a managedFields entry on {SHARED_CONFIGMAP} has no readable \
                     `time`. Unknown is NOT treated as fresh."
                )
            })
        })
        .collect();
    assert!(
        !field_times.is_empty(),
        "ConfigMap {SHARED_CONFIGMAP} has an EMPTY managedFields list - cannot \
         establish when it last changed. Unknown is NOT treated as fresh. If this \
         fires on every run, check that fetch_configmap still passes \
         --show-managed-fields (kubectl strips the list by default)."
    );
    for t in &field_times {
        assert_k8s_timestamp("ConfigMap managedFields time", t);
    }
    let cm_changed = field_times
        .iter()
        .max()
        .expect("field_times is non-empty, asserted above");

    let started = pod
        .pointer("/status/containerStatuses")
        .and_then(Value::as_array)
        .and_then(|s| {
            s.iter()
                .find(|c| c.get("name").and_then(Value::as_str) == Some(CONTAINER))
        })
        .and_then(|c| c.pointer("/state/running/startedAt"))
        .and_then(Value::as_str)
        .unwrap_or_else(|| {
            panic!(
                "pod {pod_name}: container {CONTAINER} has no state.running.startedAt \
                 - it is not running, so no value is in effect to compare. Check \
                 `kubectl describe pod {pod_name} -n {NAMESPACE}`."
            )
        });
    assert_k8s_timestamp("container startedAt", started);

    // `>=`, NOT `>`: both timestamps have one-second resolution, so a pod
    // started in the same second the ConfigMap was written is fresh. Tightening
    // this to `>` adds a same-second flake and catches nothing real.
    assert!(
        started >= *cm_changed,
        "pod {pod_name}: container {CONTAINER} started at {started}, BEFORE \
         ConfigMap {SHARED_CONFIGMAP} last changed at {cm_changed}. A ConfigMap \
         change does not restart pods, so this pod is running the OLD values while \
         every manifest check passes. Fix: \
         kubectl rollout restart deployment/mh-0 deployment/mh-1 -n {NAMESPACE}"
    );
}

/// (c) startup-log parity: the running process's reported §8 bounds equal the
/// deployed ConfigMap values. Reads the log of the SAME pod the other checks
/// read (by name, current container only — never `-l`, never `--previous`).
fn assert_policy_bounds_match_startup_log(instance: &str) {
    let cm = fetch_configmap(SHARED_CONFIGMAP);
    let pod = fetch_pod(instance);
    let pod_name = pod_name_of(&pod, instance);

    let args = ["logs", pod_name.as_str(), "-n", NAMESPACE, "-c", CONTAINER];
    let output = Command::new("kubectl")
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("kubectl not available - cannot read {pod_name} logs: {e}"));
    assert!(
        output.status.success(),
        "`kubectl {}` failed (exit status {}): {}",
        args.join(" "),
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    // Parse every line; keep ONLY the matching events, never a line. Lines that
    // are not JSON (a panic message, say) are counted, not printed.
    //
    // Tolerating an individual non-JSON line cannot make this pass vacuously:
    // passing REQUIRES exactly one parsed JSON config event whose four fields
    // equal the deployed values (asserted below). A stream that is entirely
    // non-JSON gets its own branch; a config event rendered in some non-JSON
    // shape is simply absent, and absence fails loudly.
    let stdout = String::from_utf8_lossy(&output.stdout);
    let non_empty: Vec<&str> = stdout.lines().filter(|l| !l.trim().is_empty()).collect();
    let parsed: Vec<Value> = non_empty
        .iter()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .collect();
    let json_lines = parsed.len();
    let other_lines = non_empty.len() - json_lines;
    let events: Vec<Value> = parsed
        .into_iter()
        .filter(|v| {
            v.pointer("/fields/message").and_then(Value::as_str) == Some(CONFIG_LOADED_MESSAGE)
        })
        .collect();

    // Branch: the stream is not JSON at all. Distinct from "event missing" so
    // an empty or garbage stream is not triaged as log rotation.
    assert!(
        json_lines > 0,
        "pod {pod_name}: container {CONTAINER}'s log has NO JSON lines \
         ({other_lines} non-empty non-JSON lines). MH logs JSON \
         (tracing_subscriber `fmt::layer().json()` in crates/mh-service/src/main.rs); \
         either the logger changed shape or the stream is empty. This is not a \
         config-value mismatch."
    );

    // Branch: zero matching events.
    assert!(
        !events.is_empty(),
        "pod {pod_name}: no {CONFIG_LOADED_MESSAGE:?} event in the current \
         container's log ({json_lines} JSON lines read). Most likely the kubelet \
         rotated the container log and `kubectl logs` returns only the current \
         file, so on a long-lived pod the startup line is gone. Fix: \
         kubectl rollout restart deployment/{instance} -n {NAMESPACE} and re-run. \
         Do NOT relax this test. If the pod is fresh, the startup event was \
         renamed in crates/mh-service/src/main.rs - it is a test contract."
    );

    // Branch: more than one — impossible in one container's life; taking the
    // first or last would hide whatever made it happen.
    assert_eq!(
        events.len(),
        1,
        "pod {pod_name}: {} {CONFIG_LOADED_MESSAGE:?} events in ONE container's \
         log, expected exactly one. Something re-ran MH's startup path; do not \
         pick one.",
        events.len()
    );
    let event = &events[0];

    for &(key, field) in LOGGED_POLICY_BOUNDS {
        // Branch: the field is absent or not an unsigned integer.
        let logged = event
            .pointer(&format!("/fields/{field}"))
            .and_then(Value::as_u64)
            .unwrap_or_else(|| {
                panic!(
                    "pod {pod_name}: the {CONFIG_LOADED_MESSAGE:?} event has no \
                     unsigned-integer field {field:?}. The logger shape or the field \
                     name drifted in crates/mh-service/src/main.rs - that event is a \
                     test contract (update LOGGED_POLICY_BOUNDS with the rename)."
                )
            });
        let deployed = parse_positive(key, &configmap_value(&cm, key));

        // Branch: both present, values disagree.
        assert_eq!(
            logged, deployed,
            "pod {pod_name}: the MH process reported {field}={logged} at startup, \
             but ConfigMap {SHARED_CONFIGMAP} carries {key}={deployed}. The process \
             is not running the deployed value: the pod predates the ConfigMap \
             change, or the key is not wired into this workload."
        );
    }
}

#[tokio::test]
async fn test_story2_keys_wired_into_every_mh_instance() {
    for instance in MH_INSTANCES {
        assert_story2_keys_wired(instance);
    }
}

#[tokio::test]
async fn test_mh_pods_started_after_configmap_last_changed() {
    for instance in MH_INSTANCES {
        assert_pod_started_after_configmap_change(instance);
    }
}

#[tokio::test]
async fn test_policy_bounds_reported_by_running_mh_match_configmap() {
    for instance in MH_INSTANCES {
        assert_policy_bounds_match_startup_log(instance);
    }
}

/// Reads `data.MH_EGRESS_BUDGET_BPS` from a ConfigMap manifest in the tree.
fn tree_budget(rel_path: &str) -> u64 {
    let path = repo_root().join(rel_path);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
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
    const BASE: &str = "infra/services/mh-service/configmap.yaml";

    let cm = fetch_configmap(SHARED_CONFIGMAP);

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
         patch ({PATCH}) did NOT apply. Deploy through the Kind overlay \
         (`kubectl apply -k infra/kubernetes/overlays/kind/services/mh-service/`); \
         if the base key was renamed, the strategic merge now adds a dead key and \
         the patch must follow the rename. This is a build-path problem, not a \
         budget-sizing one."
    );
    assert_eq!(
        deployed, patch_value,
        "deployed MH_EGRESS_BUDGET_BPS={deployed} is neither the base placeholder \
         nor the Kind patch value ({patch_value}): the live ConfigMap was edited \
         by hand, or the cluster predates the current patch. Redeploy through the \
         Kind overlay."
    );

    // Branch 2: the patch applied; is the value it set big enough? Computed
    // from DEPLOYED values, never a literal.
    //
    // ANCHOR (DRY): a deliberate, temporary second encoding of the MH code
    // task's derivation (budget / max(cost_audio, cost_video), floored). It is
    // checked only once that task's env-test compares MH's published
    // `mh_media_egress_stream_ceiling` against this — tracked in docs/TODO.md
    // §Observability Debt.
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
