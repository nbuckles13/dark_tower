//! Detection, Dead/Impaired declaration and removal (ADR-0039 §2, §6; story 4
//! MC-B2: R-1 survivor arm, R-4, R-10).
//!
//! Tier 1b: the real gRPC service and meeting actors under a controller,
//! `current_thread` + `start_paused` (time moves only by `advance`, never a
//! sleep), and a fake MH that behaves like a deployed MH-2 handler — it
//! heartbeats every meeting it acked (`MockMhRegistrationClient::heartbeat_via`)
//! — so a test makes a handler crash (`silence`), freeze and resume
//! (`silence` / `resume`), drain (`drain`) or stop applying (`fail_handler`).
//! Time passes CONTINUOUSLY (`advance_live`), as it does for a live MH.
//!
//! The env-test legs in `crates/env-tests/tests/40_resilience.rs` point here
//! for what Layer 7 cannot pin deterministically: the RELEASED verdict after a
//! lost `EndMeeting`, the removal ORDER, and that a zombie's `Disconnected`s
//! change nothing.

#![cfg(feature = "test-seams")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "common/mod.rs"]
mod test_common;

use std::sync::Arc;
use std::time::Duration;

use ::common::observability::testing::{MetricAssertion, MetricSnapshot};
use mc_service::actors::{JoinMedia, MeetingActorHandle, MeetingControllerActorHandle};
use mc_service::grpc::McMediaCoordinationService;
use mc_service::media_routing::teardown::EndMeetingCall;
use mc_service::media_signaling::ReceiveCapabilityDeclaration;
use proto_gen::dark_tower::internal::v1::{ApplyRefusal, HeartbeatVerdict, ReleaseCause};
use proto_gen::dark_tower::signaling::v1::{server_message, MediaKind, ReceiveCapability};
use test_common::coordination;
use test_common::media_dial::Token;
use test_common::media_session::slot;
use test_common::run_ready;
use test_common::{
    advance_live, client_media_config, handler_set, heartbeat_timing, join_media_on, mh_handler,
    MhEvent, MockMhRegistrationClient,
};
use tokio::sync::mpsc;

const MC: &str = "mc-detection-test";
const KC: (&str, &str) = ("key_custody", "operator");
const H_A: &str = "mh-a";
const H_B: &str = "mh-b";
const MS: Duration = Duration::from_millis(1);

// ----------------------------------------------------------------------------
// Rig
// ----------------------------------------------------------------------------

struct Rig {
    c: Arc<MeetingControllerActorHandle>,
    svc: McMediaCoordinationService,
    mh: Arc<MockMhRegistrationClient>,
}

fn rig() -> Rig {
    let c = Arc::new(test_common::make_controller(MC));
    let svc = McMediaCoordinationService::new(Arc::clone(&c), test_common::heartbeat_intake(MC));
    let mh = Arc::new(MockMhRegistrationClient::new());
    mh.heartbeat_to_controller(Arc::clone(&c));
    Rig { c, svc, mh }
}

fn n() -> Duration {
    heartbeat_timing().timeout()
}

fn interval() -> Duration {
    heartbeat_timing().interval()
}

fn lag_limit() -> Duration {
    heartbeat_timing().apply_lag_limit()
}

impl Rig {
    async fn create(&self, meeting: &str, handlers: &[&str]) -> MeetingActorHandle {
        let infos: Vec<_> = handlers.iter().map(|h| mh_handler(h)).collect();
        self.c
            .create_meeting(meeting.to_string(), handler_set(&infos))
            .await
            .unwrap();
        self.c
            .get_meeting_handle(meeting.to_string())
            .await
            .unwrap()
    }

    fn media(&self) -> JoinMedia {
        let mut media = join_media_on(Arc::clone(&self.mh));
        media.demand = ReceiveCapabilityDeclaration::parse(
            &ReceiveCapability {
                slots: vec![slot(0, MediaKind::Audio)],
            },
            true,
        )
        .expect("valid declaration")
        .truncate(client_media_config().receive_slot_caps);
        media
    }

    /// Join `p` (token sub `user-p`) and keep its outbound queue.
    async fn join(&self, meeting: &str, p: &str) -> mpsc::Receiver<bytes::Bytes> {
        let (tx, rx) = mpsc::channel::<bytes::Bytes>(512);
        let reply = self
            .c
            .join_connection(
                meeting.to_string(),
                format!("conn-{p}"),
                format!("user-{p}"),
                p.to_string(),
                String::new(),
                false,
                test_common::test_identity_key(),
                tx,
                self.media(),
            )
            .await
            .unwrap();
        reply.await.unwrap().unwrap();
        run_ready().await;
        rx
    }

    async fn connect(&self, meeting: &str, p: &str, h: &str, conn: &str) -> u32 {
        coordination::connect(&self.svc, meeting, p, h, conn).await
    }

