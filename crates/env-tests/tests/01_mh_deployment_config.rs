//! P0 Smoke Tests: MH Deployment Configuration
//!
//! Two families of assertion against the LIVE cluster, never a local render:
//!
//! 1. Story 1 Test-Plan item F (validates R-21): the deployed
//!    `MH_TERMINATION_GRACE_SECONDS` equals the pod spec's
//!    `terminationGracePeriodSeconds`. (Sections below through "Why
//!    `-n dark-tower`".)
//! 2. Story 2/3 configuration surface (story 2 R-19, R-22, R-23, R-24; story 3
//!    R-21, R-23): MH's egress-budget chain, §8 policy keys, story-3 stream
//!    keys and shared media-codec-profile keys are wired into BOTH
//!    instances, carry parseable values that satisfy the relationships MH
//!    boot-checks, are what the running process actually loaded (where MH
//!    publishes them), and — on Kind — size a stream ceiling inside the Kind
//!    window and run the committed Kind test codec profile. Retiring keys are
//!    optional refs to keys still present. (Section "Story 2 configuration
//!    surface".) The MC twin is `02_mc_deployment_config.rs`.
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
//!   in [`WIRED_KEYS`] is present on the live pod as a HARD `configMapKeyRef` to
//!   the ONE generation of ITS base (`mh-service-config` or, for the story-3
//!   profile keys, `media-codec-profile`) that pod is wired to, whose `key`
//!   equals the env var's own name; every key in [`RETIRING_KEYS`] is present
//!   as an `optional: true` ref AND still present and parseable in the live
//!   generation (the current image still reads it); and
//!   `MH_POD_MEMORY_LIMIT_BYTES` is a downward-API `resourceFieldRef` to the
//!   container's DECLARED `limits.memory`. The checks are the pure cores in
//!   `env_tests::fixtures::deployment_config` (unit-tested at Layer 4). This is
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
//!   the base placeholder), and the stream ceiling each live MH pod PUBLISHES
//!   (`mh_media_egress_stream_ceiling`, the value admission enforces) lies
//!   inside the two-sided window whose constants live in
//!   `env_tests::fixtures::egress_admission` — at least the largest load one
//!   handler must carry plus the retained-edge floor, at most what the S9
//!   exhaustion test can exceed.
//! - **(f) Kind test codec profile.** The live `media-codec-profile`
//!   generation MC and MH resolve (one generation across all four pods) holds
//!   exactly the base keys, with every value the committed Kind test-profile
//!   patch sets (story 3 R-28).
//!
//! ## What this does NOT prove — read before citing it
//!
//! `MH_POD_MEMORY_LIMIT_BYTES` is asserted STRUCTURALLY only (the
//! `resourceFieldRef` shape, divisor 1, and the target container declaring
//! `limits.memory`). Kubernetes then resolves exactly that limit, and
//! `dt-guard kustomize`'s `resource_field_ref_undeclared` closes the
//! fail-open case in the manifests. The value the running PROCESS read is not
//! observable here: the image is distroless (no exec), and the current image
//! neither reads nor publishes it. The gauge that would close that gap is
//! tracked in `docs/TODO.md` §Observability Debt ("MH_POD_MEMORY_LIMIT_BYTES is
//! not published"; owner media-handler, lands with MH-A2).
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
//!
//! Edge-sum (concurrent Σ P×(N+M)): 0

#![cfg(feature = "smoke")]

use env_tests::fixtures::deployment_config::{
    check_key_ref, check_resource_field_ref, check_shape, cm_value, container, fetch_configmap,
    fetch_live_pod, kind_patch_overrides, live_pod, parse_positive, pod_name_of,
    tree_configmap_data, PatchOverride, RefKind, ValueShape, KIND_SIZING_RESTORE,
    KIND_TEST_PROFILE_PATCH, PROFILE_BASE_ENV,
};
use env_tests::fixtures::egress_admission::{
    kind_window_lower_bound, max_s9_feasible_ceiling, required_edges, DEMO_M, DEMO_N, DEMO_P,
    MAX_PER_FILE_EDGE_SUM, MAX_S9_PARTICIPANTS, RETAINED_EDGE_FLOOR,
};
use env_tests::fixtures::kube::{
    configmap_name_for, configmap_name_for_all, MC_INSTANCES, MH_INSTANCES, PROFILE_CONFIGMAP,
    PROFILE_READERS, REDEPLOY_HINT,
};
use env_tests::fixtures::mh_config::{gauge_verdict, Conversion, GaugeVerdict};
use env_tests::NAMESPACE;
use serde_json::Value;

