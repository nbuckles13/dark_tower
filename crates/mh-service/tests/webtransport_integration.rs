//! Integration tests for the mh-service WebTransport accept path,
//! provisional-timeout enforcement, and MC notification lifecycle.
//!
//! # Integration value over unit tests
//!
//! - `auth/mod.rs::tests` covers the `MhJwtValidator` rejection matrix
//!   (expired / wrong-key / malformed / oversized / token-type confusion)
//!   against the validator directly.
//! - `session/mod.rs::tests` covers `SessionManagerHandle` state transitions.
//! - `mc_client_integration.rs` covers `McClient` retry semantics against a
//!   mock MC.
//!
//! This file proves the *accept path* actually wires the validator, the
//! session manager, the provisional timer, and the MC client together:
//!
//! - Real WebTransport handshake with self-signed TLS (production code path).
//! - JWT read from the length-prefixed bi-stream payload.
//! - `MhJwtValidator::validate_meeting_token` reached on the accept path
//!   (the wrong-`token_type` case specifically guards against a refactor
//!   that swaps `validate_meeting_token` for `inner.validate`).
//! - Provisional timer gated on `SessionManager` state (positive + negative).
//! - MC notifications fire end-to-end on connect and on client drop, with
//!   the correct `DisconnectReason`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "common/mod.rs"]
mod test_common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use common::secret::SecretString;
use common::token_manager::TokenReceiver;
use mh_service::auth::MhJwtValidator;
use mh_service::errors::MhError;
use mh_service::grpc::McClient;
use mh_service::session::SessionManagerHandle;
use proto_gen::internal::DisconnectReason;
use tokio::sync::{mpsc, watch};

use test_common::jwks_rig::JwksRig;
use test_common::mock_mc::{start_mock_mc_server, MockBehavior, MockMcServer};
use test_common::tokens::{
    mint_expired_meeting_token, mint_meeting_token, mint_wrong_token_type_token,
};
use test_common::wt_client::{connect_and_open_bi, write_framed};
use test_common::wt_rig::WtRig;

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
    wt: WtRig,
}

