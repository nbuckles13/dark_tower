//! Env-test: MH egress-budget exhaustion on the live Kind cluster — scenario
//! S9, MH half (story 2 task 8; R-19; ADR-0036 §11 "Admission control is keyed
//! on egress bandwidth, not connection count").
//!
//! ONE `#[tokio::test]` owns ONE meeting and drives it past the DEPLOYED stream
//! ceiling through REAL GC/MC joins, then asserts MH refused admission and
//! counted it. Every failure message names its phase: PRECONDITION /
//! JOIN-FANOUT / ADMISSION / GAUGES — so a flaky join is triaged as the
//! environment, never as admission logic.
//!
//! # Why real joins, not a direct `RegisterMeeting`
//!
//! Because the path under test is MC-DRIVEN admission, end to end: GC places
//! the meeting, MC computes each subscriber's slots and pushes the policy, and
//! MH refuses the push that would cross its ceiling. A direct `RegisterMeeting`
//! would skip the half that decides how many streams a real meeting asks for.
//!
//! (It is no longer that the suite CANNOT call MH directly. Since story 2 task
//! 11, Layer 7 forwards each MH pod's gRPC port and
//! `29_mh_meeting_teardown.rs` calls it with the Kind meeting-controller
//! credential. That relaxes the credential convention only: ADR-0028's crate
//! layering is unchanged — the suite still links no service crate.)
//!
//! # Sizing — deterministic, never a function of placement luck
//!
//! Nothing here is a literal: the ceiling is read from each running MH's
//! `mh_media_egress_stream_ceiling`, the per-subscriber slot cap from the
//! deployed MC ConfigMap. Each participant declares `S = min(P - 1,
//! MC_MAX_RECEIVE_SLOTS)` audio slots, and `P` is the smallest participant
//! count satisfying the pigeonhole condition of the shared-handler edge model
//! (story 2 task 20, ADR-0036 §9): every participant connects to every handler
//! it is offered, so every subscriber fills all `S` slots, the meeting totals
//! `P * S` streams across at most two handlers, and whichever edge chooser MC
//! uses one handler holds at least half. Require `P * S > 2 * ceiling`. (With
//! MC's co-locating chooser and full connectivity every stream is on ONE
//! handler, which exceeds the ceiling sooner; the half bound is what holds for
//! ANY choice, so it is the one sized against.)
//!
//! Task 6's round-robin placement needed a second, co-handler-only condition;
//! it is retired with that model. The sizing (`size_s9_meeting`) and the
//! economic participant limit (`MAX_S9_PARTICIPANTS`) live in
//! `env_tests::fixtures::egress_admission`, their ONE home, because
//! `01_mh_deployment_config.rs` bounds the Kind budget from ABOVE against the
//! same limit. Illustration only, derived at run time and printed as the `S9:`
//! line: with the Kind overlay's budget and `MC_MAX_RECEIVE_SLOTS` = 8 the
//! ceiling is 100 and P = 26 (26 x 8 = 208 > 200) — 26 AC registrations and
//! about 52 MH sessions, and the handler genuinely carries about 100 concurrent
//! egress streams during this test.
//!
//! Every participant opens an MH session to every handler URL it was offered.
//! That is load-bearing, not incidental: MC routes only through connectivity MH
//! OBSERVES, so a participant that opened no session would have no edges at
//! all and the meeting could never reach the ceiling.
//!
//! # Which guard trips — asserted, not assumed
//!
//! MH checks the stream ceiling BEFORE the aggregate edge bound, and refuses to
//! boot when the ceiling exceeds that bound; the per-meeting egress-stream
//! bound is a request-validation step that would count `rejected_invalid`, a
//! DIFFERENT label. The PRECONDITION phase asserts from the live ConfigMap that
//! neither other bound can trip first, and the ADMISSION phase keys on the
//! exact `rejected_stream_ceiling` label alone (never summed with
//! `mh_media_policy_applies_total`). The rejecting instance(s) are identified
//! after the fact as those whose counter moved — never a hardcoded `mh-N` —
//! and the positive control requires ADMITTED to have moved on a REJECTING
//! instance: the same handler both admitted and refused. Assertions are
//! deltas from SETTLED baselines, never an exact admission count (existing
//! load can make the first refusal land before `ceiling + 1`).
//!
//! # Long-lived pods (ADR-0038)
//!
//! Nothing restarts an unchanged MH, so a handler can carry occupancy from
//! earlier runs. Sizing is occupancy-INDEPENDENT (existing load only makes the
//! ceiling easier to cross; `P x S > 2 x ceiling` holds for any placement). The
//! per-instance `mh_media_egress_edges` / `mh_media_registered_meetings` read at
//! PRECONDITION is DIAGNOSTIC: it gates only that every instance has room for
//! one participant's slots (else the positive control could starve), and it is
//! printed in every environment message.
//!
//! # No wall-clock gate
//!
//! Counters are polled to a bound (`poll_until_any_instance_above`); nothing
//! sleeps-then-asserts, and no latency is asserted. The rejection RATIO gauge
//! is asserted for presence and range only: MH is scraped on its job's
//! `scrape_interval` (`infra/kubernetes/observability/prometheus.yml`), which
//! is not aligned with the ratio's window buckets, so a transient ratio value
//! can be invisible.
//!
//! # Cleanup — the ratchet, for meetings nobody ends
//!
//! MH releases a meeting's streams when its MC calls `EndMeeting` (story 2
//! R-20). Streams of a meeting whose MC never COMPLETES `EndMeeting` are never
//! released (`docs/TODO.md`, "A meeting whose MC never sends `EndMeeting` is
//! never reclaimed"), and a handler at its ceiling drops out of GC placement.
//! So the test ends by closing every MC session cleanly: each close but the
//! last re-pushes a shrinking policy, and the last ends the meeting, whose
//! `EndMeeting` releases it on every handler. A panic before cleanup drops the
//! sessions instead; MC ends the meeting when their reconnect grace expires and
//! releases it the same way. Only a meeting whose MC never COMPLETES
//! `EndMeeting` keeps its streams. If the FIRST join fails with 503, or an
//! instance lacks headroom at PRECONDITION, that is this leak — an environment
//! condition reported with the observed occupancy, not a GC bug and not a
//! reason to rebuild the cluster.