const CONTAINER: &str = "mh-service";
const GRACE_ENV: &str = "MH_TERMINATION_GRACE_SECONDS";

/// Asserts one instance's deployed env value equals its own pod spec's grace.
fn assert_grace_matches_pod_spec(instance: &str) {
    let pod = fetch_live_pod(instance);

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

/// Every key wired into BOTH MH workloads as a HARD ref: `(key, generator
/// base, shape)`. ONE table, iterated for every instance — never a second copy
/// per instance or per test. The base column is needed because the profile
/// keys come from a DIFFERENT ConfigMap (resolved per instance and base).
const WIRED_KEYS: &[(&str, &str, ValueShape)] = &[
    // Egress-budget chain (story 2 R-19, R-23).
    (
        "MH_EGRESS_BUDGET_BPS",
        SHARED_CONFIGMAP,
        ValueShape::PositiveInteger,
    ),
    (
        "MH_EGRESS_REJECTION_RATIO_THRESHOLD",
        SHARED_CONFIGMAP,
        ValueShape::UnitRatio,
    ),
    (
        "MH_MAX_REGISTERED_MEETINGS",
        SHARED_CONFIGMAP,
        ValueShape::PositiveInteger,
    ),
    // ADR-0036 §8 policy bounds (story 2 R-22).
    (
        "MH_MAX_EGRESS_STREAMS_PER_MEETING",
        SHARED_CONFIGMAP,
        ValueShape::PositiveInteger,
    ),
    (
        "MH_MAX_CANDIDATE_SOURCES_PER_EGRESS",
        SHARED_CONFIGMAP,
        ValueShape::PositiveInteger,
    ),
    (
        "MH_MAX_TOTAL_EGRESS_EDGES",
        SHARED_CONFIGMAP,
        ValueShape::PositiveInteger,
    ),
    (
        "MH_POLICY_APPLY_TIMEOUT_MS",
        SHARED_CONFIGMAP,
        ValueShape::PositiveInteger,
    ),
    // Story 3 MH keys (required reads from MH-A1 / MH-A2).
    (
        "MH_MAX_SOURCE_STREAMS_PER_MEETING",
        SHARED_CONFIGMAP,
        ValueShape::PositiveInteger,
    ),
    (
        "MH_STREAM_OPENS_PER_CONNECTION_PER_SEC",
        SHARED_CONFIGMAP,
        ValueShape::PositiveInteger,
    ),
    (
        "MH_EGRESS_STREAM_BACKLOG_BYTES",
        SHARED_CONFIGMAP,
        ValueShape::PositiveInteger,
    ),
    (
        "MH_STREAM_MID_FRAME_STALL_MS",
        SHARED_CONFIGMAP,
        ValueShape::PositiveInteger,
    ),
    // Story 3 shared media codec profile: the four keys MH reads.
    (
        "MEDIA_VIDEO_MAX_BITRATE_BPS",
        PROFILE_CONFIGMAP,
        ValueShape::PositiveInteger,
    ),
    (
        "MEDIA_VIDEO_FRAME_RATE_HZ",
        PROFILE_CONFIGMAP,
        ValueShape::PositiveInteger,
    ),
    (
        "MEDIA_AUDIO_MAX_BITRATE_BPS",
        PROFILE_CONFIGMAP,
        ValueShape::PositiveInteger,
    ),
    (
        "MEDIA_AUDIO_FRAME_RATE_HZ",
        PROFILE_CONFIGMAP,
        ValueShape::PositiveInteger,
    ),
];

/// RETIRING MH keys (two-deploy retirement, step 1 — docs/TODO.md §Media Path
/// Obligations, "Delete the story-3 retired config keys"). Each ref must be
/// present on BOTH instances and `optional: true`, AND the key must STILL be
/// present and parseable in the live generation: the current image reads none
/// of them, but the PREVIOUS image (the rollback target until step 2) reads all
/// but `MH_MAX_STREAMS` as REQUIRED, and under an optional ref an absent key is
/// a silently unset variable that image refuses to boot on — the optional flag
/// would hide exactly the failure the retirement must not cause.
const RETIRING_KEYS: &[(&str, ValueShape)] = &[
    ("MH_STREAM_COST_AUDIO_BPS", ValueShape::PositiveInteger),
    ("MH_STREAM_COST_VIDEO_BPS", ValueShape::PositiveInteger),
    (
        "MH_MAX_MUTED_SOURCES_PER_MEETING",
        ValueShape::PositiveInteger,
    ),
    ("MH_MAX_STREAMS", ValueShape::PositiveInteger),
];

/// The downward-API memory limit (story 3 R-21): env name, target container,
/// resource.
const MEMORY_LIMIT_ENV: (&str, &str, &str) =
    ("MH_POD_MEMORY_LIMIT_BYTES", CONTAINER, "limits.memory");

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
        "MH_MAX_SOURCE_STREAMS_PER_MEETING",
        "mh_media_source_streams_per_meeting_limit",
        Conversion::Identity,
    ),
    (
        "MH_EGRESS_BUDGET_BPS",
        "mh_media_egress_budget_bytes_per_second",
        Conversion::BitsToBytesFloor,
    ),
];

