//! Resilience tests: NetworkPolicy enforcement (canary pods), the MH pool /
//! process-injector smoke legs that prove the story 4 resilience fixture end
//! to end, and the MC-B2 handler-loss legs (crash-to-survivor, zombie)
//! (ADR-0039 §2, §6, §9; R-1, R-4, R-10, R-39).
//!
//! # Isolation convention (EVERY test in this binary is `#[serial]`)
//!
//! Story 4 has no per-test isolation of the MH pool; it relies on
//! serialization. Every destructive or pool-mutating env-test (kill, freeze,
//! drain, up) lives in THIS binary, and every test here carries `#[serial]`,
//! because `serial_test` only serializes MARKED tests: one unmarked test could
//! run beside a frozen or killed MH. `cargo test` runs binaries one at a time
//! and Playwright runs `workers: 1`, so within one Layer-7 run a killed,
//! frozen or drained MH affects only the test that did it. The rule is checked
//! at Layer 4 by `env_tests::resilience_hygiene` (token-level, for this file
//! AND any other binary that calls a pool-mutating fixture API, e.g. 44).
//!
//! Concurrent runs sharing one cluster are NOT isolated: they fail loudly with
//! `foreign-mh-activity` (`MhPool::assert_baseline`, before and after every
//! leg), never pass falsely.
//!
//! Every pool leg uses only FIXTURE-OWNED MHs (owner = the run token), tears
//! down explicitly and asserts it (`MhPool::run_leg`: `teardown_and_settle`,
//! which also checks nothing is left owned by the run), and re-asserts the
//! baseline. A
//! panicking leg's `MhPool` Drop prints the exact reclaim command.
//!
//! MC-B2 legs (crash-to-survivor, zombie): see their section below.
//!
//! Edge-sum (concurrent Σ P×(N+M)): 2

#![cfg(feature = "resilience")]

use std::sync::Arc;
use std::time::Duration;

use env_tests::canary::{CanaryConfig, CanaryPod};
use env_tests::cluster::ClusterConnection;
use env_tests::fixtures::gc_fleet::live_gc_pods;
use env_tests::fixtures::mc_session::JoinDeclaration;
use env_tests::fixtures::media::{assert_relayed_on_slot, bind_until_received};
use env_tests::fixtures::metrics::{
    format_instance_map, heartbeat_interval_of, mc_heartbeat_deadline,
    poll_until_any_instance_above, poll_until_pinned_instance, settled_baseline, sum_over_pods,
    InstanceCounters, BASELINE_SETTLE_BOUND, PINNED_METRIC_BOUND,
};
use env_tests::fixtures::mh_address::assert_addresses_unique;
use env_tests::fixtures::mh_pool::MhPool;
use env_tests::fixtures::owned_placement::{live_placement_attempts, place_on_owned};
use env_tests::fixtures::participant::{
    reach, settle_window, slots_of, Participant, AUDIO, VIEW_MARGIN,
};
use env_tests::fixtures::{AuthClient, GcClient, PrometheusClient};
use proto_gen::dark_tower::signaling::v1::StreamAssignments;
use serial_test::serial;

// ============================================================================
// MH pool fixture + injector smoke legs (T-1)
// ============================================================================

/// The pool fixture end to end on the live cluster: `up` (returns at MH
/// /ready) -> Prometheus scrapes it by pod IP -> explicit, asserted `drain`
/// -> owner teardown leaves nothing owned by the run -> baseline restored.
/// No assertion reads the exiting MH's series (drain outcomes are MH-free).
#[tokio::test]
#[serial]
async fn mh_pool_fixture_up_scrape_drain_restores_baseline() {
    MhPool::run_leg(|pool, prom_url| async move {
        let prom = PrometheusClient::new(&prom_url);
        let slot = pool.up_scraped(&prom).await;
        let listed = {
            let s = slot.clone();
            tokio::task::spawn_blocking(move || s.entry())
                .await
                .expect("list task")
        };
        assert_eq!(
            listed.owner.as_deref(),
            Some(pool.token()),
            "up's MH is run-owned"
        );
        assert_eq!(listed.pod_uid.as_deref(), Some(slot.pod_uid.as_str()));
        tokio::task::spawn_blocking(move || slot.drain())
            .await
            .unwrap_or_else(|e| std::panic::resume_unwind(e.into_panic()));
    })
    .await;
}