#![cfg(feature = "flows")]

use std::time::Duration;

use env_tests::cluster::ClusterConnection;
use env_tests::fixtures::auth_client::UserRegistrationRequest;
use env_tests::fixtures::egress_admission::{
    max_s9_feasible_ceiling, size_s9_meeting, MAX_S9_PARTICIPANTS,
};
use env_tests::fixtures::gc_client::{CreateMeetingRequest, GcClient, GcClientError};
use env_tests::fixtures::kube::{
    configmap_key, configmap_name_for_all, configmap_u64, REDEPLOY_HINT,
};
use env_tests::fixtures::mc_session::{self, McSession, MhSession};
use env_tests::fixtures::metrics::{
    gauge_by_instance_present, instances_exceeding_baseline, poll_until_any_instance_above,
    settled_baseline, InstanceCounters,
};
use env_tests::fixtures::{AuthClient, PrometheusClient};
use proto_gen::dark_tower::signaling::v1::{
    client_message, ClientMessage, MediaKind, ReceiveCapability, ReceiveSlot,
};

/// Both Kind MH instances must publish every admission gauge.
const MH_INSTANCE_COUNT: usize = 2;

/// Scrape-convergence bound for eagerly-published gauges.
const GAUGE_PRESENT_BOUND: Duration = Duration::from_secs(60);

/// Rig bound on MC pushing, MH refusing, and Prometheus scraping the count.
const ADMISSION_BOUND: Duration = Duration::from_secs(120);

