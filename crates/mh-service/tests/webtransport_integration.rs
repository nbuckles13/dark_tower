//! Integration tests for the mh-service WebTransport accept path,
//! provisional-timeout enforcement, and MC notification lifecycle.
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
//! - `auth/mod.rs::tests` covers the `MhJwtValidator` rejection matrix
//!   (expired / wrong-key / malformed / oversized / token-type confusion)
//!   against the validator directly. That unit matrix is the authoritative
//!   refactor guard for `validate_meeting_token` semantics; the former
//!   integration test `wrong_token_type_guest_rejected_on_wt_accept_path`
//!   is retired as redundant with the unit-tier matrix under ADR-0032.
//! - `session/mod.rs::tests` covers `SessionManagerHandle` state transitions,
//!   including the provisional-timer lower-bound behaviour — the component
//!   tier here only asserts end-to-end wiring and upper-bound enforcement.
//! - `mc_client_integration.rs` covers `McClient` retry semantics.
//!
//! This file proves the *accept path* actually wires the validator, the
//! session manager, the provisional timer, and the MC client together, and
//! that the real `accept_loop` emits the correct metric labels on each
//! per-connection outcome.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "common/mod.rs"]
mod test_common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use common::observability::testing::MetricAssertion;
use common::secret::SecretString;
use common::token_manager::TokenReceiver;
use mh_service::auth::MhJwtValidator;
use mh_service::grpc::McClient;
use mh_service::session::SessionManagerHandle;
use proto_gen::internal::DisconnectReason;
use tokio::sync::{mpsc, watch};

use test_common::accept_loop_rig::AcceptLoopRig;
use test_common::jwks_rig::JwksRig;
use test_common::mock_mc::{start_mock_mc_server, MockBehavior, MockMcServer};
use test_common::tokens::{
    mint_expired_meeting_token, mint_meeting_token, mint_wrong_token_type_token,
};
use test_common::wt_client::{connect_and_open_bi, write_framed};

// ---------------------------------------------------------------------------
// Rig builder
// ---------------------------------------------------------------------------

fn test_token_receiver() -> TokenReceiver {
    let (tx, rx) = watch::channel(SecretString::from("test-service-token"));
    // Keep the sender alive for the process lifetime (test only).
    std::mem::forget(tx);
    TokenReceiver::from_test_channel(rx)
}

struct WtSuite {
    jwks: JwksRig,
    session_manager: SessionManagerHandle,
    wt: AcceptLoopRig,
}

impl WtSuite {
    async fn start(register_meeting_timeout: Duration, mc_client: Arc<McClient>) -> Self {
        let jwks = JwksRig::start(42, "mh-wt-integ-01").await;
        let session_manager = SessionManagerHandle::new();
        let jwt_validator = Arc::new(MhJwtValidator::new(jwks.jwks_client(), 300));

        let wt = AcceptLoopRig::start_with(
            jwt_validator,
            session_manager.clone(),
            mc_client,
            "mh-test-001".to_string(),
            32,
            register_meeting_timeout,
        )
        .await;

        Self {
            jwks,
            session_manager,
            wt,
        }
    }
}

/// Build an `McClient` pointed at the supplied endpoint (we do not actually
/// need per-call routing for tests that do not care about notifications).
fn make_mc_client() -> Arc<McClient> {
    Arc::new(McClient::new(test_token_receiver()))
}