/// The injector end to end on a fixture-owned MH: freeze (mh.sh observes
/// `/proc/<pid>/stat` = T: the freeze TOOK EFFECT) -> resume inside the live
/// liveness budget with restart_count and pod_uid unchanged -> kill with an
/// in-place restart (same pod_uid, restart_count + 1) -> scrape re-waited ->
/// teardown -> the R-28 address invariant still holds.
#[tokio::test]
#[serial]
async fn mh_injector_freeze_resume_then_kill_restarts_in_place() {
    MhPool::run_leg(|pool, prom_url| async move {
        let prom = PrometheusClient::new(&prom_url);
        let slot = pool.up_scraped(&prom).await;
        let before = slot.restart_count;

        let frozen = tokio::task::spawn_blocking(move || slot.freeze())
            .await
            .unwrap_or_else(|e| std::panic::resume_unwind(e.into_panic()));
        // The CONT call costs about what the STOP call did (same mechanism),
        // so resuming in time needs 2 x stop_call inside the budget.
        assert!(
            2 * frozen.stop_call() < frozen.budget(),
            "the STOP call took {:?}; with a CONT of similar cost the {:?} liveness budget \
             cannot be met",
            frozen.stop_call(),
            frozen.budget()
        );
        let mut slot = frozen.resume(&prom).await;
        assert_eq!(slot.restart_count, before, "freeze/resume never restarts");

        let uid = slot.pod_uid.clone();
        slot.kill(&prom).await;
        assert_eq!(
            slot.restart_count,
            before + 1,
            "KILL restarts the container in place"
        );
        assert_eq!(
            slot.pod_uid, uid,
            "same pod: an in-place restart, never a pod delete"
        );
        // R-28 on the RESTARTED incarnation (its fresh startup log and its slot
        // address) while it is still in the pool; the gather settles all pods.
        assert_addresses_unique().await;
    })
    .await;
}

// ============================================================================
// MC-B2: handler detection and removal (crash-to-survivor, zombie)
// ============================================================================
//
// Both legs follow the module isolation convention: `#[serial]`, through
// `MhPool::run_leg`, signalling only a RUN-OWNED MH that a meeting was placed
// on by the bounded, derived search in `fixtures::owned_placement` (GC
// placement is weighted-random and `mh.sh signal` is owner-gated).
//
// TIMING. Every wait derives from MC's published heartbeat deadline N
// (`mc_media_handler_heartbeat_deadline_seconds`) and interval N/3, MC's
// connect settle window, the injector's live liveness budget, or a shared
// failure-only bound. Each is a ceiling spent only on failure, never a latency
// target, and nothing sleeps to gate an assertion.
//
// EVIDENCE. Per entity wherever one exists: the participant's own
// MediaHandlerSet and StreamAssignments, frames on a named transport, and MH
// counters pinned to the owned pod's IP. MC's declaration counters are
// fleet-wide and are read only as "rose" (supporting evidence). The
// per-meeting proof is the set the participant received.

/// Shared poll interval for the metric polls below (a cadence, not a bound).
const POLL: Duration = Duration::from_secs(2);

/// Per-sender payload markers (A, B).
const MARK: [u8; 2] = [0xA1, 0xB1];
/// Receive-slot ids A and B declare (non-ordinal, one > 255).
const SLOT: [u32; 2] = [311, 7];

const MC_OWNED_HEARTBEATS: &str =
    r#"sum by (instance) (mc_media_handler_heartbeats_total{outcome="owned"})"#;
const MC_DECLARATIONS_HT: &str =
    r#"sum by (instance) (mc_media_handler_declarations_total{trigger="heartbeat_timeout"})"#;
const MC_DETECTION_COUNT_HT: &str =
    r#"sum by (instance) (mc_media_handler_detection_seconds_count{trigger="heartbeat_timeout"})"#;
const GC_SUPERSEDED_BOTH: &str =
    r#"sum by (instance) (gc_mh_registrations_total{outcome="superseded",match="both"})"#;
const GC_UNKNOWN_HANDLER: &str =
    r#"sum by (instance) (gc_mh_load_reports_total{outcome="unknown_handler"})"#;
const MH_RELEASES_HT: &str =
    r#"sum by (instance) (mh_media_handler_releases_total{trigger="heartbeat_timeout"})"#;
const MH_RELEASES_OWNER_SILENCE: &str =
    r#"sum by (instance) (mh_media_handler_releases_total{trigger="owner_silence"})"#;
const MH_RELEASED_VERDICTS: &str =
    r#"sum by (instance) (mh_mc_heartbeat_verdicts_total{outcome="released"})"#;
const MH_OWNER_SILENCE_TIMEOUT: &str = "mh_media_meeting_owner_silence_timeout_seconds";
const MH_REGISTERED_MEETINGS: &str = "sum by (instance) (mh_media_registered_meetings)";

async fn connected_cluster() -> ClusterConnection {
    let cluster = ClusterConnection::new()
        .await
        .expect("Failed to connect to cluster - ensure port-forwards are running");
    cluster.check_ac_health().await.expect("AC must be running");
    cluster.check_gc_health().await.expect("GC must be running");
    cluster
}