// The demo requirement (`DEMO_P/N/M`, `required_edges`) and every other
// constant of the Kind window live in ONE place,
// `env_tests::fixtures::egress_admission`, shared with 28 (the S9 side).

// -----------------------------------------------------------------------------
// The independent per-stream cost oracle (story 3 R-24)
// -----------------------------------------------------------------------------
//
// MH derives each egress cost as MAX_BITRATE_BPS + per-frame overhead bits x
// FRAME_RATE_HZ from the shared media-codec-profile. This suite cannot link
// mh-service, so it derives the same costs INDEPENDENTLY from `media-protocol`
// size constants and the LIVE profile, and compares MH's published gauges
// against them.
//
// ANCHOR (DRY): the region set below mirrors
// `crates/mh-service/src/config.rs::AUDIO_FRAME_OVERHEAD_BYTES` (and its
// `NOMINAL_HEADER_BYTES`). Both sides carry the SAME pin — the header sum
// equals `MAX_HEADER_BYTES - MAX_EXT_BYTES` — so a header region added to the
// layout fails to compile in both places instead of letting one drift.

/// Non-extension header bytes of an audio frame: publisher prefix, wrapped
/// transmit key, extension-length field, relay region.
const ORACLE_AUDIO_HEADER_BYTES: usize = media_protocol::frame::PUBLISHER_FIXED_PREFIX_SIZE
    + media_protocol::frame::WRAPPED_TRANSMIT_KEY_SIZE
    + media_protocol::frame::EXT_LENGTH_FIELD_SIZE
    + media_protocol::frame::RELAY_REGION_SIZE;

const _: () = assert!(
    ORACLE_AUDIO_HEADER_BYTES
        == media_protocol::frame::MAX_HEADER_BYTES - media_protocol::frame::MAX_EXT_BYTES,
    "the frame header gained or lost a region: the cost oracle must track MAX_HEADER_BYTES"
);

/// Non-payload bytes per audio frame (the wrapped key on every frame).
const ORACLE_AUDIO_OVERHEAD_BYTES: u64 = (ORACLE_AUDIO_HEADER_BYTES
    + media_protocol::frame::SFRAME_OBJECT_OVERHEAD_BYTES
    + media_protocol::frame::SIGNATURE_SIZE) as u64;

/// Non-payload bytes per video frame: the wrapped key amortises to zero.
const ORACLE_VIDEO_OVERHEAD_BYTES: u64 =
    ORACLE_AUDIO_OVERHEAD_BYTES - media_protocol::frame::WRAPPED_TRANSMIT_KEY_SIZE as u64;

/// `(audio_bps, video_bps)` derived from the live profile ConfigMap.
fn oracle_costs_bps(profile_cm: &Value) -> (u64, u64) {
    let p = |k| positive(profile_cm, PROFILE_CONFIGMAP, k);
    let audio = p("MEDIA_AUDIO_MAX_BITRATE_BPS")
        + ORACLE_AUDIO_OVERHEAD_BYTES * 8 * p("MEDIA_AUDIO_FRAME_RATE_HZ");
    let video = p("MEDIA_VIDEO_MAX_BITRATE_BPS")
        + ORACLE_VIDEO_OVERHEAD_BYTES * 8 * p("MEDIA_VIDEO_FRAME_RATE_HZ");
    (audio, video)
}

/// The live profile generation BOTH MH instances reference.
fn profile_configmap() -> String {
    configmap_name_for_all(MH_INSTANCES, PROFILE_CONFIGMAP)
}

/// A strictly-positive deployed integer, or a panic naming the key and base.
fn positive(cm: &Value, base: &str, key: &str) -> u64 {
    parse_positive(key, &cm_value(cm, base, key), base).unwrap_or_else(|e| panic!("{e}"))
}

