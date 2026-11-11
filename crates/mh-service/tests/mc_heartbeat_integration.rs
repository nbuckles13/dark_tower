//! Story 4 MH-2: the per-meeting MH -> MC heartbeat, its verdicts, the
//! `EndMeeting` cause, and the all-or-none apply pin (ADR-0039 §1, §6, §8;
//! `docs/debates/2026-10-28-mh-replacement-and-migration/test-gates.md` "MH").
//!
//! # Rig
//!
//! The PRODUCTION session actor (`SessionManagerHandle` with a heartbeat sink)
//! and the PRODUCTION sender task (`HeartbeatSender`), behind the real
//! `MhMediaService` for every registration and `EndMeeting`. Only the RPC is
//! faked, through the `HeartbeatTransport` seam: paused time over a real socket
//! lets tokio auto-advance the clock mid-read and fire deadlines spuriously.
//! One test (`a_real_channel_is_reused_per_mc_endpoint`) runs the production
//! tonic transport against the mock MC, unpaused.
//!
//! Every test is `current_thread` (the default `tokio::test` flavour): metric
//! assertions are thread-local, and the sender's per-beat tasks must record on
//! the test's thread. Time moves only through [`run_for`] — small steps with
//! yields, never a sleep — so each due beat fires at its own instant.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ::common::observability::testing::MetricAssertion;
use bytes::Bytes;
use common::media_frame::audio_datagram;
use mh_service::config::{PolicyLimits, EGRESS_QUEUE_FRAMES, NOMINAL_AUDIO_FRAME_BYTES};
use mh_service::grpc::heartbeat_sender::{
    HeartbeatError, HeartbeatSender, HeartbeatTransport, TonicHeartbeatTransport,
};
use mh_service::grpc::MhMediaService;
use mh_service::media::forward::{forward_one, EgressQueue, IngressFrame};
use mh_service::media::forwarder::ConnectionForwarder;
use mh_service::media::queue::SharedQueue;
use mh_service::observability::metrics::resolve_media_handles;
use mh_service::routing::{MeetingKey, SenderId};
use mh_service::session::heartbeat::{HeartbeatSetup, SenderCommand};
use mh_service::session::{AdmitOutcome, ConnectionEntry, SessionManagerHandle, StreamAdmission};
use mh_test_utils::media_policy::{
    egress, register_request, TEST_HEARTBEAT_INTERVAL, TEST_MC_ENDPOINT, TEST_MC_ID,
};
use proto_gen::dark_tower::internal::v1::media_handler_service_server::MediaHandlerService;
use proto_gen::dark_tower::internal::v1::{
    ApplyRefusal, CandidateSource, EgressStream, EndMeetingRequest, HeartbeatVerdict,
    MeetingHeartbeatRequest, MeetingHeartbeatResponse, RegisterMeetingRequest, ReleaseCause,
    ServingState,
};
use rand::rngs::SmallRng;
use rand::SeedableRng;
use tokio::sync::{mpsc, Notify};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tonic::Request;

const HANDLER_ID: &str = "mh-heartbeat-test";
const SEED: u64 = 0x4D48_2D32;
/// Time step: fine enough that every due instant is visited on its own.
const STEP: Duration = Duration::from_millis(10);

// ---------------------------------------------------------------------------
// Fake MC transport
// ---------------------------------------------------------------------------

/// How the fake MC answers one meeting's beats.
#[derive(Clone)]
enum Reply {
    Owned,
    Released(i32),
    /// A raw verdict number (UNSPECIFIED / unknown).
    Verdict(i32),
    Status(tonic::Code),
    /// Never answers.
    Hang,
    /// Wait for the notify, then answer with the inner reply.
    HoldThen(Arc<Notify>, Box<Reply>),
}

#[derive(Default)]
struct FakeMc {
    replies: Mutex<HashMap<String, Reply>>,
    calls: Mutex<Vec<(Instant, String, MeetingHeartbeatRequest)>>,
}

impl FakeMc {
    fn reply(&self, meeting: &str, reply: Reply) {
        self.replies
            .lock()
            .unwrap()
            .insert(meeting.to_string(), reply);
    }

    fn calls_for(&self, meeting: &str) -> Vec<(Instant, MeetingHeartbeatRequest)> {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, _, r)| r.meeting_id == meeting)
            .map(|(t, _, r)| (*t, r.clone()))
            .collect()
    }

    fn count(&self, meeting: &str) -> usize {
        self.calls_for(meeting).len()
    }

    fn last(&self, meeting: &str) -> MeetingHeartbeatRequest {
        self.calls_for(meeting)
            .pop()
            .unwrap_or_else(|| panic!("{meeting}: no heartbeat was sent"))
            .1
    }

    fn endpoint_of_last(&self, meeting: &str) -> String {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|(_, _, r)| r.meeting_id == meeting)
            .map(|(_, e, _)| e.clone())
            .unwrap()
    }
}

fn answer(reply: &Reply) -> Result<MeetingHeartbeatResponse, HeartbeatError> {
    match reply {
        Reply::Owned => Ok(MeetingHeartbeatResponse {
            verdict: HeartbeatVerdict::Owned as i32,
            cause: 0,
        }),
        Reply::Released(cause) => Ok(MeetingHeartbeatResponse {
            verdict: HeartbeatVerdict::Released as i32,
            cause: *cause,
        }),
        Reply::Verdict(raw) => Ok(MeetingHeartbeatResponse {
            verdict: *raw,
            cause: 0,
        }),
        Reply::Status(code) => Err(HeartbeatError::Status(*code)),
        Reply::Hang | Reply::HoldThen(..) => unreachable!("resolved by the caller"),
    }
}