    async fn disconnect(&self, meeting: &str, p: &str, h: &str, conn: &str) {
        coordination::disconnect(&self.svc, meeting, p, h, conn).await;
    }

    /// One heartbeat through the real intake; `(verdict, cause)`.
    async fn beat(
        &self,
        meeting: &str,
        h: &str,
        applied: u64,
        refusal: ApplyRefusal,
    ) -> (HeartbeatVerdict, ReleaseCause) {
        coordination::beat(
            &self.svc,
            coordination::heartbeat_request(MC, meeting, h, applied, refusal, &[]),
        )
        .await
        .unwrap()
    }

    fn ends(&self) -> Vec<EndMeetingCall> {
        self.mh.end_calls()
    }

    fn acked(&self, meeting: &str, h: &str) -> u64 {
        self.mh.acked_generation(meeting, h).unwrap_or(0)
    }

    /// RegisterMeeting pushes MC issued to `h` (any meeting).
    fn registers_to(&self, h: &str) -> usize {
        let endpoint = mh_handler(h).grpc_endpoint;
        self.mh
            .events()
            .iter()
            .filter(|e| matches!(e, MhEvent::Register { mh_grpc_endpoint, .. } if *mh_grpc_endpoint == endpoint))
            .count()
    }
}

async fn health(meeting: &MeetingActorHandle, h: &str) -> Option<&'static str> {
    meeting.probe_handler_for_test(h).await.unwrap().health
}

fn handler_set_urls(queue: &[server_message::Message]) -> Vec<Vec<String>> {
    queue
        .iter()
        .filter_map(|f| match f {
            server_message::Message::MediaHandlerSet(s) => Some(
                s.handlers
                    .iter()
                    .map(|h| h.media_handler_url.clone())
                    .collect(),
            ),
            _ => None,
        })
        .collect()
}

fn declarations(snap: &MetricSnapshot, trigger: &str) -> u64 {
    snap.counter("mc_media_handler_declarations_total")
        .with_labels(&[("trigger", trigger), KC])
        .delta()
}

// ----------------------------------------------------------------------------
// The deadline arm
// ----------------------------------------------------------------------------

/// Silence past N declares the handler Dead with NO further input: the
/// `sleep_until` arm fires on its own. Exactly N (+ resolution) is the
/// boundary: N - 1 ms is alive, N + 1 ms is Dead. The meeting is left with no
/// handler (the awaiting-handler gauge), the removal releases it with
/// HEARTBEAT_TIMEOUT, and its next heartbeat is RELEASED(HEARTBEAT_TIMEOUT).
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_silent_handler_is_declared_dead_by_the_deadline_arm_alone() {
    let r = rig();
    r.mh.silence(H_A);
    let meeting = r.create("m", &[H_A]).await;
    let _a = r.join("m", "a").await; // confirmed now: the silence clock starts
    let snap = MetricAssertion::snapshot();

    advance_live(n() - MS).await;
    assert_eq!(health(&meeting, H_A).await, Some("healthy"));
    snap.counter("mc_media_handler_declarations_total")
        .with_labels(&[("trigger", "heartbeat_timeout"), KC])
        .assert_unobserved();

    advance_live(MS * 2).await;
    run_ready().await;
    assert_eq!(health(&meeting, H_A).await, None, "removed from the set");
    // Histograms drain on read: assert them before any other read.
    snap.histogram("mc_media_handler_detection_seconds")
        .with_labels(&[("trigger", "heartbeat_timeout"), KC])
        .assert_observation_count(1);
    assert_eq!(declarations(&snap, "heartbeat_timeout"), 1);
    snap.gauge("mc_media_meetings_without_handlers")
        .with_labels(&[KC])
        .assert_value(1.0);
    let ends = r.mh.wait_for_end_calls(1, n()).await;
    assert_eq!(ends.len(), 1);
    assert_eq!(ends[0].handler_id, H_A);
    assert_eq!(ends[0].cause, ReleaseCause::HeartbeatTimeout);
    assert_eq!(
        r.beat("m", H_A, r.acked("m", H_A), ApplyRefusal::Unspecified)
            .await,
        (HeartbeatVerdict::Released, ReleaseCause::HeartbeatTimeout),
        "the removed handler's next heartbeat is RELEASED(its trigger)"
    );
    r.c.cancel();
}

