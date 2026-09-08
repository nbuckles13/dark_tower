//! Component tests for the participant → `sender_id` binding contract
//! (`internal.proto` `NotifyParticipantConnectedResponse.sender_id`).
//!
//! Drives the real accept path — `WebTransportServer::bind() → accept_loop() →
//! handle_connection` via `AcceptLoopRig`, byte-identical to `main.rs` — against
//! a mock MC whose `sender_id` answer is per-participant and configurable, and
//! asserts the ordering `JWT gate → NotifyParticipantConnected → validate → bind
//! → start_media_session` together with every terminal
//! `mh_media_session_starts_total{outcome}`.
//!
//! # Why these tests exist and the env-test does not replace them
//!
//! `crates/env-tests/tests/26_mh_quic.rs::test_mh_forwards_an_audio_datagram_back_to_its_sender`
//! is R-15's stated end-to-end proof, and it is a **single-participant
//! loopback**. That is the one configuration in which a wrong binding is
//! invisible: with exactly one sender in the meeting, "bind the only ordinal in
//! the pushed policy", "bind a hardcoded 1" and "bind the right one" all produce
//! byte-identical output, and its `assert_eq!(view.stream_id(), 0)` passes for
//! any of them because the subscriber's own slot coincides with MC's
//! `MAIN_AUDIO_SLOT_ID`. Un-`#[ignore]`ing it closes R-15's criterion; it does
//! **not** close the injection question. The two-participant and two-meeting
//! arms below are what close it.
//!
//! # Non-vacuity, deliberately constructed rather than asserted
//!
//! Expected ordinals are read back from the mock's own reply table
//! (`SenderReplies::allocated_for`) rather than written as literals, so no
//! assertion is an expectation someone wrote from memory that happens to match
//! (review protocol §Assertion Vacuity, mechanism 4). The table's values are
//! chosen to be **out of connect order and not 1/2**, so three separate wrong
//! implementations each fail a *different* assertion rather than jointly:
//!
//! - "binds the first ordinal it saw" → fails the second participant's arm;
//! - "binds a hardcoded 1" → fails both, since no fixture uses 1;
//! - "binds the other participant's ordinal" → fails per-participant
//!   correspondence, which mere distinctness would not catch.
//!
//! # THE BLIND SPOT THIS TIER CANNOT COVER — read before trusting a green run
//!
//! **These tests cannot see an identity-namespace mismatch between MH and MC,
//! and one shipped past them.** The mock's reply table is keyed on the same
//! `participant_id` string MH sends, so the fixture supplies **both sides of the
//! identity comparison** — one namespace on both sides of an `==`. Every arm
//! below can pass while MH names the party in a namespace MC does not key on,
//! and mutation testing does not help: it was faithfully red on the *binding*
//! axis while the *translation* axis had no coverage at all.
//!
//! That is exactly what happened. MH names the party by the meeting token's
//! `sub`; MC keyed its roster on a per-join UUID it mints itself. Disjoint
//! namespaces, so MC resolved nothing, ever — and this file was green
//! throughout. It took the live-cluster env-test
//! (`crates/env-tests/tests/26_mh_quic.rs`) and MC's own
//! `mc_media_sender_binding_responses_total{outcome="participant_unknown"}` at
//! 100% of attempts to surface it.
//!
//! The half that IS coverable here is **provenance**, not translation:
//! [`the_participant_id_sent_to_mc_is_the_validated_tokens_sub`] pins that MH
//! names the party by the validated token's `sub` and binds that same single
//! value. Whether MC agrees on the namespace is a two-real-services question,
//! and the env-test is the only tier that can ask it. Do not add a mock arm that
//! appears to cover it — a mock that disagrees about the namespace would be
//! testing the mock's configuration, not the contract.
//!
//! `MetricAssertion` arms are pinned to `flavor = "current_thread"`: the
//! recorder is per-thread and the accept loop must run on the test's own thread
//! for its emissions to be captured. See the same note in
//! `webtransport_integration.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "common/mod.rs"]
mod test_common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use common::observability::testing::MetricAssertion;
use mh_service::auth::MhJwtValidator;
use mh_service::grpc::McClient;
use mh_service::observability::metrics::MediaSessionStartOutcome;
use mh_service::routing::{MeetingKey, SenderId};
use mh_service::session::{MeetingRegistration, SessionManagerHandle};