impl HeartbeatTransport for FakeMc {
    async fn send(
        &self,
        endpoint: &str,
        request: MeetingHeartbeatRequest,
    ) -> Result<MeetingHeartbeatResponse, HeartbeatError> {
        let reply = self
            .replies
            .lock()
            .unwrap()
            .get(&request.meeting_id)
            .cloned()
            .unwrap_or(Reply::Owned);
        self.calls
            .lock()
            .unwrap()
            .push((Instant::now(), endpoint.to_string(), request));
        match reply {
            Reply::Hang => std::future::pending().await,
            Reply::HoldThen(notify, inner) => {
                notify.notified().await;
                answer(&inner)
            }
            other => answer(&other),
        }
    }

    fn evict(&self, _endpoint: &str) {}
}

// ---------------------------------------------------------------------------
// Rig
// ---------------------------------------------------------------------------

struct Rig {
    sm: SessionManagerHandle,
    svc: MhMediaService,
    mc: Arc<FakeMc>,
    shutdown: CancellationToken,
}

impl Drop for Rig {
    fn drop(&mut self) {
        self.shutdown.cancel();
    }
}

fn rig_with(admission: StreamAdmission, limits: PolicyLimits) -> Rig {
    let (tx, rx) = mpsc::channel(mh_service::session::heartbeat::HEARTBEAT_QUEUE_CAPACITY);
    let (sm, actor) = SessionManagerHandle::new_with_heartbeat_for_tests(
        admission,
        HeartbeatSetup {
            handler_id: HANDLER_ID.to_string(),
            commands: tx,
            rng: SmallRng::seed_from_u64(SEED),
        },
    );
    tokio::spawn(actor.run());
    let mc = Arc::new(FakeMc::default());
    let shutdown = CancellationToken::new();
    tokio::spawn(
        HeartbeatSender::new(Arc::clone(&mc), rx, sm.verdict_sink(), 64).run(shutdown.clone()),
    );
    let svc = MhMediaService::new(sm.clone(), HANDLER_ID.to_string(), limits);
    Rig {
        sm,
        svc,
        mc,
        shutdown,
    }
}

fn rig() -> Rig {
    rig_with(
        mh_test_utils::admission::never_binding(),
        mh_test_utils::admission::fixture_policy_limits(),
    )
}

/// Let every runnable task run to quiescence (no time passes).
async fn settle() {
    for _ in 0..50 {
        tokio::task::yield_now().await;
    }
}

/// Advance paused time by `total` in [`STEP`]s, settling after each.
async fn run_for(total: Duration) {
    let mut elapsed = Duration::ZERO;
    while elapsed < total {
        tokio::time::advance(STEP).await;
        settle().await;
        elapsed += STEP;
    }
}

async fn register(rig: &Rig, request: RegisterMeetingRequest) -> Result<(), tonic::Status> {
    rig.svc
        .register_meeting(Request::new(request))
        .await
        .map(|_| ())
}

/// A valid, edge-free registration.
fn plain(meeting: &str) -> RegisterMeetingRequest {
    register_request(meeting, 1, Vec::new())
}

fn end(meeting: &str, mc_id: &str, cause: i32) -> Request<EndMeetingRequest> {
    Request::new(EndMeetingRequest {
        meeting_id: meeting.to_string(),
        mc_id: mc_id.to_string(),
        cause,
    })
}

fn connection(id: &str, participant: &str) -> ConnectionEntry {
    ConnectionEntry {
        connection_id: id.to_string(),
        participant_id: participant.to_string(),
        connected_at: std::time::Instant::now(),
        close: CancellationToken::new(),
        is_bound: false,
    }
}

/// Admit `entry` into `meeting` (awaiting its bind), asserting it was admitted.
async fn admit(sm: &SessionManagerHandle, meeting: &str, entry: ConnectionEntry) {
    assert_eq!(
        sm.admit_connection(
            meeting,
            entry,
            mh_test_utils::media_dial::meeting_dial_key(),
            8
        )
        .await,
        AdmitOutcome::Admitted,
        "{meeting}: fixture admission"
    );
}

const HEARTBEATS: &str = "mh_mc_heartbeats_total";
const VERDICTS: &str = "mh_mc_heartbeat_verdicts_total";
const RELEASES: &str = "mh_media_handler_releases_total";
const TEARDOWNS: &str = "mh_media_meeting_teardowns_total";
const KC: (&str, &str) = ("key_custody", "operator");

// ---------------------------------------------------------------------------
// Schedule
// ---------------------------------------------------------------------------

/// Heartbeats start at the registration upsert, the first one jittered inside
/// one interval, then exactly one per interval; the LATEST registration's
/// interval applies; release removes the schedule. Each beat carries the
/// registered `mc_id`, this handler, SERVING, and goes to the registered
/// endpoint only.
#[tokio::test(start_paused = true)]
async fn beats_start_at_registration_jitter_then_follow_the_latest_interval() {
    let rig = rig();
    let start = Instant::now();
    register(&rig, plain("m-cadence")).await.unwrap();

    run_for(TEST_HEARTBEAT_INTERVAL).await;
    let calls = rig.mc.calls_for("m-cadence");
    assert_eq!(calls.len(), 1, "exactly one beat in the first interval");
    let first = calls[0].0 - start;
    assert!(
        first < TEST_HEARTBEAT_INTERVAL,
        "first beat jittered in [0, interval): {first:?}"
    );

    run_for(TEST_HEARTBEAT_INTERVAL * 3).await;
    let calls = rig.mc.calls_for("m-cadence");
    assert_eq!(calls.len(), 4);
    for pair in calls.windows(2) {
        assert_eq!(
            pair[1].0 - pair[0].0,
            TEST_HEARTBEAT_INTERVAL,
            "exact cadence"
        );
    }
    let beat = rig.mc.last("m-cadence");
    assert_eq!(beat.mc_id, TEST_MC_ID, "the REGISTERED mc_id");
    assert_eq!(beat.handler_id, HANDLER_ID);
    assert_eq!(beat.serving_state, ServingState::Serving as i32);
    assert_eq!(beat.drain_remaining_ms, 0);
    assert_eq!(
        beat.applied_generation, 1,
        "read from the installed snapshot"
    );
    assert_eq!(rig.mc.endpoint_of_last("m-cadence"), TEST_MC_ENDPOINT);

    // Re-register with a SHORTER interval: the new one applies at once.
    let short = Duration::from_millis(500);
    let mut again = plain("m-cadence");
    again.heartbeat_interval_ms = 500;
    register(&rig, again).await.unwrap();
    let before = rig.mc.count("m-cadence");
    run_for(Duration::from_secs(2)).await;
    let calls = rig.mc.calls_for("m-cadence");
    assert_eq!(calls.len(), before + 4, "four beats in 2 s at 500 ms");
    for pair in calls[before..].windows(2) {
        assert_eq!(pair[1].0 - pair[0].0, short);
    }

    // Release: the schedule is gone (positive arm: beats flowed above).
    rig.svc
        .end_meeting(end(
            "m-cadence",
            TEST_MC_ID,
            ReleaseCause::MeetingEnded as i32,
        ))
        .await
        .unwrap();
    let released_at = rig.mc.count("m-cadence");
    run_for(TEST_HEARTBEAT_INTERVAL * 3).await;
    assert_eq!(
        rig.mc.count("m-cadence"),
        released_at,
        "no beat after release"
    );
}