/// The stall guard: a heartbeat stamped BEFORE the deadline that is still in
/// the actor's mailbox when the deadline arm fires is applied first — the arm
/// drains the queued backlog before judging — so the handler is not declared.
/// The actor is parked while the heartbeat is queued and the deadline passes,
/// so on release both the arm and the mailbox are ready; `select!` picks one at
/// random, so the scenario runs many times to exercise the arm-first order.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_heartbeat_queued_before_the_deadline_is_applied_before_the_judgment() {
    let r = Arc::new(rig());
    r.mh.silence(H_A);
    let snap = MetricAssertion::snapshot();
    for i in 0..16 {
        let id = format!("m{i}");
        let meeting = r.create(&id, &[H_A]).await;
        let _a = r.join(&id, "a").await;
        advance_live(n() - MS).await;
        let release = meeting.park_for_test().await.unwrap();
        let (r2, id2) = (Arc::clone(&r), id.clone());
        let applied = r.acked(&id, H_A);
        let beat =
            tokio::spawn(
                async move { r2.beat(&id2, H_A, applied, ApplyRefusal::Unspecified).await },
            );
        run_ready().await;
        tokio::time::advance(MS * 2).await; // the deadline passes while parked
        drop(release);
        run_ready().await;
        assert_eq!(
            beat.await.unwrap(),
            (HeartbeatVerdict::Owned, ReleaseCause::Unspecified)
        );
        assert_eq!(
            health(&meeting, H_A).await,
            Some("healthy"),
            "{id}: the queued heartbeat counted"
        );
        meeting.close_meeting("done".into()).await.unwrap();
        run_ready().await;
    }
    snap.counter("mc_media_handler_declarations_total")
        .with_labels(&[("trigger", "heartbeat_timeout"), KC])
        .assert_unobserved();
}

/// The stall guard is BOUNDED: a meeting whose mailbox never empties (here,
/// continuous unrelated traffic) still declares a silent handler, within one
/// interval of the deadline. A guard that waited for an empty mailbox would
/// never declare it.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn continuous_mailbox_traffic_cannot_postpone_a_declaration() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let r = rig();
    r.mh.silence(H_A);
    let meeting = r.create("m", &[H_A]).await;
    let _a = r.join("m", "a").await;
    let snap = MetricAssertion::snapshot();
    let served = Arc::new(AtomicUsize::new(0));
    let spam: Vec<_> = (0..8)
        .map(|_| {
            let (m, served) = (meeting.clone(), Arc::clone(&served));
            tokio::spawn(async move {
                while m.get_state().await.is_ok() {
                    served.fetch_add(1, Ordering::Relaxed);
                }
            })
        })
        .collect();
    advance_live(n() - MS).await;
    let at_deadline = served.load(Ordering::Relaxed);
    assert!(at_deadline > 0, "positive control: the traffic is flowing");
    advance_live(MS + interval()).await;
    assert!(
        served.load(Ordering::Relaxed) > at_deadline,
        "positive control: traffic kept flowing across the deadline"
    );
    assert_eq!(declarations(&snap, "heartbeat_timeout"), 1);
    assert_eq!(health(&meeting, H_A).await, None);
    for task in spam {
        task.abort();
    }
    r.c.cancel();
}

/// A meeting END queued in the backlog stops the drain: nothing is assessed
/// and no removal starts on a meeting being torn down. Its teardown releases
/// the (current) set with MEETING_ENDED only.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn an_end_queued_in_the_backlog_starts_no_removal() {
    let r = rig();
    r.mh.silence(H_A);
    let meeting = r.create("m", &[H_A]).await;
    let _a = r.join("m", "a").await;
    let snap = MetricAssertion::snapshot();
    advance_live(n() - MS).await;
    let release = meeting.park_for_test().await.unwrap();
    let m = meeting.clone();
    let close = tokio::spawn(async move { m.close_meeting("ended".into()).await });
    run_ready().await;
    tokio::time::advance(MS * 2).await;
    drop(release);
    run_ready().await;
    close.await.unwrap().unwrap();
    let ends = r.mh.wait_for_end_calls(1, n()).await;
    run_ready().await;
    assert_eq!(r.ends().len(), 1, "one release, the teardown's");
    assert_eq!(ends[0].cause, ReleaseCause::MeetingEnded);
    snap.counter("mc_media_handler_declarations_total")
        .with_labels(&[("trigger", "heartbeat_timeout"), KC])
        .assert_unobserved();
    r.c.cancel();
}

// ----------------------------------------------------------------------------
// The removal transaction
// ----------------------------------------------------------------------------

