//! Integration tests for the mh-service WebTransport accept path,
//! `media_dial` admission on it, and MC notification lifecycle.
//!
//! # Migration note — ADR-0032 Step 2
//!
//! These tests previously used `tests/common/wt_rig.rs`, which bypassed
//! `WebTransportServer::accept_loop` by calling `handle_connection` directly
//! so assertions could match on `MhError` variants. ADR-0032 §Decision
//! rejected that bypass. Tests now drive the real `accept_loop` via
//! `tests/common/accept_loop_rig.rs` (byte-identical to `main.rs:258-260`)
//! and observe outcomes via three channels:
//!
//! - **Metric emissions** (`common::observability::testing::MetricAssertion`):
//!   `mh_webtransport_connections_total{status}`, `mh_jwt_validations_total`,
//!   `mh_webtransport_handshake_duration_seconds`, `mh_active_connections`.
//! - **Session-manager state** (`SessionManagerHandle::active_connection_count`).
//! - **Mock MC channels** (for the connect/disconnect notification test).
//!
//! # Integration value over unit tests
//!
//! - `auth/mod.rs::tests` covers the `media_dial` verifier's rejection matrix
//!   (ADR-0039 §5 steps a–f) against the pure functions directly, and
//!   `tests/media_dial_admission_integration.rs` covers the full admission
//!   matrix end to end. `wrong_token_type_rejected_on_wt_accept_path` is
//!   retained here because it adds a distinguishing signal:
//!   `session_manager.active_connection_count()` remaining at 0 catches a
//!   call-site refactor that would bypass admission.
//! - `session/mod.rs::tests` covers `SessionManagerHandle` state transitions
//!   (admission, the pending cap, bind, removal, active counts). There is no
//!   pending-before-registration path (ADR-0039 §3).
//! - `mc_client_integration.rs` covers `McClient` retry semantics.
//!
//! This file proves the *accept path* actually wires admission, the session
//! manager and the MC client together, and that the real `accept_loop` emits
//! the correct metric labels on each per-connection outcome.
//!
//! # Tokio flavor pinning — do not "simplify"
//!
//! Every `MetricAssertion`-using test below is pinned to
//! `#[tokio::test(flavor = "current_thread")]`. This is LOAD-BEARING:
//! `MetricAssertion` binds a per-thread recorder, and the accept_loop
//! emits its status counter + handshake histogram from inside a
//! `tokio::spawn` task. Under `flavor = "multi_thread"` those tasks can
//! land on a worker thread, the per-thread recorder sees nothing, and
//! `assert_delta(N)` silently passes as 0. See
//! `common/src/observability/testing.rs:60-72`. The `mc_notify_*` test
//! (mock_mc channels, no `MetricAssertion`) is intentionally left unpinned.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "common/mod.rs"]
mod test_common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use common::observability::testing::MetricAssertion;
use mh_service::grpc::McClient;
use mh_service::session::SessionManagerHandle;
use proto_gen::dark_tower::internal::v1::DisconnectReason;
use tokio::sync::mpsc;

use test_common::accept_loop_rig::AcceptLoopRig;
use test_common::mock_mc::{start_mock_mc_server, MockBehavior, MockMcServer, SenderReplies};
use test_common::test_token_receiver;
use test_common::tokens::{mint_aged_dial_token, mint_dial_token, DIAL_TTL_SECS};
use test_common::wt_client::{connect_and_open_bi, write_framed, write_mh_connect};

// ---------------------------------------------------------------------------
// Rig builder
// ---------------------------------------------------------------------------

/// This rig's handler id — the `aud` its dial tokens carry.
const HANDLER_ID: &str = "mh-test-001";

struct WtSuite {
    session_manager: SessionManagerHandle,
    wt: AcceptLoopRig,
}

impl WtSuite {
    async fn start(mc_client: Arc<McClient>) -> Self {
        let session_manager =
            SessionManagerHandle::new_for_tests(mh_test_utils::admission::never_binding());
        let wt =
            AcceptLoopRig::start(session_manager.clone(), mc_client, HANDLER_ID.to_string()).await;
        Self {
            session_manager,
            wt,
        }
    }

    /// Register `meeting_id` with the forge's key, pointed at `mc_endpoint`.
    async fn register(&self, meeting_id: &str, mc_id: &str, mc_endpoint: &str) {
        self.session_manager
            .register_meeting(
                meeting_id.to_string(),
                mh_service::session::MeetingRegistration {
                    mc_id: mc_id.to_string(),
                    mc_grpc_endpoint: mc_endpoint.to_string(),
                    heartbeat_interval: mh_test_utils::media_policy::TEST_HEARTBEAT_INTERVAL,
                    registered_at: Instant::now(),
                    media_dial_key: mh_test_utils::media_dial::meeting_dial_key(),
                },
                None,
                mh_test_utils::admission::fixture_policy_limits().max_registered_meetings,
            )
            .await
            .expect("a test registration is below the registration cap");
    }
}