/// A hung call for M1 never delays M2's beats; M1 is never retried (one call
/// per due tick); the per-attempt deadline is below the interval and counts
/// one `timeout` exactly when it expires.
#[tokio::test(start_paused = true)]
async fn a_hung_meeting_never_delays_another_and_is_not_retried() {
    let snap = MetricAssertion::snapshot();
    let rig = rig();
    rig.mc.reply("m-hung", Reply::Hang);
    register(&rig, plain("m-hung")).await.unwrap();
    register(&rig, plain("m-live")).await.unwrap();

    // Find M1's first call.
    while rig.mc.count("m-hung") == 0 {
        tokio::time::advance(STEP).await;
        settle().await;
    }
    let t1 = rig.mc.calls_for("m-hung")[0].0;
    let deadline = mh_service::session::heartbeat::per_attempt_deadline(TEST_HEARTBEAT_INTERVAL);
    assert!(deadline < TEST_HEARTBEAT_INTERVAL);
    // Just before the deadline: no timeout yet.
    let until = t1 + deadline - STEP;
    while Instant::now() < until {
        tokio::time::advance(STEP).await;
        settle().await;
    }
    assert_eq!(
        snap.counter(HEARTBEATS)
            .with_labels(&[("outcome", "timeout")])
            .delta(),
        0
    );
    tokio::time::advance(t1 + deadline - Instant::now()).await;
    settle().await;
    snap.counter(HEARTBEATS)
        .with_labels(&[("outcome", "timeout")])
        .assert_delta(1);
    assert_eq!(
        rig.mc.count("m-hung"),
        1,
        "no retry: the next tick is the retry"
    );

    let live_before = rig.mc.count("m-live");
    let hung_before = rig.mc.count("m-hung");
    run_for(TEST_HEARTBEAT_INTERVAL * 3).await;
    let live = rig.mc.calls_for("m-live");
    assert_eq!(live.len(), live_before + 3, "M2 beats every interval");
    for pair in live.windows(2) {
        assert_eq!(
            pair[1].0 - pair[0].0,
            TEST_HEARTBEAT_INTERVAL,
            "M2 is never delayed"
        );
    }
    assert_eq!(
        rig.mc.count("m-hung"),
        hung_before + 3,
        "M1: exactly one call per due tick, never more"
    );
    snap.counter(HEARTBEATS)
        .with_labels(&[("outcome", "skipped_in_flight")])
        .assert_delta(0);
    assert!(
        rig.sm.is_meeting_registered("m-hung").await,
        "MH takes no action"
    );
}

/// `mh_media_meetings_without_clients` is recomputed and SET on the actor's
/// 10 s tick from the same "has clients" predicate as the release `clients`
/// label: a meeting with an active connection (even one awaiting its
/// `sender_id`) is not counted, one without is.
#[tokio::test(start_paused = true)]
async fn meetings_without_clients_is_set_on_the_actor_tick() {
    let snap = MetricAssertion::snapshot();
    let rig = rig();
    register(&rig, plain("m-empty")).await.unwrap();
    register(&rig, plain("m-busy")).await.unwrap();
    admit(&rig.sm, "m-busy", connection("c-1", "p-1")).await;
    run_for(Duration::from_secs(10)).await;
    snap.gauge("mh_media_meetings_without_clients")
        .with_labels(&[KC])
        .assert_value(1.0);

    // Positive arm: the empty meeting gains a client; the next tick reads 0.
    admit(&rig.sm, "m-empty", connection("c-2", "p-2")).await;
    run_for(Duration::from_secs(10)).await;
    snap.gauge("mh_media_meetings_without_clients")
        .with_labels(&[KC])
        .assert_value(0.0);
}

