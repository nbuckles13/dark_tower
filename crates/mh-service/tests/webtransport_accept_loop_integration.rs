// current_thread flavor is load-bearing: `MetricAssertion` is per-thread,
// and the handshake histogram + gauge write + status counter emit inside
// a `tokio::spawn` task. Under current_thread those tasks run on the test
// thread so the per-thread recorder captures them; under
// `flavor = "multi_thread"` the tasks could run on a worker thread and the
// emissions would be invisible. See common/src/observability/testing.rs:60-72.
//
//! Component tests for the real `WebTransportServer::accept_loop`.
//!
//! Drives the production accept-loop byte-identically to `main.rs:258-260`
//! (via `tests/common/accept_loop_rig.rs`) and asserts on the three
//! accept-path status labels of `mh_webtransport_connections_total`:
//! `accepted`, `rejected`, `error`. Companion metrics
//! (`mh_active_connections` gauge, `mh_webtransport_handshake_duration_seconds`
//! histogram) are observed on the happy path.
//!
//! # Why a separate file from `webtransport_integration.rs`
//!
//! These 3 cases exercise the accept-loop *itself* (capacity check, status
//! label emission on spawn exit) — distinct from the JWT-enforcement /
//! provisional-timer / MC-notification cases that live in
//! `webtransport_integration.rs`. The two files share `AcceptLoopRig`
//! infrastructure but have orthogonal test surfaces.

#![expect(
    clippy::expect_used,
    reason = "component test; panics on setup failure (rig start, token mint, client connect) are intentional"
)]

#[path = "common/mod.rs"]
mod test_common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use common::observability::testing::MetricAssertion;
use mh_service::auth::MhJwtValidator;
use mh_service::grpc::McClient;
use mh_service::session::{MeetingRegistration, SessionManagerHandle};

use test_common::accept_loop_rig::AcceptLoopRig;
use test_common::jwks_rig::JwksRig;
use test_common::test_token_receiver;
use test_common::tokens::{mint_expired_meeting_token, mint_meeting_token};
use test_common::wt_client::{connect_and_open_bi, write_framed};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_mc_client() -> Arc<McClient> {
    Arc::new(McClient::new(test_token_receiver()))
}

async fn start_rig(
    session_manager: SessionManagerHandle,
    jwks: &JwksRig,
    max_connections: usize,
) -> AcceptLoopRig {
    let jwt_validator = Arc::new(MhJwtValidator::new(jwks.jwks_client(), 300));
    AcceptLoopRig::start_with(
        jwt_validator,
        session_manager,
        make_mc_client(),
        "mh-accept-loop-test".to_string(),
        max_connections,
        Duration::from_secs(30),
    )
    .await
}

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
// Accept-path status labels: `accepted` | `rejected` | `error`
// ---------------------------------------------------------------------------

#[tokio::test]
async fn accept_loop_emits_accepted_status_and_handshake_observation_on_happy_path() {
    // Valid JWT + pre-registered meeting → handler completes the handshake,
    // promotes the connection, and holds it open. Observations:
    //   - `mh_webtransport_connections_total{status=accepted}` delta=1 (accept_loop)
    //   - `mh_active_connections` gauge in `[1.0, 2.0]` while conn is live
    //     (1 = expected value; 2.0 upper bound absorbs the timing race
    //     between the accept-path set at server.rs:217 and the spawned
    //     handler's fetch_sub+set at server.rs:200-202 on handler exit)
    //   - `mh_webtransport_handshake_duration_seconds` at-least-one observation
    let jwks = JwksRig::start(1, "mh-accept-loop-ok").await;
    let session_manager = SessionManagerHandle::new();
    let rig = start_rig(session_manager.clone(), &jwks, 2).await;

    session_manager
        .register_meeting(
            "meeting-accept-ok".to_string(),
            MeetingRegistration {
                mc_id: "mc-accept-ok".to_string(),
                mc_grpc_endpoint: "http://localhost:1".to_string(),
                registered_at: Instant::now(),
            },
        )
        .await;

    let snap = MetricAssertion::snapshot();
    let token = mint_meeting_token(&jwks.keypair, "meeting-accept-ok", "user-accept-ok");
    let (_conn, _send, _recv) = connect_and_send_jwt(&rig.url, &token).await;

    // Hold the connection live until after the gauge assertion.
    assert!(
        wait_for_active_count(&session_manager, 1, Duration::from_secs(3)).await,
        "accept path did not promote connection within 3s",
    );

    // Histogram first (drain-on-read).
    snap.histogram("mh_webtransport_handshake_duration_seconds")
        .assert_observation_count_at_least(1);
    snap.counter("mh_webtransport_connections_total")
        .with_labels(&[("status", "accepted")])
        .assert_delta(1);
    // Gauge in [1.0, 2.0] while conn is live. assert_value_in_range is the
    // recommended shape for concurrently-updated gauges per the task brief
    // (the set-from-accept and decrement-from-spawn races through
    // fetch_sub+fetch_add on the atomic).
    snap.gauge("mh_active_connections")
        .assert_value_in_range(1.0..=2.0);
}

