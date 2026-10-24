//! P0 Smoke Tests: MC Deployment Configuration (story 3 R-1, R-23, R-26)
//!
//! The MC twin of `01_mh_deployment_config.rs`, against the LIVE cluster,
//! never a local render. For both `mc-0` and `mc-1`:
//!
//! - **Wiring.** Every key in [`WIRED_KEYS`] is a HARD `configMapKeyRef`, with
//!   `key` == env name, to the ONE live generation of its base that this pod
//!   is wired to — `mc-service-config` for MC-only knobs (the per-kind slot
//!   caps, including the R-26 video-off lever, and the minimum keyframe
//!   interval `MC_VIDEO_MIN_KEYFRAME_INTERVAL_MS`, MC-only since story 3
//!   amendment 4), `media-codec-profile` for the eight shared encoding keys.
//!   Per workload, never union: a key on mc-0 missing from mc-1 crash-loops
//!   mc-1 alone, because MC-A reads every one as REQUIRED.
//! - **Retirement step 1.** Every key in [`RETIRING_KEYS`] is an
//!   `optional: true` ref AND the key is STILL present and parseable in the
//!   live generation. The current MC image (MC-A) no longer reads them; the
//!   PREVIOUS image — the rollback target until the retirement's step 2 —
//!   still reads all four as REQUIRED, and under an optional ref an absent key
//!   is a silently unset variable that image refuses to boot on.
//! - **Moved key absent.** `MEDIA_VIDEO_MIN_KEYFRAME_INTERVAL_MS` (the
//!   interval's pre-amendment-4 name and home) is absent from the live profile
//!   generation and from every MC container's env: the ONE home for that
//!   absence (`01_mh_deployment_config.rs` points here).
//! - **Parse.** Every resolved value has the SHAPE its reader accepts (shape
//!   only — bounds stay with the readers; no deployed number is restated here).
//! - **One profile generation.** mc-0, mc-1, mh-0 and mh-1 all resolve the SAME
//!   `media-codec-profile` generation — the shared-source property proven on
//!   the live cluster rather than by inspection: MC directs exactly the
//!   encoding MH costs against.
//!
//! Static reads of converged objects: no polling, no sleeps. Layer 7 waits for
//! every rollout, and `configmap_name_for` fails loudly on one in flight.
//!
//! # Honest limitation
//!
//! The pod spec as stored by the API server is the strongest reading available
//! (the image is distroless; no exec). What the running PROCESS loaded is read
//! from the static gauges MC publishes at startup from the SAME parsed values
//! it enforces and directs ([`CONFIG_GAUGES`]): the per-kind receive-slot caps
//! (`mc_media_receive_slot_cap{media_kind}`, so a video cap of 0 is how an
//! operator confirms the R-26 lever) and the minimum keyframe interval
//! (`mc_media_video_keyframe_min_interval_seconds`). As in 01(c), a gauge
//! cannot prove WIRING on its own (a literal `value:` equal to the ConfigMap
//! would also match); the `configMapKeyRef` checks above are what prove that.
//!
//! # Diagnostic output policy (PII)
//!
//! As in `01_mh_deployment_config.rs`: names and the scalar values of named
//! configuration keys only. `mc-service-config` and `media-codec-profile`
//! hold no secrets; MC's credentials arrive via `secretKeyRef`.
//!
//! Edge-sum (concurrent Σ P×(N+M)): 0

#![cfg(feature = "smoke")]

use env_tests::fixtures::deployment_config::{
    check_key_ref, check_shape, cm_value, container, fetch_configmap, fetch_live_pod, parse_u64,
    pod_name_of, RefKind, ValueShape,
};
use env_tests::fixtures::kube::{
    configmap_name_for, configmap_name_for_all, MC_INSTANCES, PROFILE_CONFIGMAP, PROFILE_READERS,
    REDEPLOY_HINT,
};
use env_tests::fixtures::mh_config::{gauge_verdict, Conversion, GaugeVerdict};

