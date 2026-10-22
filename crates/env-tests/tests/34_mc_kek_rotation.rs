//! Env-test: a roster departure rotates the meeting KEK on the live Kind cluster
//! (story 2 task 9 — scenario S6, MC half; R-12, R-14, R-26; ADR-0036 §4
//! Rotation).
//!
//! Two real users join one meeting through GC and MC's real WebTransport path.
//! B leaves with a clean close. A must then receive a `MeetingKekUpdate` at the
//! NEXT generation carrying W — and the rotation must be visible on MC's own
//! counters in Prometheus.
//!
//! # No assertion is made on wall-clock W
//!
//! The rotation fires W after the departure (the debounce is measured from the
//! oldest un-rotated removal). The test does NOT assert WHEN it fires. W is
//! read from `mc_meeting_kek_rotation_window_seconds` — the value the timer
//! enforces, never a literal — and used only to bound the wait, plus a margin.
//! A ConfigMap change to W therefore cannot erode the margin.
//!
//! # The frame is the positive control, and its absence is a HARD failure
//!
//! Waiting past the deadline without a `MeetingKekUpdate` panics. It never
//! falls through to a skipped or softened assertion: the counters below are
//! read only after the frame has arrived.
//!
//! # Counters on a shared cluster
//!
//! Other suites may rotate KEKs concurrently, so the counters are asserted as
//! "some instance rose above its OWN settled baseline", never as an exact
//! delta. The frame at the right generation is what attributes the rotation to
//! THIS meeting; the counters prove it was recorded.
//!
//! # Story 3 MC-A: rotation with video flowing
//!
//! A second fn (three AV participants) proves a leave while video is directed
//! and flowing rotates the KEK for exactly the post-removal members and leaves
//! every video directive alone; see its doc.
//!
//! # Edge sum (README per-file edge-sum header rule)
//!
//! The two fns run concurrently. The story-2 fn opens no media session (0);
//! the video fn: 3 participants x (2 audio + 2 video) = 12.
//!
//! Edge-sum (concurrent Σ P×(N+M)): 12
//!
//! Registration cost: 2 + 3 = five AC registrations per run.

#![cfg(feature = "flows")]

use env_tests::cluster::ClusterConnection;
use env_tests::fixtures::auth_client::UserRegistrationRequest;
use env_tests::fixtures::gc_client::{CreateMeetingRequest, GcClient, JoinMeetingResponse};
use env_tests::fixtures::mc_session::{self, McSession};
use env_tests::fixtures::metrics::{
    format_instance_map, gauge_by_instance_present, poll_until_any_instance_above,
    settled_baseline, PrometheusClient,
};
use env_tests::fixtures::AuthClient;
use proto_gen::dark_tower::signaling::v1::{server_message, JoinResponse, MeetingKekUpdate};
use std::time::{Duration, Instant};

/// Margin over W before the rotation frame must have arrived: covers the
/// actor's wake, the push fan-out and the stream write. A rig bound, not a
/// performance assertion.
const ROTATION_MARGIN: Duration = Duration::from_secs(30);

/// How long to wait for MC's window gauge to be scraped.
const GAUGE_SCRAPE_BOUND: Duration = Duration::from_secs(60);

/// Per-read bound while waiting for frames; each read that times out is quiet,
/// not a failure, until the overall deadline.
const READ_BOUND: Duration = Duration::from_secs(5);

/// Counter stability budget. The SETTLE is not a local constant: both KEK
/// counters are MC series, scraped by the `mc-service` job, so the wait uses
/// `SERVICE_JOB_SCRAPE_SETTLE` through `service_job_scrape_settle` (one scrape
/// interval of that job plus margin, checked against the LIVE config — a settle
/// inside one scrape would make the baseline vacuous). The budget is a
/// failure-only ceiling: several non-equal rounds of that settle plus a
/// cluster-load allowance; it is not scaled with the interval, because a faster
/// scrape does not make a loaded cluster converge sooner.
const COUNTER_BUDGET: Duration = Duration::from_secs(90);

const ROTATIONS_PROMQL: &str =
    "sum by(instance) (mc_meeting_kek_generated_total{trigger=\"participant_left\"})";
const DELIVERED_PROMQL: &str =
    "sum by(instance) (mc_meeting_kek_pushes_total{outcome=\"delivered\"})";

async fn cluster() -> ClusterConnection {
    let cluster = ClusterConnection::new()
        .await
        .expect("Failed to connect to cluster - ensure port-forwards are running");
    cluster.check_ac_health().await.expect("AC must be running");
    cluster.check_gc_health().await.expect("GC must be running");
    cluster
}

