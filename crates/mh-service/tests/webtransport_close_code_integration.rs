//! MH close codes on the real accept path (ADR-0039 §7; story 4 MH-CLOSE).
//!
//! Every session MH ends ON PURPOSE closes with exactly one `MhCloseCode`,
//! carried as a WebTransport SESSION close (one `CLOSE_WEBTRANSPORT_SESSION`
//! capsule, empty message, then FIN). Every other end MH initiates carries no
//! capsule and no CONNECT FIN, so "no code" keeps meaning transport loss.
//!
//! # What "observed" means here
//!
//! The task's "shim-observed" means observed by the REAL wtransport client over
//! the in-process QUIC pair (`AcceptLoopRig`), not by the `MediaTransport`
//! shim: the seam deliberately carries no lifecycle (`crate::transport` has no
//! `close`), so a shim-level close would certify something production lacks.
//! The primary observer is the vendored `Connection::session_close_received()`,
//! filled only by upstream's capsule parser; `closed()` alone cannot tell a
//! capsule from a QUIC close carrying the same number, and only the capsule
//! reaches a browser. See `common/wt_client.rs` `expect_session_close`.
//!
//! Decline arms that need a mock MC (no binding, out of range, conflict, MC
//! unavailable, MC auth, bad endpoint) live in
//! `media_session_binding_integration.rs`, extended in place.
//!
//! # Runtimes
//!
//! # What these arms do NOT observe (residuals)
//!
//! - **MH's own QUIC close after a capsule.** wtransport's client closes QUIC
//!   itself on receiving the capsule, so the coded arms' "never 0x104 / never an
//!   `MhCloseCode` on QUIC" secondary check cannot see MH's close there. It is real
//!   only on the code-less arms, where MH closes first. The completion-on-reset/FIN
//!   branch of the vendored patch is UNTESTED behaviourally (`docs/TODO.md`
//!   §Media Path Obligations: a raw HTTP/3 peer is needed).
//! - **A bare CONNECT FIN on the code-less arms.** With `end_without_code` turned
//!   into a plain drop, the multi-thread shutdown arm went red 20/20, but through
//!   the `H3_NO_ERROR` pin (it saw code 0), not by observing a FIN. Read it as
//!   supporting evidence, NOT a FIN detector; the primary control is the
//!   structural scan in `close_emission_scan.rs`.
//!
//! `MetricAssertion` is per-thread, so metric arms run on `current_thread`. The
//! code-less arms ALSO run on a multi-thread runtime: on `current_thread` a
//! dropped stream's FIN might never be driven before the QUIC close, so a
//! regression back to "close by drop" would pass vacuously there.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "common/mod.rs"]
mod test_common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use common::observability::testing::MetricAssertion;
use mh_service::config::CLOSE_SESSION_LINGER;
use mh_service::grpc::McClient;
use mh_service::routing::{MeetingKey, SenderId};
use mh_service::session::{MeetingRegistration, SessionManagerHandle};
use mh_service::webtransport::close::CloseCode;
use prost::Message;
use proto_gen::dark_tower::signaling::v1::{
    mh_client_message, MhClientMessage, MhCloseCode, RecoveryRequest,
};

use test_common::accept_loop_rig::AcceptLoopRig;
use test_common::close_metrics::{assert_closed_only, assert_no_close_counted};
use test_common::mock_mc::{
    start_mock_mc_server, MockBehavior, MockMcHandle, MockMcServer, SenderReplies,
};
use test_common::test_token_receiver;
use test_common::tokens::{meeting_claims, mint_aged_dial_token, mint_dial_token, DIAL_TTL_SECS};
use test_common::udp_proxy::UdpProxy;
use test_common::wt_client::{
    build_client, build_client_with_stream_window, connect_and_open_bi, connect_and_open_bi_with,
    expect_no_session_close, expect_session_close, expect_still_open, write_framed,
    write_mh_connect,
};
use test_common::TestKeypair;

const HANDLER_ID: &str = "mh-close-code-test";
const MEETING: &str = "meeting-close-codes";
const PARTICIPANT: &str = "participant-close";
const SENDER: u32 = 41;
const MC_ID: &str = "mc-close-test";

/// Generous bound for a close that involves no MC retry budget.
const PROMPT: Duration = Duration::from_secs(5);

struct Suite {
    session_manager: SessionManagerHandle,
    wt: AcceptLoopRig,
    _mc: MockMcHandle,
}