/// (a) wiring + (b) parse, for one instance, plus the relationships between
/// deployed values that a reading image refuses to boot on.
fn assert_story2_keys_wired(instance: &str) {
    // The generation THIS instance's pod references, per base — so the values
    // checked below are the config in effect for this pod.
    let mh_cm_name = shared_configmap_for(instance);
    let profile_cm_name = configmap_name_for(instance, PROFILE_CONFIGMAP);
    let mh_cm = fetch_configmap(&mh_cm_name);
    let profile_cm = fetch_configmap(&profile_cm_name);
    let live = |base: &str| -> (&str, &Value) {
        if base == PROFILE_CONFIGMAP {
            (profile_cm_name.as_str(), &profile_cm)
        } else {
            (mh_cm_name.as_str(), &mh_cm)
        }
    };
    let pod = fetch_live_pod(instance);

    // Positive control that CAN fail: the table wires every base this file
    // covers (a table that quietly lost the profile rows would otherwise pass
    // on what remains), and the retiring list is not empty.
    for base in [SHARED_CONFIGMAP, PROFILE_CONFIGMAP] {
        assert!(
            WIRED_KEYS.iter().any(|(_, b, _)| *b == base),
            "WIRED_KEYS has no row for base {base}"
        );
    }
    assert!(!RETIRING_KEYS.is_empty());
    for &(key, base, shape) in WIRED_KEYS {
        let (cm_name, cm) = live(base);
        check_key_ref(&pod, CONTAINER, key, cm_name, RefKind::Hard)
            .unwrap_or_else(|e| panic!("{e}"));
        check_shape(key, &cm_value(cm, base, key), shape, base).unwrap_or_else(|e| panic!("{e}"));
    }
    for &(key, shape) in RETIRING_KEYS {
        check_key_ref(&pod, CONTAINER, key, &mh_cm_name, RefKind::Optional)
            .unwrap_or_else(|e| panic!("{e}"));
        // STILL present and parseable: the current image still reads it.
        check_shape(
            key,
            &cm_value(&mh_cm, SHARED_CONFIGMAP, key),
            shape,
            SHARED_CONFIGMAP,
        )
        .unwrap_or_else(|e| panic!("{e}"));
    }

    let (env, target, resource) = MEMORY_LIMIT_ENV;
    check_resource_field_ref(&pod, CONTAINER, env, target, resource)
        .unwrap_or_else(|e| panic!("{e}"));

    assert_removed_keys_absent(&pod, &mh_cm);
    assert_deployed_relationships(&mh_cm);
}

/// Keys story 3 amendment 4 removed from MH (R-23): MH's three keyframe /
/// recovery keys (keyframe and recovery requests are event-driven, with no
/// retry, aggregation window or rate limit), and MH's ref to the minimum
/// keyframe interval (MC-only config; MH has no use for it). No image ever
/// read the three, so they are deleted outright, not retired.
///
/// Asserted ABSENT because `dt-guard env-config` cannot catch a
/// reintroduction: it checks that every key the Rust code reads is declared,
/// not that every declared key is read — re-adding a key to config.env plus
/// both refs passes it cleanly. This also catches a cluster still running the
/// pre-amendment manifests.
const REMOVED_KEYS: &[&str] = &[
    "MH_KEYFRAME_REQUEST_RETRY_MS",
    "MH_KEYFRAME_REQUEST_AGGREGATION_MS",
    "MH_RECOVERY_REQUESTS_PER_CONNECTION_PER_SEC",
    "MEDIA_VIDEO_MIN_KEYFRAME_INTERVAL_MS",
];