#[tokio::test]
async fn accept_loop_emits_rejected_status_when_at_capacity() {
    // `max_connections = 1`. Open conn #1 (valid JWT, pre-registered meeting)
    // and hold it live. Open conn #2 — accept_loop sees `active_connections
    // >= max_connections` at server.rs:166, emits `status=rejected`, and
    // drops `incoming_session` without accepting.
    let jwks = JwksRig::start(2, "mh-accept-loop-rejected").await;
    let session_manager = SessionManagerHandle::new();
    let rig = start_rig(session_manager.clone(), &jwks, 1).await;

    session_manager
        .register_meeting(
            "meeting-rejected".to_string(),
            MeetingRegistration {
                mc_id: "mc-rejected".to_string(),
                mc_grpc_endpoint: "http://localhost:1".to_string(),
                registered_at: Instant::now(),
            },
        )
        .await;

    let snap = MetricAssertion::snapshot();
    let token1 = mint_meeting_token(&jwks.keypair, "meeting-rejected", "user-1");

    // Conn #1 — this one must land. Hold it live for the rest of the test.
    let (_conn1, _send1, _recv1) = connect_and_send_jwt(&rig.url, &token1).await;
    assert!(
        wait_for_active_count(&session_manager, 1, Duration::from_secs(3)).await,
        "conn #1 did not promote; capacity test cannot meaningfully run",
    );

    // Conn #2 — attempt to connect; server should reject at capacity. The
    // client connect may succeed at the QUIC handshake level and then get
    // dropped at the WT session level, OR may fail earlier; either way the
    // accept_loop emits `status=rejected` synchronously before any spawn.
    let client = test_common::wt_client::build_client();
    let _conn2_result = tokio::time::timeout(Duration::from_secs(3), client.connect(&rig.url))
        .await
        .ok();

    // Give the accept_loop a bounded window to observe the incoming + emit.
    // The `rejected` emission happens on the accept task (no spawn), so it's
    // immediate on current_thread once the accept_loop resumes.
    tokio::time::sleep(Duration::from_millis(300)).await;

    snap.counter("mh_webtransport_connections_total")
        .with_labels(&[("status", "rejected")])
        .assert_delta(1);
}

#[tokio::test]
async fn accept_loop_emits_error_status_when_handler_returns_err() {
    // Valid handshake to the WT layer but invalid JWT — handler reaches step
    // 4 and returns `Err(MhError::JwtValidation(_))`. accept_loop emits
    // `status=error` when the spawned task exits with Err at server.rs:205.
    let jwks = JwksRig::start(3, "mh-accept-loop-error").await;
    let session_manager = SessionManagerHandle::new();
    let rig = start_rig(session_manager.clone(), &jwks, 8).await;

    let snap = MetricAssertion::snapshot();
    let token = mint_expired_meeting_token(&jwks.keypair, "meeting-error");
    let (_conn, _send, _recv) = connect_and_send_jwt(&rig.url, &token).await;

    // Wait for the spawned handler to complete.
    tokio::time::sleep(Duration::from_millis(500)).await;

    snap.counter("mh_webtransport_connections_total")
        .with_labels(&[("status", "error")])
        .assert_delta(1);
    // JWT was well-formed enough to be parsed; validation fails with
    // `failure_reason=validation_failed` for meeting tokens (only label
    // emitted on the meeting-path failure branch per connection.rs:122).
    snap.counter("mh_jwt_validations_total")
        .with_labels(&[
            ("result", "failure"),
            ("token_type", "meeting"),
            ("failure_reason", "validation_failed"),
        ])
        .assert_delta(1);
    assert_eq!(
        session_manager.active_connection_count().await,
        0,
        "no connection should have been promoted on handler error",
    );
}