/// Two-handler meeting, both participants on both handlers; mh-a stops
/// beating. AT the moment MC issues `EndMeeting(mh-a)` (held there):
/// - the client already holds the set WITHOUT mh-a,
/// - mh-a's bindings are already released, mh-b's kept;
/// so BOTH precede the drain/EndMeeting, and the release carries
/// HEARTBEAT_TIMEOUT. The order set-push BEFORE binding-release is NOT
/// observable here: both happen in ONE actor turn with no mailbox processed
/// between them (`actors/meeting/health.rs::remove_handler_completely`), so it
/// is externally atomic and pinned by that function's structure. Afterwards: edges are on mh-b
/// (its latest snapshot carries both directions), nobody sharing mh-b was
/// reported unreachable, no post-removal set carries a token for mh-a, MC
/// never pushes to mh-a again, and the meeting's teardown releases ONLY the
/// current set, with MEETING_ENDED.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn removal_pushes_the_set_then_releases_bindings_then_ends_and_survivors_carry_the_edges() {
    let r = rig();
    let meeting = r.create("m", &[H_A, H_B]).await;
    let mut a = r.join("m", "a").await;
    let _b = r.join("m", "b").await;
    for (p, h, k) in [
        ("a", H_A, "a1"),
        ("b", H_A, "b1"),
        ("a", H_B, "a2"),
        ("b", H_B, "b2"),
    ] {
        assert_ne!(r.connect("m", p, h, k).await, 0);
    }
    advance_live(client_media_config().connect_settle_window + interval()).await;
    // Precondition: no set without mh-a was sent before the silence.
    let before = handler_set_urls(&test_common::drain_frames(&mut a));
    assert!(
        before
            .iter()
            .all(|urls| urls.contains(&mh_handler(H_A).webtransport_endpoint)),
        "every set before the silence still names mh-a: {before:?}"
    );

    r.mh.hold_ends();
    r.mh.silence(H_A);
    let snap = MetricAssertion::snapshot();
    advance_live(n() + interval()).await;
    let ends = r.mh.wait_for_end_calls(1, n()).await;
    // --- AT EndMeeting(mh-a): the set and the binding release came first.
    let queue = test_common::drain_frames(&mut a);
    let sets = handler_set_urls(&queue);
    assert_eq!(
        sets,
        vec![vec![mh_handler(H_B).webtransport_endpoint]],
        "exactly one set, without mh-a, already on the client's queue"
    );
    let probe_a = meeting.probe_handler_for_test(H_A).await.unwrap();
    assert!(
        probe_a.bindings.is_empty() && probe_a.connectivity.is_empty(),
        "mh-a's bindings and keys were released before its EndMeeting"
    );
    let probe_b = meeting.probe_handler_for_test(H_B).await.unwrap();
    assert_eq!(probe_b.bindings, vec!["a2", "b2"], "the survivor's kept");
    assert_eq!(ends[0].handler_id, H_A);
    assert_eq!(ends[0].cause, ReleaseCause::HeartbeatTimeout);
    r.mh.release_ends();
    run_ready().await;

    // Tokens: none in a post-removal set is for mh-a.
    for f in &queue {
        if let server_message::Message::MediaHandlerSet(s) = f {
            for h in &s.handlers {
                assert_ne!(Token::parse(&h.media_dial_token).str("aud"), H_A);
            }
        }
    }
    // Edges re-placed on mh-b: its latest snapshot forwards both directions.
    let last_b =
        r.mh.calls()
            .into_iter()
            .rfind(|c| c.expected_handler_id == H_B)
            .unwrap();
    assert_eq!(
        last_b.egress_stream_count, 2,
        "a<->b now ride mh-b: exactly one stream each way (one audio slot each)"
    );
    snap.counter("mc_media_unreachable_senders_total")
        .with_labels(&[KC])
        .assert_delta(0);
    snap.gauge("mc_media_meetings_without_handlers")
        .with_labels(&[KC])
        .assert_value(0.0);

    // One-way: a later structural change never pushes to mh-a.
    let to_a = r.registers_to(H_A);
    let _c = r.join("m", "c").await;
    run_ready().await;
    assert_eq!(
        r.registers_to(H_A),
        to_a,
        "no RegisterMeeting to a removed handler"
    );
    assert!(r.registers_to(H_B) > 0);

    // Teardown releases the CURRENT set only.
    meeting.close_meeting("done".into()).await.unwrap();
    r.mh.wait_for_end_calls(2, n()).await;
    run_ready().await;
    let ends: Vec<(String, ReleaseCause)> = r
        .ends()
        .into_iter()
        .map(|e| (e.handler_id, e.cause))
        .collect();
    assert_eq!(
        ends,
        vec![
            (H_A.to_string(), ReleaseCause::HeartbeatTimeout),
            (H_B.to_string(), ReleaseCause::MeetingEnded),
        ],
        "no second EndMeeting for the removed handler"
    );
    r.c.cancel();
}