/// None of [`REMOVED_KEYS`] is an env entry of THIS pod's MH container, and
/// the three MH keys are absent from its live `mh-service-config` generation.
/// The minimum interval's absence from the shared profile generation (it
/// moved to MC's own ConfigMap as `MC_VIDEO_MIN_KEYFRAME_INTERVAL_MS`, story 3
/// MC-A) has ONE home: `02_mc_deployment_config.rs`, beside that key's new
/// wiring, so the move and its absence are asserted together.
///
/// Reads the SAME parsed pod and ConfigMap the positive wiring checks above
/// just used, so the absence cannot be vacuous: an empty or unread env list is
/// its own loud failure here.
fn assert_removed_keys_absent(pod: &Value, mh_cm: &Value) {
    let pod_name = pod_name_of(pod, CONTAINER);
    let env: Vec<&str> = container(pod, &pod_name, CONTAINER)
        .unwrap_or_else(|e| panic!("{e}"))
        .get("env")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|e| e.get("name").and_then(Value::as_str))
                .collect()
        })
        .unwrap_or_default();
    // Positive control: the list read is the one the wiring checks passed on.
    assert!(
        WIRED_KEYS.iter().all(|(key, _, _)| env.contains(key)),
        "pod {pod_name}: the env list read for the absence check does not hold every wired key \
         — the absence check below would be vacuous"
    );
    for key in REMOVED_KEYS {
        assert!(
            !env.contains(key),
            "pod {pod_name}: env {key} is present, but story 3 amendment 4 removed it from MH \
             (R-23). Remove the ref from infra/services/mh-service/mh-0-deployment.yaml and \
             mh-1-deployment.yaml (and the key from config.env, if it is an MH key)."
        );
    }
    for key in REMOVED_KEYS.iter().filter(|key| key.starts_with("MH_")) {
        assert!(
            mh_cm.pointer("/data").and_then(|d| d.get(*key)).is_none(),
            "live ConfigMap {SHARED_CONFIGMAP} still holds {key}, removed by story 3 amendment 4 \
             (R-23). Remove it from infra/services/mh-service/config.env."
        );
    }
}