/// The pinned value of `promql` on the pod at `pod_ip`, waiting only for the
/// series to be scraped. Every series read this way is zero-initialised at
/// process start, so ABSENCE is a presence failure, never a zero.
async fn pinned(prom: &PrometheusClient, promql: &str, pod_ip: &str, phase: &str) -> f64 {
    poll_until_pinned_instance(
        prom,
        promql,
        pod_ip,
        PINNED_METRIC_BOUND,
        POLL,
        phase,
        "present (zero-initialised at start)",
        |_| true,
    )
    .await
}

/// Wait for `promql` to rise above `baseline` on some instance.
async fn rose(prom: &PrometheusClient, promql: &str, baseline: &InstanceCounters, phase: &str) {
    let phase = phase.to_string();
    let q = promql.to_string();
    poll_until_any_instance_above(prom, promql, baseline, PINNED_METRIC_BOUND, POLL, |now| {
        format!(
            "{phase}: `{q}` did not rise within {PINNED_METRIC_BOUND:?}: before {}, now {}",
            format_instance_map(baseline),
            format_instance_map(now)
        )
    })
    .await;
}

/// R-1 (survivor arm), R-4, R-39: kill the run-owned MH carrying a
/// two-handler meeting's media. MC declares it dead (heartbeat_timeout),
/// removes it from the set, and re-places the pair's edges onto the
/// SURVIVOR, where media resumes. GC counts the in-place restart's
/// registration as superseded{match=both} exactly once, and no load report
/// from an unknown handler.
///
/// NON-VACUITY (the kill must matter): A and B first reach ONLY the owned MH,
/// so the pair's edges can land nowhere else. Frames are then proven on the
/// owned transport in both directions. Only after that do A and B reach the
/// survivor, and MC never re-chooses a placed, still-valid edge
/// (`mc-service/src/media_routing/edges.rs`, edge stability). So before the
/// kill the pair's media flows through the owned MH, and after it the same
/// pair is heard on the survivor's transport.
#[tokio::test]
#[serial]
async fn mc_b2_crash_of_a_carrying_handler_moves_media_to_the_survivor() {
    MhPool::run_leg(|pool, prom_url| async move { crash_leg(pool, &prom_url).await }).await;
}