const CONTAINER: &str = "mc-service";
/// Generator base names; live names are hash-suffixed (ADR-0038 §2) and
/// resolved from each pod spec.
const MC_CONFIGMAP: &str = "mc-service-config";

/// Every key wired into BOTH MC workloads as a HARD ref: `(key, base, shape)`.
/// ONE table, iterated for every instance.
const WIRED_KEYS: &[(&str, &str, ValueShape)] = &[
    // Per-kind receive-slot caps (R-1, R-26). Video 0 is the lever, so it is
    // NonNegative; audio 0 would be a silent outage MC refuses.
    (
        "MC_MAX_AUDIO_RECEIVE_SLOTS",
        MC_CONFIGMAP,
        ValueShape::PositiveInteger,
    ),
    (
        "MC_MAX_VIDEO_RECEIVE_SLOTS",
        MC_CONFIGMAP,
        ValueShape::NonNegativeInteger,
    ),
    (
        "MC_MAX_RECEIVE_CAPABILITY_DECLARATIONS",
        MC_CONFIGMAP,
        ValueShape::PositiveInteger,
    ),
    // MC-only since story 3 amendment 4 (was the profile's
    // MEDIA_VIDEO_MIN_KEYFRAME_INTERVAL_MS); carried on every video directive.
    (
        "MC_VIDEO_MIN_KEYFRAME_INTERVAL_MS",
        MC_CONFIGMAP,
        ValueShape::PositiveInteger,
    ),
    // Shared media codec profile: all eight keys MC directs from.
    ("MEDIA_VIDEO_CODEC", PROFILE_CONFIGMAP, ValueShape::Token),
    (
        "MEDIA_VIDEO_WIDTH",
        PROFILE_CONFIGMAP,
        ValueShape::PositiveInteger,
    ),
    (
        "MEDIA_VIDEO_HEIGHT",
        PROFILE_CONFIGMAP,
        ValueShape::PositiveInteger,
    ),
    (
        "MEDIA_VIDEO_FRAME_RATE_HZ",
        PROFILE_CONFIGMAP,
        ValueShape::PositiveInteger,
    ),
    (
        "MEDIA_VIDEO_MAX_BITRATE_BPS",
        PROFILE_CONFIGMAP,
        ValueShape::PositiveInteger,
    ),
    ("MEDIA_AUDIO_CODEC", PROFILE_CONFIGMAP, ValueShape::Token),
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

/// RETIRING MC keys (two-deploy retirement, step 1 — docs/TODO.md §Media Path
/// Obligations, "Delete the story-3 retired config keys").
const RETIRING_KEYS: &[(&str, ValueShape)] = &[
    ("MC_MAX_RECEIVE_SLOTS", ValueShape::PositiveInteger),
    ("MC_AUDIO_CODEC", ValueShape::Token),
    ("MC_AUDIO_MAX_BITRATE_BPS", ValueShape::PositiveInteger),
    ("MC_AUDIO_FRAME_RATE_HZ", ValueShape::PositiveInteger),
];

/// Keys that MOVED out of a base this file checks: absent from the named base's
/// live generation AND from the MC container env. `(key, base it left)`.
///
/// Asserted absent because `dt-guard env-config` checks only that every key
/// the code reads is declared, never that a declared key is read: a stale
/// profile row plus a stale ref would pass it.
const MOVED_KEYS: &[(&str, &str)] = &[("MEDIA_VIDEO_MIN_KEYFRAME_INTERVAL_MS", PROFILE_CONFIGMAP)];

fn env_names<'a>(pod: &'a serde_json::Value, instance: &str) -> Vec<&'a str> {
    let pod_name = pod_name_of(pod, instance);
    container(pod, &pod_name, CONTAINER)
        .unwrap_or_else(|e| panic!("{e}"))
        .get("env")
        .and_then(serde_json::Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|e| e.get("name").and_then(serde_json::Value::as_str))
                .collect()
        })
        .unwrap_or_default()
}