/// At most one outstanding beat per meeting: a beat due while the previous
/// one is in flight is skipped and counted once; once the reply arrives the
/// slot frees and the next due beat is sent.
#[tokio::test(start_paused = true)]
async fn a_beat_due_while_its_predecessor_is_in_flight_is_skipped() {
    let snap = MetricAssertion::snapshot();
    let rig = rig();
    let hold = Arc::new(Notify::new());
    rig.mc.reply(
        "m-skip",
        Reply::HoldThen(Arc::clone(&hold), Box::new(Reply::Owned)),
    );
    register(&rig, plain("m-skip")).await.unwrap();
    while rig.mc.count("m-skip") == 0 {
        tokio::time::advance(STEP).await;
        settle().await;
    }
    // Shorten the interval while the beat is held: the next due (200 ms) lands
    // inside the held beat's 1.5 s deadline.
    let mut shorter = plain("m-skip");
    shorter.heartbeat_interval_ms = 200;
    register(&rig, shorter).await.unwrap();
    run_for(Duration::from_millis(300)).await;
    assert_eq!(rig.mc.count("m-skip"), 1, "the due beat was not sent");
    snap.counter(HEARTBEATS)
        .with_labels(&[("outcome", "skipped_in_flight")])
        .assert_delta(1);

    // Positive arm: the reply frees the slot and the next due beat goes out.
    rig.mc.reply("m-skip", Reply::Owned);
    hold.notify_waiters();
    settle().await;
    run_for(Duration::from_millis(200)).await;
    assert_eq!(rig.mc.count("m-skip"), 2);
    snap.counter(HEARTBEATS)
        .with_labels(&[("outcome", "skipped_in_flight")])
        .assert_delta(1);
}

/// A full hand-off queue drops the beat and counts it; the actor never blocks
/// (it still answers a lifecycle request).
#[tokio::test(start_paused = true)]
async fn a_full_queue_drops_beats_and_never_blocks_the_actor() {
    let snap = MetricAssertion::snapshot();
    let (tx, mut rx) = mpsc::channel(1);
    let (sm, actor) = SessionManagerHandle::new_with_heartbeat_for_tests(
        mh_test_utils::admission::never_binding(),
        HeartbeatSetup {
            handler_id: HANDLER_ID.to_string(),
            commands: tx,
            rng: SmallRng::seed_from_u64(SEED),
        },
    );
    tokio::spawn(actor.run());
    let svc = MhMediaService::new(
        sm.clone(),
        HANDLER_ID.to_string(),
        mh_test_utils::admission::fixture_policy_limits(),
    );
    // No sender task drains the queue.
    for m in ["q-1", "q-2", "q-3"] {
        svc.register_meeting(Request::new(plain(m))).await.unwrap();
    }
    run_for(TEST_HEARTBEAT_INTERVAL).await;
    snap.counter(HEARTBEATS)
        .with_labels(&[("outcome", "dropped_queue_full")])
        .assert_delta(2);
    assert!(
        sm.is_meeting_registered("q-1").await,
        "the actor still serves its lifecycle mailbox"
    );
    assert!(
        matches!(rx.try_recv(), Ok(SenderCommand::Beat(_))),
        "positive arm: the one beat that fit was handed off"
    );
}

// ---------------------------------------------------------------------------
// Verdicts
// ---------------------------------------------------------------------------

/// Every verdict arm, one meeting each, in one test (test-gates "MH").
///
/// - RELEASED(removal) ends the meeting through the release path: its active
///   connections (a bound one and one still awaiting its `sender_id`) are
///   closed — which `ConnectionEnd::MeetingReleased` turns into
///   `HANDLER_RELEASED` with NO `NotifyParticipantDisconnected` — its pending
///   connections are closed, and it counts on the release series with
///   `clients`.
/// - RELEASED(MEETING_ENDED) ends it on the teardown series and leaves pending
///   connections.
/// - RELEASED with no valid cause (UNSPECIFIED, 99) is the misroute: it STILL
///   ends the meeting.
/// - OWNED, FAILED_PRECONDITION, an UNSPECIFIED/unknown verdict and a
///   transport error do nothing: the meeting is held, proven by a LATER beat.
/// - A RELEASED for a registration superseded while the beat was in flight is
///   dropped (`released_stale`); its positive twin is the current-fence
///   release above.
#[tokio::test(start_paused = true)]
async fn every_verdict_arm_in_one_test() {
    let snap = MetricAssertion::snapshot();
    let rig = rig();
    let released = [
        ("rel-hb", ReleaseCause::HeartbeatTimeout as i32),
        ("rel-lag", ReleaseCause::ApplyLag as i32),
        ("rel-pt", ReleaseCause::PlannedTransfer as i32),
        ("rel-ended", ReleaseCause::MeetingEnded as i32),
        ("misroute", ReleaseCause::Unspecified as i32),
        ("misroute-99", 99),
    ];
    let held = [
        ("owned", Reply::Owned),
        ("mismatch", Reply::Status(tonic::Code::FailedPrecondition)),
        (
            "unspec",
            Reply::Verdict(HeartbeatVerdict::Unspecified as i32),
        ),
        ("verdict-99", Reply::Verdict(99)),
        ("err", Reply::Status(tonic::Code::Unavailable)),
    ];
    for (m, cause) in released {
        rig.mc.reply(m, Reply::Released(cause));
        register(&rig, plain(m)).await.unwrap();
    }
    for (m, reply) in held.clone() {
        rig.mc.reply(m, reply);
        register(&rig, plain(m)).await.unwrap();
    }
    // Clients: rel-hb has a bound connection; rel-pt one awaiting its
    // sender_id (admitted, Notify not answered); rel-ended one awaiting its
    // sender_id; rel-lag none. (There is no pending-before-registration state:
    // ADR-0039 §3.)
    let bound = connection("c-bound", "p-1");
    let bound_close = bound.close.clone();
    admit(&rig.sm, "rel-hb", bound).await;
    assert_eq!(
        rig.sm.bind_connection("rel-hb", "c-bound", sender(7)).await,
        mh_service::session::BindOutcome::Bound
    );
    let awaiting = connection("c-awaiting", "p-2");
    let awaiting_close = awaiting.close.clone();
    admit(&rig.sm, "rel-pt", awaiting).await;
    let ended_awaiting = connection("c-ended-awaiting", "p-3");
    let ended_awaiting_close = ended_awaiting.close.clone();
    admit(&rig.sm, "rel-ended", ended_awaiting).await;

    // The stale-fence arm: its RELEASED is held until the meeting has been
    // re-registered to a DIFFERENT MC.
    let hold = Arc::new(Notify::new());
    rig.mc.reply(
        "stale",
        Reply::HoldThen(
            Arc::clone(&hold),
            Box::new(Reply::Released(ReleaseCause::HeartbeatTimeout as i32)),
        ),
    );
    register(&rig, plain("stale")).await.unwrap();

    run_for(TEST_HEARTBEAT_INTERVAL).await;
    let mut takeover = plain("stale");
    takeover.mc_id = "mc-new-owner".to_string();
    register(&rig, takeover).await.unwrap();
    rig.mc.reply("stale", Reply::Owned);
    hold.notify_waiters();
    settle().await;

    for (m, _) in released {
        assert!(
            !rig.sm.is_meeting_registered(m).await,
            "{m}: RELEASED ends the meeting"
        );
    }
    assert!(
        bound_close.is_cancelled(),
        "rel-hb: the bound session is closed"
    );
    assert!(
        awaiting_close.is_cancelled(),
        "rel-pt: a removal closes the session awaiting its sender_id"
    );
    assert!(
        !ended_awaiting_close.is_cancelled(),
        "rel-ended: MEETING_ENDED leaves a session awaiting its sender_id to the bind outcome"
    );
    for (trigger, clients, expected) in [
        ("heartbeat_timeout", "present", 1),
        ("heartbeat_timeout", "none", 0),
        ("apply_lag", "none", 1),
        ("apply_lag", "present", 0),
        ("planned_transfer", "present", 1),
        ("planned_transfer", "none", 0),
    ] {
        snap.counter(RELEASES)
            .with_labels(&[("trigger", trigger), ("clients", clients), KC])
            .assert_delta(expected);
    }
    // MEETING_ENDED + both misroutes: one home, the teardown series.
    snap.counter(TEARDOWNS)
        .with_labels(&[("outcome", "released"), KC])
        .assert_delta(3);
    for (outcome, expected) in [
        ("released", 4),
        ("misroute", 2),
        ("released_stale", 1),
        ("owned", 1),
        ("owner_mismatch", 1),
        ("unspecified_verdict", 2),
    ] {
        snap.counter(VERDICTS)
            .with_labels(&[("outcome", outcome)])
            .assert_delta(expected);
    }

    // Held: still registered AND still beating after a further interval.
    let mut held_names: Vec<&str> = held.iter().map(|(m, _)| *m).collect();
    held_names.push("stale");
    let before: Vec<usize> = held_names.iter().map(|m| rig.mc.count(m)).collect();
    run_for(TEST_HEARTBEAT_INTERVAL).await;
    for (m, n) in held_names.iter().zip(before) {
        assert!(rig.sm.is_meeting_registered(m).await, "{m}: held");
        assert!(rig.mc.count(m) > n, "{m}: still heartbeating");
    }
    assert_eq!(rig.mc.last("stale").mc_id, "mc-new-owner");
    snap.counter(HEARTBEATS)
        .with_labels(&[("outcome", "error")])
        .assert_delta(4);
}