impl Suite {
    async fn start(max_connections: usize) -> Self {
        Self::start_with_disconnects(max_connections, None).await
    }

    /// [`Self::start`], with the mock MC reporting every
    /// `NotifyParticipantDisconnected` it receives on `disconnects`.
    async fn start_with_disconnects(
        max_connections: usize,
        disconnects: Option<
            tokio::sync::mpsc::Sender<
                proto_gen::dark_tower::internal::v1::NotifyParticipantDisconnectedRequest,
            >,
        >,
    ) -> Self {
        let session_manager =
            SessionManagerHandle::new_for_tests(mh_test_utils::admission::never_binding());
        let mc_client = Arc::new(McClient::new(test_token_receiver()));
        let wt = AcceptLoopRig::start_with(
            session_manager.clone(),
            mc_client,
            HANDLER_ID.to_string(),
            max_connections,
            test_common::accept_loop_rig::DEPLOYED_ADMISSION,
        )
        .await;
        let mut server = MockMcServer::new(MockBehavior::Accept)
            .with_sender_replies(SenderReplies::default().with(PARTICIPANT, SENDER));
        if let Some(tx) = disconnects {
            server = server.with_disconnected_tx(tx);
        }
        let mc = start_mock_mc_server(server).await;
        session_manager
            .register_meeting(
                MEETING.to_string(),
                MeetingRegistration {
                    mc_id: MC_ID.to_string(),
                    mc_grpc_endpoint: format!("http://{}", mc.addr),
                    heartbeat_interval: mh_test_utils::media_policy::TEST_HEARTBEAT_INTERVAL,
                    registered_at: Instant::now(),
                    media_dial_key: mh_test_utils::media_dial::meeting_dial_key(),
                },
                None,
                mh_test_utils::admission::fixture_policy_limits().max_registered_meetings,
            )
            .await
            .expect("a test registration is below the registration cap");
        Self {
            session_manager,
            wt,
            _mc: mc,
        }
    }

    fn valid_token(&self) -> String {
        mint_dial_token(MEETING, PARTICIPANT, HANDLER_ID)
    }

    /// Connect with a valid token to `url` and wait until the session is BOUND
    /// (the positive control every release/negative arm stands on).
    async fn connect_bound(
        &self,
        url: &str,
    ) -> (
        wtransport::Connection,
        wtransport::stream::SendStream,
        wtransport::stream::RecvStream,
    ) {
        self.connect_bound_with(&build_client(), url).await
    }

