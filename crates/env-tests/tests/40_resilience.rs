//! Resilience tests: NetworkPolicy enforcement (canary pods), the MH pool /
//! process-injector smoke legs that prove the story 4 resilience fixture end
//! to end, the handler-loss legs — crash with replacement (MC-B2 survivor arm
//! extended by MC-C), zombie (MC-B2) and the stranded pair (MC-C) — (ADR-0039
//! §2, §3, §4, §6, §9; R-1, R-2, R-4, R-6, R-10, R-39).
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
//! Handler-loss legs (crash with replacement, zombie, stranded pair): see
//! their section below. Since MC-C, MC asks GC for a replacement after EVERY
//! removal, so each such leg ends its participants INSIDE the leg and waits
//! for MC's meetings to drain before `run_leg` tears the owned MH down —
//! otherwise MC would react to the teardown by placing the still-live meeting
//! on a baseline MH and perturb later baselines.
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
    format_instance_map, heartbeat_interval_of, mc_heartbeat_deadline, mc_replacement_retry_budget,
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
const MC_MIGRATIONS_COMPLETED_HT: &str = r#"sum by (instance) (mc_media_handler_migrations_total{trigger="heartbeat_timeout",outcome="completed"})"#;
// Summed over gc_view (never pinned: GC's view of the killed MH is healthy or
// stale depending on timing) and trigger.
const GC_REPLACEMENTS_GRANTED: &str =
    r#"sum by (instance) (gc_mh_replacement_requests_total{outcome="granted"})"#;
// Per instance (instance_counter_map requires the label); summed by the caller.
const MC_MEETINGS_ACTIVE: &str = "sum by (instance) (mc_meetings_active)";

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

/// R-1 (survivor arm AND replacement), R-4, R-6, R-39: kill the run-owned MH
/// carrying a two-handler meeting's media. MC declares it dead
/// (heartbeat_timeout), removes it from the set, and re-places the pair's
/// edges onto the SURVIVOR, where media resumes — without waiting on GC. GC
/// counts the in-place restart's registration as superseded{match=both}
/// exactly once, and no load report from an unknown handler.
///
/// REPLACEMENT (story 4 MC-C, crash = break-before-make): MC then asks GC for
/// a replacement. Per participant, the final set holds a handler_ref NOT in
/// its pre-kill set, at the pre-kill size; the set without the dead ref came
/// BEFORE the first set naming the new ref; the new entry carries a
/// `media_dial` token and MH admits a dial with it (MH admits only a meeting
/// it holds a key for, so this is register-before-token on the live
/// cluster). MC counted `completed` and GC counted `granted`. Refs, never
/// urls: the replacement may be the restarted owned incarnation at the same
/// address.
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
async fn mc_c_crash_moves_media_to_the_survivor_then_gc_replaces_the_handler() {
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

    let retry_budget = mc_replacement_retry_budget(&prom).await;

    let mut slot = pool.up_scraped(&prom).await;
    let owned = slot.address.clone();
    let attempts = live_placement_attempts(&pool).await;
    let owned_heartbeats_before = prom.instance_counter_map(MC_OWNED_HEARTBEATS).await;
    let meetings_before = settled_meetings_active(&prom).await;

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

    // MC-C baselines: the replacement under test (MC's completed migrations,
    // GC's grants summed over the serving replicas), and each participant's
    // pre-kill refs and the dead handler's ref.
    let completed_before = settled_baseline(
        &prom,
        MC_MIGRATIONS_COMPLETED_HT,
        BASELINE_SETTLE_BOUND,
        "the completed migration under test",
    )
    .await;
    let granted_before = settled_baseline(
        &prom,
        GC_REPLACEMENTS_GRANTED,
        BASELINE_SETTLE_BOUND,
        "the replacement grant under test",
    )
    .await;
    // Presence on every serving replica (zero-initialised at GC start).
    let _present_on_every_replica = sum_over_pods(
        &granted_before,
        &gc_pods.serving,
        GC_REPLACEMENTS_GRANTED,
        "CRASH (gc-presence)",
    );
    let pre_kill: Vec<PreKill> = ps
        .iter()
        .map(|p| PreKill::of("CRASH pre-kill", p, &owned))
        .collect();

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

    // REMOVAL, per entity: each participant's set becomes {survivor} FIRST
    // (break-before-make: the replacement is added only once it registered,
    // and `await_handler_set` checks after every absorbed set, so the
    // intermediate {survivor} set is observed even if the replacement's set
    // is already queued behind it). Bound: one detection (N, armed per
    // heartbeat interval) plus the view margin, from now; the declaration may
    // already have happened during the restart wait.
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

    // REPLACEMENT (MC-C), per entity: a new ref at the pre-kill size, after
    // the removal set; its token admits a dial.
    let replacement_bound = removal_bound + retry_budget + VIEW_MARGIN;
    for (p, pre) in ps.iter_mut().zip(&pre_kill) {
        let new_ref = await_replacement("CRASH replacement", p, pre, replacement_bound).await;
        let url = p.sets.url_of(new_ref);
        drop(reach("CRASH replacement (token admits a dial)", p, &url).await);
    }
    rose(
        &prom,
        MC_MIGRATIONS_COMPLETED_HT,
        &completed_before,
        "CRASH (migration-completed)",
    )
    .await;
    // GC granted it: some replica's `granted` (summed over gc_view and
    // trigger) rose, with the serving replica set unchanged (no reset).
    rose(
        &prom,
        GC_REPLACEMENTS_GRANTED,
        &granted_before,
        "CRASH (gc-granted)",
    )
    .await;
    assert_eq!(
        live_gc_pods().await.names(),
        gc_names_before,
        "CRASH (gc-replica-set-changed): the serving GC pods changed, so the per-replica \
         counters may have reset"
    );

    // R-28 on the restarted incarnation (and whatever the replacement is).
    assert_addresses_unique().await;

    // Hygiene: end the meeting INSIDE the leg (module doc).
    drop(on_owned);
    drop(on_survivor);
    drop(ps);
    drop(meeting);
    await_meetings_drained(&prom, meetings_before, "CRASH").await;
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
    let meetings_before = settled_meetings_active(&prom).await;

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
    // MC-C acquires a replacement for the one-handler meeting here too; this
    // leg pins nothing about set size (only that the zombie left it). End the
    // meeting INSIDE the leg (module doc).
    drop(z);
    drop(meeting);
    await_meetings_drained(&prom, meetings_before, "ZOMBIE").await;
    assert_addresses_unique().await;
}