/// W as MC enforces it, from the gauge MC publishes from the same value.
async fn rotation_window(prom: &PrometheusClient) -> Duration {
    let by_instance = gauge_by_instance_present(
        prom,
        "mc_meeting_kek_rotation_window_seconds",
        1,
        GAUGE_SCRAPE_BOUND,
    )
    .await;
    let seconds = by_instance.values().copied().fold(0.0_f64, f64::max);
    assert!(
        seconds > 0.0,
        "mc_meeting_kek_rotation_window_seconds must be positive (W is required and bounded at \
         MC config load); got {by_instance:?}"
    );
    Duration::from_secs_f64(seconds)
}

async fn join(
    gc: &JoinMeetingResponse,
    display_name: &str,
    label: &str,
) -> (McSession, JoinResponse) {
    let url = gc
        .mc_assignment
        .webtransport_endpoint
        .clone()
        .expect("MC assignment must include webtransport_endpoint");
    let mut session = McSession::connect(&url).await;
    let response = mc_session::mc_join(
        &mut session,
        &gc.meeting_id.to_string(),
        &gc.token,
        display_name,
        label,
    )
    .await;
    (session, response)
}

/// Read frames until a `MeetingKekUpdate` arrives or `deadline` passes.
async fn await_kek_update(session: &mut McSession, deadline: Instant) -> Option<MeetingKekUpdate> {
    while Instant::now() < deadline {
        if let Some(msg) = session.try_read(READ_BOUND).await {
            if let Some(server_message::Message::MeetingKekUpdate(update)) = msg.message {
                return Some(update);
            }
        }
    }
    None
}

#[tokio::test]
async fn a_departure_rotates_the_meeting_kek_for_the_remaining_member() {
    let cluster = cluster().await;
    let prom = PrometheusClient::new(&cluster.prometheus_base_url);
    let w = rotation_window(&prom).await;
    let auth = AuthClient::new(&cluster.ac_base_url);
    let gc = GcClient::new(&cluster.gc_base_url);

    // Baselines FIRST, settled, so a later rise is attributable to this run's
    // action rather than to a pre-existing instance missing from a single read.
    let rotations_before = settled_baseline(
        &prom,
        ROTATIONS_PROMQL,
        COUNTER_BUDGET,
        "the KEK-rotation assertion under test",
    )
    .await;
    let delivered_before = settled_baseline(
        &prom,
        DELIVERED_PROMQL,
        COUNTER_BUDGET,
        "the KEK-rotation assertion under test",
    )
    .await;

    let mut users = Vec::new();
    for label in ["kek-a", "kek-b"] {
        let request = UserRegistrationRequest::unique(format!("KEK rotation {label}"));
        let display = request.display_name.clone();
        let registered = auth
            .register_user(&request)
            .await
            .expect("AC should register a test user");
        users.push((registered.access_token, display));
    }
    let created = gc
        .create_meeting(
            &users[0].0,
            &CreateMeetingRequest::new("KEK rotation on leave"),
        )
        .await
        .expect("GC should create the meeting");
    let gc_a = gc
        .join_meeting(&created.meeting_code, &users[0].0)
        .await
        .expect("GC should issue A a meeting token");
    let gc_b = gc
        .join_meeting(&created.meeting_code, &users[1].0)
        .await
        .expect("GC should issue B a meeting token");

    let (mut a, a_join) = join(&gc_a, &users[0].1, "kek-a").await;
    let (b, _b_join) = join(&gc_b, &users[1].1, "kek-b").await;

    assert_eq!(
        a_join.meeting_kek.len(),
        32,
        "PRECONDITION: A was admitted with a provisioned AES-256 KEK"
    );
    assert_eq!(
        Duration::from_secs(u64::from(a_join.kek_rotation_debounce_seconds)),
        w,
        "the JoinResponse carries W from the SAME value the window gauge publishes"
    );

    // B leaves with a clean close: MC removes it immediately (no grace), and
    // the removal schedules the debounced rotation.
    b.connection()
        .close(wtransport::VarInt::from_u32(0), b"leave");

    let deadline = Instant::now() + w + ROTATION_MARGIN;
    let update = await_kek_update(&mut a, deadline).await.unwrap_or_else(|| {
        panic!(
            "A received NO MeetingKekUpdate within W ({w:?}) + {ROTATION_MARGIN:?} of B's \
             departure. The leave-debounced rotation did not fire or did not reach the remaining \
             member. Check mc_meeting_kek_rotation_pending_age_seconds against \
             mc_meeting_kek_rotation_overdue_threshold_seconds, \
             mc_meeting_kek_rotation_failures_total{{reason}} and \
             mc_meeting_kek_pushes_total{{outcome}} (mc-incident-response.md Scenario 19)."
        )
    });

    assert_eq!(
        update.kek_generation,
        a_join.kek_generation + 1,
        "a departure advances the KEK generation by exactly one"
    );
    assert_eq!(update.meeting_kek.len(), 32, "the rotated KEK is AES-256");
    assert_ne!(
        update.meeting_kek, a_join.meeting_kek,
        "a rotation that keeps the key is not a rotation"
    );
    assert_eq!(
        Duration::from_secs(u64::from(update.kek_rotation_debounce_seconds)),
        w,
        "W rides every MeetingKekUpdate, from the value the timer enforces"
    );

    poll_until_any_instance_above(
        &prom,
        ROTATIONS_PROMQL,
        &rotations_before,
        COUNTER_BUDGET,
        Duration::from_secs(5),
        |current| {
            format!(
                "the rotation A observed was never recorded: {ROTATIONS_PROMQL} rose on no \
                 instance (baseline {}, now {})",
                format_instance_map(&rotations_before),
                format_instance_map(current),
            )
        },
    )
    .await;
    poll_until_any_instance_above(
        &prom,
        DELIVERED_PROMQL,
        &delivered_before,
        COUNTER_BUDGET,
        Duration::from_secs(5),
        |current| {
            format!(
                "the push A received was never recorded: {DELIVERED_PROMQL} rose on no instance \
                 (baseline {}, now {})",
                format_instance_map(&delivered_before),
                format_instance_map(current),
            )
        },
    )
    .await;

    a.connection()
        .close(wtransport::VarInt::from_u32(0), b"leave");
}