/// Open a WT bi-stream and write the supplied token as a length-prefixed
/// frame. Returns the live connection + streams so the caller can control
/// its own disconnect.
async fn connect_and_send_jwt(
    wt_url: &str,
    token: &str,
) -> (
    wtransport::Connection,
    wtransport::stream::SendStream,
    wtransport::stream::RecvStream,
) {
    let (conn, mut send, recv) = connect_and_open_bi(wt_url).await;
    write_framed(&mut send, token.as_bytes())
        .await
        .expect("failed to write JWT frame");
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
// JWT enforcement on the accept path
// ---------------------------------------------------------------------------

#[tokio::test]
async fn valid_meeting_jwt_connection_accepted_and_tracked() {
    let mc_client = make_mc_client();
    let suite = WtSuite::start(Duration::from_secs(30), mc_client).await;

    // Pre-register the meeting so the connection is promoted immediately.
    suite
        .session_manager
        .register_meeting(
            "meeting-wt-valid".to_string(),
            mh_service::session::MeetingRegistration {
                mc_id: "mc-wt-test".to_string(),
                mc_grpc_endpoint: "http://localhost:1".to_string(),
                registered_at: Instant::now(),
            },
        )
        .await;

    let snap = MetricAssertion::snapshot();
    let token = mint_meeting_token(&suite.jwks.keypair, "meeting-wt-valid", "user-valid");
    let (_conn, _send, _recv) = connect_and_send_jwt(&suite.wt.url, &token).await;

    // Side-effect observation: the real accept_loop + handler must register
    // the active connection within a bounded deadline.
    assert!(
        wait_for_active_count(&suite.session_manager, 1, Duration::from_secs(3)).await,
        "accept path did not register an active connection within 3s",
    );

    // Assert histogram FIRST — `Snapshotter::snapshot()` drains histograms on
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
            ("token_type", "meeting"),
            ("failure_reason", "none"),
        ])
        .assert_delta(1);
}