// ---------------------------------------------------------------------------
// Registration record and EndMeeting cause
// ---------------------------------------------------------------------------

/// `heartbeat_interval_ms == 0` is a record reject: nothing registered, no
/// schedule. Positive arm: the same request with an interval registers and
/// beats.
#[tokio::test(start_paused = true)]
async fn a_zero_heartbeat_interval_is_refused_and_schedules_nothing() {
    let rig = rig();
    let mut zero = plain("m-zero");
    zero.heartbeat_interval_ms = 0;
    let err = register(&rig, zero).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(!rig.sm.is_meeting_registered("m-zero").await);
    run_for(TEST_HEARTBEAT_INTERVAL * 2).await;
    assert_eq!(rig.mc.count("m-zero"), 0);

    register(&rig, plain("m-zero")).await.unwrap();
    run_for(TEST_HEARTBEAT_INTERVAL).await;
    assert_eq!(rig.mc.count("m-zero"), 1);
}

/// `EndMeeting.cause`: UNSPECIFIED and an unknown number are refused
/// INVALID_ARGUMENT, counted, and release nothing; MEETING_ENDED releases.
/// A removal cause closes connections awaiting their bind (HANDLER_RELEASED) and counts
/// on the release series; MEETING_ENDED leaves them to the bind outcome.
#[tokio::test(start_paused = true)]
async fn end_meeting_requires_a_cause_and_the_cause_decides_awaiting_bind_connections() {
    let snap = MetricAssertion::snapshot();
    let rig = rig();
    register(&rig, plain("m-end")).await.unwrap();
    for cause in [ReleaseCause::Unspecified as i32, 99] {
        let err = rig
            .svc
            .end_meeting(end("m-end", TEST_MC_ID, cause))
            .await
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument, "cause {cause}");
        assert!(
            rig.sm.is_meeting_registered("m-end").await,
            "cause {cause}: held"
        );
    }
    snap.counter(TEARDOWNS)
        .with_labels(&[("outcome", "rejected_invalid_cause"), KC])
        .assert_delta(2);

    // Removal cause vs MEETING_ENDED, on connections awaiting their bind.
    register(&rig, plain("m-removed")).await.unwrap();
    let removed_awaiting = connection("c-r", "p-r");
    let removed_close = removed_awaiting.close.clone();
    admit(&rig.sm, "m-removed", removed_awaiting).await;
    let kept_awaiting = connection("c-k", "p-k");
    let kept_close = kept_awaiting.close.clone();
    admit(&rig.sm, "m-end", kept_awaiting).await;

    rig.svc
        .end_meeting(end(
            "m-removed",
            TEST_MC_ID,
            ReleaseCause::PlannedTransfer as i32,
        ))
        .await
        .unwrap();
    rig.svc
        .end_meeting(end("m-end", TEST_MC_ID, ReleaseCause::MeetingEnded as i32))
        .await
        .unwrap();
    assert!(!rig.sm.is_meeting_registered("m-end").await);
    assert!(!rig.sm.is_meeting_registered("m-removed").await);
    assert!(
        removed_close.is_cancelled(),
        "a removal closes the connection awaiting its bind"
    );
    assert!(
        !kept_close.is_cancelled(),
        "MEETING_ENDED leaves it to the bind outcome"
    );
    // An admitted connection awaiting its bind is a client (the same "has
    // clients" predicate as the gauge), so this release is `clients=present`.
    snap.counter(RELEASES)
        .with_labels(&[("trigger", "planned_transfer"), ("clients", "present"), KC])
        .assert_delta(1);
    snap.counter(TEARDOWNS)
        .with_labels(&[("outcome", "released"), KC])
        .assert_delta(1);
}