/// Build an `McClient` pointed at the supplied endpoint (we do not actually
/// need per-call routing for tests that do not care about notifications).
fn make_mc_client() -> Arc<McClient> {
    Arc::new(McClient::new(test_token_receiver()))
}

/// Open a WT bi-stream and write the supplied token wrapped in the typed
/// `MhClientMessage{ConnectRequest{media_dial_token}}` envelope. Returns the live
/// connection + streams so the caller can control its own disconnect.
async fn connect_and_send_jwt(
    wt_url: &str,
    token: &str,
) -> (
    wtransport::Connection,
    wtransport::stream::SendStream,
    wtransport::stream::RecvStream,
) {
    let (conn, mut send, recv) = connect_and_open_bi(wt_url).await;
    write_mh_connect(&mut send, token)
        .await
        .expect("failed to write MhClientMessage frame");
    (conn, send, recv)
}

/// Bounded-deadline poll on `active_connection_count` — matches the pattern
/// used across migrated tests for observing accept-path side-effects without
/// peeking at the per-handler `Result`.
async fn wait_for_active_count(
    session_manager: &SessionManagerHandle,
    expected: usize,
    deadline: Duration,
) -> bool {
    let stop = Instant::now() + deadline;
    while Instant::now() < stop {
        if session_manager.active_connection_count().await == expected {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    false
}

// ---------------------------------------------------------------------------
// media_dial admission on the accept path
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn valid_meeting_jwt_connection_accepted_and_tracked() {
    let mc_client = make_mc_client();
    let suite = WtSuite::start(mc_client).await;

    // Register the meeting: admission needs its key.
    suite
        .register("meeting-wt-valid", "mc-wt-test", "http://localhost:1")
        .await;

    let snap = MetricAssertion::snapshot();
    let token = mint_dial_token("meeting-wt-valid", "user-valid", HANDLER_ID);
    let (_conn, _send, _recv) = connect_and_send_jwt(&suite.wt.url, &token).await;

    // Side-effect observation: the real accept_loop + handler must register
    // the active connection within a bounded deadline.
    assert!(
        wait_for_active_count(&suite.session_manager, 1, Duration::from_secs(3)).await,
        "accept path did not register an active connection within 3s",
    );

    // Assert histogram FIRST — a `MetricAssertion` snapshot drains histograms on
    // read per common::observability::testing §"Delta semantics"; counters
    // are idempotent under re-reads.
    snap.histogram("mh_webtransport_handshake_duration_seconds")
        .assert_observation_count_at_least(1);
    snap.counter("mh_webtransport_connections_total")
        .with_labels(&[("status", "accepted")])
        .assert_delta(1);
    snap.counter("mh_jwt_validations_total")
        .with_labels(&[
            ("result", "success"),
            ("token_type", "media_dial"),
            ("failure_reason", "none"),
        ])
        .assert_delta(1);
}

#[tokio::test(flavor = "current_thread")]
async fn missing_jwt_stream_closed_before_write_rejects_connection() {
    // The client opens the bi-stream and closes it without sending any bytes.
    // The server's `read_framed_message` fails and the handler returns an
    // error before admission runs. Observation shape:
    //   - `mh_webtransport_connections_total{status=error}` increments (accept_loop
    //     emits on any handler `Err(_)`).
    //   - `mh_jwt_validations_total` does NOT fire with `result=failure` or
    //     `result=success` because the handler never reached step 4.
    //
    // Partial-label `assert_delta(0)` works because `find_of_kind`
    // (common/src/observability/testing.rs:279-281) returns the FIRST
    // matching entry; any V>0 in any matching entry trips the assertion.
    // Do NOT use partial-label filters with `assert_delta(N>0)` — ambiguous
    // when multiple matching entries exist.
    let mc_client = make_mc_client();
    let suite = WtSuite::start(mc_client).await;

    let snap = MetricAssertion::snapshot();
    let (conn, mut send, _recv) = connect_and_open_bi(&suite.wt.url).await;
    // Close the send side immediately; the server's read_exact will get 0 bytes.
    send.finish().await.ok();
    drop(conn);

    // Bounded wait so the spawned handler has time to observe the close,
    // return Err, and the accept_loop emits the error counter. We avoid
    // polling the MetricAssertion itself because the API is panic-only and
    // per-snapshot recorder binding breaks under repeated-snapshot patterns
    // (see common::observability::testing §"Invariants — nested snapshots").
    tokio::time::sleep(Duration::from_millis(500)).await;

    snap.counter("mh_webtransport_connections_total")
        .with_labels(&[("status", "error")])
        .assert_delta(1);
    snap.counter("mh_jwt_validations_total")
        .with_labels(&[("result", "failure")])
        .assert_delta(0);
    snap.counter("mh_jwt_validations_total")
        .with_labels(&[("result", "success")])
        .assert_delta(0);
}

#[tokio::test(flavor = "current_thread")]
async fn malformed_envelope_bytes_rejected_on_wt_accept_path() {
    // The client sends a framed payload of garbage bytes that fails
    // `MhClientMessage::decode`. Locks in the metric-shape contract for
    // pre-validation decode failure: the accept_loop emits
    // `mh_webtransport_connections_total{status=error}`, and `mh_jwt_validations_total`
    // does NOT fire — no JWT was extracted, validation never ran. A future
    // refactor that folds decode failure into the validation metric path will
    // fail this test loudly.
    let mc_client = make_mc_client();
    let suite = WtSuite::start(mc_client).await;

    let snap = MetricAssertion::snapshot();
    let (_conn, mut send, _recv) = connect_and_open_bi(&suite.wt.url).await;
    write_framed(&mut send, &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF])
        .await
        .expect("failed to write garbage frame");

    // Bounded wait so the spawned handler observes the bytes, decode fails,
    // returns Err, and accept_loop emits the error counter on spawn exit.
    tokio::time::sleep(Duration::from_millis(500)).await;

    snap.counter("mh_webtransport_connections_total")
        .with_labels(&[("status", "error")])
        .assert_delta(1);
    snap.counter("mh_jwt_validations_total")
        .with_labels(&[("result", "failure")])
        .assert_delta(0);
    snap.counter("mh_jwt_validations_total")
        .with_labels(&[("result", "success")])
        .assert_delta(0);
}

#[tokio::test(flavor = "current_thread")]
async fn empty_envelope_oneof_rejected_on_wt_accept_path() {
    // The client sends a well-formed `MhClientMessage` with `message: None`
    // (no oneof variant set). Decode succeeds, but the handler's match arm on
    // the `connect_request` variant fails. Same observable as the malformed
    // bytes case: accept_loop emits `status=error`, no JWT-validations counter
    // increment. Distinguishes "decode succeeded but no variant" from "decode
    // failed" — both must close, neither emits validation counters.
    use prost::Message;
    use proto_gen::dark_tower::signaling::v1::MhClientMessage;

    let mc_client = make_mc_client();
    let suite = WtSuite::start(mc_client).await;

    let snap = MetricAssertion::snapshot();
    let envelope = MhClientMessage {
        message: None,
        trace_parent: String::new(),
        trace_state: String::new(),
    };
    let encoded = envelope.encode_to_vec();

    let (_conn, mut send, _recv) = connect_and_open_bi(&suite.wt.url).await;
    write_framed(&mut send, &encoded)
        .await
        .expect("failed to write empty-oneof envelope");

    tokio::time::sleep(Duration::from_millis(500)).await;

    snap.counter("mh_webtransport_connections_total")
        .with_labels(&[("status", "error")])
        .assert_delta(1);
    snap.counter("mh_jwt_validations_total")
        .with_labels(&[("result", "failure")])
        .assert_delta(0);
    snap.counter("mh_jwt_validations_total")
        .with_labels(&[("result", "success")])
        .assert_delta(0);
}

#[tokio::test(flavor = "current_thread")]
async fn recovery_request_as_first_message_rejected_on_wt_accept_path() {
    // A known non-connect variant (story 3 `RecoveryRequest`) as the FIRST
    // framed message is not a connect (signaling.proto `MhClientMessage`):
    // the same generic invalid-connect reject as an empty oneof, with no JWT
    // ever extracted, so no validation counter moves.
    use prost::Message;
    use proto_gen::dark_tower::signaling::v1::{
        mh_client_message, MhClientMessage, RecoveryRequest,
    };

    let mc_client = make_mc_client();
    let suite = WtSuite::start(mc_client).await;

    let snap = MetricAssertion::snapshot();
    let envelope = MhClientMessage {
        message: Some(mh_client_message::Message::RecoveryRequest(
            RecoveryRequest {
                slot_id: 0,
                sender_id: 1,
                gop_counter: 0,
            },
        )),
        trace_parent: String::new(),
        trace_state: String::new(),
    };
    let encoded = envelope.encode_to_vec();

    let (_conn, mut send, _recv) = connect_and_open_bi(&suite.wt.url).await;
    write_framed(&mut send, &encoded)
        .await
        .expect("failed to write recovery-request-first envelope");

    tokio::time::sleep(Duration::from_millis(500)).await;

    snap.counter("mh_webtransport_connections_total")
        .with_labels(&[("status", "error")])
        .assert_delta(1);
    snap.counter("mh_jwt_validations_total")
        .with_labels(&[("result", "failure")])
        .assert_delta(0);
    snap.counter("mh_jwt_validations_total")
        .with_labels(&[("result", "success")])
        .assert_delta(0);
}

#[tokio::test(flavor = "current_thread")]
async fn expired_dial_token_rejected_on_wt_accept_path() {
    let mc_client = make_mc_client();
    let suite = WtSuite::start(mc_client).await;
    // Registered, so the token fails on its expiry (step e), not on a missing
    // key (step c).
    suite
        .register("meeting-wt-expired", "mc-wt-expired", "http://localhost:1")
        .await;

    let snap = MetricAssertion::snapshot();
    // Expired 100 s ago: well beyond the deployed 30 s skew.
    let token = mint_aged_dial_token(
        "meeting-wt-expired",
        "user-expired",
        HANDLER_ID,
        DIAL_TTL_SECS + 100,
    );
    let (_conn, _send, _recv) = connect_and_send_jwt(&suite.wt.url, &token).await;

    // Wait for the handler to reach step 4, fail admission, and the
    // accept_loop to emit on spawn exit.
    tokio::time::sleep(Duration::from_millis(500)).await;

    snap.counter("mh_jwt_validations_total")
        .with_labels(&[
            ("result", "failure"),
            ("token_type", "media_dial"),
            ("failure_reason", "expired"),
        ])
        .assert_delta(1);
    snap.counter("mh_webtransport_connections_total")
        .with_labels(&[("status", "error")])
        .assert_delta(1);
    assert_eq!(
        suite.session_manager.active_connection_count().await,
        0,
        "active connection count should remain 0 when JWT is rejected",
    );
}

#[tokio::test(flavor = "current_thread")]
async fn oversized_dial_token_rejected_on_wt_accept_path() {
    // Token payload fits under the 64KB framing cap but exceeds
    // `MAX_JWT_SIZE_BYTES` (8KB), admission's step (a). Confirms the size
    // check fires end-to-end through the accept path.
    let mc_client = make_mc_client();
    let suite = WtSuite::start(mc_client).await;

    let snap = MetricAssertion::snapshot();
    // 9000 > MAX_JWT_SIZE_BYTES (8192, defined in `common::jwt`).
    let oversized = "a".repeat(9000);
    let (_conn, _send, _recv) = connect_and_send_jwt(&suite.wt.url, &oversized).await;

    tokio::time::sleep(Duration::from_millis(500)).await;

    snap.counter("mh_jwt_validations_total")
        .with_labels(&[
            ("result", "failure"),
            ("token_type", "media_dial"),
            ("failure_reason", "malformed"),
        ])
        .assert_delta(1);
    snap.counter("mh_webtransport_connections_total")
        .with_labels(&[("status", "error")])
        .assert_delta(1);
}

#[tokio::test(flavor = "current_thread")]
async fn wrong_token_type_rejected_on_wt_accept_path() {
    // SECURITY INVARIANT: a token signed with the meeting's own registered key
    // whose `token_type` is not `media_dial` must NOT admit the bearer — not at
    // validation, not at session admission (ADR-0039 §5 step e).
    //
    // The authoritative unit-tier guard is `auth/mod.rs::tests`. This
    // component-tier test adds the DISTINGUISHING signal: if a future refactor
    // bypassed admission at the call site, the token would reach the session
    // manager and `active_connection_count` would transition from 0 to 1. The
    // session-manager state assertion below catches that class of refactor.
    use mh_test_utils::media_dial::{DialForge, MEETING_SEED};

    let mc_client = make_mc_client();
    let suite = WtSuite::start(mc_client).await;

    // Registered, so IF admission were bypassed the handler would proceed
    // straight to admitting the connection (state transition 0 → 1). Makes
    // the negative signal unambiguous.
    suite
        .register("meeting-wt-guest", "mc-wt-guest", "http://localhost:1")
        .await;

    let snap = MetricAssertion::snapshot();
    let mut claims = DialForge::claims_json(
        "meeting-wt-guest",
        HANDLER_ID,
        "user-guest",
        chrono::Utc::now().timestamp(),
        DIAL_TTL_SECS,
    );
    claims["token_type"] = serde_json::json!("guest");
    let token = DialForge::new(MEETING_SEED).sign(&claims);
    let (_conn, _send, _recv) = connect_and_send_jwt(&suite.wt.url, &token).await;

    // Give the handler time to run through admission and the accept_loop
    // to observe the spawned task's Err exit.
    tokio::time::sleep(Duration::from_millis(500)).await;

    snap.counter("mh_jwt_validations_total")
        .with_labels(&[
            ("result", "failure"),
            ("token_type", "media_dial"),
            ("failure_reason", "wrong_type"),
        ])
        .assert_delta(1);
    snap.counter("mh_webtransport_connections_total")
        .with_labels(&[("status", "error")])
        .assert_delta(1);

    // Distinguishing signal — if this transitions to 1, the token-type
    // check was bypassed at the call site.
    assert_eq!(
        suite.session_manager.active_connection_count().await,
        0,
        "a wrong-type token reached the session manager — admission was bypassed",
    );
}

// ---------------------------------------------------------------------------
// MC notification lifecycle
// ---------------------------------------------------------------------------

#[tokio::test]
async fn mc_notify_connected_fires_on_join_and_disconnected_fires_on_client_drop() {
    // Stand up a mock MC that captures both notification payloads on channels.
    let (connect_tx, mut connect_rx) = mpsc::channel(4);
    let (disconnect_tx, mut disconnect_rx) = mpsc::channel(4);
    // The mock must ANSWER with an ordinal: this test's subject is the
    // client-initiated disconnect (`DISCONNECT_REASON_CLIENT_CLOSED`), which is
    // only reachable once a media session actually started. `SenderReplies`
    // answers 0 by default — MC has no answer — and that path closes the
    // connection with `DISCONNECT_REASON_ERROR` instead, so leaving the default
    // here would silently retarget the test at the decline path.
    let mock_mc = MockMcServer::new(MockBehavior::Accept)
        .with_sender_replies(SenderReplies::default().with("user-notify", 9))
        .with_connected_tx(connect_tx)
        .with_disconnected_tx(disconnect_tx);
    let mc = start_mock_mc_server(mock_mc).await;

    let mc_client = make_mc_client();
    let suite = WtSuite::start(mc_client).await;

    // Register the meeting pointing at the mock MC endpoint.
    suite
        .register(
            "meeting-wt-notify",
            "mc-wt-notify",
            &format!("http://{}", mc.addr),
        )
        .await;

    let token = mint_dial_token("meeting-wt-notify", "user-notify", HANDLER_ID);
    let (conn, mut send, _recv) = connect_and_send_jwt(&suite.wt.url, &token).await;

    // Assert connect notification — bounded, strict equality on all fields.
    let connected = tokio::time::timeout(Duration::from_secs(3), connect_rx.recv())
        .await
        .expect("NotifyParticipantConnected did not arrive within 3s")
        .expect("connect channel closed before payload arrived");

    assert_eq!(connected.meeting_id, "meeting-wt-notify");
    assert_eq!(connected.participant_id, "user-notify");
    assert_eq!(connected.handler_id, HANDLER_ID);
    assert!(
        !connected.connection_id.is_empty(),
        "MH must send a connection_id: MC refuses an empty one (sender_id 0, ADR-0039 §6), so \
         the session would never bind"
    );

    // Clean close: finish the send stream so the server's `recv_stream.read()`
    // returns `Ok(None)` (the `ClientClosed` branch in `connection.rs`).
    send.finish()
        .await
        .expect("send.finish() on client side must succeed");
    drop(conn);

    let disconnected = tokio::time::timeout(Duration::from_secs(3), disconnect_rx.recv())
        .await
        .expect("NotifyParticipantDisconnected did not arrive within 3s")
        .expect("disconnect channel closed before payload arrived");

    assert_eq!(disconnected.meeting_id, "meeting-wt-notify");
    assert_eq!(disconnected.participant_id, "user-notify");
    assert_eq!(disconnected.handler_id, HANDLER_ID);
    // Read off the Connected MH actually sent, never a literal: MC retires the
    // entry keyed on this id, so a disconnect naming any other value leaves the
    // closed session's connectivity live (phantom connectivity) at MC.
    assert_eq!(
        disconnected.connection_id, connected.connection_id,
        "a session's disconnect must carry the SAME connection_id as its Connected"
    );
    // ClientClosed corresponds to the read-returned-None branch in connection.rs.
    assert_eq!(
        disconnected.reason,
        DisconnectReason::ClientClosed as i32,
        "disconnect reason must be CLIENT_CLOSED when the client drops"
    );

    // `mc` is dropped at end of scope — its `Drop` cancels + aborts the server.
    drop(mc);
}