async fn crash_leg(pool: Arc<MhPool>, prom_url: &str) {
    let prom = PrometheusClient::new(prom_url);
    let cluster = connected_cluster().await;
    let auth = AuthClient::new(&cluster.ac_base_url);
    let gc = GcClient::new(&cluster.gc_base_url);
    let n = mc_heartbeat_deadline(&prom).await;
    let interval = heartbeat_interval_of(n);
    let view_bound = settle_window(&cluster).await + VIEW_MARGIN;

    let mut slot = pool.up_scraped(&prom).await;
    let owned = slot.address.clone();
    let attempts = live_placement_attempts(&pool).await;
    let owned_heartbeats_before = prom.instance_counter_map(MC_OWNED_HEARTBEATS).await;

    // PLACEMENT: a two-handler meeting holding the owned MH (precondition: a
    // survivor exists by construction).
    let (a, meeting) = place_on_owned(
        &auth,
        &gc,
        "MC-B2 crash survivor",
        2,
        "A",
        &owned,
        Some(2),
        attempts,
    )
    .await;
    eprintln!(
        "40/crash: meeting placed on run-owned slot {} ({owned}) after {} of {attempts} attempts",
        slot.slot, meeting.attempts
    );
    let b = meeting.join(1, "B", &JoinDeclaration::default()).await;
    let mut ps = vec![a, b];
    assert_eq!(
        ps[1].offered(),
        ps[0].offered(),
        "CRASH: both participants are offered the meeting's one set"
    );
    let survivor = ps[0]
        .offered()
        .into_iter()
        .find(|u| *u != owned)
        .expect("placement_hit guarantees exactly one other member");
    let ids = [ps[0].sender_id(), ps[1].sender_id()];

    // STAGE 1: both reach ONLY the owned MH, so the pair's edges are on it.
    let mut on_owned = Vec::new();
    for p in &ps {
        on_owned.push(reach("CRASH pre-kill", p, &owned).await);
    }
    for (i, p) in ps.iter_mut().enumerate() {
        p.declare(&[SLOT[i]], &[]).await;
    }
    for i in 0..2 {
        let (peer, on) = (ids[1 - i], handler_ref_of("CRASH pre-kill", &ps[i], &owned));
        ps[i]
            .converge("CRASH pre-kill", view_bound, move |v, _| {
                v.is_some_and(|v| heard_on(v, peer, on))
            })
            .await;
    }
    for (r, s) in [(0_usize, 1_usize), (1, 0)] {
        let phase = format!("CRASH pre-kill {}<-{}", ps[r].label, ps[s].label);
        let got = bind_until_received(
            &phase,
            on_owned[r].connection(),
            &[(on_owned[s].connection(), MARK[s])],
            &[],
        )
        .await;
        assert_relayed_on_slot(
            &phase,
            &got[&MARK[s]],
            ps[r].slot_for(ids[s], AUDIO),
            MARK[s],
            ps[s].label,
        );
    }

    // STAGE 2: both also reach the survivor (placed edges stay on the owned MH).
    let mut on_survivor = Vec::new();
    for p in &ps {
        on_survivor.push(reach("CRASH survivor", p, &survivor).await);
    }

    // MC answered OWNED heartbeats for the owned MH's meetings before the
    // kill (fleet-wide counter: supporting evidence only).
    rose(
        &prom,
        MC_OWNED_HEARTBEATS,
        &owned_heartbeats_before,
        "CRASH (owned-heartbeats-before-kill)",
    )
    .await;

    // Baselines: settled, after every pre-kill action.
    let declarations_before = settled_baseline(
        &prom,
        MC_DECLARATIONS_HT,
        BASELINE_SETTLE_BOUND,
        "the heartbeat_timeout declaration under test",
    )
    .await;
    let detections_before = settled_baseline(
        &prom,
        MC_DETECTION_COUNT_HT,
        BASELINE_SETTLE_BOUND,
        "the heartbeat_timeout detection observation under test",
    )
    .await;
    let gc_pods = live_gc_pods().await;
    let gc_names_before = gc_pods.names();
    let superseded_before = sum_over_pods(
        &settled_baseline(
            &prom,
            GC_SUPERSEDED_BOTH,
            BASELINE_SETTLE_BOUND,
            "the supersede under test",
        )
        .await,
        &gc_pods.serving,
        GC_SUPERSEDED_BOTH,
        "CRASH (gc-presence)",
    );
    let unknown_before = sum_over_pods(
        &settled_baseline(
            &prom,
            GC_UNKNOWN_HANDLER,
            BASELINE_SETTLE_BOUND,
            "the unknown-handler count under test",
        )
        .await,
        &gc_pods.serving,
        GC_UNKNOWN_HANDLER,
        "CRASH (gc-presence)",
    );

    // KILL (debug container, never a pod delete); `kill` waits for the
    // in-place restart and the re-scrape.
    let (uid, restarts) = (slot.pod_uid.clone(), slot.restart_count);
    slot.kill(&prom).await;
    assert_eq!(
        slot.restart_count,
        restarts + 1,
        "CRASH: KILL restarts in place"
    );
    assert_eq!(slot.pod_uid, uid, "CRASH: same pod, never a pod delete");

    // REMOVAL, per entity: each participant's set becomes {survivor}. The
    // restarted incarnation may reuse the owned address, and MC-B2 never adds
    // a handler, so the set is exactly the survivor. Bound: one detection
    // (N, armed per heartbeat interval) plus the view margin, from now; the
    // declaration may already have happened during the restart wait.
    let removal_bound = n + interval + VIEW_MARGIN;
    for p in ps.iter_mut() {
        let want = vec![survivor.clone()];
        p.await_handler_set("CRASH removal", removal_bound, move |sets| {
            sets.current().is_some_and(|v| v.urls() == want)
        })
        .await;
    }

    // RE-PLACEMENT: each receiver's slot for its peer names the survivor, and
    // frames arrive on the survivor's transport, both directions.
    for i in 0..2 {
        let (peer, on) = (
            ids[1 - i],
            handler_ref_of("CRASH re-placed", &ps[i], &survivor),
        );
        ps[i]
            .converge("CRASH re-placed", view_bound, move |v, _| {
                v.is_some_and(|v| heard_on(v, peer, on))
            })
            .await;
    }
    for (r, s) in [(0_usize, 1_usize), (1, 0)] {
        let phase = format!("CRASH survivor {}<-{}", ps[r].label, ps[s].label);
        let got = bind_until_received(
            &phase,
            on_survivor[r].connection(),
            &[(on_survivor[s].connection(), MARK[s])],
            &[],
        )
        .await;
        assert_relayed_on_slot(
            &phase,
            &got[&MARK[s]],
            ps[r].slot_for(ids[s], AUDIO),
            MARK[s],
            ps[s].label,
        );
    }

    // MC counted the declaration and its detection time (fleet-wide: rose).
    rose(
        &prom,
        MC_DECLARATIONS_HT,
        &declarations_before,
        "CRASH (declaration-counted)",
    )
    .await;
    rose(
        &prom,
        MC_DETECTION_COUNT_HT,
        &detections_before,
        "CRASH (detection-observed)",
    )
    .await;

    // GC: the in-place restart re-registered the SAME endpoints, so exactly
    // one superseded{match=both} across the replicas, and nothing reported
    // load as an unknown handler.
    let deadline = std::time::Instant::now() + PINNED_METRIC_BOUND;
    let (superseded_after, last) = loop {
        let map = prom.instance_counter_map(GC_SUPERSEDED_BOTH).await;
        let now = sum_over_pods(
            &map,
            &gc_pods.serving,
            GC_SUPERSEDED_BOTH,
            "CRASH (gc-presence)",
        );
        if now - superseded_before >= 1.0 || std::time::Instant::now() > deadline {
            break (now, map);
        }
        tokio::time::sleep(POLL).await;
    };
    assert_eq!(
        live_gc_pods().await.names(),
        gc_names_before,
        "CRASH (gc-replica-set-changed): the serving GC pods changed, so the per-replica \
         counters may have reset"
    );
    #[expect(
        clippy::float_cmp,
        reason = "counter sums of small integers are exact in f64"
    )]
    let exactly_one = superseded_after - superseded_before == 1.0;
    assert!(
        exactly_one,
        "CRASH (gc-superseded-both): the in-place restart must supersede its predecessor's row \
         exactly once with both endpoints matching: before {superseded_before}, after \
         {superseded_after}; instances {}",
        format_instance_map(&last)
    );
    let unknown_after = sum_over_pods(
        &prom.instance_counter_map(GC_UNKNOWN_HANDLER).await,
        &gc_pods.serving,
        GC_UNKNOWN_HANDLER,
        "CRASH (gc-presence)",
    );
    #[expect(
        clippy::float_cmp,
        reason = "counter sums of small integers are exact in f64"
    )]
    let unknown_unchanged = unknown_after == unknown_before;
    assert!(
        unknown_unchanged,
        "CRASH (gc-unknown-handler): no load report may come from an unregistered handler \
         across a crash-in-place: before {unknown_before}, after {unknown_after}"
    );

    // R-28 on the restarted incarnation.
    assert_addresses_unique().await;
}