// ---------------------------------------------------------------------------
// All-or-none apply (R-8), pinned on frames
// ---------------------------------------------------------------------------

const A: u32 = 1;
const B: u32 = 2;
const C: u32 = 3;
const D: u32 = 4;

fn sender(v: u32) -> SenderId {
    SenderId::from_wire(v).unwrap()
}

/// A -> B, plus optionally A -> C.
fn edges(with_c: bool) -> Vec<EgressStream> {
    let mut e = vec![egress(1, B, 1, A)];
    if with_c {
        e.push(egress(2, C, 1, A));
    }
    e
}

/// One refusal kind's rig and its "valid change + refusing change" request.
struct Case {
    kind: ApplyRefusal,
    /// How the REFUSING RPC answers: validation refusals are
    /// INVALID_ARGUMENT; capacity refusals answer OK with the prior
    /// generation (the refusal is on the policy outcome, not the status).
    refused_status: Option<tonic::Code>,
    admission: StreamAdmission,
    limits: PolicyLimits,
    /// Add a change that makes the WHOLE registration refuse as `kind`.
    poison: fn(&mut RegisterMeetingRequest),
}

fn cases() -> Vec<Case> {
    let fixture = mh_test_utils::admission::fixture_policy_limits();
    let mut edge_cap = fixture;
    edge_cap.max_total_egress_edges = 2;
    vec![
        Case {
            kind: ApplyRefusal::Invalid,
            refused_status: Some(tonic::Code::InvalidArgument),
            admission: mh_test_utils::admission::never_binding(),
            limits: fixture,
            // A duplicate egress_stream_id.
            poison: |r| r.egress_streams.push(egress(1, D, 2, A)),
        },
        Case {
            kind: ApplyRefusal::TransportInconsistent,
            refused_status: Some(tonic::Code::InvalidArgument),
            admission: mh_test_utils::admission::never_binding(),
            limits: fixture,
            // A candidate stream nobody declared.
            poison: |r| {
                r.egress_streams.push(EgressStream {
                    egress_stream_id: 9,
                    subscriber: egress(9, D, 1, A).subscriber,
                    candidate_sources: vec![CandidateSource {
                        sender_id: A,
                        stream_number: 7,
                    }],
                    priority_group: 0,
                    supersede_on_independent_frame: false,
                });
            },
        },
        Case {
            kind: ApplyRefusal::StreamCeiling,
            refused_status: None,
            admission: mh_test_utils::admission::with_ceiling(2),
            limits: fixture,
            // A third stream: over the 2-stream ceiling.
            poison: |r| r.egress_streams.push(egress(3, D, 1, A)),
        },
        Case {
            kind: ApplyRefusal::EdgeCap,
            refused_status: None,
            admission: mh_test_utils::admission::never_binding(),
            limits: edge_cap,
            // A third edge: over the 2-edge aggregate bound (inside apply_failed).
            poison: |r| r.egress_streams.push(egress(3, D, 1, A)),
        },
    ]
}

/// The refusing registration's RPC answer, per kind: the expected status, or
/// OK reporting the PRIOR applied generation (never the refused one).
async fn assert_refused(
    rig: &Rig,
    request: RegisterMeetingRequest,
    status: Option<tonic::Code>,
    prior_generation: u64,
    kind: ApplyRefusal,
) {
    let answer = rig.svc.register_meeting(Request::new(request)).await;
    match (status, answer) {
        (Some(code), Err(got)) => assert_eq!(got.code(), code, "{kind:?}"),
        (None, Ok(response)) => assert_eq!(
            response.into_inner().applied_generation,
            prior_generation,
            "{kind:?}: the response reports the prior generation"
        ),
        (expected, got) => panic!("{kind:?}: expected {expected:?}, got {got:?}"),
    }
}

fn request(meeting: &str, generation: u64, with_c: bool) -> RegisterMeetingRequest {
    let mut r = register_request(meeting, generation, edges(with_c));
    mh_test_utils::media_policy::declare_forward(&mut r, &[A, B, C, D]);
    r
}

fn queue_for(sm: &SessionManagerHandle, meeting: &MeetingKey, who: u32) -> Arc<EgressQueue> {
    let queue: Arc<EgressQueue> = Arc::new(SharedQueue::new(EGRESS_QUEUE_FRAMES));
    sm.subscribers()
        .register(meeting.clone(), sender(who), &format!("conn-{who}"), &queue);
    queue
}

/// Send one A frame through the live forward path; return (B got it, C got it).
fn send_a(
    sm: &SessionManagerHandle,
    forwarder: &mut ConnectionForwarder,
    b: &EgressQueue,
    c: &EgressQueue,
    seq: u32,
) -> (bool, bool) {
    let frame: Bytes = audio_datagram(seq, 0xA1);
    forward_one(
        forwarder,
        &sm.routing_snapshot(),
        &sm.subscribers().load(),
        IngressFrame {
            payload: frame,
            received_at: std::time::Instant::now(),
        },
    );
    (b.try_pop().is_some(), c.try_pop().is_some())
}