fn assert_mc_keys_wired(instance: &str) {
    let mc_cm_name = configmap_name_for(instance, MC_CONFIGMAP);
    let profile_cm_name = configmap_name_for(instance, PROFILE_CONFIGMAP);
    let mc_cm = fetch_configmap(&mc_cm_name);
    let profile_cm = fetch_configmap(&profile_cm_name);
    let pod = fetch_live_pod(instance);

    // Positive control that CAN fail: the table wires every base this file
    // covers (a table that quietly lost the profile rows would otherwise pass
    // on what remains), and the retiring list is not empty.
    for base in [MC_CONFIGMAP, PROFILE_CONFIGMAP] {
        assert!(
            WIRED_KEYS.iter().any(|(_, b, _)| *b == base),
            "WIRED_KEYS has no row for base {base}"
        );
    }
    assert!(!RETIRING_KEYS.is_empty());
    for &(key, base, shape) in WIRED_KEYS {
        let (cm_name, cm) = if base == PROFILE_CONFIGMAP {
            (profile_cm_name.as_str(), &profile_cm)
        } else {
            (mc_cm_name.as_str(), &mc_cm)
        };
        check_key_ref(&pod, CONTAINER, key, cm_name, RefKind::Hard)
            .unwrap_or_else(|e| panic!("{e}"));
        check_shape(key, &cm_value(cm, base, key), shape, base).unwrap_or_else(|e| panic!("{e}"));
    }
    for &(key, shape) in RETIRING_KEYS {
        check_key_ref(&pod, CONTAINER, key, &mc_cm_name, RefKind::Optional)
            .unwrap_or_else(|e| panic!("{e}"));
        check_shape(
            key,
            &cm_value(&mc_cm, MC_CONFIGMAP, key),
            shape,
            MC_CONFIGMAP,
        )
        .unwrap_or_else(|e| panic!("{e}"));
    }

    // MOVED keys. Positive control first: the env list read here holds every
    // wired key, so an unread or empty list cannot make the absence vacuous.
    let env = env_names(&pod, instance);
    assert!(
        WIRED_KEYS.iter().all(|(key, _, _)| env.contains(key)),
        "{instance}: the env list read for the moved-key check does not hold every wired key \
         — the absence check below would be vacuous"
    );
    assert!(!MOVED_KEYS.is_empty());
    for &(key, left) in MOVED_KEYS {
        assert!(
            !env.contains(&key),
            "{instance}: env {key} is still wired into the MC container, but story 3 \
             amendment 4 moved the minimum keyframe interval to {MC_CONFIGMAP} as \
             MC_VIDEO_MIN_KEYFRAME_INTERVAL_MS. Remove the ref from \
             infra/services/mc-service/{instance}-deployment.yaml."
        );
        let cm = if left == PROFILE_CONFIGMAP {
            &profile_cm
        } else {
            &mc_cm
        };
        assert!(
            cm.pointer("/data").and_then(|d| d.get(key)).is_none(),
            "live ConfigMap {left} still holds {key}, moved to {MC_CONFIGMAP} by story 3 \
             amendment 4. Remove it from infra/services/media-codec-profile/profile.env."
        );
    }
}

#[tokio::test]
async fn test_story3_keys_wired_into_every_mc_instance() {
    for instance in MC_INSTANCES {
        assert_mc_keys_wired(instance);
    }
}

/// MC and MH resolve the SAME `media-codec-profile` generation: one hashed
/// name across all four pods. `configmap_name_for_all` resolves each pod's
/// generation from its spec, rejects a bare name and an in-flight rollout, and
/// fails on disagreement — two generations would mean MC directs an encoding
/// MH does not cost against.
#[tokio::test]
async fn test_mc_and_mh_resolve_one_media_codec_profile_generation() {
    let name = configmap_name_for_all(PROFILE_READERS, PROFILE_CONFIGMAP);
    assert!(
        name.starts_with(&format!("{PROFILE_CONFIGMAP}-")),
        "resolved {name:?} is not a hash-suffixed generation of {PROFILE_CONFIGMAP}"
    );
}