/// R-10, R-39: the zombie. Freeze (SIGSTOP via the injector, which proves the
/// freeze took effect) the run-owned MH holding a meeting. While it is STILL
/// FROZEN, MC removes it from the meeting's set: that is the mandatory first
/// assertion, so a freeze that did not take effect, or a kubelet restart,
/// cannot pass as a zombie. Then resume. The same incarnation (restart_count
/// and pod_uid unchanged, asserted by the guard) releases the meeting as
/// heartbeat_timeout, exactly once on ITS pod, and never as owner_silence.
///
/// CARRIER-AGNOSTIC (media-handler ruling): the release may ride the first
/// heartbeat's RELEASED verdict or MC's EndMeeting(HEARTBEAT_TIMEOUT), if that
/// call was still buffered across the freeze. Both are trigger
/// heartbeat_timeout. The carrier is logged, not asserted. MH no longer
/// holding the meeting is asserted directly (`mh_media_registered_meetings`
/// on that pod back to its baseline). The `clients` label and
/// HANDLER_RELEASED closes are NOT pinned here: the clients' QUIC sessions
/// idle out at N during the freeze. Where each property IS pinned:
/// - MH sends NO `Disconnected` on a release and closes its sessions with
///   HANDLER_RELEASED: mh-service `tests/mc_heartbeat_integration.rs` (the
///   RELEASED-verdict arms) and
///   `tests/webtransport_close_code_integration.rs::an_end_meeting_release_closes_an_active_session_with_handler_released`;
/// - a `Disconnected` the resumed zombie DOES send here (an idle-out during
///   the freeze legitimately sends one) is refused by MC without touching the
///   survivor: mc-service
///   `tests/handler_detection_integration.rs::a_zombie_disconnected_with_a_colliding_id_leaves_the_survivor_untouched`.
///
/// No fallback branch: an injector that reports unavailable fails loudly
/// with its own reason (the story records any fallback as a decision, never
/// a runtime skip).
#[tokio::test]
#[serial]
async fn mc_b2_zombie_is_removed_while_frozen_and_released_on_resume() {
    MhPool::run_leg(|pool, prom_url| async move { zombie_leg(pool, &prom_url).await }).await;
}