/// Smallest `P` satisfying the pigeonhole condition (see the module doc), with
/// the slots each participant declares — or a PRECONDITION failure naming the
/// arithmetic and the knob.
fn size_meeting(ceiling: u64, slot_cap: u64) -> (u64, u64) {
    size_s9_meeting(ceiling, slot_cap).unwrap_or_else(|| {
        panic!(
            "PRECONDITION: no meeting of at most {MAX_S9_PARTICIPANTS} participants (slot cap \
             {slot_cap}) exceeds a stream ceiling of {ceiling} on some handler (needs P x S > \
             2 x ceiling; the largest feasible ceiling is {}). Lower MH_EGRESS_BUDGET_BPS in \
             infra/kubernetes/overlays/kind/services/mh-service/configmap-egress-budget-patch.yaml \
             — 01_mh_deployment_config.rs asserts this bound in seconds.",
            max_s9_feasible_ceiling(slot_cap)
        )
    })
}

/// Per-instance occupancy against ceiling, for the environment messages:
/// `instance: edges/ceiling streams, registered/limit meetings`. Diagnostic
/// only — sizing never subtracts occupancy (module doc).
fn occupancy_report(
    ceilings: &InstanceCounters,
    edges: &InstanceCounters,
    registered: &InstanceCounters,
    registered_limit: &InstanceCounters,
) -> String {
    let mut instances: Vec<&String> = ceilings.keys().collect();
    instances.sort();
    instances
        .iter()
        .map(|i| {
            let get = |m: &InstanceCounters| {
                m.get(*i)
                    .map_or_else(|| "?".to_string(), |v| format!("{v}"))
            };
            format!(
                "{i}: {}/{} streams, {}/{} meetings",
                get(edges),
                get(ceilings),
                get(registered),
                get(registered_limit)
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn admission_promql(outcome: &str) -> String {
    format!("sum by (instance) (mh_media_stream_admission_total{{outcome=\"{outcome}\"}})")
}

struct Participant {
    session: McSession,
    _media: Vec<MhSession>,
}

#[tokio::test]
async fn test_mh_refuses_admission_past_the_deployed_stream_ceiling() {
    let cluster = ClusterConnection::new()
        .await
        .expect("PRECONDITION: failed to connect to cluster - ensure port-forwards are running");
    cluster
        .check_ac_health()
        .await
        .expect("PRECONDITION: AC must be running");
    cluster
        .check_gc_health()
        .await
        .expect("PRECONDITION: GC must be running");
    let prom = PrometheusClient::new(&cluster.prometheus_base_url);

    // ---- PRECONDITION: read everything from the live system ----------------
    let ceilings = gauge_by_instance_present(
        &prom,
        "mh_media_egress_stream_ceiling",
        MH_INSTANCE_COUNT,
        GAUGE_PRESENT_BOUND,
    )
    .await;
    // Size against the LARGEST ceiling, so whichever handler a stream lands on
    // is exceeded.
    let ceiling = ceilings
        .values()
        .map(|v| {
            assert!(
                v.is_finite() && *v >= 2.0,
                "PRECONDITION: a published stream ceiling of {v} is below MH's refuse-boot floor"
            );
            // Integral by construction (MH publishes a u32).
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let c = *v as u64;
            c
        })
        .max()
        .expect("PRECONDITION: at least one ceiling");
    // Resolved ONCE per test, from EVERY instance (they must agree): the live,
    // content-addressed generations the pods run (ADR-0038 §2).
    let mc_config = configmap_name_for_all(&["mc-0", "mc-1"], "mc-service-config");
    let mh_config = configmap_name_for_all(&["mh-0", "mh-1"], "mh-service-config");
    let slot_cap = configmap_u64(&mc_config, "MC_MAX_RECEIVE_SLOTS");
    let (participants, slots) = size_meeting(ceiling, slot_cap);

    let per_meeting = configmap_u64(&mh_config, "MH_MAX_EGRESS_STREAMS_PER_MEETING");
    assert!(
        per_meeting >= participants * slots,
        "PRECONDITION: MH_MAX_EGRESS_STREAMS_PER_MEETING={per_meeting} is below this meeting's \
         worst case of {} streams on one handler; the per-meeting bound (rejected_invalid) could \
         trip instead of the stream ceiling and make this test vacuous.",
        participants * slots
    );
    let edge_bound = configmap_u64(&mh_config, "MH_MAX_TOTAL_EGRESS_EDGES");
    assert!(
        edge_bound >= ceiling,
        "PRECONDITION: MH_MAX_TOTAL_EGRESS_EDGES={edge_bound} is below the stream ceiling \
         {ceiling}; MH refuses to boot in that state, so the running pods predate the ConfigMap; \
         {REDEPLOY_HINT}."
    );
    eprintln!(
        "S9: ceiling={ceiling} slot_cap={slot_cap} -> {participants} participants x {slots} slots"
    );

    // OCCUPANCY, diagnostic only (pods persist across runs, ADR-0038: nothing
    // restarts an unchanged workload). Sizing above is occupancy-independent —
    // existing load only makes the ceiling EASIER to cross — but the meeting
    // lands on ONE handler MC chooses by rotation, and a handler already without
    // room for even one participant's slots could refuse the whole meeting
    // before any ADMITTED decision, starving the positive control. So EVERY
    // instance needs headroom >= `slots`.
    let edges = gauge_by_instance_present(
        &prom,
        "mh_media_egress_edges",
        MH_INSTANCE_COUNT,
        GAUGE_PRESENT_BOUND,
    )
    .await;
    let registered = gauge_by_instance_present(
        &prom,
        "mh_media_registered_meetings",
        MH_INSTANCE_COUNT,
        GAUGE_PRESENT_BOUND,
    )
    .await;
    let registered_limit = gauge_by_instance_present(
        &prom,
        "mh_media_registered_meetings_limit",
        MH_INSTANCE_COUNT,
        GAUGE_PRESENT_BOUND,
    )
    .await;
    let occupancy = occupancy_report(&ceilings, &edges, &registered, &registered_limit);
    eprintln!("S9 occupancy (gauges): {occupancy}");
    // An instance with a ceiling but NO edges series is not "empty": MH publishes
    // `mh_media_egress_edges` when its session actor is built, so absence is an
    // environment fault — never read as zero occupancy (which would pass the
    // headroom check below on no data).
    for i in ceilings.keys() {
        assert!(
            edges.contains_key(i),
            "PRECONDITION: no mh_media_egress_edges series for {i}, which publishes a stream \
             ceiling — MH publishes the edges gauge when its session actor is built, so this \
             is a scrape/startup fault, not zero occupancy ({occupancy})."
        );
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "slots is a per-participant slot count, far below 2^53"
    )]
    let full: Vec<&String> = ceilings
        .iter()
        .filter(|(i, c)| *c - edges[*i] < slots as f64)
        .map(|(i, _)| i)
        .collect();
    assert!(
        full.is_empty(),
        "PRECONDITION: MH instance(s) {full:?} have less than {slots} streams of headroom \
         before S9 starts (test-side check over the published gauges — {occupancy}). A handler \
         holds streams no live meeting releases: a meeting's MC never completed EndMeeting \
         (compare mh_media_registered_meetings with pod uptime). An ENVIRONMENT/leak condition, \
         not an admission defect."
    );

    let admitted_baseline = settled_baseline(
        &prom,
        &admission_promql("admitted"),
        ADMISSION_BOUND,
        "the admission assertion under test",
    )
    .await;
    let rejected_baseline = settled_baseline(
        &prom,
        &admission_promql("rejected_stream_ceiling"),
        ADMISSION_BOUND,
        "the admission assertion under test",
    )
    .await;

    // ---- JOIN-FANOUT: distinct users, one meeting, real GC -> MC joins ------
    let auth = AuthClient::new(&cluster.ac_base_url);
    let gc = GcClient::new(&cluster.gc_base_url);
    let mut users = Vec::new();
    for i in 0..participants {
        let request = UserRegistrationRequest::unique(format!("Egress Admission {i}"));
        let display = request.display_name.clone();
        let registered = auth.register_user(&request).await.unwrap_or_else(|e| {
            panic!("JOIN-FANOUT: AC did not register participant {i}: {e} (environment, not admission)")
        });
        users.push((registered.access_token, display));
    }
    let created = gc
        .create_meeting(
            &users[0].0,
            &CreateMeetingRequest::new("Egress Admission Meeting"),
        )
        .await
        .expect("JOIN-FANOUT: GC should create the meeting");

    let mut ps: Vec<Participant> = Vec::new();
    for (i, (token, display)) in users.iter().enumerate() {
        let gc_join = match gc.join_meeting(&created.meeting_code, token).await {
            Ok(join) => join,
            Err(GcClientError::RequestFailed { status: 503, body }) if i == 0 => panic!(
                "PRECONDITION: GC answered 503 to the FIRST join (no handler has headroom in \
                 GC's view). MH gauges read before the test: {occupancy}. GC's view comes from \
                 load reports and can lag the gauges, so the two need not agree. Streams no live \
                 meeting releases: a meeting never released its streams (MC never completed \
                 EndMeeting). An environment/leak condition, not a GC bug. Body: {body}"
            ),
            Err(e) => panic!("JOIN-FANOUT: GC join for participant {i} failed: {e}"),
        };
        let mc_url = gc_join
            .mc_assignment
            .webtransport_endpoint
            .clone()
            .expect("JOIN-FANOUT: MC assignment must include webtransport_endpoint");
        let label = format!("P{i}");
        let mut session = McSession::connect(&mc_url).await;
        let join = mc_session::mc_join(
            &mut session,
            &gc_join.meeting_id.to_string(),
            &gc_join.token,
            display,
            &label,
        )
        .await;
        let mut media = Vec::new();
        for server in &join.media_servers {
            media.push(mc_session::mh_connect(&server.media_handler_url, &gc_join.token).await);
        }
        ps.push(Participant {
            session,
            _media: media,
        });
    }

    // Every participant declares S audio slots (ids 0..S). Declarations are
    // the structural changes that make MC re-render and push to each handler.
    for p in &mut ps {
        let slot_ids: Vec<u32> = (0..u32::try_from(slots).expect("slot count fits u32")).collect();
        p.session
            .write(&ClientMessage {
                message: Some(client_message::Message::ReceiveCapability(
                    ReceiveCapability {
                        slots: slot_ids
                            .iter()
                            .map(|id| ReceiveSlot {
                                slot_id: *id,
                                media_kind: MediaKind::Audio as i32,
                                pinned_sender_id: None,
                            })
                            .collect(),
                    },
                )),
                trace_parent: String::new(),
                trace_state: String::new(),
            })
            .await;
    }
    // Keep each signalling stream drained so MC is never back-pressured by an
    // unread client while it pushes.
    for p in &mut ps {
        while p
            .session
            .try_read(Duration::from_millis(500))
            .await
            .is_some()
        {}
    }

    // ---- ADMISSION: refused and counted, with a positive control -----------
    poll_until_any_instance_above(
        &prom,
        &admission_promql("rejected_stream_ceiling"),
        &rejected_baseline,
        ADMISSION_BOUND,
        Duration::from_secs(3),
        |current| {
            format!(
                "ADMISSION: no MH instance counted mh_media_stream_admission_total\
                 {{outcome=\"rejected_stream_ceiling\"}} within {ADMISSION_BOUND:?} after a \
                 {participants}-participant meeting was driven past a stream ceiling of \
                 {ceiling} (baseline {rejected_baseline:?}, now {current:?}). Check MH's \
                 `mh.session.policy` WARN with reason egress_stream_ceiling_exceeded and \
                 mh_media_policy_applies_total{{outcome}}."
            )
        },
    )
    .await;
    // The instance(s) that refused — whichever MC placed the meeting on, never
    // a hardcoded mh-N. "Exactly one" is true of MC's placement today but is
    // LOGGED, not asserted: other meetings (env-tests head toward prod) can
    // refuse on the sibling inside the window.
    let rejecting = instances_exceeding_baseline(
        &rejected_baseline,
        &prom
            .instance_counter_map(&admission_promql("rejected_stream_ceiling"))
            .await,
    );
    eprintln!("S9: rejected_stream_ceiling moved on {rejecting:?}");
    assert!(
        !rejecting.is_empty(),
        "ADMISSION: the rejection counter rose and then no instance exceeds its baseline \
         ({rejected_baseline:?}) — a pod rollover mid-test?"
    );
    // Positive control, on the SAME handler: an instance that refused also
    // ADMITTED, so the refusal is the ceiling binding, not a handler refusing
    // everything.
    let deadline = std::time::Instant::now() + ADMISSION_BOUND;
    loop {
        let current = prom
            .instance_counter_map(&admission_promql("admitted"))
            .await;
        let admitted_on = instances_exceeding_baseline(&admitted_baseline, &current);
        if admitted_on.iter().any(|i| rejecting.contains(i)) {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "ADMISSION (positive control): no REJECTING instance ({rejecting:?}) counted an \
             ADMITTED decision within {ADMISSION_BOUND:?} (baseline {admitted_baseline:?}, now \
             {current:?}; admitted rose on {admitted_on:?}). The refusing handler never admitted \
             below the ceiling, so the rejection proves less than it seems."
        );
        tokio::time::sleep(Duration::from_secs(3)).await;
    }

    // ---- GAUGES: ratio and threshold present on every instance -------------
    let ratios = gauge_by_instance_present(
        &prom,
        "mh_media_stream_admission_rejection_ratio",
        MH_INSTANCE_COUNT,
        GAUGE_PRESENT_BOUND,
    )
    .await;
    for (instance, ratio) in &ratios {
        assert!(
            (0.0..=1.0).contains(ratio),
            "GAUGES: {instance} publishes a rejection ratio of {ratio}, outside 0..=1"
        );
    }
    let deployed_threshold: f64 = {
        let raw = configmap_key(&mh_config, "MH_EGRESS_REJECTION_RATIO_THRESHOLD");
        raw.parse().unwrap_or_else(|e| {
            panic!("PRECONDITION: MH_EGRESS_REJECTION_RATIO_THRESHOLD={raw:?} is not a number: {e}")
        })
    };
    let thresholds = gauge_by_instance_present(
        &prom,
        "mh_media_stream_admission_rejection_ratio_threshold",
        MH_INSTANCE_COUNT,
        GAUGE_PRESENT_BOUND,
    )
    .await;
    for (instance, threshold) in &thresholds {
        assert!(
            (0.0..=1.0).contains(threshold),
            "GAUGES: {instance} publishes a threshold of {threshold}, outside 0..=1"
        );
        assert!(
            (threshold - deployed_threshold).abs() < 1e-12,
            "GAUGES: {instance} publishes threshold {threshold}, but the deployed ConfigMap \
             carries MH_EGRESS_REJECTION_RATIO_THRESHOLD={deployed_threshold}: the gauge must \
             publish the SAME field enforcement reads (or the pod predates the ConfigMap: \
             {REDEPLOY_HINT})."
        );
    }

    // ---- CLEANUP: clean closes; the last one ends the meeting and MC releases it on every handler ----
    for p in ps.into_iter().rev() {
        p.session
            .connection()
            .close(wtransport::VarInt::from_u32(0), b"leave");
    }
}