/// A failed `EndMeeting` never blocks removal and is never retried: across
/// several intervals it is called exactly once, the removal completed, and the
/// handler's next heartbeat is RELEASED(HEARTBEAT_TIMEOUT) — the verdict is the
/// retry. The failure is counted.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_failed_end_meeting_is_not_retried_and_the_next_heartbeat_is_released() {
    let r = rig();
    r.mh.fail_ends();
    let meeting = r.create("m", &[H_A, H_B]).await;
    let _a = r.join("m", "a").await;
    r.mh.silence(H_A);
    let snap = MetricAssertion::snapshot();
    advance_live(n() * 4).await;
    assert_eq!(health(&meeting, H_A).await, None, "removal completed");
    let to_a: Vec<_> = r
        .ends()
        .into_iter()
        .filter(|e| e.handler_id == H_A)
        .collect();
    assert_eq!(to_a.len(), 1, "exactly one EndMeeting, never retried");
    snap.counter("mc_media_handler_removal_releases_total")
        .with_labels(&[
            ("trigger", "heartbeat_timeout"),
            ("outcome", "unavailable_exhausted"),
            KC,
        ])
        .assert_delta(1);
    snap.counter("mc_media_end_meeting_total")
        .with_labels(&[("outcome", "unavailable_exhausted"), KC])
        .assert_unobserved();
    // The zombie resumes: its own next heartbeat is the retry.
    assert_eq!(
        r.beat("m", H_A, r.acked("m", H_A), ApplyRefusal::Unspecified)
            .await,
        (HeartbeatVerdict::Released, ReleaseCause::HeartbeatTimeout)
    );
    assert!(
        meeting
            .probe_handler_for_test(H_A)
            .await
            .unwrap()
            .liveness
            .is_none(),
        "a RELEASED heartbeat recreates no liveness"
    );
    r.c.cancel();
}

/// A zombie's `Disconnected` after removal is `handler_not_in_set` and
/// touches nothing — even when its connection id COLLIDES with a key the
/// survivor holds. Positive control: the survivor's own Disconnected does
/// change its connectivity.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_zombie_disconnected_with_a_colliding_id_leaves_the_survivor_untouched() {
    let r = rig();
    let meeting = r.create("m", &[H_A, H_B]).await;
    let _a = r.join("m", "a").await;
    let _b = r.join("m", "b").await;
    r.connect("m", "a", H_A, "K").await;
    r.connect("m", "a", H_B, "Ka").await;
    r.connect("m", "b", H_B, "K").await;
    advance_live(client_media_config().connect_settle_window).await;
    r.mh.silence(H_A);
    advance_live(n() + interval()).await;
    assert_eq!(health(&meeting, H_A).await, None);
    let before = meeting.probe_handler_for_test(H_B).await.unwrap();

    let snap = MetricAssertion::snapshot();
    r.disconnect("m", "a", H_A, "K").await;
    snap.counter("mc_mh_notifications_unapplied_total")
        .with_labels(&[("reason", "handler_not_in_set"), KC])
        .assert_delta(1);
    let after = meeting.probe_handler_for_test(H_B).await.unwrap();
    assert_eq!(
        (after.connectivity, after.bindings),
        (before.connectivity.clone(), before.bindings.clone())
    );

    r.disconnect("m", "b", H_B, "K").await;
    let after = meeting.probe_handler_for_test(H_B).await.unwrap();
    assert!(
        !after
            .connectivity
            .contains(&("b".to_string(), "K".to_string())),
        "positive control: the survivor's own Disconnected applies"
    );
    assert!(before
        .connectivity
        .contains(&("b".to_string(), "K".to_string())));
    r.c.cancel();
}

/// Bulk release never returns an id to the allocator (R-35): a participant
/// that left with its mh-a binding outstanding, then mh-a's removal (whose
/// bulk release is proved to have run BEFORE the next admission), then a new
/// joiner — the joiner's sender id is not the departed one's, within ONE
/// admission epoch (no `sender_space_exhausted` reset happened, the only path
/// that may reissue an id; a leave-debounced KEK rotation keeps the allocator).
///
/// The allocator has no free path at all; this test is the regression net
/// against one being added for released bindings.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_bulk_released_sender_id_is_never_reissued() {
    let r = rig();
    let snap = MetricAssertion::snapshot();
    let meeting = r.create("m", &[H_A, H_B]).await;
    let _stay = r.join("m", "stay").await; // keeps the meeting alive
    let _a = r.join("m", "a").await;
    let sa = r.connect("m", "a", H_A, "Ka").await;
    assert_ne!(sa, 0);
    meeting.participant_leave("a".into()).await.unwrap();
    assert_eq!(
        meeting.probe_handler_for_test(H_A).await.unwrap().bindings,
        vec!["Ka"],
        "the departed participant's binding is outstanding (no Disconnected)"
    );
    r.mh.silence(H_A);
    advance_live(n() + interval()).await;
    let removed = meeting.probe_handler_for_test(H_A).await.unwrap();
    assert_eq!(removed.health, None, "mh-a removed");
    assert!(
        removed.bindings.is_empty(),
        "the bulk release ran BEFORE the next admission"
    );
    let _c = r.join("m", "c").await;
    let sc = r.connect("m", "c", H_B, "Kc").await;
    assert_ne!(sc, 0);
    assert_ne!(sc, sa, "a released id is never reissued within the epoch");
    snap.counter("mc_meeting_kek_generated_total")
        .with_labels(&[("trigger", "sender_space_exhausted"), KC])
        .assert_unobserved();
    r.c.cancel();
}