#[tokio::test]
async fn missing_jwt_stream_closed_before_write_rejects_connection() {
    // The client opens the bi-stream and closes it without sending any bytes.
    // The server's `read_framed_message` fails and the handler returns an
    // error before JWT validation runs. Observation shape:
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
    let suite = WtSuite::start(Duration::from_secs(30), mc_client).await;

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

#[tokio::test]
async fn expired_meeting_jwt_rejected_on_wt_accept_path() {
    let mc_client = make_mc_client();
    let suite = WtSuite::start(Duration::from_secs(30), mc_client).await;

    let snap = MetricAssertion::snapshot();
    let token = mint_expired_meeting_token(&suite.jwks.keypair, "meeting-wt-expired");
    let (_conn, _send, _recv) = connect_and_send_jwt(&suite.wt.url, &token).await;

    // Wait for the handler to reach step 4, fail validation, and the
    // accept_loop to emit on spawn exit.
    tokio::time::sleep(Duration::from_millis(500)).await;

    snap.counter("mh_jwt_validations_total")
        .with_labels(&[
            ("result", "failure"),
            ("token_type", "meeting"),
            ("failure_reason", "validation_failed"),
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

#[tokio::test]
async fn oversized_jwt_rejected_on_wt_accept_path() {
    // JWT payload fits under the 64KB framing cap but exceeds
    // `MAX_JWT_SIZE_BYTES` (8KB) enforced by `MhJwtValidator`. Confirms the
    // validator's size check fires end-to-end through the accept path.
    let mc_client = make_mc_client();
    let suite = WtSuite::start(Duration::from_secs(30), mc_client).await;

    let snap = MetricAssertion::snapshot();
    // 9000 > MAX_JWT_SIZE_BYTES (8192, defined in `common::jwt`).
    let oversized = "a".repeat(9000);
    let (_conn, _send, _recv) = connect_and_send_jwt(&suite.wt.url, &oversized).await;

    tokio::time::sleep(Duration::from_millis(500)).await;

    snap.counter("mh_jwt_validations_total")
        .with_labels(&[
            ("result", "failure"),
            ("token_type", "meeting"),
            ("failure_reason", "validation_failed"),
        ])
        .assert_delta(1);
    snap.counter("mh_webtransport_connections_total")
        .with_labels(&[("status", "error")])
        .assert_delta(1);
}

#[tokio::test]
async fn wrong_token_type_guest_rejected_on_wt_accept_path() {
    // SECURITY INVARIANT: the WT accept path must call `validate_meeting_token`
    // (which enforces `token_type == "meeting"`), NOT `inner.validate` or any
    // hypothetical permissive variant. A correctly-signed guest-typed token
    // must NOT admit the bearer — not at validation, not at session
    // registration.
    //
    // Authoritative unit-tier guard on `validate_meeting_token`'s contract:
    // `crates/mh-service/src/auth/mod.rs::tests::test_validate_meeting_token_rejects_guest_token`.
    // This component-tier test adds the DISTINGUISHING signal: if a future
    // refactor edits `connection.rs:110` to call a permissive method (or
    // makes `MhJwtValidator::inner` crate-visible and calls it directly), the
    // guest token would promote to the session manager and
    // `active_connection_count` would transition from 0 to 1. Neither the
    // metric-label assertion (labels at `connection.rs:112,122` are
    // string-literal and don't depend on which validator method was called)
    // nor the unit-tier matrix catch that class of call-site refactor. The
    // session-manager state assertion below does.
    let mc_client = make_mc_client();
    let suite = WtSuite::start(Duration::from_secs(30), mc_client).await;

    // Pre-register the meeting so that IF validation were bypassed, the
    // handler would proceed straight to `add_connection` (state transition
    // 0 → 1), not `add_pending_connection`. Makes the negative signal
    // unambiguous.
    suite
        .session_manager
        .register_meeting(
            "meeting-wt-guest".to_string(),
            mh_service::session::MeetingRegistration {
                mc_id: "mc-wt-guest".to_string(),
                mc_grpc_endpoint: "http://localhost:1".to_string(),
                registered_at: Instant::now(),
            },
        )
        .await;

    let snap = MetricAssertion::snapshot();
    let token = mint_wrong_token_type_token(&suite.jwks.keypair, "meeting-wt-guest");
    let (_conn, _send, _recv) = connect_and_send_jwt(&suite.wt.url, &token).await;

    // Give the handler time to run through validation and the accept_loop
    // to observe the spawned task's Err exit.
    tokio::time::sleep(Duration::from_millis(500)).await;

    snap.counter("mh_jwt_validations_total")
        .with_labels(&[
            ("result", "failure"),
            ("token_type", "meeting"),
            ("failure_reason", "validation_failed"),
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
        "guest token reached session manager — validate_meeting_token was bypassed or a permissive variant was called",
    );
}

// ---------------------------------------------------------------------------
// Provisional-timeout enforcement
// ---------------------------------------------------------------------------

#[tokio::test]
async fn provisional_connection_kicked_after_register_meeting_timeout() {
    // Provisional timeout = 1s. Asserts end-to-end wiring AND the original
    // lower-bound guarantee ("timer does not fire BEFORE ~900ms"):
    //
    //   1. At 800ms (production timer = 1000ms, 200ms safety margin), the
    //      counter is still 0 — the handler hasn't exited yet.
    //   2. At ~3000ms (2200ms after the lower-bound check), the timer has
    //      fired, handler has returned Err, and accept_loop emitted
    //      `mh_webtransport_connections_total{status=error}` → counter = 1.
    //
    // Both reads hit the SAME `MetricAssertion` snapshot — counters are
    // idempotent under repeat reads per common::observability::testing
    // §"Delta semantics". Do NOT take a fresh snapshot between the two
    // reads; per-snapshot recorder binding is fresh, so a new snapshot
    // would see counter=0 regardless of the production emission.
    let mc_client = make_mc_client();
    let suite = WtSuite::start(Duration::from_secs(1), mc_client).await;

    let snap = MetricAssertion::snapshot();
    let token = mint_meeting_token(&suite.jwks.keypair, "meeting-wt-timeout", "user-timeout");
    let (_conn, _send, _recv) = connect_and_send_jwt(&suite.wt.url, &token).await;

    // Lower-bound check: 200ms before the 1s production timer would fire,
    // the error counter must still be 0. This guards against a refactor
    // that accidentally drops the provisional window entirely.
    tokio::time::sleep(Duration::from_millis(800)).await;
    snap.counter("mh_webtransport_connections_total")
        .with_labels(&[("status", "error")])
        .assert_delta(0);

    // Upper-bound check: 2200ms later (3000ms total wall-clock from
    // snapshot, well past the 1s production timer), the counter MUST have
    // incremented. 2000ms+ headroom covers any reasonable scheduling delay
    // under CI load.
    tokio::time::sleep(Duration::from_millis(2200)).await;
    snap.counter("mh_webtransport_connections_total")
        .with_labels(&[("status", "error")])
        .assert_delta(1);
    assert_eq!(
        suite.session_manager.active_connection_count().await,
        0,
        "provisional connection should not have been promoted after timeout",
    );
}

#[tokio::test]
async fn provisional_connection_survives_when_register_meeting_arrives_within_window() {
    // Positive case: client connects with a valid JWT for an unregistered
    // meeting; ~200ms later, RegisterMeeting arrives; the connection must
    // NOT be kicked at the 1s timeout — the timer must gate on state.
    let mc_client = make_mc_client();
    let suite = WtSuite::start(Duration::from_secs(1), mc_client).await;

    let token = mint_meeting_token(&suite.jwks.keypair, "meeting-wt-survive", "user-survive");
    let (_conn, _send, _recv) = connect_and_send_jwt(&suite.wt.url, &token).await;

    // Simulate RegisterMeeting arriving ~200ms into the 1s window.
    let sm = suite.session_manager.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        sm.register_meeting(
            "meeting-wt-survive".to_string(),
            mh_service::session::MeetingRegistration {
                mc_id: "mc-wt-survive".to_string(),
                mc_grpc_endpoint: "http://localhost:1".to_string(),
                registered_at: Instant::now(),
            },
        )
        .await;
    });

    // Wait well past the provisional timeout. The handler must still be
    // running (holding the connection open), so the active count stays at 1.
    assert!(
        wait_for_active_count(&suite.session_manager, 1, Duration::from_millis(1500)).await,
        "pending connection was not promoted after RegisterMeeting arrival",
    );
    // Remain at 1 past the provisional timeout — confirms the timer gated on state.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        suite.session_manager.active_connection_count().await,
        1,
        "promoted connection was kicked; timer did not gate on state",
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
    let mock_mc = MockMcServer::new(MockBehavior::Accept)
        .with_connected_tx(connect_tx)
        .with_disconnected_tx(disconnect_tx);
    let mc = start_mock_mc_server(mock_mc).await;

    let mc_client = make_mc_client();
    let suite = WtSuite::start(Duration::from_secs(30), mc_client).await;

    // Pre-register the meeting pointing at the mock MC endpoint.
    suite
        .session_manager
        .register_meeting(
            "meeting-wt-notify".to_string(),
            mh_service::session::MeetingRegistration {
                mc_id: "mc-wt-notify".to_string(),
                mc_grpc_endpoint: format!("http://{}", mc.addr),
                registered_at: Instant::now(),
            },
        )
        .await;

    let token = mint_meeting_token(&suite.jwks.keypair, "meeting-wt-notify", "user-notify");
    let (conn, mut send, _recv) = connect_and_send_jwt(&suite.wt.url, &token).await;

    // Assert connect notification — bounded, strict equality on all fields.
    let connected = tokio::time::timeout(Duration::from_secs(3), connect_rx.recv())
        .await
        .expect("NotifyParticipantConnected did not arrive within 3s")
        .expect("connect channel closed before payload arrived");

    assert_eq!(connected.meeting_id, "meeting-wt-notify");
    assert_eq!(connected.participant_id, "user-notify");
    assert_eq!(connected.handler_id, "mh-test-001");

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
    assert_eq!(disconnected.handler_id, "mh-test-001");
    // ClientClosed corresponds to the read-returned-None branch in connection.rs.
    assert_eq!(
        disconnected.reason,
        DisconnectReason::ClientClosed as i32,
        "disconnect reason must be CLIENT_CLOSED when the client drops"
    );

    // `mc` is dropped at end of scope — its `Drop` cancels + aborts the server.
    drop(mc);
}