    async fn connect_bound_with(
        &self,
        client: &wtransport::Endpoint<wtransport::endpoint::endpoint_side::Client>,
        url: &str,
    ) -> (
        wtransport::Connection,
        wtransport::stream::SendStream,
        wtransport::stream::RecvStream,
    ) {
        let (conn, mut send, recv) = connect_and_open_bi_with(client, url).await;
        write_mh_connect(&mut send, &self.valid_token())
            .await
            .expect("write connect");
        let meeting = MeetingKey::new(MEETING);
        let sender = SenderId::from_wire(SENDER).unwrap();
        let stop = Instant::now() + PROMPT;
        while self
            .session_manager
            .sender_bindings()
            .holder_of(&meeting, sender)
            .is_none()
        {
            assert!(
                Instant::now() < stop,
                "positive control: the session never bound"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        (conn, send, recv)
    }

    async fn release(&self) {
        self.session_manager
            .end_meeting(
                MEETING.to_string(),
                MC_ID.to_string(),
                mh_service::session::release::ReleaseKind::MeetingEnded,
                PROMPT,
            )
            .await
            .expect("EndMeeting");
    }
}

/// One way to fail before the credential validates.
enum PreValidation {
    /// A connect envelope carrying this token.
    Token(String),
    /// These raw bytes, length-prefixed, as the first control message.
    Framed(Vec<u8>),
    /// A zero length prefix (a framing break).
    ZeroLength,
}

/// Security's "no oracle" condition as a test: every pre-validation exit and
/// every `media_dial` credential-failure kind (ADR-0039 §5 steps a–e) is the
/// SAME close at the peer — DECLINED, an empty message — with no other code on
/// any of them, and no kind-dependent delay MH adds (MH never fetches a key on
/// this path). Nothing is held for any of them. Positive control on the same
/// rig: a valid token is NOT closed.
#[tokio::test(flavor = "current_thread")]
async fn every_pre_validation_and_credential_failure_is_one_identical_declined() {
    use mh_test_utils::media_dial::{DialForge, ATTACKER_SEED, MEETING_SEED};

    let suite = Suite::start(32).await;
    let meeting_key = DialForge::new(MEETING_SEED);
    let attacker = DialForge::new(ATTACKER_SEED);
    let claims = || {
        DialForge::claims_json(
            MEETING,
            HANDLER_ID,
            PARTICIPANT,
            chrono::Utc::now().timestamp(),
            DIAL_TTL_SECS,
        )
    };
    let mut wrong_type = claims();
    wrong_type["token_type"] = serde_json::json!("meeting");
    // An AC-signed meeting token (another key entirely, with a `kid`).
    let ac_key = TestKeypair::new(61, "ac-kid");

    let empty_oneof = MhClientMessage {
        message: None,
        trace_parent: "00-ffffffffffffffffffffffffffffffff-ffffffffffffffff-01".to_string(),
        trace_state: String::new(),
    }
    .encode_to_vec();
    let recovery_first = MhClientMessage {
        message: Some(mh_client_message::Message::RecoveryRequest(
            RecoveryRequest {
                slot_id: 0,
                sender_id: 1,
                gop_counter: 0,
            },
        )),
        trace_parent: String::new(),
        trace_state: String::new(),
    }
    .encode_to_vec();

    let token_cases: Vec<(&str, String)> = vec![
        (
            "expired",
            mint_aged_dial_token(MEETING, PARTICIPANT, HANDLER_ID, DIAL_TTL_SECS + 100),
        ),
        ("another-meeting-key", attacker.sign(&claims())),
        (
            "attacker-key-announced-in-header",
            attacker.sign_with_header(
                &serde_json::json!({"alg": "EdDSA", "kid": "k", "jwk": attacker.jwk()}),
                &claims(),
            ),
        ),
        (
            "alg-none",
            meeting_key.sign_with_header(&serde_json::json!({"alg": "none"}), &claims()),
        ),
        (
            "alg-hs256",
            meeting_key.sign_with_header(&serde_json::json!({"alg": "HS256"}), &claims()),
        ),
        ("wrong-token-type", meeting_key.sign(&wrong_type)),
        (
            "ac-meeting-token",
            ac_key.sign_token(&meeting_claims(MEETING, PARTICIPANT, "meeting")),
        ),
        (
            "unheld-meeting",
            mint_dial_token("meeting-never-registered", PARTICIPANT, HANDLER_ID),
        ),
        ("malformed", "not-a-jwt".to_string()),
        ("oversized", "a".repeat(9000)),
    ];
    let mut cases: Vec<(&str, PreValidation)> = token_cases
        .iter()
        .map(|(name, t)| (*name, PreValidation::Token(t.clone())))
        .collect();
    cases.push((
        "undecodable-envelope",
        PreValidation::Framed(vec![0xff, 0xff, 0xff, 0xff]),
    ));
    cases.push(("empty-oneof", PreValidation::Framed(empty_oneof)));
    cases.push((
        "recovery-request-first",
        PreValidation::Framed(recovery_first),
    ));
    cases.push(("framing-break", PreValidation::ZeroLength));

    let snap = MetricAssertion::snapshot();
    let mut seen = Vec::new();
    for (name, case) in &cases {
        let (conn, mut send, _recv) = connect_and_open_bi(&suite.wt.url).await;
        match case {
            PreValidation::Token(token) => write_mh_connect(&mut send, token).await.unwrap(),
            PreValidation::Framed(bytes) => write_framed(&mut send, bytes).await.unwrap(),
            PreValidation::ZeroLength => send.write_all(&0u32.to_be_bytes()).await.unwrap(),
        }
        let elapsed = expect_session_close(&conn, MhCloseCode::Declined, PROMPT, name).await;
        seen.push((*name, elapsed));
    }
    // No MH-added, kind-dependent wait (jitter, extra round trip) on any arm.
    for (name, elapsed) in &seen {
        assert!(
            *elapsed < Duration::from_secs(2),
            "{name}: a pre-validation decline took {elapsed:?} — MH must add no delay here"
        );
    }
    // NEGATIVE arm: no other code was counted on any of them; all DECLINED.
    assert_closed_only(&snap, CloseCode::Declined, cases.len() as u64);
    // The kinds stay metric-only: every token case is a validation failure,
    // counted under its own reason (a partial-label delta would read only the
    // first matching series, so each reason is asserted exactly).
    let by_reason = [
        ("expired", 1_u64),
        ("signature_invalid", 3), // another meeting's key, attacker key, AC token
        ("malformed", 4),         // alg none, alg HS256, not-a-jwt, oversized
        ("wrong_type", 1),
        ("no_meeting_key", 1),
        ("aud_mismatch", 0),
    ];
    assert_eq!(
        by_reason.iter().map(|(_, n)| n).sum::<u64>(),
        token_cases.len() as u64,
        "every token case is classified exactly once"
    );
    for (reason, expected) in by_reason {
        snap.counter("mh_jwt_validations_total")
            .with_labels(&[
                ("result", "failure"),
                ("token_type", "media_dial"),
                ("failure_reason", reason),
            ])
            .assert_delta(expected);
    }
    // Nothing was held for any of them.
    assert_eq!(suite.session_manager.active_connection_count().await, 0);
    // F9 / unchanged accounting: pre-validation exits still report as errors.
    // The accept loop counts when `handle_connection` returns, which is AFTER
    // the close completes (the peer's reaction, or the linger) — a bounded poll
    // on that observation, not a sleep.
    suite.wt.wait_idle(PROMPT).await;
    snap.counter("mh_webtransport_connections_total")
        .with_labels(&[("status", "error")])
        .assert_delta(cases.len() as u64);

    // POSITIVE CONTROL: a valid token on the same rig is admitted and stays open,
    // so the DECLINEDs above came from validation, not from a broken rig.
    let (valid, _s, _r) = suite.connect_bound(&suite.wt.url).await;
    expect_still_open(&valid, Duration::from_millis(500), "valid token").await;
}

/// A dial for a meeting this handler does not hold — never registered, or
/// released — is DECLINED AT ONCE with nothing parked (ADR-0039 §3: the
/// pending-before-registration path is deleted). Positive control: the same
/// rig admits a dial for a meeting it does hold.
#[tokio::test(flavor = "current_thread")]
async fn a_dial_for_an_unheld_meeting_is_declined_at_once_with_nothing_parked() {
    let suite = Suite::start(32).await;
    let snap = MetricAssertion::snapshot();
    for (name, meeting) in [("never registered", "meeting-never-registered")] {
        let (conn, mut send, _recv) = connect_and_open_bi(&suite.wt.url).await;
        let token = mint_dial_token(meeting, PARTICIPANT, HANDLER_ID);
        write_mh_connect(&mut send, &token).await.unwrap();
        let elapsed = expect_session_close(&conn, MhCloseCode::Declined, PROMPT, name).await;
        assert!(
            elapsed < Duration::from_secs(2),
            "{name}: parked for {elapsed:?}"
        );
    }
    // Released: held, then EndMeeting drops its key.
    suite.release().await;
    let (conn, mut send, _recv) = connect_and_open_bi(&suite.wt.url).await;
    write_mh_connect(&mut send, &suite.valid_token())
        .await
        .unwrap();
    let elapsed = expect_session_close(&conn, MhCloseCode::Declined, PROMPT, "released").await;
    assert!(
        elapsed < Duration::from_secs(2),
        "released: parked for {elapsed:?}"
    );

    assert_closed_only(&snap, CloseCode::Declined, 2);
    snap.counter("mh_jwt_validations_total")
        .with_labels(&[
            ("result", "failure"),
            ("token_type", "media_dial"),
            ("failure_reason", "no_meeting_key"),
        ])
        .assert_delta(2);
    assert_eq!(suite.session_manager.active_connection_count().await, 0);

    // POSITIVE CONTROL: re-registered, the same token is admitted.
    suite
        .session_manager
        .register_meeting(
            MEETING.to_string(),
            MeetingRegistration {
                mc_id: MC_ID.to_string(),
                mc_grpc_endpoint: format!("http://{}", suite._mc.addr),
                heartbeat_interval: mh_test_utils::media_policy::TEST_HEARTBEAT_INTERVAL,
                registered_at: Instant::now(),
                media_dial_key: mh_test_utils::media_dial::meeting_dial_key(),
            },
            None,
            mh_test_utils::admission::fixture_policy_limits().max_registered_meetings,
        )
        .await
        .unwrap();
    let (valid, _s, _r) = suite.connect_bound(&suite.wt.url).await;
    expect_still_open(&valid, Duration::from_millis(300), "re-registered").await;
}

/// A connection still AWAITING its bind (MC's answer held) is closed
/// HANDLER_RELEASED at a REMOVAL-cause release (ADR-0039 §6: the handler is
/// leaving the meeting), and the abandoned answer is a no-op — exactly one
/// close counted, never re-coded DECLINED.
#[tokio::test(flavor = "current_thread")]
async fn a_removal_closes_a_connection_awaiting_its_bind_with_handler_released() {
    use common::observability::labels::RemovalTrigger;
    use mh_service::session::release::ReleaseKind;

    let suite = Suite::start(32).await;
    // A mock MC whose `NotifyParticipantConnected` is held: its capture channel
    // is pre-filled, so the handler's send waits until the test drains it.
    let (held_tx, mut held_rx) = tokio::sync::mpsc::channel(1);
    held_tx
        .send(proto_gen::dark_tower::internal::v1::NotifyParticipantConnectedRequest::default())
        .await
        .unwrap();
    let holding_mc = start_mock_mc_server(
        MockMcServer::new(MockBehavior::Accept)
            .with_sender_replies(SenderReplies::default().with(PARTICIPANT, SENDER))
            .with_connected_tx(held_tx),
    )
    .await;
    const AWAITING: &str = "meeting-awaiting-bind";
    suite
        .session_manager
        .register_meeting(
            AWAITING.to_string(),
            MeetingRegistration {
                mc_id: MC_ID.to_string(),
                mc_grpc_endpoint: format!("http://{}", holding_mc.addr),
                heartbeat_interval: mh_test_utils::media_policy::TEST_HEARTBEAT_INTERVAL,
                registered_at: Instant::now(),
                media_dial_key: mh_test_utils::media_dial::meeting_dial_key(),
            },
            None,
            mh_test_utils::admission::fixture_policy_limits().max_registered_meetings,
        )
        .await
        .unwrap();

    let snap = MetricAssertion::snapshot();
    let (conn, mut send, _recv) = connect_and_open_bi(&suite.wt.url).await;
    write_mh_connect(
        &mut send,
        &mint_dial_token(AWAITING, PARTICIPANT, HANDLER_ID),
    )
    .await
    .unwrap();
    // Admitted and awaiting its bind (positive control: it is held, not closed).
    let stop = Instant::now() + PROMPT;
    while suite.session_manager.active_connection_count().await != 1 {
        assert!(Instant::now() < stop, "the connection was never admitted");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    expect_still_open(&conn, Duration::from_millis(200), "awaiting its bind").await;

    suite
        .session_manager
        .end_meeting(
            AWAITING.to_string(),
            MC_ID.to_string(),
            ReleaseKind::Removal(RemovalTrigger::HeartbeatTimeout),
            PROMPT,
        )
        .await
        .unwrap();
    expect_session_close(&conn, MhCloseCode::HandlerReleased, PROMPT, "removal").await;

    // Let MC's held answer go: it must change nothing.
    while held_rx.try_recv().is_ok() {}
    drop(held_rx);
    suite.wt.wait_idle(PROMPT).await;
    assert_closed_only(&snap, CloseCode::HandlerReleased, 1);
    assert!(suite
        .session_manager
        .sender_bindings()
        .holder_of(
            &MeetingKey::new(AWAITING),
            SenderId::from_wire(SENDER).unwrap()
        )
        .is_none());
}

/// `EndMeeting` releasing a meeting closes its ACTIVE (bound, media-started)
/// session with HANDLER_RELEASED. Positive control: the session was bound and
/// open before the release.
#[tokio::test(flavor = "current_thread")]
async fn an_end_meeting_release_closes_an_active_session_with_handler_released() {
    let suite = Suite::start(32).await;
    let (conn, _send, _recv) = suite.connect_bound(&suite.wt.url).await;
    expect_still_open(&conn, Duration::from_millis(200), "bound before release").await;

    let snap = MetricAssertion::snapshot();
    suite.release().await;
    expect_session_close(&conn, MhCloseCode::HandlerReleased, PROMPT, "release").await;
    assert_closed_only(&snap, CloseCode::HandlerReleased, 1);
}

/// D6 (story 4 MH-3): the planned drain's DEADLINE shed reaches a real client
/// as HANDLER_RELEASED — the shed closes the session before the drain cancels
/// the server's shutdown token (which would end it code-less, below). Real
/// time, a short window; the production `drain::run` with the server's own
/// token. Positive control: bound and open until the deadline.
#[tokio::test(flavor = "current_thread")]
async fn the_drain_deadline_shed_closes_an_active_session_with_handler_released() {
    let (disc_tx, mut disconnects) = tokio::sync::mpsc::channel(8);
    let suite = Suite::start_with_disconnects(32, Some(disc_tx)).await;
    let (conn, _send, _recv) = suite.connect_bound(&suite.wt.url).await;
    let snap = MetricAssertion::snapshot();
    let (drain_tx, _drain_rx) = mh_service::drain::channel();
    let shutdown = suite.wt.shutdown_token();
    let window = Duration::from_millis(300);
    let started = Instant::now();
    let outcome = tokio::join!(
        mh_service::drain::run(&suite.session_manager, &drain_tx, window, &shutdown),
        async {
            // Positive control: bound and OPEN for most of the window.
            expect_still_open(&conn, Duration::from_millis(150), "open while draining").await;
            expect_session_close(&conn, MhCloseCode::HandlerReleased, PROMPT, "drain shed").await;
            assert!(
                started.elapsed() >= window,
                "closed by the DEADLINE shed, not earlier"
            );
        },
    )
    .0;
    assert_eq!(outcome.exit, mh_service::drain::DrainExit::Deadline);
    assert_eq!(outcome.shed, 1);
    assert!(outcome.shed_completed);
    assert!(shutdown.is_cancelled(), "cancelled only after the shed");
    assert_closed_only(&snap, CloseCode::HandlerReleased, 1);
    // No Disconnected: decided by `ConnectionEnd::MeetingReleased`, which is
    // independent of the `ReleaseKind`, so this also stands for the
    // owner-silence release (which cannot run unpaused at 300 s).
    suite.wt.wait_idle(PROMPT).await;
    assert!(
        matches!(
            disconnects.try_recv(),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty)
        ),
        "a released session sends MC no NotifyParticipantDisconnected"
    );
}

/// A client-initiated close carries no code: MH never echoes one, sends no
/// capsule, counts nothing.
#[tokio::test(flavor = "current_thread")]
async fn a_client_initiated_close_carries_no_code() {
    let suite = Suite::start(32).await;
    let (conn, _send, _recv) = suite.connect_bound(&suite.wt.url).await;
    let snap = MetricAssertion::snapshot();
    conn.close(wtransport::VarInt::from_u32(0), b"leave");
    let end = expect_no_session_close(&conn, PROMPT, "client close").await;
    assert!(
        matches!(end, wtransport::error::ConnectionError::LocallyClosed),
        "the client closed it: {end:?}"
    );
    // Let MH observe the end and run its cleanup before reading the counters.
    let stop = Instant::now() + PROMPT;
    while suite.session_manager.active_connection_count().await != 0 {
        assert!(Instant::now() < stop, "MH never saw the client close");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_no_close_counted(&snap);
}

/// Server shutdown cancellation carries no code (under the drain, only for
/// sessions no held meeting owns — every held one is shed first): no
/// capsule, no bare CONNECT FIN, and MH's own QUIC close is H3_NO_ERROR. Metric
/// half on `current_thread`.
#[tokio::test(flavor = "current_thread")]
async fn a_shutdown_cancellation_carries_no_code_and_counts_nothing() {
    let suite = Suite::start(32).await;
    let (conn, _send, _recv) = suite.connect_bound(&suite.wt.url).await;
    let snap = MetricAssertion::snapshot();
    suite.wt.shutdown();
    let end = expect_no_session_close(&conn, PROMPT, "shutdown (current_thread)").await;
    assert!(
        matches!(
            end,
            wtransport::error::ConnectionError::ApplicationClosed(_)
        ),
        "MH must close QUIC itself (H3_NO_ERROR), not leave the client to time out: {end:?}"
    );
    assert_no_close_counted(&snap);
}

/// The same, on a MULTI-THREAD runtime — where a regression to "close by drop"
/// can FIN the CONNECT stream (quinn's `SendStream` drop) before the implicit
/// QUIC close, which a browser reads as a clean close with code 0.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_shutdown_cancellation_sends_no_capsule_and_no_fin_multi_thread() {
    let suite = Suite::start(32).await;
    let (conn, _send, _recv) = suite.connect_bound(&suite.wt.url).await;
    suite.wt.shutdown();
    let end = expect_no_session_close(&conn, PROMPT, "shutdown (multi_thread)").await;
    assert!(
        matches!(
            end,
            wtransport::error::ConnectionError::ApplicationClosed(_)
        ),
        "MH must close QUIC itself (H3_NO_ERROR): {end:?}"
    );
}

/// A client close on a MULTI-THREAD runtime: MH never answers with a capsule or
/// a FIN.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_client_initiated_close_sends_nothing_back_multi_thread() {
    let suite = Suite::start(32).await;
    let (conn, _send, _recv) = suite.connect_bound(&suite.wt.url).await;
    conn.close(wtransport::VarInt::from_u32(0), b"leave");
    expect_no_session_close(&conn, PROMPT, "client close (multi_thread)").await;
}

/// THE LINGER BOUND, and that the session keeps its connection slot while it
/// lingers — and no longer.
///
/// The released session's peer is silenced (relay blackhole), so it neither
/// acknowledges nor reacts to the capsule: MH must end the wait on
/// `CLOSE_SESSION_LINGER` (read, never retyped), not on the peer.
///
/// Measured on MH's OWN held-connection counter (the one the
/// `MH_MAX_CONNECTIONS` gate reads), which drops only when the handler returns:
/// the slot is held for at least the bound (a lower bound, safe under any load)
/// and released not long after. No client dials and no wall-clock windows on the
/// client side, which made an earlier version of this arm load-sensitive.
async fn assert_slot_held_for_the_linger_bound_only(
    client: &wtransport::Endpoint<wtransport::endpoint::endpoint_side::Client>,
) {
    let suite = Suite::start(32).await;
    let relay = UdpProxy::start(suite.wt.addr).await;
    let (_silent, _s, _r) = suite.connect_bound_with(client, &relay.url()).await;

    relay.set_blackhole(true);
    let released_at = Instant::now();
    suite.release().await;
    suite
        .wt
        .wait_held(0, CLOSE_SESSION_LINGER + Duration::from_secs(10))
        .await;
    let held = released_at.elapsed();
    assert!(
        held >= CLOSE_SESSION_LINGER,
        "MH released the slot after {held:?}, before the linger bound — it did not wait"
    );
    assert!(
        held <= CLOSE_SESSION_LINGER + Duration::from_secs(3),
        "MH held the slot for {held:?}, well past the linger bound"
    );
}

/// A silent peer: the capsule is written, the peer never reacts.
#[tokio::test(flavor = "current_thread")]
async fn a_silent_peer_holds_its_slot_for_the_linger_bound_and_no_longer() {
    let snap = MetricAssertion::snapshot();
    assert_slot_held_for_the_linger_bound_only(&build_client()).await;
    // The capsule WAS written (the peer just never reacted): counted once.
    assert_closed_only(&snap, CloseCode::HandlerReleased, 1);
}

/// A peer that WITHHOLDS flow-control credit: its per-stream receive window is
/// smaller than the capsule and the blackhole stops every credit update, so the
/// capsule WRITE itself never completes. The one deadline must cover the write
/// too — otherwise the handler (and its slot) waited until the QUIC idle
/// timeout, an unauthenticated slot-holding primitive on the DECLINED path. The
/// capsule was never fully written, so nothing is counted.
#[tokio::test(flavor = "current_thread")]
async fn a_peer_withholding_credit_cannot_hold_its_slot_past_the_linger_bound() {
    // 8 bytes: enough to receive the session's response a few bytes at a time
    // while credit flows, less than the 9-byte capsule frame once it stops.
    let starved = build_client_with_stream_window(8);
    let snap = MetricAssertion::snapshot();
    assert_slot_held_for_the_linger_bound_only(&starved).await;
    assert_no_close_counted(&snap);
}

/// A session already gone when MH goes to close it (the client closed before
/// sending anything, so MH's `accept_bi` fails — a pre-validation exit) sends
/// nothing and is NOT counted; the connection is still accounted as an error.
#[tokio::test(flavor = "current_thread")]
async fn a_session_already_gone_is_skipped_and_not_counted() {
    let suite = Suite::start(32).await;
    let snap = MetricAssertion::snapshot();
    let client = build_client();
    let conn = client.connect(&suite.wt.url).await.expect("connect");
    conn.close(wtransport::VarInt::from_u32(0), b"");
    suite.wt.wait_idle(PROMPT).await;
    snap.counter("mh_webtransport_connections_total")
        .with_labels(&[("status", "error")])
        .assert_delta(1);
    assert_no_close_counted(&snap);
}