// ----------------------------------------------------------------------------
// Lag, contract faults and draining
// ----------------------------------------------------------------------------

/// Lag a meeting's only handler: MH stops applying (every push fails), a
/// structural change takes a new generation, and the heartbeat keeps
/// reporting the old one. Returns once the handler reads Lagging.
async fn make_lagging(r: &Rig, meeting: &MeetingActorHandle) {
    advance_live(interval() * 2).await;
    r.mh.fail_handler(H_A);
    let _b = r.join("m", "b").await;
    advance_live(interval() * 3).await;
    assert_eq!(health(meeting, H_A).await, Some("lagging"));
}

/// The ADR-0036 §10 apply-failure gate: the applied generation does not
/// advance, the handler goes Lagging (and MC re-publishes its snapshot to the
/// worker every heartbeat), and lag held past L = 2N is Impaired: declared
/// `apply_lag`, a `declared` suspicion, and EndMeeting(APPLY_LAG). The keys
/// survive while Lagging.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn lag_held_past_l_is_impaired_and_the_lagging_handler_is_republished() {
    let r = rig();
    let meeting = r.create("m", &[H_A]).await;
    let _a = r.join("m", "a").await;
    r.connect("m", "a", H_A, "Ka").await;
    let snap = MetricAssertion::snapshot();
    make_lagging(&r, &meeting).await;
    assert_eq!(
        meeting.probe_handler_for_test(H_A).await.unwrap().bindings,
        vec!["Ka"],
        "keys survive Lagging"
    );
    let pushes = r.registers_to(H_A);
    advance_live(interval() * 2).await;
    assert!(
        r.registers_to(H_A) > pushes,
        "while Lagging, MC re-publishes the snapshot to the worker"
    );
    assert_eq!(health(&meeting, H_A).await, Some("lagging"));

    advance_live(lag_limit()).await;
    run_ready().await;
    assert_eq!(health(&meeting, H_A).await, None, "Impaired: removed");
    // Histograms drain on read: assert them before any other read.
    snap.histogram("mc_media_handler_detection_seconds")
        .with_labels(&[("trigger", "apply_lag"), KC])
        .assert_observation_count(1);
    assert_eq!(declarations(&snap, "apply_lag"), 1);
    snap.counter("mc_media_handler_suspicions_total")
        .with_labels(&[("outcome", "declared"), KC])
        .assert_delta(1);
    let ends = r.mh.wait_for_end_calls(1, n()).await;
    assert_eq!(ends[0].cause, ReleaseCause::ApplyLag);
    r.c.cancel();
}

/// The paired arm: a catch-up before L returns to Healthy — a `cleared`
/// suspicion, NO declaration and no detection observation, even well past L.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_catch_up_before_l_clears_the_suspicion_without_a_declaration() {
    let r = rig();
    let meeting = r.create("m", &[H_A]).await;
    let _a = r.join("m", "a").await;
    let snap = MetricAssertion::snapshot();
    make_lagging(&r, &meeting).await;
    r.mh.unfail_handler(H_A);
    advance_live(interval() * 3).await;
    assert_eq!(health(&meeting, H_A).await, Some("healthy"));
    advance_live(lag_limit() * 2).await;
    assert_eq!(health(&meeting, H_A).await, Some("healthy"));
    // Histograms drain on read: the unobserved check comes FIRST, before any
    // other read of this snapshot, or it could pass vacuously.
    snap.histogram("mc_media_handler_detection_seconds")
        .with_labels(&[("trigger", "apply_lag"), KC])
        .assert_unobserved();
    snap.counter("mc_media_handler_declarations_total")
        .with_labels(&[("trigger", "apply_lag"), KC])
        .assert_unobserved();
    snap.counter("mc_media_handler_suspicions_total")
        .with_labels(&[("outcome", "cleared"), KC])
        .assert_delta(1);
    r.c.cancel();
}