use test_common::accept_loop_rig::AcceptLoopRig;
use test_common::jwks_rig::JwksRig;
use test_common::mock_mc::{
    start_mock_mc_server, MockBehavior, MockMcHandle, MockMcServer, SenderReplies,
};
use test_common::test_token_receiver;
use test_common::tokens::mint_meeting_token;
use test_common::wt_client::{connect_and_open_bi, write_mh_connect};

const HANDLER_ID: &str = "mh-binding-test-001";

/// Ordinals the mock allocates. **Not 1, not 2, and not in connect order** —
/// see the module docs; this is the property that separates three distinct
/// wrong implementations.
const SENDER_FIRST_TO_CONNECT: u32 = 77;
const SENDER_SECOND_TO_CONNECT: u32 = 12;

/// One MH accept path plus one mock MC, wired together.
struct BindingSuite {
    jwks: JwksRig,
    session_manager: SessionManagerHandle,
    wt: AcceptLoopRig,
    replies: SenderReplies,
    /// Held for the suite's lifetime; dropping it stops the mock MC.
    mc: MockMcHandle,
}

impl BindingSuite {
    async fn start(behavior: MockBehavior, replies: SenderReplies) -> Self {
        Self::start_with_capture(behavior, replies, None, None).await
    }

    /// As [`Self::start`], but capturing each inbound `NotifyParticipantDisconnected`.
    ///
    /// The disconnect notification is the reject-and-close observable for the
    /// decline arms: `close_declined_connection` sends it (for outcomes where
    /// [`mc_may_hold_a_registration`] is true) immediately before
    /// `handle_connection` returns and drops the connection, so its arrival
    /// proves the teardown ran. It survives the F9 fix, which removed the
    /// `status="error"` increment the arms previously used as the close proxy.
    async fn start_capturing_disconnects(
        behavior: MockBehavior,
        replies: SenderReplies,
    ) -> (
        Self,
        tokio::sync::mpsc::Receiver<
            proto_gen::dark_tower::internal::v1::NotifyParticipantDisconnectedRequest,
        >,
    ) {
        let (tx, rx) = tokio::sync::mpsc::channel(4);
        let suite = Self::start_with_capture(behavior, replies, None, Some(tx)).await;
        (suite, rx)
    }

    /// As [`Self::start`], but capturing each inbound `NotifyParticipantConnected`
    /// so a test can assert on the value that actually crossed the wire rather
    /// than on an intermediate MH held.
    async fn start_with_capture(
        behavior: MockBehavior,
        replies: SenderReplies,
        connected_tx: Option<
            tokio::sync::mpsc::Sender<
                proto_gen::dark_tower::internal::v1::NotifyParticipantConnectedRequest,
            >,
        >,
        disconnected_tx: Option<
            tokio::sync::mpsc::Sender<
                proto_gen::dark_tower::internal::v1::NotifyParticipantDisconnectedRequest,
            >,
        >,
    ) -> Self {
        let jwks = JwksRig::start(46, "mh-binding-integ-01").await;
        let session_manager = SessionManagerHandle::new();
        let jwt_validator = Arc::new(MhJwtValidator::new(jwks.jwks_client(), 300));
        let mc_client = Arc::new(McClient::new(test_token_receiver()));

        let wt = AcceptLoopRig::start_with(
            jwt_validator,
            session_manager.clone(),
            mc_client,
            HANDLER_ID.to_string(),
            32,
            Duration::from_secs(30),
        )
        .await;

        let mut mock = MockMcServer::new(behavior).with_sender_replies(replies.clone());
        if let Some(tx) = connected_tx {
            mock = mock.with_connected_tx(tx);
        }
        if let Some(tx) = disconnected_tx {
            mock = mock.with_disconnected_tx(tx);
        }
        let mc = start_mock_mc_server(mock).await;

        Self {
            jwks,
            session_manager,
            wt,
            replies,
            mc,
        }
    }

    /// Register `meeting_id` on this handler, pointed at the mock MC.
    async fn register(&self, meeting_id: &str) {
        self.session_manager
            .register_meeting(
                meeting_id.to_string(),
                MeetingRegistration {
                    mc_id: "mc-binding-test".to_string(),
                    mc_grpc_endpoint: format!("http://{}", self.mc.addr),
                    registered_at: Instant::now(),
                },
            )
            .await;
    }