/// Relationships between DEPLOYED values that a reading image refuses to boot
/// on — asserted here so the cause is named in seconds, before that image
/// ships (the point of landing manifests first). Each message names both keys
/// and their homes.
fn assert_deployed_relationships(mh_cm: &Value) {
    let mh = |k| positive(mh_cm, SHARED_CONFIGMAP, k);
    let egress = mh("MH_MAX_EGRESS_STREAMS_PER_MEETING");
    let sources = mh("MH_MAX_SOURCE_STREAMS_PER_MEETING");
    assert!(
        sources >= egress,
        "MH_MAX_SOURCE_STREAMS_PER_MEETING={sources} is BELOW \
         MH_MAX_EGRESS_STREAMS_PER_MEETING={egress} (both {SHARED_CONFIGMAP}). MH refuses to boot \
         on this (the floor rule lives in crates/mh-service/src/config.rs). Note \
         this floor is MH's boot rule, NOT a guarantee that a legitimate registration fits — \
         the source-stream bound is sized against publishers (see its config.env comment)."
    );
    // Kept until the retirement's step 2: the current image no longer reads
    // the muted bound, but the PREVIOUS image (the rollback target) still
    // refuses to boot on this inversion.
    let muted = mh("MH_MAX_MUTED_SOURCES_PER_MEETING");
    assert!(
        muted >= egress,
        "MH_MAX_MUTED_SOURCES_PER_MEETING={muted} is BELOW \
         MH_MAX_EGRESS_STREAMS_PER_MEETING={egress} (both {SHARED_CONFIGMAP}). The current MH \
         image ignores the retired key, but the PREVIOUS image (the rollback target until the \
         retirement's step 2) refuses to boot on this inversion. Fix: raise \
         MH_MAX_MUTED_SOURCES_PER_MEETING to at least the egress bound, or lower the egress \
         bound."
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
    use env_tests::fixtures::metrics::gauge_on_live_pod;
    use env_tests::fixtures::PrometheusClient;

    let cluster = ClusterConnection::new()
        .await
        .expect("Failed to connect to cluster - ensure port-forwards are running");
    let prom = PrometheusClient::new(&cluster.prometheus_base_url);

    // Per instance: its own generation and its own live pod. Every read is
    // pinned to that pod (`gauge_on_live_pod`): right after a deploy rolls MH,
    // the terminating predecessor is still a scrape target for its drain
    // window, so "enough instances have the series" can be satisfied before
    // the NEW pod's first scrape. Its PRESENCE panic is the no-data branch.
    let pods: Vec<_> = MH_INSTANCES
        .iter()
        .map(|&instance| {
            let cm = fetch_configmap(&shared_configmap_for(instance));
            (live_pod(instance), cm)
        })
        .collect();

    for &(key, gauge, conversion) in CONFIG_GAUGES {
        for (pod, cm) in &pods {
            let (instance, pod_name, pod_ip) = (&pod.instance, &pod.name, &pod.ip);
            let deployed = positive(cm, SHARED_CONFIGMAP, key);
            let value = gauge_on_live_pod(&prom, gauge, pod, GAUGE_PRESENT_BOUND).await;
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

    let deployed = positive(&cm, SHARED_CONFIGMAP, "MH_EGRESS_BUDGET_BPS");
    // The ONE patch-vs-base rule (a dead key or a no-op override fails, naming
    // the patch) — the same one the codec-profile patch is held to.
    let overrides = kind_patch_overrides(PATCH, BASE).unwrap_or_else(|e| panic!("{e}"));
    let budget_of = |raw: &str, home: &str| {
        parse_positive("MH_EGRESS_BUDGET_BPS", raw, home).unwrap_or_else(|e| panic!("{e}"))
    };
    let (patch_value, base_value) = match overrides.as_slice() {
        [PatchOverride { key, patch, base }] if key == "MH_EGRESS_BUDGET_BPS" => {
            (budget_of(patch, PATCH), budget_of(base, BASE))
        }
        _ => panic!(
            "{PATCH} must override exactly MH_EGRESS_BUDGET_BPS and nothing else (it overrides \
             {:?})",
            overrides.iter().map(|o| &o.key).collect::<Vec<_>>()
        ),
    };

    // Branch 1: the overlay did not apply. A BUILD-PATH failure, not a sizing
    // one — "ceiling too low" would send a responder to re-size a budget whose
    // override never ran.
    assert_ne!(
        deployed, base_value,
        "OVERLAY-NOT-APPLIED: deployed MH_EGRESS_BUDGET_BPS is the BASE placeholder: the Kind \
         overlay patch ({PATCH}) did NOT apply. {REDEPLOY_HINT} (never a plain `kubectl \
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

    // Branch 2: the patch applied; is the value it set inside the window? Read
    // from the gauge MH publishes — the ceiling admission ENFORCES and GC is
    // told — on EACH live pod, never re-derived here: that the gauge equals the
    // documented rule over the deployed values is
    // `test_running_mh_publishes_the_deployed_stream_ceiling`'s job.
    use env_tests::cluster::ClusterConnection;
    use env_tests::fixtures::metrics::gauge_by_live_pod;
    use env_tests::fixtures::PrometheusClient;
    let cluster = ClusterConnection::new()
        .await
        .expect("Failed to connect to cluster - ensure port-forwards are running");
    let prom = PrometheusClient::new(&cluster.prometheus_base_url);
    let ceilings = gauge_by_live_pod(
        &prom,
        "mh_media_egress_stream_ceiling",
        MH_INSTANCES,
        GAUGE_PRESENT_BOUND,
    )
    .await;

    let lower = kind_window_lower_bound();
    let demo = required_edges(DEMO_P, DEMO_N, DEMO_M);
    // S9 drives AUDIO slots only, so the per-kind AUDIO cap bounds it.
    let slot_cap = env_tests::fixtures::kube::configmap_u64(
        &configmap_name_for_all(MC_INSTANCES, "mc-service-config"),
        "MC_MAX_AUDIO_RECEIVE_SLOTS",
    );
    let upper = max_s9_feasible_ceiling(slot_cap);
    // ONE restore for both halves of the unit (profile and budget roll back
    // together): after a production-quality demo the likelier cause is an
    // unrestored PROFILE, which a budget re-size would only hide.
    let restore = format!(
        "If the Kind test profile or the budget was edited (e.g. for a production-quality \
         demo), {KIND_SIZING_RESTORE}. If \
         test_kind_media_codec_profile_is_the_committed_test_profile also fails, the profile is \
         the cause — restore it, do not re-size the budget"
    );
    let window = format!(
        "window [{lower}, {upper}] per handler: lower = max(demo P x (N + M) = {demo}, \
         MAX_PER_FILE_EDGE_SUM = {MAX_PER_FILE_EDGE_SUM}) + RETAINED_EDGE_FLOOR = \
         {RETAINED_EDGE_FLOOR}; upper = max_s9_feasible_ceiling(MC_MAX_AUDIO_RECEIVE_SLOTS = \
         {slot_cap}) with MAX_S9_PARTICIPANTS = {MAX_S9_PARTICIPANTS} (all in \
         crates/env-tests/src/fixtures/egress_admission.rs)"
    );
    #[expect(
        clippy::cast_precision_loss,
        reason = "window bounds are stream counts, far below 2^53"
    )]
    let (lower_f, upper_f) = (lower as f64, upper as f64);
    let mut sorted: Vec<_> = ceilings.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(b.0));
    for (instance, &value) in sorted {
        assert!(
            value.is_finite() && value >= 0.0 && value.fract() == 0.0,
            "MH instance {instance} publishes mh_media_egress_stream_ceiling={value}, which is \
             not a stream count: the publisher's shape changed \
             (crates/mh-service/src/observability/metrics.rs). Not a sizing failure."
        );
        assert!(
            value >= lower_f,
            "WINDOW-LOWER: MH instance {instance} publishes mh_media_egress_stream_ceiling={value}, \
             BELOW the Kind {window}. Under-sizing makes admission reject partway through the \
             largest scenario, which looks exactly like a routing bug (\"some receivers miss \
             some senders\"). {restore}; otherwise RAISE MH_EGRESS_BUDGET_BPS in {PATCH}. Do not \
             shrink the retained floor, shorten the grace period or serialise env-tests."
        );
        assert!(
            value <= upper_f,
            "WINDOW-UPPER: MH instance {instance} publishes mh_media_egress_stream_ceiling={value}, \
             ABOVE the Kind {window}: the S9 exhaustion test (28_mh_egress_admission.rs) could not \
             drive a registration past it. {restore}; otherwise LOWER MH_EGRESS_BUDGET_BPS in \
             {PATCH}. Raising MAX_S9_PARTICIPANTS instead is a decision about test cost, not a \
             sizing fix."
        );
    }
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
/// gauge), this file's `required_edges(DEMO_P, DEMO_N, DEMO_M)`, and the Kind budget patch
/// sized against it.
#[tokio::test]
async fn test_running_mh_publishes_the_deployed_stream_ceiling() {
    use env_tests::cluster::ClusterConnection;
    use env_tests::fixtures::metrics::gauge_by_live_pod;
    use env_tests::fixtures::PrometheusClient;

    let cm = fetch_configmap(&shared_configmap());
    let budget = positive(&cm, SHARED_CONFIGMAP, "MH_EGRESS_BUDGET_BPS");
    let (audio, video) = oracle_costs_bps(&fetch_configmap(&profile_configmap()));
    let expected = mh_documented_ceiling(budget, audio, video);

    let cluster = ClusterConnection::new()
        .await
        .expect("Failed to connect to cluster - ensure port-forwards are running");
    let prom = PrometheusClient::new(&cluster.prometheus_base_url);
    let ceilings = gauge_by_live_pod(
        &prom,
        "mh_media_egress_stream_ceiling",
        MH_INSTANCES,
        GAUGE_PRESENT_BOUND,
    )
    .await;
    for (instance, value) in &ceilings {
        assert_eq!(
            *value, expected as f64,
            "MH instance {instance} publishes mh_media_egress_stream_ceiling={value}, but \
             the deployed ConfigMaps (MH_EGRESS_BUDGET_BPS={budget}; derived costs audio \
             {audio}, video {video} bits/s from the live media-codec-profile) derive \
             {expected} under MH's documented rule. Either the pod predates the config \
             ({REDEPLOY_HINT}) or MH's derivation diverged from its documentation."
        );
    }

    let mins = gauge_by_live_pod(
        &prom,
        "mh_media_egress_stream_ceiling_recommended_min",
        MH_INSTANCES,
        GAUGE_PRESENT_BOUND,
    )
    .await;
    let required = required_edges(DEMO_P, DEMO_N, DEMO_M);
    for (instance, value) in &mins {
        assert_eq!(
            *value, required as f64,
            "MH instance {instance} publishes \
             mh_media_egress_stream_ceiling_recommended_min={value}, but this suite's demo \
             requirement is required_edges(P={DEMO_P}, N={DEMO_N}, M={DEMO_M})={required}. The \
             two encode ONE requirement (one full see-and-hear meeting at the demo size); \
             change both together."
        );
    }
}

/// The running MH processes publish the per-stream costs this file's
/// INDEPENDENT oracle derives from the live `media-codec-profile` (story 3
/// R-24), in bytes (`div_ceil(8)`, as MH converts). Replaces the retired
/// `MH_STREAM_COST_*` rows of [`CONFIG_GAUGES`]: MH no longer reads those keys.
#[tokio::test]
async fn test_running_mh_publishes_the_derived_stream_costs() {
    use env_tests::cluster::ClusterConnection;
    use env_tests::fixtures::metrics::gauge_by_live_pod;
    use env_tests::fixtures::PrometheusClient;

    let (audio, video) = oracle_costs_bps(&fetch_configmap(&profile_configmap()));
    let cluster = ClusterConnection::new()
        .await
        .expect("Failed to connect to cluster - ensure port-forwards are running");
    let prom = PrometheusClient::new(&cluster.prometheus_base_url);
    for (gauge, bits) in [
        ("mh_media_stream_cost_audio_bytes_per_second", audio),
        ("mh_media_stream_cost_video_bytes_per_second", video),
    ] {
        let expected = bits.div_ceil(8);
        let values = gauge_by_live_pod(&prom, gauge, MH_INSTANCES, GAUGE_PRESENT_BOUND).await;
        for (instance, value) in &values {
            assert_eq!(
                *value, expected as f64,
                "MH instance {instance} publishes {gauge}={value}, but the live \
                 media-codec-profile derives {bits} bits/s = {expected} bytes/s under \
                 MAX_BITRATE + per-frame overhead x FRAME_RATE (this file's oracle). Either \
                 the pod predates the profile ({REDEPLOY_HINT}) or MH's overhead derivation \
                 diverged from the frame layout."
            );
        }
    }
}

/// (f) The live `media-codec-profile` IS the committed Kind test profile
/// (story 3 R-28): MC and MH resolve ONE profile generation (they roll
/// together), and every value the Kind patch sets is what that generation
/// carries. Catches an overlay that did not apply (a demo edit left in place,
/// a hand `kubectl apply`) and — with the live key set equal to the base
/// key set — a dead patch key, which a strategic merge would ADD beside the
/// renamed base key. The hermetic twin (no cluster) is
/// `fixtures::deployment_config`'s
/// `the_kind_test_profile_patch_overrides_base_keys_only`; both run the ONE
/// patch-vs-base rule, `kind_patch_overrides`.
///
/// Cited by path::fn from the Kind test-profile patch header — keep the name
/// stable.
#[tokio::test]
async fn test_kind_media_codec_profile_is_the_committed_test_profile() {
    let generation = configmap_name_for_all(PROFILE_READERS, PROFILE_CONFIGMAP);
    let live = fetch_configmap(&generation);
    assert_eq!(
        live.pointer("/metadata/labels/environment")
            .and_then(Value::as_str),
        Some("kind"),
        "ConfigMap {generation} does not carry label environment=kind, which the Kind overlay \
         adds. This test pins the KIND test profile and is not meaningful against another \
         cluster - this is not a profile mismatch."
    );

    let overrides = kind_patch_overrides(KIND_TEST_PROFILE_PATCH, PROFILE_BASE_ENV)
        .unwrap_or_else(|e| panic!("{e}; {KIND_SIZING_RESTORE}"));
    assert!(
        !overrides.is_empty(),
        "{KIND_TEST_PROFILE_PATCH} overrides no key: the comparison below would be vacuous"
    );

    let live_data = live
        .pointer("/data")
        .and_then(Value::as_object)
        .unwrap_or_else(|| panic!("ConfigMap {generation} has no data map"));
    let live_keys: std::collections::BTreeSet<&str> =
        live_data.keys().map(String::as_str).collect();
    let base = tree_configmap_data(PROFILE_BASE_ENV).unwrap_or_else(|e| panic!("{e}"));
    let base_keys: std::collections::BTreeSet<&str> = base.keys().map(String::as_str).collect();
    assert_eq!(
        live_keys, base_keys,
        "the live profile generation {generation} does not hold exactly the keys \
         {PROFILE_BASE_ENV} declares: a key was added by the Kind test-profile patch \
         ({KIND_TEST_PROFILE_PATCH}) — a DEAD key after a base rename — or the cluster \
         predates the tree; {KIND_SIZING_RESTORE}."
    );

    for PatchOverride {
        key,
        patch: patch_value,
        base: base_value,
    } in &overrides
    {
        let live_value = cm_value(&live, PROFILE_CONFIGMAP, key);
        assert_ne!(
            &live_value, base_value,
            "OVERLAY-NOT-APPLIED: the live profile generation {generation} carries the \
             PRODUCTION value of {key} from {PROFILE_BASE_ENV}, not the Kind test profile \
             ({KIND_TEST_PROFILE_PATCH}). The patch is removed from the overlay or the cluster \
             was deployed without it — a production-quality demo left in place? Layer 7 runs \
             the TEST profile: {KIND_SIZING_RESTORE}."
        );
        assert_eq!(
            &live_value, patch_value,
            "the live profile generation {generation} carries {key}={live_value}, but the \
             committed Kind test-profile patch ({KIND_TEST_PROFILE_PATCH}) sets {patch_value}: \
             the patch was edited and not restored, or the cluster predates the tree; \
             {KIND_SIZING_RESTORE}."
        );
    }
}