impl WtSuite {
    async fn start(register_meeting_timeout: Duration, mc_client: Arc<McClient>) -> Self {
        let jwks = JwksRig::start(42, "mh-wt-integ-01").await;
        let session_manager = SessionManagerHandle::new();
        let jwt_validator = Arc::new(MhJwtValidator::new(jwks.jwks_client(), 300));

        let wt = WtRig::start(
            jwt_validator,
            session_manager.clone(),
            mc_client,
            "mh-test-001".to_string(),
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

    let token = mint_meeting_token(&suite.jwks.keypair, "meeting-wt-valid", "user-valid");

    let (_conn, _send, _recv) = connect_and_send_jwt(&suite.wt.url, &token).await;

    // Poll until the accept path registers the connection (bounded deadline).
    // Avoids a fixed-duration sleep that could mask a real regression by
    // silently flaking under CI load.
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut registered = false;
    while Instant::now() < deadline {
        if suite.session_manager.active_connection_count().await == 1 {
            registered = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        registered,
        "accept path did not register an active connection within 3s"
    );
}

#[tokio::test]
async fn missing_jwt_stream_closed_before_write_rejects_connection() {
    // The client opens the bi-stream and closes it without sending any bytes.
    // The server's `read_framed_message` must fail and return a WT error —
    // proving the accept path actually *reads* the JWT.
    let mc_client = make_mc_client();
    let mut suite = WtSuite::start(Duration::from_secs(30), mc_client).await;

    let (conn, mut send, _recv) = connect_and_open_bi(&suite.wt.url).await;
    // Close the send side immediately; the server's read_exact will get 0 bytes.
    send.finish().await.ok();
    drop(conn);

    let result = suite
        .wt
        .next_result(Duration::from_secs(5))
        .await
        .expect("handler result not delivered within timeout");

    assert!(
        matches!(&result, Err(MhError::WebTransportError(_))),
        "expected WebTransportError for missing JWT; got: {:?}",
        result.as_ref().map_err(error_label),
    );
}

#[tokio::test]
async fn expired_meeting_jwt_rejected_on_wt_accept_path() {
    let mc_client = make_mc_client();
    let mut suite = WtSuite::start(Duration::from_secs(30), mc_client).await;

    let token = mint_expired_meeting_token(&suite.jwks.keypair, "meeting-wt-expired");
    let (_conn, _send, _recv) = connect_and_send_jwt(&suite.wt.url, &token).await;

    let result = suite
        .wt
        .next_result(Duration::from_secs(5))
        .await
        .expect("handler result not delivered");

    assert!(
        matches!(&result, Err(MhError::JwtValidation(_))),
        "expected JwtValidation for expired token; got: {:?}",
        result.as_ref().map_err(error_label),
    );
}

#[tokio::test]
async fn oversized_jwt_rejected_on_wt_accept_path() {
    // JWT payload fits under the 64KB framing cap but exceeds
    // `MAX_JWT_SIZE_BYTES` (8KB) enforced by `MhJwtValidator`. Confirms the
    // validator's size check fires end-to-end through the accept path.
    let mc_client = make_mc_client();
    let mut suite = WtSuite::start(Duration::from_secs(30), mc_client).await;

    // 9000 > MAX_JWT_SIZE_BYTES (8192, defined in `common::jwt`).
    let oversized = "a".repeat(9000);
    let (_conn, _send, _recv) = connect_and_send_jwt(&suite.wt.url, &oversized).await;

    let result = suite
        .wt
        .next_result(Duration::from_secs(5))
        .await
        .expect("handler result not delivered");

    assert!(
        matches!(&result, Err(MhError::JwtValidation(_))),
        "expected JwtValidation for oversized token; got: {:?}",
        result.as_ref().map_err(error_label),
    );
}

#[tokio::test]
async fn wrong_token_type_guest_rejected_on_wt_accept_path() {
    // Non-negotiable security invariant: the WT accept path must call
    // `validate_meeting_token` (which enforces `token_type == "meeting"`),
    // NOT `inner.validate` (which does not). A well-signed guest-typed
    // token must not admit the bearer.
    let mc_client = make_mc_client();
    let mut suite = WtSuite::start(Duration::from_secs(30), mc_client).await;

    let token = mint_wrong_token_type_token(&suite.jwks.keypair, "meeting-wt-guest");
    let (_conn, _send, _recv) = connect_and_send_jwt(&suite.wt.url, &token).await;

    let result = suite
        .wt
        .next_result(Duration::from_secs(5))
        .await
        .expect("handler result not delivered");

    assert!(
        matches!(&result, Err(MhError::JwtValidation(_))),
        "expected JwtValidation for wrong token_type; got: {:?}",
        result.as_ref().map_err(error_label),
    );
}

// ---------------------------------------------------------------------------
// Provisional-timeout enforcement
// ---------------------------------------------------------------------------

#[tokio::test]
async fn provisional_connection_kicked_after_register_meeting_timeout() {
    // No meeting registered. A valid JWT produces a provisional acceptance;
    // after the configured timeout the handler returns MeetingNotRegistered.
    //
    // We use a 1s timeout and assert the elapsed time falls within a
    // generous 0.9s-6s window to survive CI jitter.
    let mc_client = make_mc_client();
    let mut suite = WtSuite::start(Duration::from_secs(1), mc_client).await;

    let token = mint_meeting_token(&suite.jwks.keypair, "meeting-wt-timeout", "user-timeout");

    let (_conn, _send, _recv) = connect_and_send_jwt(&suite.wt.url, &token).await;

    // Start the clock *after* the client has sent the JWT; this approximates
    // the server-side "session accepted, JWT read" moment.
    let start = Instant::now();
    let result = suite
        .wt
        .next_result(Duration::from_secs(6))
        .await
        .expect("handler result not delivered within 6s");
    let elapsed = start.elapsed();

    assert!(
        matches!(&result, Err(MhError::MeetingNotRegistered(_))),
        "expected MeetingNotRegistered after timeout; got: {:?}",
        result.as_ref().map_err(error_label),
    );

    assert!(
        elapsed >= Duration::from_millis(900),
        "handler exited in {elapsed:?} — before the provisional window could have elapsed"
    );
    assert!(
        elapsed <= Duration::from_secs(6),
        "handler took {elapsed:?} — provisional timeout did not fire in a reasonable window"
    );
}

#[tokio::test]
async fn provisional_connection_survives_when_register_meeting_arrives_within_window() {
    // Positive case: client connects with a valid JWT for an unregistered
    // meeting; ~200ms later, RegisterMeeting arrives; the connection must
    // NOT be kicked at the 1s timeout — the timer must gate on state.
    let mc_client = make_mc_client();
    let mut suite = WtSuite::start(Duration::from_secs(1), mc_client).await;

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
    // running (holding the connection open), so `next_result` must TIME OUT.
    let outcome = suite.wt.next_result(Duration::from_millis(1500)).await;
    assert!(
        outcome.is_none(),
        "handler exited within the provisional window; the timer did not gate on state — got: {:?}",
        outcome.as_ref().map(|r| r.as_ref().map_err(error_label)),
    );

    // And the session manager should reflect the promoted active connection.
    assert_eq!(
        suite.session_manager.active_connection_count().await,
        1,
        "pending connection was not promoted after RegisterMeeting arrival"
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

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Map `MhError` to a compact label for assertion messages.
///
/// This intentionally avoids including the full error payload (which may
/// contain token prefixes or other PII); only the variant name is surfaced
/// so test output stays clean even when a JWT sneaks into a wrapped error.
fn error_label(err: &MhError) -> &'static str {
    match err {
        MhError::JwtValidation(_) => "JwtValidation",
        MhError::WebTransportError(_) => "WebTransportError",
        MhError::MeetingNotRegistered(_) => "MeetingNotRegistered",
        MhError::Grpc(_) => "Grpc",
        MhError::Internal(_) => "Internal",
        MhError::NotRegistered => "NotRegistered",
        MhError::Config(_) => "Config",
        MhError::TokenAcquisition(_) => "TokenAcquisition",
        MhError::TokenAcquisitionTimeout => "TokenAcquisitionTimeout",
    }
}