    /// Register `meeting_id` pointed at an MC that will never answer.
    ///
    /// A reserved-discard address rather than a closed local port: the point is
    /// an endpoint the MC client cannot complete against.
    async fn register_with_unreachable_mc(&self, meeting_id: &str) {
        self.session_manager
            .register_meeting(
                meeting_id.to_string(),
                MeetingRegistration {
                    mc_id: "mc-binding-test".to_string(),
                    mc_grpc_endpoint: "http://127.0.0.1:1".to_string(),
                    registered_at: Instant::now(),
                },
            )
            .await;
    }

    /// Connect as `participant_id` into `meeting_id` and hold the connection.
    async fn connect(
        &self,
        meeting_id: &str,
        participant_id: &str,
    ) -> (
        wtransport::Connection,
        wtransport::stream::SendStream,
        wtransport::stream::RecvStream,
    ) {
        let token = mint_meeting_token(&self.jwks.keypair, meeting_id, participant_id);
        let (conn, mut send, recv) = connect_and_open_bi(&self.wt.url).await;
        write_mh_connect(&mut send, &token)
            .await
            .expect("failed to write MhClientMessage frame");
        (conn, send, recv)
    }

    /// The ordinal the mock says it allocated to `participant_id`, as a
    /// [`SenderId`]. Read from the mock's own record, never re-typed.
    fn allocated(&self, participant_id: &str) -> SenderId {
        SenderId::from_wire(self.replies.allocated_for(participant_id))
            .expect("fixture ordinals must be valid; a 0 here means the reply table is misspelled")
    }
}