/// R-8, parameterised over the four deterministic refusals. Generation 4
/// forwards A->B. Generation 5 = a valid change (add A->C) + one refusing
/// change: the valid change does NOT take effect, A->B keeps forwarding, and
/// the heartbeat reports 4 + the refusal. Generation 6 (valid, with A->C)
/// applies — C now receives, proving the change is real — and clears the
/// refusal. Extra arm: a FIRST-EVER apply refused still heartbeats, with 0 +
/// the refusal.
#[tokio::test(start_paused = true)]
async fn apply_is_all_or_none_and_the_heartbeat_reports_the_refusal() {
    for case in cases() {
        let kind = case.kind;
        let rig = rig_with(case.admission, case.limits);
        let meeting = format!("aon-{}", kind.as_str_name());
        let key = MeetingKey::new(&meeting);
        let b = queue_for(&rig.sm, &key, B);
        let c = queue_for(&rig.sm, &key, C);
        let mut forwarder = ConnectionForwarder::new(
            key.clone(),
            sender(A),
            Arc::new(resolve_media_handles()),
            1.0,
            NOMINAL_AUDIO_FRAME_BYTES,
        );

        register(&rig, request(&meeting, 4, false)).await.unwrap();
        assert_eq!(
            send_a(&rig.sm, &mut forwarder, &b, &c, 1),
            (true, false),
            "{kind:?} gen 4"
        );

        let mut five = request(&meeting, 5, true);
        (case.poison)(&mut five);
        assert_refused(&rig, five, case.refused_status, 4, kind).await;
        assert_eq!(
            send_a(&rig.sm, &mut forwarder, &b, &c, 2),
            (true, false),
            "{kind:?} gen 5: A->B keeps forwarding and the valid A->C did NOT take effect"
        );
        run_for(TEST_HEARTBEAT_INTERVAL).await;
        let beat = rig.mc.last(&meeting);
        assert_eq!(beat.applied_generation, 4, "{kind:?}: still reports 4");
        assert_eq!(
            beat.apply_refusal, kind as i32,
            "{kind:?}: reports the refusal"
        );

        register(&rig, request(&meeting, 6, true)).await.unwrap();
        assert_eq!(
            send_a(&rig.sm, &mut forwarder, &b, &c, 3),
            (true, true),
            "{kind:?} gen 6: the change is real"
        );
        run_for(TEST_HEARTBEAT_INTERVAL).await;
        let beat = rig.mc.last(&meeting);
        assert_eq!(beat.applied_generation, 6, "{kind:?}");
        assert_eq!(
            beat.apply_refusal,
            ApplyRefusal::Unspecified as i32,
            "{kind:?}: success clears the refusal"
        );

        // A first-ever apply refused: heartbeats start at the upsert anyway.
        let fresh = format!("fresh-{}", kind.as_str_name());
        let mut first = request(&fresh, 1, true);
        (case.poison)(&mut first);
        assert_refused(&rig, first, case.refused_status, 0, kind).await;
        assert!(
            rig.sm.is_meeting_registered(&fresh).await,
            "{kind:?}: the record upserted"
        );
        run_for(TEST_HEARTBEAT_INTERVAL).await;
        let beat = rig.mc.last(&fresh);
        assert_eq!(beat.applied_generation, 0, "{kind:?}: nothing installed");
        assert_eq!(beat.apply_refusal, kind as i32, "{kind:?}");
    }
}

// ---------------------------------------------------------------------------
// Production transport (real tonic, unpaused)
// ---------------------------------------------------------------------------

/// The production transport reuses ONE channel per MC endpoint (one TCP
/// accept over several beats) and the request carries the registered ids.
#[tokio::test]
async fn a_real_channel_is_reused_per_mc_endpoint() {
    let snap = MetricAssertion::snapshot();
    let (beats_tx, mut beats_rx) = mpsc::channel(64);
    let mc = common::mock_mc::start_mock_mc_server(
        common::mock_mc::MockMcServer::new(common::mock_mc::MockBehavior::Accept)
            .with_heartbeat_tx(beats_tx),
    )
    .await;
    let endpoint = format!("http://{}", mc.addr);

    let (tx, rx) = mpsc::channel(16);
    let (sm, actor) = SessionManagerHandle::new_with_heartbeat_for_tests(
        mh_test_utils::admission::never_binding(),
        HeartbeatSetup {
            handler_id: HANDLER_ID.to_string(),
            commands: tx,
            rng: SmallRng::seed_from_u64(SEED),
        },
    );
    tokio::spawn(actor.run());
    let shutdown = CancellationToken::new();
    let transport = Arc::new(TonicHeartbeatTransport::new(
        common::test_token_receiver(),
        8,
    ));
    tokio::spawn(
        HeartbeatSender::new(Arc::clone(&transport), rx, sm.verdict_sink(), 8)
            .run(shutdown.clone()),
    );
    let svc = MhMediaService::new(
        sm.clone(),
        HANDLER_ID.to_string(),
        mh_test_utils::admission::fixture_policy_limits(),
    );
    for meeting in ["real-1", "real-2"] {
        let mut r = plain(meeting);
        r.mc_grpc_endpoint = endpoint.clone();
        r.mc_id = "mc-real".to_string();
        r.heartbeat_interval_ms = 50;
        svc.register_meeting(Request::new(r)).await.unwrap();
    }

    let mut seen = Vec::new();
    while seen.len() < 6 {
        let beat = tokio::time::timeout(Duration::from_secs(10), beats_rx.recv())
            .await
            .expect("beats arrive")
            .expect("mock open");
        seen.push(beat);
    }
    for beat in &seen {
        assert_eq!(beat.mc_id, "mc-real");
        assert_eq!(beat.handler_id, HANDLER_ID);
        assert!(beat.meeting_id == "real-1" || beat.meeting_id == "real-2");
    }
    assert_eq!(
        mc.accepts.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "six beats for two meetings on one endpoint ride ONE connection"
    );
    assert_eq!(transport.cached_endpoints(), 1);
    // The mock captures BEFORE it answers, so the last captured beat may not
    // have completed yet; every earlier one has. Latency is observed for each
    // COMPLETED call (read first: histogram reads drain the snapshot).
    snap.histogram("mh_mc_heartbeat_latency_seconds")
        .assert_observation_count_at_least(seen.len() - 1);
    let succeeded = snap
        .counter(HEARTBEATS)
        .with_labels(&[("outcome", "success")])
        .delta();
    assert!(
        succeeded + 1 >= seen.len() as u64,
        "{succeeded} of {}",
        seen.len()
    );
    snap.counter(HEARTBEATS)
        .with_labels(&[("outcome", "error")])
        .assert_delta(0);
    shutdown.cancel();
}