/// INVALID / TRANSPORT_INCONSISTENT are MC contract faults: counted on every
/// heartbeat carrying them, never a declaration — even with the lag held far
/// past L. The positive twin: a load-dependent STREAM_CEILING is Impaired at
/// once.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_contract_fault_refusal_is_counted_and_never_declared_but_a_capacity_refusal_is() {
    let r = rig();
    r.mh.silence(H_A);
    let meeting = r.create("m", &[H_A]).await;
    let _a = r.join("m", "a").await;
    // One plain beat first: a pair's FIRST heartbeat owes nothing yet, so a
    // refusal on it is not (yet) evidence about MC's pending policy.
    r.beat("m", H_A, 0, ApplyRefusal::Unspecified).await;
    advance_live(interval()).await;
    let snap = MetricAssertion::snapshot();
    let mut beats = 0;
    for refusal in [ApplyRefusal::Invalid, ApplyRefusal::TransportInconsistent] {
        for _ in 0..(lag_limit().as_millis() / interval().as_millis() + 2) {
            assert_eq!(
                r.beat("m", H_A, 0, refusal).await.0,
                HeartbeatVerdict::Owned
            );
            beats += 1;
            advance_live(interval()).await;
        }
    }
    assert_eq!(health(&meeting, H_A).await, Some("contract_fault"));
    let invalid = snap
        .counter("mc_media_handler_heartbeat_anomalies_total")
        .with_labels(&[("reason", "apply_refused_invalid"), KC])
        .delta();
    let inconsistent = snap
        .counter("mc_media_handler_heartbeat_anomalies_total")
        .with_labels(&[("reason", "apply_refused_transport_inconsistent"), KC])
        .delta();
    assert_eq!(
        invalid + inconsistent,
        beats,
        "every heartbeat carrying one"
    );
    assert!(invalid > 0 && inconsistent > 0);
    snap.counter("mc_media_handler_declarations_total")
        .with_labels(&[("trigger", "apply_lag"), KC])
        .assert_unobserved();

    // Positive twin: the same pair, BEHIND what it owes (applied 0 < owed),
    // refuses for capacity -> Impaired at once.
    assert_eq!(
        r.beat("m", H_A, 0, ApplyRefusal::StreamCeiling).await.0,
        HeartbeatVerdict::Owned
    );
    run_ready().await;
    assert_eq!(
        health(&meeting, H_A).await,
        None,
        "capacity refusal: removed"
    );
    assert_eq!(declarations(&snap, "apply_lag"), 1);
    r.c.cancel();
}

/// DRAINING is recorded (for the planned path) and is NOT a removal: no
/// declaration, no detection, no EndMeeting, and the keys survive.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn draining_is_recorded_and_removes_nothing() {
    let r = rig();
    let meeting = r.create("m", &[H_A]).await;
    let _a = r.join("m", "a").await;
    r.connect("m", "a", H_A, "Ka").await;
    let snap = MetricAssertion::snapshot();
    r.mh.drain(H_A);
    advance_live(n() * 3).await;
    assert_eq!(health(&meeting, H_A).await, Some("draining"));
    assert_eq!(
        meeting.probe_handler_for_test(H_A).await.unwrap().bindings,
        vec!["Ka"]
    );
    assert!(r.ends().is_empty());
    // Histograms drain on read: every unobserved histogram check first.
    for t in ["heartbeat_timeout", "apply_lag"] {
        snap.histogram("mc_media_handler_detection_seconds")
            .with_labels(&[("trigger", t), KC])
            .assert_unobserved();
    }
    for t in ["heartbeat_timeout", "apply_lag"] {
        snap.counter("mc_media_handler_declarations_total")
            .with_labels(&[("trigger", t), KC])
            .assert_unobserved();
    }
    r.c.cancel();
}

// ----------------------------------------------------------------------------
// Layer-7 regression: an out-of-band writer is not a dead or impaired handler
// ----------------------------------------------------------------------------

/// MC-B2 Layer 7 regression (env-tests 26/37/38/39, whose `Injection`
/// fixture registers policy for MC's meeting directly on MH, at a generation
/// MC never sent). The handler then heartbeats an applied generation AHEAD of
/// what MC sent, and may report a refusal of THAT policy while MC's own is
/// installed. Neither is evidence about MC's handler:
/// - ahead heartbeats are liveness (watermark clamped to sent; counted
///   `applied_ahead_of_sent`), so the handler is never Dead, past N and L;
/// - a capacity refusal while caught up (applied >= owed) is ignored.
/// Positive twin: the same handler going silent IS declared Dead.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn an_out_of_band_ahead_generation_and_its_refusal_never_declare_the_handler() {
    let r = rig();
    r.mh.silence(H_A); // this test drives mh-a's heartbeats itself
    let meeting = r.create("m", &[H_A]).await;
    let _a = r.join("m", "a").await;
    let sent = r.acked("m", H_A);
    let snap = MetricAssertion::snapshot();
    let injected = sent + 100;
    let mut beats = 0u64;
    let mut elapsed = Duration::ZERO;
    while elapsed <= lag_limit() + n() {
        let refusal = [
            ApplyRefusal::Unspecified,
            ApplyRefusal::StreamCeiling,
            ApplyRefusal::Invalid,
        ][usize::try_from(beats % 3).unwrap()];
        assert_eq!(
            r.beat("m", H_A, injected, refusal).await,
            (HeartbeatVerdict::Owned, ReleaseCause::Unspecified)
        );
        beats += 1;
        advance_live(interval()).await;
        elapsed += interval();
    }
    assert_eq!(health(&meeting, H_A).await, Some("healthy"));
    let probe = meeting.probe_handler_for_test(H_A).await.unwrap();
    assert_eq!(
        probe.liveness.unwrap().applied_generation,
        sent,
        "the watermark is clamped to what MC sent"
    );
    snap.counter("mc_media_handler_heartbeat_anomalies_total")
        .with_labels(&[("reason", "applied_ahead_of_sent"), KC])
        .assert_delta(beats);
    snap.counter("mc_media_handler_heartbeat_anomalies_total")
        .with_labels(&[("reason", "apply_refused_invalid"), KC])
        .assert_unobserved(); // the SAME gate as assess: not an MC contract fault
    for t in ["heartbeat_timeout", "apply_lag"] {
        snap.counter("mc_media_handler_declarations_total")
            .with_labels(&[("trigger", t), KC])
            .assert_unobserved();
    }
    assert!(r.ends().is_empty(), "no removal release");

    // Positive twin: silence past N is still Dead.
    advance_live(n() + interval()).await;
    assert_eq!(health(&meeting, H_A).await, None);
    assert_eq!(declarations(&snap, "heartbeat_timeout"), 1);
    r.c.cancel();
}