async fn zombie_leg(pool: Arc<MhPool>, prom_url: &str) {
    let prom = PrometheusClient::new(prom_url);
    let cluster = connected_cluster().await;
    let auth = AuthClient::new(&cluster.ac_base_url);
    let gc = GcClient::new(&cluster.gc_base_url);
    let n = mc_heartbeat_deadline(&prom).await;
    let interval = heartbeat_interval_of(n);

    let slot = pool.up_scraped(&prom).await;
    let owned = slot.address.clone();
    let pod_ip = slot.pod_ip.clone();
    let owner_silence = Duration::from_secs_f64(
        pinned(
            &prom,
            MH_OWNER_SILENCE_TIMEOUT,
            &pod_ip,
            "ZOMBIE PRECONDITION (owner-silence-gauge)",
        )
        .await,
    );
    // The owned MH is fresh, and a placement miss never lands on it, so its
    // registered-meeting gauge moves only for the meeting under test.
    let registered_before = pinned(
        &prom,
        MH_REGISTERED_MEETINGS,
        &pod_ip,
        "ZOMBIE (registered-baseline)",
    )
    .await;
    let attempts = live_placement_attempts(&pool).await;
    let owned_heartbeats_before = prom.instance_counter_map(MC_OWNED_HEARTBEATS).await;

    let (mut z, meeting) =
        place_on_owned(&auth, &gc, "MC-B2 zombie", 1, "Z", &owned, None, attempts).await;
    poll_until_pinned_instance(
        &prom,
        MH_REGISTERED_MEETINGS,
        &pod_ip,
        PINNED_METRIC_BOUND,
        POLL,
        "ZOMBIE (registered-on-owned)",
        "the meeting registered on the owned MH (baseline + 1)",
        |v| (v - registered_before - 1.0).abs() < f64::EPSILON,
    )
    .await;
    eprintln!(
        "40/zombie: meeting placed on run-owned slot {} ({owned}) after {} of {attempts} attempts",
        slot.slot, meeting.attempts
    );
    rose(
        &prom,
        MC_OWNED_HEARTBEATS,
        &owned_heartbeats_before,
        "ZOMBIE (owned-heartbeats-before-freeze)",
    )
    .await;

    // Baselines: MH's per-pod releases (the process survives the freeze, so
    // its counters do not reset) and MC's declarations.
    let released_before =
        pinned(&prom, MH_RELEASES_HT, &pod_ip, "ZOMBIE (releases-presence)").await;
    let silence_before = pinned(
        &prom,
        MH_RELEASES_OWNER_SILENCE,
        &pod_ip,
        "ZOMBIE (releases-presence)",
    )
    .await;
    let verdicts_before = pinned(
        &prom,
        MH_RELEASED_VERDICTS,
        &pod_ip,
        "ZOMBIE (verdicts-presence)",
    )
    .await;
    let declarations_before = settled_baseline(
        &prom,
        MC_DECLARATIONS_HT,
        BASELINE_SETTLE_BOUND,
        "the heartbeat_timeout declaration under test",
    )
    .await;

    // FREEZE. From here a panic drops the guard, which sends a best-effort
    // CONT and prints the resume command.
    let frozen = tokio::task::spawn_blocking(move || slot.freeze())
        .await
        .unwrap_or_else(|e| std::panic::resume_unwind(e.into_panic()));
    // PRECONDITIONS, from live values: MC must be able to declare (N, armed
    // per interval) and the CONT must land inside the liveness budget, and
    // the budget must sit inside MH's owner-silence timeout, so the release
    // on resume can only be MC's.
    let needed = n + interval + 2 * frozen.stop_call();
    assert!(
        needed < frozen.budget(),
        "ZOMBIE PRECONDITION (freeze-budget): N {n:?} + interval {interval:?} + 2 x STOP call \
         {:?} = {needed:?} does not fit the live liveness budget {:?}; kubelet would restart \
         the MH before MC can declare it",
        frozen.stop_call(),
        frozen.budget()
    );
    assert!(
        frozen.budget() < owner_silence,
        "ZOMBIE PRECONDITION (owner-silence): the freeze budget {:?} must be shorter than MH's \
         owner-silence timeout {owner_silence:?}, or MH could release on its own clock",
        frozen.budget()
    );

    // MANDATORY FIRST ASSERTION: removed while frozen. The bound is what is
    // left of the budget after reserving the CONT, so a miss fails BEFORE
    // kubelet can restart the MH.
    let remaining = frozen
        .budget()
        .saturating_sub(frozen.elapsed() + frozen.stop_call());
    let gone = owned.clone();
    z.await_handler_set("ZOMBIE removed-while-frozen", remaining, move |sets| {
        sets.current().is_some_and(|v| !v.urls().contains(&gone))
    })
    .await;
    eprintln!(
        "40/zombie: MC removed the frozen MH after {:?} frozen (N {n:?})",
        frozen.elapsed()
    );

    // RESUME: same incarnation (asserted by the guard), re-scraped.
    let slot = frozen.resume(&prom).await;

    rose(
        &prom,
        MC_DECLARATIONS_HT,
        &declarations_before,
        "ZOMBIE (declaration-counted)",
    )
    .await;

    // RELEASED, on THIS pod: exactly one heartbeat_timeout release.
    let released_after = poll_until_pinned_instance(
        &prom,
        MH_RELEASES_HT,
        &slot.pod_ip,
        PINNED_METRIC_BOUND,
        POLL,
        "ZOMBIE (released-on-resume)",
        "one more heartbeat_timeout release",
        |v| v > released_before,
    )
    .await;
    #[expect(
        clippy::float_cmp,
        reason = "counter values of small integers are exact in f64"
    )]
    let exactly_one = released_after - released_before == 1.0;
    assert!(
        exactly_one,
        "ZOMBIE (released-once): the resumed MH must release the meeting exactly once as \
         heartbeat_timeout: before {released_before}, after {released_after}"
    );
    let silence_after = pinned(
        &prom,
        MH_RELEASES_OWNER_SILENCE,
        &slot.pod_ip,
        "ZOMBIE (releases-presence)",
    )
    .await;
    #[expect(
        clippy::float_cmp,
        reason = "counter values of small integers are exact in f64"
    )]
    let silence_unchanged = silence_after == silence_before;
    assert!(
        silence_unchanged,
        "ZOMBIE (not-owner-silence): the release must be MC's, never MH's own owner-silence \
         clock: before {silence_before}, after {silence_after}"
    );
    // MH stopped serving the meeting: its registration is gone, and stays
    // gone (a stale RegisterMeeting buffered across the freeze would have
    // re-created it after the release).
    poll_until_pinned_instance(
        &prom,
        MH_REGISTERED_MEETINGS,
        &slot.pod_ip,
        PINNED_METRIC_BOUND,
        POLL,
        "ZOMBIE (no-longer-held)",
        "back to the pre-placement baseline",
        |v| (v - registered_before).abs() < f64::EPSILON,
    )
    .await;
    let verdicts_after = pinned(
        &prom,
        MH_RELEASED_VERDICTS,
        &slot.pod_ip,
        "ZOMBIE (verdicts-presence)",
    )
    .await;
    eprintln!(
        "40/zombie: release carrier = {} (released verdicts {verdicts_before} -> \
         {verdicts_after})",
        if verdicts_after > verdicts_before {
            "RELEASED heartbeat verdict"
        } else {
            "buffered EndMeeting(HEARTBEAT_TIMEOUT)"
        }
    );
    drop(z);
}