/// How long a startup gauge may take to be scraped for a pod.
const GAUGE_PRESENT_BOUND: std::time::Duration = std::time::Duration::from_secs(60);

/// `(mc-service-config key, published series selector, Conversion)` for every
/// value the running MC process loaded and publishes from the SAME parsed
/// field it enforces (caps) or directs (interval). ONE table.
///
/// The video cap may legitimately be 0 (the R-26 lever), so values are parsed
/// as non-negative, never `positive`.
const CONFIG_GAUGES: &[(&str, &str, Conversion)] = &[
    (
        "MC_MAX_AUDIO_RECEIVE_SLOTS",
        "mc_media_receive_slot_cap{media_kind=\"audio\"}",
        Conversion::Identity,
    ),
    (
        "MC_MAX_VIDEO_RECEIVE_SLOTS",
        "mc_media_receive_slot_cap{media_kind=\"video\"}",
        Conversion::Identity,
    ),
    (
        "MC_VIDEO_MIN_KEYFRAME_INTERVAL_MS",
        "mc_media_video_keyframe_min_interval_seconds",
        Conversion::MsToSeconds,
    ),
];

/// What the RUNNING MC process loaded equals its OWN ConfigMap generation, per
/// instance, per [`CONFIG_GAUGES`] row. Three distinct outcomes, as in 01(c):
/// no data (absent for this pod), shape (the publisher changed), mismatch (the
/// process runs another value).
#[tokio::test]
async fn test_config_reported_by_running_mc_matches_configmap() {
    use env_tests::cluster::ClusterConnection;
    use env_tests::fixtures::metrics::poll_until_pinned_instance;
    use env_tests::fixtures::PrometheusClient;
    use std::time::Duration;

    let cluster = ClusterConnection::new()
        .await
        .expect("Failed to connect to cluster - ensure port-forwards are running");
    let prom = PrometheusClient::new(&cluster.prometheus_base_url);

    assert!(!CONFIG_GAUGES.is_empty());
    for instance in MC_INSTANCES {
        let pod = fetch_live_pod(instance);
        let pod_name = pod_name_of(&pod, instance);
        // Pinned to THIS pod's IP: right after a roll, the terminating
        // predecessor is still scraped and would otherwise satisfy the read.
        let pod_ip = pod
            .pointer("/status/podIP")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_else(|| panic!("pod {pod_name} has no status.podIP"))
            .to_string();
        let cm = fetch_configmap(&configmap_name_for(instance, MC_CONFIGMAP));
        for &(key, series, conversion) in CONFIG_GAUGES {
            let deployed = parse_u64(key, &cm_value(&cm, MC_CONFIGMAP, key), MC_CONFIGMAP)
                .unwrap_or_else(|e| panic!("{e}"));
            let promql = format!("max by (instance) ({series})");
            let value = poll_until_pinned_instance(
                &prom,
                &promql,
                &pod_ip,
                GAUGE_PRESENT_BOUND,
                Duration::from_secs(2),
                &format!(
                    "PRESENCE ({instance}, pod {pod_name}): MC publishes {series} at process \
                     start, so absence is a scrape/startup problem — NOT a configuration \
                     mismatch"
                ),
                "present",
                |_| true,
            )
            .await;
            match gauge_verdict(conversion, deployed, value) {
                GaugeVerdict::Match => {}
                GaugeVerdict::Shape => panic!(
                    "{series} on {instance} (pod {pod_name}) is not a value its {conversion:?} \
                     conversion can produce ({value}): the publisher's shape changed in \
                     crates/mc-service/src/observability/metrics.rs — update CONFIG_GAUGES. \
                     This is not a configuration mismatch."
                ),
                GaugeVerdict::Mismatch(expected) => panic!(
                    "{instance} (pod {pod_name}) runs {series}={value}, but its {MC_CONFIGMAP} \
                     generation carries {key}={deployed} ({conversion:?}: {expected}). The \
                     process is not running the deployed value; {REDEPLOY_HINT}."
                ),
            }
        }
    }
}