/// Story 3 MC-A (R-14, R-33 "rotation on leave with video flowing"): a leave
/// while video is DIRECTED AND FLOWING rotates the KEK for exactly the
/// post-removal members, and does NOT re-issue, withdraw or restart any video
/// directive.
///
/// A, B, C each publish both kinds and declare 2 audio + 2 video slots, all on
/// both handlers (co-located: one target per stream). Joined and converged one
/// at a time, so each holds the earlier joiners first (R-3). B's first video
/// slot holds A; A's video frames reaching it is the FLOWING positive control.
///
/// C leaves. A and B each receive a `MeetingKekUpdate` at the next generation
/// carrying the SAME new key. A's video is still watched by B on the same
/// handler, so its directive has nothing to change: no `SendDirective` may
/// reach A between C's departure and a FENCE taken after the rotation — A
/// redeclares its audio slot count and reads until the resulting
/// `StreamAssignments` (a PRE-fence the same way, before the leave, drains
/// everything A was sent earlier). MC writes one participant's outbound stream in
/// meeting-actor turn order (the leave's flush, the rotation push and the
/// redeclaration's flush all enqueue on A's one mailbox), so anything the leave
/// or the rotation emitted for A is ordered BEFORE the fence: the absence is
/// bounded by an event, never by a timeout. Finally A's video still reaches B.
///
/// The browser half (decoding continues, no rotation keyframe) is C-C1's.
///
/// Edge sum and registration cost: see the file header.
#[tokio::test]
async fn a_leave_with_video_flowing_rotates_the_kek_without_touching_video_directives() {
    use env_tests::fixtures::mc_session::JoinDeclaration;
    use env_tests::fixtures::participant::{
        reach, settle_window, slots_of, Invited, Participant, AUDIO, VIDEO, VIEW_MARGIN,
    };
    use env_tests::fixtures::video::{until_video_on_slot, BackgroundPublisher, VideoSubscriber};
    use proto_gen::dark_tower::signaling::v1::{SlotState, StreamAssignments};
    use std::collections::HashMap;

    const PHASE: &str = "KEK+VIDEO";
    /// Failure bound for a directed video frame (rig bound, never a pass).
    const FRAME_DEADLINE: Duration = Duration::from_secs(30);

    let cluster = cluster().await;
    let prom = PrometheusClient::new(&cluster.prometheus_base_url);
    let w = rotation_window(&prom).await;
    let bound = settle_window(&cluster).await + VIEW_MARGIN;
    let auth = AuthClient::new(&cluster.ac_base_url);
    let gc = GcClient::new(&cluster.gc_base_url);
    let invited = Invited::new(&auth, &gc, "KEK rotation with video", 3).await;
    let both = [AUDIO, VIDEO];

    // A view where `kind`'s slots are `n` occupied (state != FEWER) then
    // FEWER, the occupied ones ACTIVE.
    let filled = |v: &StreamAssignments, kind, n: usize| {
        let s = slots_of(v, kind);
        s.len() == 2
            && s.iter().enumerate().all(|(i, a)| {
                let want = if i < n {
                    SlotState::Active
                } else {
                    SlotState::FewerSourcesThanSlots
                };
                a.slot_state == want as i32
            })
    };

    let labels = ["A", "B", "C"];
    let mut ps: Vec<Participant> = Vec::new();
    let mut mh: Vec<HashMap<String, env_tests::fixtures::mc_session::MhSession>> = Vec::new();
    for (i, label) in labels.iter().enumerate() {
        let base = 10 * u32::try_from(i).unwrap();
        let p = invited
            .join(
                i,
                label,
                &JoinDeclaration::default()
                    .publishing(&both)
                    .receiving(&[base + 1, base + 2], &[base + 3, base + 4]),
            )
            .await;
        let mut sessions = HashMap::new();
        for url in p.offered() {
            sessions.insert(url.clone(), reach(PHASE, label, &url, &p.token).await);
        }
        ps.push(p);
        mh.push(sessions);
        // Converge EVERY member on holding every earlier/later peer before the
        // next join, so slot order is the join order (R-3).
        let peers = ps.len() - 1;
        for p in &mut ps {
            p.converge(PHASE, bound, |v, _| {
                v.is_some_and(|v| filled(v, AUDIO, peers) && filled(v, VIDEO, peers))
            })
            .await;
        }
    }
    for p in &mut ps {
        // One co-located target per published kind.
        p.converge(PHASE, bound, |_, d| {
            d.is_some_and(|d| {
                d.streams.len() == 2 && d.streams.iter().all(|s| s.targets.len() == 1)
            })
        })
        .await;
    }
    let a_id = ps[0].sender_id();
    let a_join = ps[0].join.clone();
    let b_join = ps[1].join.clone();
    assert_eq!(
        a_join.kek_generation, b_join.kek_generation,
        "PRECONDITION: one meeting KEK generation"
    );
    let base_generation = a_join.kek_generation.max(b_join.kek_generation);

    // FLOWING: A's video on B's first video slot (it holds A: the earliest).
    let a_target = ps[0]
        .directive
        .as_ref()
        .unwrap()
        .streams
        .iter()
        .find(|s| s.media_kind == VIDEO as i32)
        .unwrap()
        .targets[0]
        .media_handler_url
        .clone();
    let b_video_slot = slots_of(ps[1].view(), VIDEO)[0].clone();
    assert_eq!(
        b_video_slot.media_handler_url, a_target,
        "PRECONDITION: B's edge from A is on A's one video target"
    );
    let publisher = BackgroundPublisher::start(mh[0][&a_target].connection().clone(), a_id);
    let mut b_sub = VideoSubscriber::spawn(mh[1][&a_target].connection().clone());
    until_video_on_slot(
        "KEK+VIDEO before",
        "B",
        &mut b_sub,
        b_video_slot.slot_id,
        a_id,
        FRAME_DEADLINE,
    )
    .await;

    // PRE-FENCE: everything A was sent before the leave is read off its stream
    // first, so nothing emitted earlier can be counted against the leave. A
    // adds a third audio slot and reads to the view carrying it.
    fence(PHASE, &mut ps[0], &[1, 2, 5], 3, bound).await;

    // C leaves (clean close: immediate removal, debounced rotation). C's MC
    // session closes FIRST, and its media sessions are dropped only once A's
    // view shows C removed. Dropping them first would make the handlers report
    // C's connections closing ONE AT A TIME while C is still on the roster: a
    // genuine partial-connectivity change that legitimately moves C's edges
    // from A to C's remaining handler (A's targets widen, then narrow) — real
    // routing, but not what this phase measures. Once C is off the roster the
    // handlers' disconnect reports are for an unknown participant and change
    // nothing.
    let c = ps.pop().unwrap();
    let c_id = c.sender_id();
    c.session
        .connection()
        .close(wtransport::VarInt::from_u32(0), b"leave");
    let deadline = Instant::now() + w + ROTATION_MARGIN;
    let mut a_directives_since_leave = 0usize;
    let mut early_update = None;
    loop {
        assert!(
            Instant::now() < deadline,
            "{PHASE}: A's view never dropped C after C's clean close"
        );
        match ps[0]
            .session
            .try_read(READ_BOUND)
            .await
            .and_then(|m| m.message)
        {
            Some(server_message::Message::StreamAssignments(v)) => {
                let gone = !v.assignments.iter().any(|a| a.sender_id == Some(c_id));
                ps[0].view = Some(v);
                if gone {
                    break;
                }
            }
            Some(server_message::Message::SendDirective(d)) => {
                a_directives_since_leave += 1;
                ps[0].directive = Some(d);
            }
            Some(server_message::Message::MeetingKekUpdate(u)) => early_update = Some(u),
            _ => {}
        }
    }
    drop((c, mh.pop()));

    // Read A and B until each has its rotation; count A's SendDirectives.
    let mut updates = Vec::new();
    for (i, p) in ps.iter_mut().enumerate() {
        if i == 0 {
            if let Some(u) = early_update.take() {
                updates.push(u);
                continue;
            }
        }
        let update = loop {
            assert!(
                Instant::now() < deadline,
                "{PHASE}: {} received NO MeetingKekUpdate within W ({w:?}) + {ROTATION_MARGIN:?} \
                 of C's departure while video was flowing (mc-incident-response.md Scenario 19)",
                p.label
            );
            match p.session.try_read(READ_BOUND).await.and_then(|m| m.message) {
                Some(server_message::Message::MeetingKekUpdate(u)) => break u,
                Some(server_message::Message::SendDirective(d)) => {
                    if i == 0 {
                        a_directives_since_leave += 1;
                    }
                    p.directive = Some(d);
                }
                Some(server_message::Message::StreamAssignments(v)) => p.view = Some(v),
                _ => {}
            }
        };
        updates.push(update);
    }
    for (p, u) in ps.iter().zip(&updates) {
        assert_eq!(
            u.kek_generation,
            base_generation + 1,
            "{PHASE}: {} — a departure advances the generation by exactly one",
            p.label
        );
        assert_eq!(u.meeting_kek.len(), 32, "{PHASE}: AES-256");
    }
    assert_eq!(
        updates[0].meeting_kek, updates[1].meeting_kek,
        "{PHASE}: every post-removal member receives the SAME rotated key"
    );
    assert_ne!(
        updates[0].meeting_kek, a_join.meeting_kek,
        "{PHASE}: a new key"
    );

    // FENCE: A drops back to two audio slots; read to the view carrying it.
    a_directives_since_leave += fence(PHASE, &mut ps[0], &[1, 2], 2, bound).await;
    assert_eq!(
        a_directives_since_leave, 0,
        "{PHASE}: C's leave and the KEK rotation re-issued A's send directive — A's video is \
         still watched by B on the same handler, so nothing about it changed"
    );

    // Still flowing after the rotation, on the same slot.
    until_video_on_slot(
        "KEK+VIDEO after",
        "B",
        &mut b_sub,
        b_video_slot.slot_id,
        a_id,
        FRAME_DEADLINE,
    )
    .await;
    publisher.stop().await;
}

/// Redeclare `p`'s audio slots to `audio` (video kept at its two join ids) and
/// read until a view with `audio_len` audio slots arrives. Returns how many
/// `SendDirective`s arrived on the way. A redeclaration of a participant's OWN
/// slots changes what it receives, never what it is directed to send.
async fn fence(
    phase: &str,
    p: &mut env_tests::fixtures::participant::Participant,
    audio: &[u32],
    audio_len: usize,
    bound: Duration,
) -> usize {
    use env_tests::fixtures::participant::{slots_of, AUDIO};
    p.declare(audio, &[3, 4]).await;
    let deadline = Instant::now() + bound;
    let mut directives = 0;
    loop {
        assert!(
            Instant::now() < deadline,
            "{phase}: {}'s fence redeclaration produced no StreamAssignments within {bound:?}",
            p.label
        );
        match p.session.try_read(READ_BOUND).await.and_then(|m| m.message) {
            Some(server_message::Message::StreamAssignments(v)) => {
                let done = slots_of(&v, AUDIO).len() == audio_len;
                p.view = Some(v);
                if done {
                    return directives;
                }
            }
            Some(server_message::Message::SendDirective(d)) => {
                directives += 1;
                p.directive = Some(d);
            }
            _ => {}
        }
    }
}