/// `v`'s audio slot naming `peer` is on the handler `handler_ref` names.
fn heard_on(v: &StreamAssignments, peer: u32, handler_ref: u32) -> bool {
    slots_of(v, AUDIO)
        .iter()
        .any(|a| a.sender_id == Some(peer) && a.handler_ref == handler_ref)
}

/// `p`'s current-set ref for the handler at `url` (fails loudly if absent:
/// the handler is expected in the set at this point).
fn handler_ref_of(phase: &str, p: &Participant, url: &str) -> u32 {
    p.sets.ref_for_url(url).unwrap_or_else(|| {
        panic!(
            "{phase}: {} has no current-set entry for the expected handler (set urls {:?})",
            p.label,
            p.sets.urls()
        )
    })
}

// ============================================================================
// NetworkPolicy Tests
// ============================================================================

/// Test that pods with allowed labels can reach the AC service.
/// This is a POSITIVE test to verify NetworkPolicy allows expected traffic.
///
/// The canary uses `app=gc-service` label to match the NetworkPolicy's
/// ingress rules, which allow traffic from gc-service pods.
///
/// If this test fails, it indicates either:
/// 1. The AC service is not running or not healthy
/// 2. The NetworkPolicy ingress rules don't allow gc-service traffic
/// 3. The canary pod deployment failed
#[tokio::test]
#[serial]
async fn test_same_namespace_connectivity() {
    // Deploy a canary pod with labels that match the NetworkPolicy ingress rules
    // The AC NetworkPolicy allows traffic from pods with app=gc-service
    let config = CanaryConfig {
        labels: "app=gc-service,test=network-policy".to_string(),
        ..CanaryConfig::default()
    };

    let canary = CanaryPod::deploy_with_config("dark-tower", config)
        .await
        .expect("Failed to deploy canary pod in dark-tower namespace");

    // Test connectivity to AC service health endpoint
    // Using service DNS name which resolves within the cluster
    let target_url = "http://ac-service:8082/health";
    let can_reach = canary.can_reach(target_url).await;

    // Cleanup before asserting (ensure cleanup even on failure)
    let cleanup_result = canary.cleanup().await;

    // Now assert
    assert!(
        can_reach,
        "Canary pod with app=gc-service label should be able to reach AC service at {}. \
         This validates NetworkPolicy allows expected ingress traffic. \
         If this fails, check: 1) AC service is running, 2) NetworkPolicy ingress rules, \
         3) Pod labels match NetworkPolicy selectors.",
        target_url
    );

    // A failed cleanup leaks a canary pod into the next run: fail loudly,
    // after the primary assertion so it never hides that result.
    if let Err(e) = cleanup_result {
        panic!("canary cleanup failed (pod leaked into dark-tower): {e}");
    }
}

