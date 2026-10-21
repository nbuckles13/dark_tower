//! P0 Smoke Tests: MC Deployment Configuration (story 3 R-1, R-23, R-26)
//!
//! The MC twin of `01_mh_deployment_config.rs`, against the LIVE cluster,
//! never a local render. For both `mc-0` and `mc-1`:
//!
//! - **Wiring.** Every key in [`WIRED_KEYS`] is a HARD `configMapKeyRef`, with
//!   `key` == env name, to the ONE live generation of its base that this pod
//!   is wired to — `mc-service-config` for MC-only knobs (the per-kind slot
//!   caps, including the R-26 video-off lever), `media-codec-profile` for the
//!   nine shared encoding keys. Per workload, never union: a key on mc-0
//!   missing from mc-1 crash-loops mc-1 alone once MC-A requires it.
//! - **Retirement step 1.** Every key in [`RETIRING_KEYS`] is an
//!   `optional: true` ref AND the key is STILL present and parseable in the
//!   live generation — the current MC image still reads all four as REQUIRED,
//!   and under an optional ref an absent key is a silently unset variable that
//!   image refuses to boot on.
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
//! (the image is distroless; no exec). The current MC image ignores the new
//! keys, so nothing here proves a process LOADED them; MC-A's published caps
//! (`mc_media_receive_slot_cap{media_kind}`) are where that parity lands.
//!
//! # Diagnostic output policy (PII)
//!
//! As in `01_mh_deployment_config.rs`: names and the scalar values of named
//! configuration keys only. `mc-service-config` and `media-codec-profile`
//! hold no secrets; MC's credentials arrive via `secretKeyRef`.

#![cfg(feature = "smoke")]

use env_tests::fixtures::deployment_config::{
    check_key_ref, check_shape, cm_value, fetch_configmap, fetch_live_pod, RefKind, ValueShape,
};
use env_tests::fixtures::kube::{configmap_name_for, configmap_name_for_all, PROFILE_CONFIGMAP};

const CONTAINER: &str = "mc-service";
const MC_INSTANCES: &[&str] = &["mc-0", "mc-1"];
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
    // Shared media codec profile: all nine keys MC directs from.
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
    (
        "MEDIA_VIDEO_MIN_KEYFRAME_INTERVAL_MS",
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
    let name = configmap_name_for_all(&["mc-0", "mc-1", "mh-0", "mh-1"], PROFILE_CONFIGMAP);
    assert!(
        name.starts_with(&format!("{PROFILE_CONFIGMAP}-")),
        "resolved {name:?} is not a hash-suffixed generation of {PROFILE_CONFIGMAP}"
    );
}