// ============================================================================
// MC-C: the stranded pair (R-2)
// ============================================================================

/// R-2: A and B reach ONLY the run-owned MH of a two-handler meeting (never
/// the survivor), and frames are proven on it both ways. The owned MH is
/// killed. MC removes it (break-before-make) and GC grants a replacement.
/// Before the replacement is reached, neither hears the other on the
/// survivor (neither ever connected to it: the kill stranded the pair).
/// Recovery is the join path, with no special case: both reach the
/// replacement with its token, MC places their edges on it, and frames flow
/// on the replacement's transports both ways; neither is reported
/// unreachable to the other on the final view.
#[tokio::test]
#[serial]
async fn mc_c_stranded_pair_hears_each_other_on_the_replacement() {
    MhPool::run_leg(|pool, prom_url| async move { stranded_leg(pool, &prom_url).await }).await;
}

async fn stranded_leg(pool: Arc<MhPool>, prom_url: &str) {
    let prom = PrometheusClient::new(prom_url);
    let cluster = connected_cluster().await;
    let auth = AuthClient::new(&cluster.ac_base_url);
    let gc = GcClient::new(&cluster.gc_base_url);
    let n = mc_heartbeat_deadline(&prom).await;
    let interval = heartbeat_interval_of(n);
    let view_bound = settle_window(&cluster).await + VIEW_MARGIN;
    let retry_budget = mc_replacement_retry_budget(&prom).await;

    let mut slot = pool.up_scraped(&prom).await;
    let owned = slot.address.clone();
    let attempts = live_placement_attempts(&pool).await;
    let meetings_before = settled_meetings_active(&prom).await;

    let (a, meeting) = place_on_owned(
        &auth,
        &gc,
        "MC-C stranded pair",
        2,
        "A",
        &owned,
        Some(2),
        attempts,
    )
    .await;
    eprintln!(
        "40/stranded: meeting placed on run-owned slot {} ({owned}) after {} of {attempts} \
         attempts",
        slot.slot, meeting.attempts
    );
    let b = meeting.join(1, "B", &JoinDeclaration::default()).await;
    let mut ps = vec![a, b];
    let survivor = ps[0]
        .offered()
        .into_iter()
        .find(|u| *u != owned)
        .expect("placement_hit guarantees exactly one other member");
    let ids = [ps[0].sender_id(), ps[1].sender_id()];

    // Both reach ONLY the owned MH; prove frames on it both ways.
    let mut on_owned = Vec::new();
    for p in &ps {
        on_owned.push(reach("STRANDED pre-kill", p, &owned).await);
    }
    for (i, p) in ps.iter_mut().enumerate() {
        p.declare(&[SLOT[i]], &[]).await;
    }
    for i in 0..2 {
        let (peer, on) = (
            ids[1 - i],
            handler_ref_of("STRANDED pre-kill", &ps[i], &owned),
        );
        ps[i]
            .converge("STRANDED pre-kill", view_bound, move |v, _| {
                v.is_some_and(|v| heard_on(v, peer, on))
            })
            .await;
    }
    for (r, s) in [(0_usize, 1_usize), (1, 0)] {
        let phase = format!("STRANDED pre-kill {}<-{}", ps[r].label, ps[s].label);
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
    let pre_kill: Vec<PreKill> = ps
        .iter()
        .map(|p| PreKill::of("STRANDED pre-kill", p, &owned))
        .collect();
    let survivor_ref = handler_ref_of("STRANDED pre-kill", &ps[0], &survivor);

    let (uid, restarts) = (slot.pod_uid.clone(), slot.restart_count);
    slot.kill(&prom).await;
    assert_eq!(
        slot.restart_count,
        restarts + 1,
        "STRANDED: KILL restarts in place"
    );
    assert_eq!(slot.pod_uid, uid, "STRANDED: same pod, never a pod delete");
    drop(on_owned); // the dead incarnation's sessions

    let removal_bound = n + interval + VIEW_MARGIN;
    for p in ps.iter_mut() {
        let want = vec![survivor.clone()];
        p.await_handler_set("STRANDED removal", removal_bound, move |sets| {
            sets.current().is_some_and(|v| v.urls() == want)
        })
        .await;
    }
    // NON-VACUITY: the pair is stranded — neither hears the other on the
    // survivor, which neither ever connected to.
    for i in 0..2 {
        assert!(
            !heard_on(ps[i].view(), ids[1 - i], survivor_ref),
            "STRANDED (non-vacuity): {} must not hear its peer on the survivor before the \
             replacement is reached",
            ps[i].label
        );
    }

    let replacement_bound = removal_bound + retry_budget + VIEW_MARGIN;
    let mut new_refs = Vec::new();
    for (p, pre) in ps.iter_mut().zip(&pre_kill) {
        new_refs.push(await_replacement("STRANDED replacement", p, pre, replacement_bound).await);
    }
    let mut on_new = Vec::new();
    for (p, new_ref) in ps.iter().zip(&new_refs) {
        let url = p.sets.url_of(*new_ref);
        on_new.push(reach("STRANDED replacement", p, &url).await);
    }
    for i in 0..2 {
        let (peer, on) = (ids[1 - i], new_refs[i]);
        ps[i]
            .converge("STRANDED recovered", view_bound, move |v, _| {
                v.is_some_and(|v| heard_on(v, peer, on))
            })
            .await;
    }
    for (r, s) in [(0_usize, 1_usize), (1, 0)] {
        let phase = format!("STRANDED replacement {}<-{}", ps[r].label, ps[s].label);
        let got = bind_until_received(
            &phase,
            on_new[r].connection(),
            &[(on_new[s].connection(), MARK[s])],
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
    for p in &ps {
        assert!(
            p.view().unreachable_sender_ids.is_empty(),
            "STRANDED (reachable): {} is told a peer is unreachable on the final view",
            p.label
        );
    }

    assert_addresses_unique().await;
    drop(on_new);
    drop(ps);
    drop(meeting);
    await_meetings_drained(&prom, meetings_before, "STRANDED").await;
}

/// One participant's pre-kill handler set: its refs and the dead handler's.
struct PreKill {
    refs: Vec<u32>,
    dead_ref: u32,
}

impl PreKill {
    fn of(phase: &str, p: &Participant, dead_url: &str) -> Self {
        Self {
            refs: p
                .sets
                .current()
                .map(|v| v.refs())
                .unwrap_or_else(|| panic!("{phase}: {} has no handler set", p.label)),
            dead_ref: handler_ref_of(phase, p, dead_url),
        }
    }
}

/// Wait for `p`'s set to hold a ref NOT in its pre-kill set at the pre-kill
/// size (the replacement), then assert break-before-make from its history:
/// the first set without the dead ref precedes the first set naming the new
/// ref. Returns the new ref.
async fn await_replacement(
    phase: &str,
    p: &mut Participant,
    pre: &PreKill,
    bound: Duration,
) -> u32 {
    let (pre_refs, want_len) = (pre.refs.clone(), pre.refs.len());
    p.await_handler_set(phase, bound, move |sets| {
        sets.current().is_some_and(|v| {
            v.refs().len() == want_len && v.refs().iter().any(|r| !pre_refs.contains(r))
        })
    })
    .await;
    let current = p.sets.current().expect("awaited above");
    let new_ref = current
        .refs()
        .into_iter()
        .find(|r| !pre.refs.contains(r))
        .expect("awaited above");
    assert!(
        p.sets.dial_token(new_ref).is_some(),
        "{phase}: {}'s entry for the replacement carries no media_dial token",
        p.label
    );
    // A set STRICTLY BETWEEN the last one naming the dead handler and the
    // first naming the replacement, holding neither: break-before-make. (The
    // FIRST set without the dead ref would be vacuous: under register-before-
    // token the join's set may not name it yet.)
    let history = p.sets.history();
    let last_with_dead = history
        .iter()
        .rposition(|v| v.refs().contains(&pre.dead_ref))
        .unwrap_or_else(|| {
            panic!(
                "{phase}: {} never received a set naming the dead handler",
                p.label
            )
        });
    let added_at = history
        .iter()
        .position(|v| v.refs().contains(&new_ref))
        .expect("the current set names it");
    assert!(
        added_at > last_with_dead + 1,
        "{phase} (break-before-make): {} got no set between the last naming the dead handler \
         (#{last_with_dead}) and the first naming the replacement (#{added_at})",
        p.label
    );
    let between = &history[last_with_dead + 1];
    assert!(
        !between.refs().contains(&pre.dead_ref) && !between.refs().contains(&new_ref),
        "{phase} (break-before-make): {}'s set #{} must hold neither the dead handler nor the \
         replacement: {:?}",
        p.label,
        last_with_dead + 1,
        between.refs()
    );
    new_ref
}

/// MC's active meetings, fleet-wide.
async fn mc_meetings_active(prom: &PrometheusClient) -> f64 {
    prom.instance_counter_map(MC_MEETINGS_ACTIVE)
        .await
        .values()
        .sum()
}

/// The pre-placement value of [`mc_meetings_active`], SETTLED: two reads one
/// poll apart agreeing (a meeting ending from an earlier leg must not be
/// mid-flight in the baseline).
async fn settled_meetings_active(prom: &PrometheusClient) -> f64 {
    let deadline = std::time::Instant::now() + BASELINE_SETTLE_BOUND;
    let mut previous = mc_meetings_active(prom).await;
    loop {
        tokio::time::sleep(POLL).await;
        let now = mc_meetings_active(prom).await;
        #[expect(
            clippy::float_cmp,
            reason = "gauge sums of small integers are exact in f64"
        )]
        let settled = now == previous;
        if settled {
            return now;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "HYGIENE PRECONDITION (meetings-baseline-unsettled): mc_meetings_active did not \
             settle within {BASELINE_SETTLE_BOUND:?} ({previous} -> {now}) — environment, not \
             an MC defect"
        );
        previous = now;
    }
}

/// LEG HYGIENE, NOT EVIDENCE. After a handler-loss leg dropped its
/// participants, wait until MC's active meetings are back to the settled
/// pre-placement count, so `run_leg`'s owned-MH teardown cannot make MC
/// acquire a replacement for a still-live meeting and place it on a
/// baseline MH (perturbing later legs' baselines).
///
/// Why the README §Evidence rule's exception applies: the fixture has no
/// per-entity leave/end signal (no clean-leave API; dropping a participant is
/// a disconnect, ended by MC after its disconnect grace period), so the only
/// observable is the shared fleet gauge. RESIDUAL: a vacuous pass is possible
/// (another meeting ending can satisfy the comparison while ours is still in
/// grace), so nothing here proves anything about MC; it only makes the
/// common case clean. The bound is the shared failure-only
/// `PINNED_METRIC_BOUND`: MC publishes no gauge for its disconnect grace
/// period (`MC_DISCONNECT_GRACE_PERIOD_SECONDS`, default 30 s) to derive one
/// from.
async fn await_meetings_drained(prom: &PrometheusClient, before: f64, phase: &str) {
    let deadline = std::time::Instant::now() + PINNED_METRIC_BOUND;
    loop {
        let now = mc_meetings_active(prom).await;
        if now <= before {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "{phase} HYGIENE (meetings-not-drained): MC still holds {now} active meetings \
             (settled before the leg: {before}) {PINNED_METRIC_BOUND:?} after the participants \
             left — environment hygiene for the next leg, not an MC defect"
        );
        tokio::time::sleep(POLL).await;
    }
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