/// Test that NetworkPolicy blocks cross-namespace traffic to AC service.
/// This is a NEGATIVE test to verify NetworkPolicy enforcement.
///
/// Test interpretation:
/// - If ONLY this test passes (positive fails): NetworkPolicy is blocking ALL traffic (misconfigured)
/// - If BOTH tests pass: NetworkPolicy is NOT enforced (security gap!)
/// - If positive passes, negative fails: NetworkPolicy is working correctly
#[tokio::test]
#[serial]
async fn test_network_policy_blocks_cross_namespace() {
    // Use a unique test namespace to ensure isolation
    let test_namespace = "canary-test-isolated";

    // Deploy a canary pod in a DIFFERENT namespace from AC service
    let canary = CanaryPod::deploy(test_namespace).await.unwrap_or_else(|e| {
        panic!(
            "Failed to deploy canary pod in {}: {}. \
                 Ensure kubectl is configured and has permissions to create namespaces/pods.",
            test_namespace, e
        )
    });

    // Try to reach AC service from the different namespace
    // Using fully-qualified service DNS: <service>.<namespace>.svc.cluster.local
    let target_url = "http://ac-service.dark-tower.svc.cluster.local:8082/health";
    let can_reach = canary.can_reach(target_url).await;

    // Cleanup before asserting (ensure cleanup even on failure)
    let cleanup_result = canary.cleanup().await;
    let namespace_result = cleanup_test_namespace(test_namespace);

    // Assert that cross-namespace traffic is blocked
    assert!(
        !can_reach,
        "Canary pod in '{}' namespace should NOT be able to reach AC service at {}. \
         This indicates NetworkPolicy is NOT enforced - SECURITY GAP! \
         Ensure NetworkPolicy is deployed in the dark-tower namespace to restrict ingress. \
         Expected: Connection blocked/timeout. Got: Connection succeeded.",
        test_namespace, target_url
    );

    // A failed cleanup leaks a pod or namespace into the next run, which then
    // fails confusingly: fail loudly, after the primary assertion so it never
    // hides that result.
    if let Err(e) = cleanup_result {
        panic!("canary cleanup failed (pod leaked into {test_namespace}): {e}");
    }
    if let Err(e) = namespace_result {
        panic!("test namespace cleanup failed ({test_namespace} leaked): {e}");
    }
}

/// Test that NetworkPolicy blocks cross-namespace traffic to GC's HTTP port.
///
/// GC's HTTP port is externally reachable (the ingress gateway in production,
/// the NodePort in Kind), which once tempted an admit-all `from: []` rule on
/// it, and that admitted every pod in every namespace too. The Kind overlay's
/// NodePort rule admits only NON-pod sources, so a pod in another namespace
/// must still be blocked. GC's allowed external path is exercised by every
/// env-test that calls GC through `ENV_TEST_GC_URL` (the NodePort).
#[tokio::test]
#[serial]
async fn test_network_policy_blocks_cross_namespace_to_gc() {
    let test_namespace = "canary-test-isolated";

    let canary = CanaryPod::deploy(test_namespace).await.unwrap_or_else(|e| {
        panic!(
            "Failed to deploy canary pod in {}: {}. \
                 Ensure kubectl is configured and has permissions to create namespaces/pods.",
            test_namespace, e
        )
    });

    let target_url = "http://gc-service.dark-tower.svc.cluster.local:8080/health";
    let can_reach = canary.can_reach(target_url).await;

    // Cleanup before asserting (ensure cleanup even on failure)
    let cleanup_result = canary.cleanup().await;
    let namespace_result = cleanup_test_namespace(test_namespace);

    assert!(
        !can_reach,
        "Canary pod in '{}' namespace should NOT be able to reach GC service at {}. \
         GC's NetworkPolicy admits external clients only via the ingress gateway (prod) \
         or non-pod sources (Kind overlay); a pod in another namespace reaching it is a \
         SECURITY GAP. Expected: Connection blocked/timeout. Got: Connection succeeded.",
        test_namespace, target_url
    );

    // A failed cleanup leaks a pod or namespace into the next run: fail
    // loudly, after the primary assertion so it never hides that result.
    if let Err(e) = cleanup_result {
        panic!("canary cleanup failed (pod leaked into {test_namespace}): {e}");
    }
    if let Err(e) = namespace_result {
        panic!("test namespace cleanup failed ({test_namespace} leaked): {e}");
    }
}

/// Delete a test namespace created by a NetworkPolicy test. Errors are
/// returned, never discarded: a leaked namespace breaks the next run.
fn cleanup_test_namespace(namespace: &str) -> Result<(), String> {
    use std::process::Command;

    // Only cleanup namespaces we created for testing
    if !namespace.starts_with("canary-test") {
        return Err(format!(
            "refusing to delete '{namespace}': not a canary-test namespace"
        ));
    }

    let output = Command::new("kubectl")
        .args(["delete", "namespace", namespace, "--ignore-not-found=true"])
        .output()
        .map_err(|e| format!("kubectl could not run: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "kubectl delete namespace exited {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}