/// Poll until `meeting`/`sender` is held, or the deadline passes.
///
/// Returns the holder, so callers assert on WHICH participant holds the ordinal
/// rather than merely that someone does.
async fn holder_within(
    session_manager: &SessionManagerHandle,
    meeting: &MeetingKey,
    sender: SenderId,
    deadline: Duration,
) -> Option<String> {
    let stop = Instant::now() + deadline;
    loop {
        if let Some(holder) = session_manager.sender_bindings().holder_of(meeting, sender) {
            return Some(holder);
        }
        if Instant::now() >= stop {
            return None;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

// ---------------------------------------------------------------------------
// The arms that close the injection question
// ---------------------------------------------------------------------------

/// Two participants in ONE meeting each bind THEIR OWN ordinal.
///
/// Asserts **per-participant correspondence**, not distinctness: two ordinals
/// swapped between two participants are still distinct, and a swap is exactly
/// the cross-participant misroute this contract exists to prevent. The
/// single-participant env-test cannot see any of this.
#[tokio::test]
async fn two_participants_in_one_meeting_each_bind_their_own_ordinal() {
    let replies = SenderReplies::default()
        .with("participant-alpha", SENDER_FIRST_TO_CONNECT)
        .with("participant-beta", SENDER_SECOND_TO_CONNECT);
    let suite = BindingSuite::start(MockBehavior::Accept, replies).await;
    suite.register("meeting-two-participants").await;
    let meeting = MeetingKey::new("meeting-two-participants");

    let alpha_sender = suite.allocated("participant-alpha");
    let beta_sender = suite.allocated("participant-beta");
    assert_ne!(
        alpha_sender, beta_sender,
        "fixture precondition: the two participants must be allocated DIFFERENT ordinals, or \
         this test cannot distinguish a correct binding from a swapped one"
    );

    let _alpha = suite
        .connect("meeting-two-participants", "participant-alpha")
        .await;
    let _beta = suite
        .connect("meeting-two-participants", "participant-beta")
        .await;

    assert_eq!(
        holder_within(
            &suite.session_manager,
            &meeting,
            alpha_sender,
            Duration::from_secs(5)
        )
        .await
        .as_deref(),
        Some("participant-alpha"),
        "the ordinal MC allocated to alpha must be held by ALPHA. Holding it for beta is the \
         cross-participant misroute: MH would forward beta's frames onto alpha's edges"
    );
    assert_eq!(
        holder_within(
            &suite.session_manager,
            &meeting,
            beta_sender,
            Duration::from_secs(5)
        )
        .await
        .as_deref(),
        Some("participant-beta"),
        "the ordinal MC allocated to beta must be held by BETA"
    );
}

/// The SAME `participant_id` string in two different meetings binds two
/// different ordinals, each visible only inside its own meeting.
///
/// `internal.proto`: a `sender_id` is a per-meeting ordinal, so ordinal 5 exists
/// concurrently in every meeting on a handler. The two-participant arm above
/// cannot catch a dropped meeting key — both its participants share one meeting.
#[tokio::test]
async fn one_participant_id_in_two_meetings_binds_per_meeting_ordinals() {
    // One reply table, one participant id, two meetings: the mock answers the
    // same ordinal in both, which is the WORST case for a registry that drops
    // the meeting key — it would look consistent. So the assertion is on the
    // per-meeting holder, and on the negative arm below.
    let replies = SenderReplies::default().with("participant-shared", SENDER_FIRST_TO_CONNECT);
    let suite = BindingSuite::start(MockBehavior::Accept, replies).await;
    suite.register("meeting-tenant-a").await;
    suite.register("meeting-tenant-b").await;

    let meeting_a = MeetingKey::new("meeting-tenant-a");
    let meeting_b = MeetingKey::new("meeting-tenant-b");
    let meeting_c = MeetingKey::new("meeting-tenant-c-never-registered");
    let sender = suite.allocated("participant-shared");

    let _a = suite
        .connect("meeting-tenant-a", "participant-shared")
        .await;
    let _b = suite
        .connect("meeting-tenant-b", "participant-shared")
        .await;

    assert_eq!(
        holder_within(
            &suite.session_manager,
            &meeting_a,
            sender,
            Duration::from_secs(5)
        )
        .await
        .as_deref(),
        Some("participant-shared"),
        "meeting A must hold its own binding"
    );
    assert_eq!(
        holder_within(
            &suite.session_manager,
            &meeting_b,
            sender,
            Duration::from_secs(5)
        )
        .await
        .as_deref(),
        Some("participant-shared"),
        "meeting B holds the SAME ordinal concurrently — that is legal and is why the registry \
         is keyed on (meeting, sender) rather than on the ordinal alone. A registry that \
         refused this would break every second meeting on the handler"
    );
    assert_eq!(
        suite
            .session_manager
            .sender_bindings()
            .holder_of(&meeting_c, sender),
        None,
        "the ordinal must not resolve in a meeting nobody bound it in; resolving across meetings \
         is the cross-meeting media-crossing primitive internal.proto bars"
    );
}

// ---------------------------------------------------------------------------
// Ordering and the six terminal outcomes
// ---------------------------------------------------------------------------

/// The happy path: a bound connection counts `started` and nothing else.
#[tokio::test(flavor = "current_thread")]
async fn a_bound_connection_counts_started() {
    let replies = SenderReplies::default().with("participant-ok", SENDER_FIRST_TO_CONNECT);
    let suite = BindingSuite::start(MockBehavior::Accept, replies).await;
    suite.register("meeting-started").await;

    let snap = MetricAssertion::snapshot();
    let _conn = suite.connect("meeting-started", "participant-ok").await;
    tokio::time::sleep(Duration::from_millis(800)).await;

    assert_started_only(&snap, MediaSessionStartOutcome::Started);
}

/// The PROVISIONAL-accept path binds too: a connection promoted by a late
/// `RegisterMeeting` runs the same sequence as one whose meeting was already
/// registered.
///
/// Two call sites reach the binding sequence — the already-registered branch and
/// the `RegistrationOutcome::Registered` branch after provisional accept. They go
/// through ONE helper precisely so they cannot drift, and this is the arm that
/// proves the second one is wired: before task 24 it had its own copy of the MC
/// notification, and a refactor that fixed only the first path would leave every
/// provisionally-accepted connection permanently unable to forward.
#[tokio::test]
async fn a_connection_promoted_after_provisional_accept_also_binds() {
    let replies = SenderReplies::default().with("participant-late", SENDER_SECOND_TO_CONNECT);
    let suite = BindingSuite::start(MockBehavior::Accept, replies).await;
    let meeting = MeetingKey::new("meeting-provisional");

    // Deliberately NOT registered yet: the connection enters provisional accept.
    let _conn = suite
        .connect("meeting-provisional", "participant-late")
        .await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        suite
            .session_manager
            .sender_bindings()
            .holder_of(&meeting, suite.allocated("participant-late")),
        None,
        "fixture precondition: nothing may bind while the meeting is unregistered — a binding \
         here would mean the JWT gate is not the only thing standing before the media path"
    );

    // RegisterMeeting arrives; the pending connection is promoted.
    suite.register("meeting-provisional").await;

    assert_eq!(
        holder_within(
            &suite.session_manager,
            &meeting,
            suite.allocated("participant-late"),
            Duration::from_secs(5)
        )
        .await
        .as_deref(),
        Some("participant-late"),
        "a promoted connection must run the same bind sequence as a directly-accepted one"
    );
}

/// `sender_id = 0` — MC has no answer. Declined, counted, connection closed.
///
/// The mock answers `acknowledged: true` alongside `sender_id: 0`, which is the
/// legal shape `internal.proto` describes. An implementation that gated the
/// media session on `acknowledged` would start a session here.
#[tokio::test(flavor = "current_thread")]
async fn mc_answering_zero_declines_the_session_and_closes() {
    // No reply for this participant: the table answers 0, which IS the contract's
    // "MC has no answer".
    let (suite, mut disconnect_rx) =
        BindingSuite::start_capturing_disconnects(MockBehavior::Accept, SenderReplies::default())
            .await;
    suite.register("meeting-zero").await;
    let meeting = MeetingKey::new("meeting-zero");

    let snap = MetricAssertion::snapshot();
    let _conn = suite.connect("meeting-zero", "participant-unknown").await;

    // REJECT-AND-CLOSE: the disconnect notification is sent as the connection is
    // torn down, so its arrival is the close signal (and the sync point for the
    // metric assertions below). See `BindingSuite::start_capturing_disconnects`.
    expect_reject_and_close(&mut disconnect_rx, "participant-unknown").await;

    assert_started_only(&snap, MediaSessionStartOutcome::DeclinedNoSenderBinding);
    // F9: a counted decline must NOT also land on the connection error series —
    // that series drives an immediate-rollback gate whose rollback restores the
    // pre-contract silent-no-media behaviour. The decline's home is
    // `mh_media_session_starts_total{outcome}`, asserted above.
    snap.counter("mh_webtransport_connections_total")
        .with_labels(&[("status", "error")])
        .assert_delta(0);
    assert!(
        !any_binding_exists(&suite.session_manager, &meeting),
        "a refused sender_id must leave no binding behind"
    );
}

/// A value above the 16-bit ordinal range. Declined on a DIFFERENT counter value
/// from `0` — that split is what makes the should-read-zero arm alertable.
#[tokio::test(flavor = "current_thread")]
async fn mc_answering_out_of_range_declines_on_its_own_outcome() {
    // 65_536 named as a literal deliberately. `routing/mod.rs`'s own docstring
    // sanctions boundary-test literals: a test written against the same constant
    // as the code passes no matter what the constant becomes.
    let replies = SenderReplies::default().with("participant-huge", 65_536);
    let (suite, mut disconnect_rx) =
        BindingSuite::start_capturing_disconnects(MockBehavior::Accept, replies).await;
    suite.register("meeting-out-of-range").await;
    let meeting = MeetingKey::new("meeting-out-of-range");

    let snap = MetricAssertion::snapshot();
    let _conn = suite
        .connect("meeting-out-of-range", "participant-huge")
        .await;

    // DoD 7 (@test F1): the out-of-range arm must assert reject-AND-close, not
    // only the counted outcome — the same standard as the `0` arm. Reject: no
    // binding left and no error-series pollution (F9). Close: the disconnect
    // notification.
    expect_reject_and_close(&mut disconnect_rx, "participant-huge").await;

    assert_started_only(
        &snap,
        MediaSessionStartOutcome::DeclinedSenderBindingOutOfRange,
    );
    snap.counter("mh_webtransport_connections_total")
        .with_labels(&[("status", "error")])
        .assert_delta(0);
    assert!(
        !any_binding_exists(&suite.session_manager, &meeting),
        "an out-of-range sender_id must leave no binding behind"
    );
}

/// The largest valid ordinal binds. The other half of the boundary: without it,
/// an off-by-one that rejected `65_535` would pass the arm above.
#[tokio::test]
async fn the_largest_valid_ordinal_binds() {
    let replies = SenderReplies::default().with("participant-max", 65_535);
    let suite = BindingSuite::start(MockBehavior::Accept, replies).await;
    suite.register("meeting-max").await;
    let meeting = MeetingKey::new("meeting-max");

    let _conn = suite.connect("meeting-max", "participant-max").await;

    assert_eq!(
        holder_within(
            &suite.session_manager,
            &meeting,
            suite.allocated("participant-max"),
            Duration::from_secs(5)
        )
        .await
        .as_deref(),
        Some("participant-max"),
        "65_535 is the largest VALID ordinal and must bind; rejecting it would be an off-by-one \
         that the out-of-range arm alone cannot see"
    );
}

/// Two participants handed the SAME live ordinal: the incumbent keeps it, the
/// newcomer is refused and counted.
#[tokio::test(flavor = "current_thread")]
async fn a_colliding_ordinal_is_refused_and_the_incumbent_keeps_it() {
    let replies = SenderReplies::default()
        .with("participant-incumbent", SENDER_FIRST_TO_CONNECT)
        .with("participant-newcomer", SENDER_FIRST_TO_CONNECT);
    let suite = BindingSuite::start(MockBehavior::Accept, replies).await;
    suite.register("meeting-collision").await;
    let meeting = MeetingKey::new("meeting-collision");
    let sender = suite.allocated("participant-incumbent");

    let _incumbent = suite
        .connect("meeting-collision", "participant-incumbent")
        .await;
    assert_eq!(
        holder_within(
            &suite.session_manager,
            &meeting,
            sender,
            Duration::from_secs(5)
        )
        .await
        .as_deref(),
        Some("participant-incumbent"),
        "fixture precondition: the incumbent must be bound before the newcomer arrives"
    );

    let snap = MetricAssertion::snapshot();
    let _newcomer = suite
        .connect("meeting-collision", "participant-newcomer")
        .await;
    tokio::time::sleep(Duration::from_millis(800)).await;

    assert_started_only(
        &snap,
        MediaSessionStartOutcome::DeclinedSenderBindingConflict,
    );
    assert_eq!(
        suite
            .session_manager
            .sender_bindings()
            .holder_of(&meeting, sender)
            .as_deref(),
        Some("participant-incumbent"),
        "INCUMBENT WINS. Handing the ordinal to the newcomer would blackhole the incumbent and \
         deliver its media to the newcomer's subscribers — the exact primitive the refusal exists \
         to prevent"
    );
}

/// MC unreachable: no binding can arrive, so the session is declined on the
/// reachability outcome rather than on any of MC's answers.
#[tokio::test(flavor = "current_thread")]
async fn an_unreachable_mc_declines_on_the_reachability_outcome() {
    let suite = BindingSuite::start(MockBehavior::Accept, SenderReplies::default()).await;
    suite
        .register_with_unreachable_mc("meeting-mc-unreachable")
        .await;

    let snap = MetricAssertion::snapshot();
    let _conn = suite
        .connect("meeting-mc-unreachable", "participant-any")
        .await;
    // The MC client's full retry budget plus the close jitter. Long by design:
    // the budget is deliberately NOT shortened (it rate-limits the reconnect
    // herd), so a test that waits less would be asserting on a race.
    tokio::time::sleep(Duration::from_secs(55)).await;

    assert_started_only(&snap, MediaSessionStartOutcome::DeclinedMcUnavailable);
}

/// The `participant_id` MH sends MC is the VALIDATED TOKEN'S `sub`, and the same
/// single value is what gets bound.
///
/// # Why this is a provenance test and not a plumbing test
///
/// After this contract, `participant_id` stopped being a notification payload
/// and became the **authorization query key**: MC answers an identity question
/// about it and MH binds the answer to a media route. Nothing structurally pins
/// it to the validated token — a later edit could substitute a client-supplied
/// hint from the connect envelope and every other test in this file would stay
/// green, because they never look at where the value came from.
///
/// So the token's `sub` here is deliberately unlike every other string in the
/// fixture (not the meeting id, not a display name, not a connection id), and
/// the assertion reads the **actual wire value** the mock MC received rather
/// than an intermediate MH held.
///
/// The second assertion is the half that matters more: the value MH **bound** is
/// the same one it **sent**. MH reads `claims.sub` once and threads that single
/// value into both, rather than reading it twice — two reads are two places for
/// an edit to substitute a hint into one of them, and a request-side assertion
/// alone would stay green while the bind side diverged.
#[tokio::test]
async fn the_participant_id_sent_to_mc_is_the_validated_tokens_sub() {
    const TOKEN_SUB: &str = "sub-from-the-validated-token-only";

    let replies = SenderReplies::default().with(TOKEN_SUB, SENDER_FIRST_TO_CONNECT);
    let (connected_tx, mut connected_rx) = tokio::sync::mpsc::channel(4);
    let suite =
        BindingSuite::start_with_capture(MockBehavior::Accept, replies, Some(connected_tx), None)
            .await;
    suite.register("meeting-provenance").await;
    let meeting = MeetingKey::new("meeting-provenance");

    let _conn = suite.connect("meeting-provenance", TOKEN_SUB).await;

    let request = tokio::time::timeout(Duration::from_secs(5), connected_rx.recv())
        .await
        .expect("NotifyParticipantConnected must arrive within 5s")
        .expect("the capture channel must not close before the payload arrives");

    assert_eq!(
        request.participant_id, TOKEN_SUB,
        "MH must name the party by the VALIDATED TOKEN'S `sub` and nothing else. A value sourced \
         from the client's connect envelope would make the identity MC answers about \
         client-asserted, which is the injection primitive this whole contract exists to close"
    );
    assert_eq!(
        holder_within(
            &suite.session_manager,
            &meeting,
            suite.allocated(TOKEN_SUB),
            Duration::from_secs(5)
        )
        .await
        .as_deref(),
        Some(TOKEN_SUB),
        "the value MH BOUND must be the same one it SENT. If these can differ, MC answered an \
         identity question about one party and MH routed media for another"
    );
}

/// MC refusing MH's credential is NOT MC being unavailable.
///
/// MC dialled fine and rejected the token, so **MC is healthy** and the remedy
/// is MH's outbound auth — a different service to open. Reporting it as
/// unavailability would send a responder to check MC's health while an expired
/// MH service token or a JWKS rotation takes down every connection on the
/// handler at once, which is exactly when the label is being read under
/// pressure.
#[tokio::test(flavor = "current_thread")]
async fn mc_refusing_mhs_credential_declines_on_the_auth_outcome() {
    let suite = BindingSuite::start(MockBehavior::Unauthenticated, SenderReplies::default()).await;
    suite.register("meeting-auth-rejected").await;

    let snap = MetricAssertion::snapshot();
    let _conn = suite
        .connect("meeting-auth-rejected", "participant-any")
        .await;
    // Terminal on the first attempt, so this needs no retry budget — which is
    // itself part of what the assertion pins: a value that took the full budget
    // would not have been recorded inside this window.
    tokio::time::sleep(Duration::from_millis(800)).await;

    assert_started_only(&snap, MediaSessionStartOutcome::DeclinedMcAuthRejected);
}

/// An MC endpoint MH cannot dial gets its OWN outcome, distinct from "MC did not
/// answer".
///
/// # This test was wrong first, and how it was wrong is the point
///
/// It originally asserted only `started == 0`, which is **fail-open on the
/// outcome label**: it passes if the code emits *any* non-started value,
/// including the wrong decline token. @test ruled it had to pin the token, and
/// pinning it revealed the arm was not exercising this outcome at all —
/// `get_mc_endpoint` returns `Some("")` for a registered meeting with an empty
/// endpoint, never `None`, so this fixture was landing on
/// `declined_mc_unavailable` while the assertion looked satisfied.
///
/// The fix was in the code, not the test. "The registration named an endpoint
/// MH cannot dial" and "MH dialled a real endpoint and got no answer" are
/// different faults with different first moves — fix the registration, versus
/// fix MC or the network — so they now carry different outcomes, split by
/// `MhError::McEndpointInvalid`. Folding them under `MhError::Config` would have
/// swept in an auth-header parse failure, which is neither.
///
/// # The fixture string was MEASURED, not reasoned about
///
/// The boundary is "does `tonic::transport::Endpoint::from_shared` reject it",
/// which is **not** the same as "does it look like a URL". Probed directly:
/// `""` → rejected, `"http://"` → rejected, `"http:// x"` → rejected, but
/// `"not-a-url"` → **accepted** (it parses as a relative URI and fails later, at
/// connect, as a reachability error). The first version of this test used
/// `"not-a-url"` for readability and landed on `declined_mc_unavailable` — the
/// pinned assertion is what caught it. An empty endpoint is also the shape a
/// real registration defect produces, so it is both the honest fixture and the
/// one that works.
#[tokio::test(flavor = "current_thread")]
async fn an_undialable_mc_endpoint_declines_on_the_registration_outcome() {
    let suite = BindingSuite::start(MockBehavior::Accept, SenderReplies::default()).await;
    // Registered — so the connection is not held in provisional accept — but
    // carrying an endpoint tonic will not parse.
    suite
        .session_manager
        .register_meeting(
            "meeting-bad-endpoint".to_string(),
            MeetingRegistration {
                mc_id: "mc-binding-test".to_string(),
                mc_grpc_endpoint: String::new(),
                registered_at: Instant::now(),
            },
        )
        .await;

    let snap = MetricAssertion::snapshot();
    let _conn = suite
        .connect("meeting-bad-endpoint", "participant-any")
        .await;
    tokio::time::sleep(Duration::from_millis(800)).await;

    assert_started_only(&snap, MediaSessionStartOutcome::DeclinedMcEndpointUnknown);
}

// ---------------------------------------------------------------------------
// Label-space coverage
// ---------------------------------------------------------------------------

/// Every catalogued `outcome` value is emitted by the recorder under the
/// documented label set, iterated from `ALL` rather than hand-listed.
///
/// Hand-listing is the enumeration that goes silently short when a seventh value
/// lands — the failure `PolicyApplyOutcome::ALL`'s own doc comment warns about.
#[tokio::test(flavor = "current_thread")]
async fn every_media_session_start_outcome_emits_under_the_catalogued_labels() {
    let snap = MetricAssertion::snapshot();
    for outcome in MediaSessionStartOutcome::ALL {
        mh_service::observability::metrics::record_media_session_start(outcome);
    }

    for outcome in MediaSessionStartOutcome::ALL {
        snap.counter("mh_media_session_starts_total")
            .with_labels(&[("outcome", outcome.as_label()), ("key_custody", "operator")])
            .assert_delta(1);
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Await the `NotifyParticipantDisconnected` a declined connection sends as it
/// closes, and assert it names `participant`.
///
/// This is the positive close observable for the decline arms after F9 removed
/// the `status="error"` increment they used to key on. It doubles as the sync
/// point: MH records the decline outcome before `close_declined_connection`
/// sends this, so a caller that has received it can assert the metric without a
/// sleep.
async fn expect_reject_and_close(
    disconnect_rx: &mut tokio::sync::mpsc::Receiver<
        proto_gen::dark_tower::internal::v1::NotifyParticipantDisconnectedRequest,
    >,
    participant: &str,
) {
    let req = tokio::time::timeout(Duration::from_secs(5), disconnect_rx.recv())
        .await
        .expect(
            "a declined connection must send NotifyParticipantDisconnected within 5s — that \
                 notification is the reject-and-close signal (DoD 7)",
        )
        .expect("disconnect capture channel closed before the payload arrived");
    assert_eq!(
        req.participant_id, participant,
        "the disconnect must name the declined participant, not some other connection"
    );
}

/// Assert that `expected` incremented by exactly 1 and every other outcome by 0.
///
/// Iterating `ALL` rather than checking only the expected value is what makes
/// this a positive control: a bug that emitted two outcomes for one connection,
/// or the wrong one, fails here rather than passing on the arm that happens to
/// be checked.
fn assert_started_only(
    snap: &common::observability::testing::MetricSnapshot,
    expected: MediaSessionStartOutcome,
) {
    for outcome in MediaSessionStartOutcome::ALL {
        let want = u64::from(outcome == expected);
        snap.counter("mh_media_session_starts_total")
            .with_labels(&[("outcome", outcome.as_label()), ("key_custody", "operator")])
            .assert_delta(want);
    }
}

/// Whether ANY ordinal is bound in `meeting`, across the whole valid range.
///
/// Used only by the reject arms, where the expected answer is "none".
fn any_binding_exists(session_manager: &SessionManagerHandle, meeting: &MeetingKey) -> bool {
    (1..=u16::MAX).any(|ordinal| {
        SenderId::from_wire(u32::from(ordinal)).is_ok_and(|sender| {
            session_manager
                .sender_bindings()
                .holder_of(meeting, sender)
                .is_some()
        })
    })
}