// ---------------------------------------------------------------------------
// Connection set over the real session path
// ---------------------------------------------------------------------------

/// A disconnect whose `NotifyParticipantDisconnected` EXHAUSTED its retries
/// (`MAX_RETRY_ATTEMPTS`, MC answering UNAVAILABLE every time) is no longer in
/// the meeting's heartbeat connection set; a still-connected session of the
/// same meeting is (positive arm). Real WebTransport sessions, real McClient
/// retry path (backoff shortened through the test constructor), unpaused;
/// synchronised on events (the mock's captured calls, the actor's beats).
#[tokio::test]
async fn a_disconnect_whose_notify_exhausted_its_retries_leaves_the_set() {
    use common::accept_loop_rig::AcceptLoopRig;
    use common::mock_mc::{start_mock_mc_server, MockBehavior, MockMcServer, SenderReplies};
    use common::tokens::mint_dial_token;
    use common::wt_client::{connect_and_open_bi, write_mh_connect};
    use mh_service::grpc::mc_client::MAX_RETRY_ATTEMPTS;
    use mh_service::grpc::McClient;

    const MEETING: &str = "m-retry-exhausted";
    let (tx, mut beats) = mpsc::channel(4096);
    let (sm, actor) = SessionManagerHandle::new_with_heartbeat_for_tests(
        mh_test_utils::admission::never_binding(),
        HeartbeatSetup {
            handler_id: HANDLER_ID.to_string(),
            commands: tx,
            rng: SmallRng::seed_from_u64(SEED),
        },
    );
    tokio::spawn(actor.run());
    let (disc_tx, mut disconnects) = mpsc::channel(16);
    let mc = start_mock_mc_server(
        MockMcServer::new(MockBehavior::FailDisconnects)
            .with_sender_replies(
                SenderReplies::default()
                    .with("p-leave", 21)
                    .with("p-stay", 22),
            )
            .with_disconnected_tx(disc_tx),
    )
    .await;
    let wt = AcceptLoopRig::start(
        sm.clone(),
        Arc::new(McClient::with_retry_base_delay(
            common::test_token_receiver(),
            Duration::from_millis(1),
        )),
        HANDLER_ID.to_string(),
    )
    .await;
    sm.register_meeting(
        MEETING.to_string(),
        mh_service::session::MeetingRegistration {
            mc_id: TEST_MC_ID.to_string(),
            mc_grpc_endpoint: format!("http://{}", mc.addr),
            heartbeat_interval: Duration::from_millis(50),
            registered_at: std::time::Instant::now(),
            media_dial_key: mh_test_utils::media_dial::meeting_dial_key(),
        },
        None,
        mh_test_utils::admission::fixture_policy_limits().max_registered_meetings,
    )
    .await
    .unwrap();

    let mut sessions = Vec::new();
    for participant in ["p-leave", "p-stay"] {
        let token = mint_dial_token(MEETING, participant, HANDLER_ID);
        let (conn, mut send, recv) = connect_and_open_bi(&wt.url).await;
        write_mh_connect(&mut send, &token).await.unwrap();
        sessions.push((conn, send, recv));
    }

    // Both sessions in the set: wait on the actor's beats (events, no polling).
    async fn next_set(beats: &mut mpsc::Receiver<SenderCommand>) -> Vec<String> {
        loop {
            let command = tokio::time::timeout(Duration::from_secs(5), beats.recv())
                .await
                .expect("the actor beats every 50 ms")
                .expect("queue open");
            if let SenderCommand::Beat(beat) = command {
                return beat.request.connection_ids;
            }
        }
    }
    let bind_deadline = std::time::Instant::now() + Duration::from_secs(10);
    let both = loop {
        let set = next_set(&mut beats).await;
        if set.len() == 2 {
            break set;
        }
        assert!(
            std::time::Instant::now() < bind_deadline,
            "both sessions must enter the connection set within 10 s; last set {set:?}"
        );
    };

    // End p-leave's session; MC fails every Disconnected attempt.
    let (leave_conn, mut leave_send, _leave_recv) = sessions.remove(0);
    leave_send.finish().await.unwrap();
    drop(leave_conn);
    let mut attempts = Vec::new();
    while attempts.len() < MAX_RETRY_ATTEMPTS as usize {
        let d = tokio::time::timeout(Duration::from_secs(10), disconnects.recv())
            .await
            .expect("every retry reaches the mock")
            .expect("capture open");
        attempts.push(d.connection_id);
    }
    let leaving = attempts[0].clone();
    assert!(
        attempts.iter().all(|id| *id == leaving),
        "one connection, retried"
    );
    let staying: Vec<String> = both.into_iter().filter(|id| *id != leaving).collect();
    assert_eq!(staying.len(), 1, "the leaving id was in the set before");

    // Drain the beats queued so far, then take the next one the actor builds.
    while beats.try_recv().is_ok() {}
    let after = next_set(&mut beats).await;
    assert_eq!(
        after, staying,
        "the exhausted disconnect is out; the live session stays"
    );
}