/// The refusal gate must not swallow MH refusing MC's OWN push. A healthy
/// handler starts refusing (STREAM_CEILING, the applied generation stuck); a
/// structural change pushes a new generation; once the heartbeat's owed
/// generation catches up (at most a couple of intervals) the pair is declared
/// Impaired by the CAPACITY refusal — well before L, so not by lag held — and
/// released with APPLY_LAG. The racing first heartbeat is not asserted.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_real_capacity_refusal_of_mcs_push_is_declared_within_two_intervals() {
    let r = rig();
    let meeting = r.create("m", &[H_A]).await;
    let _a = r.join("m", "a").await;
    advance_live(interval() * 2).await;
    assert_eq!(health(&meeting, H_A).await, Some("healthy"));
    let snap = MetricAssertion::snapshot();
    r.mh.refuse(H_A, ApplyRefusal::StreamCeiling);
    let _b = r.join("m", "b").await; // MC pushes a new generation; refused
    let pushed = tokio::time::Instant::now();
    let mut declared_after = None;
    for step in 1..=4u32 {
        advance_live(interval()).await;
        if health(&meeting, H_A).await.is_none() {
            declared_after = Some(step);
            break;
        }
    }
    let step = declared_after.expect("a capacity refusal of MC's own push is declared");
    assert!(
        step <= 3,
        "within two intervals of the owed catch-up (step {step})"
    );
    assert!(
        tokio::time::Instant::now() - pushed < lag_limit(),
        "by the refusal path, not by lag held past L"
    );
    assert_eq!(declarations(&snap, "apply_lag"), 1);
    let ends = r.mh.wait_for_end_calls(1, n()).await;
    assert_eq!(ends[0].cause, ReleaseCause::ApplyLag);
    r.c.cancel();
}

/// The contract-fault anomaly uses the SAME gate as `assess`: an INVALID
/// refusal of MC's pending push is counted as an MC contract fault; once MC's
/// policy is installed the same refusal value is not.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_contract_fault_is_counted_only_while_mcs_policy_is_pending() {
    let r = rig();
    r.mh.silence(H_A);
    let _meeting = r.create("m", &[H_A]).await;
    let _a = r.join("m", "a").await;
    let sent = r.acked("m", H_A);
    // Establish owed == sent (two accepted beats), caught up.
    for _ in 0..2 {
        r.beat("m", H_A, sent, ApplyRefusal::Unspecified).await;
        advance_live(interval()).await;
    }
    let snap = MetricAssertion::snapshot();
    r.beat("m", H_A, sent, ApplyRefusal::Invalid).await;
    snap.counter("mc_media_handler_heartbeat_anomalies_total")
        .with_labels(&[("reason", "apply_refused_invalid"), KC])
        .assert_unobserved();
    // Positive twin: behind what it owes (applied below sent is stale, so the
    // pending case is a pair whose watermark is still behind: a fresh pair).
    let r2 = rig();
    r2.mh.silence(H_A);
    let _m2 = r2.create("m2", &[H_A]).await;
    let _a2 = r2.join("m2", "a").await;
    for _ in 0..2 {
        r2.beat("m2", H_A, 0, ApplyRefusal::Unspecified).await;
        advance_live(interval()).await;
    }
    let snap = MetricAssertion::snapshot();
    r2.beat("m2", H_A, 0, ApplyRefusal::Invalid).await;
    snap.counter("mc_media_handler_heartbeat_anomalies_total")
        .with_labels(&[("reason", "apply_refused_invalid"), KC])
        .assert_delta(1);
    r.c.cancel();
    r2.c.cancel();
}
