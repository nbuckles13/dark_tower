//! Integration tests for MH→MC notification delivery.
//!
//! Tests the `McClient` retry logic and auth-error short-circuit using
//! the shared `common::mock_mc` rig (`MediaCoordinationService` gRPC mock).

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "common/mod.rs"]
mod test_common;

use std::time::Duration;

use mh_service::errors::MhError;
use mh_service::grpc::McClient;

use test_common::mock_mc::{start_mock_mc_server, MockBehavior, MockMcServer, SenderReplies};
use test_common::test_token_receiver;
use tokio::sync::mpsc;

// ============================================================================
// Tests
// ============================================================================

#[tokio::test]
async fn test_notify_connected_success() {
    let mc = start_mock_mc_server(MockMcServer::new(MockBehavior::Accept)).await;

    let mc_url = format!("http://{}", mc.addr);
    let client = McClient::new(test_token_receiver());

    let result = client
        .notify_participant_connected(&mc_url, "meeting-1", "user-1", "mh-1", "conn-1")
        .await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn test_notify_disconnected_success() {
    let mc = start_mock_mc_server(MockMcServer::new(MockBehavior::Accept)).await;

    let mc_url = format!("http://{}", mc.addr);
    let client = McClient::new(test_token_receiver());

    let result = client
        .notify_participant_disconnected(&mc_url, "meeting-1", "user-1", "mh-1", "conn-1", 1)
        .await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn test_retry_succeeds_after_transient_failure() {
    // Fail the first call, succeed on retry
    let mc = start_mock_mc_server(MockMcServer::new(MockBehavior::FailThenAccept {
        fail_count: 1,
    }))
    .await;

    let mc_url = format!("http://{}", mc.addr);
    let client = McClient::new(test_token_receiver());

    let result = client
        .notify_participant_connected(&mc_url, "meeting-1", "user-1", "mh-1", "conn-1")
        .await;

    assert!(result.is_ok());
}

/// A TERMINAL error short-circuits; a TRANSIENT one still takes the full budget.
///
/// # This pins a boundary two rulings sit on either side of
///
/// @operations ruled the reachability retry budget must **not** be shortened:
/// with no client-side backoff yet, it is functioning as an accidental rate
/// limiter on the reconnect herd, and cutting it multiplies the retry rate
/// against a service that is already the bottleneck. Separately, an unparsable
/// endpoint is retried pointlessly — every attempt re-parses the same string
/// **without reaching the network** — and since `NotifyParticipantConnected`
/// became a blocking precondition, that delay is a connection held open for a
/// fault already certain at the first attempt.
///
/// Both are right, and they are only compatible if `is_terminal_error` covers
/// **strictly** the can-never-succeed set. A predicate even slightly too broad
/// silently shortens the herd-limiting budget for a class operations
/// deliberately kept slow — and it would do so invisibly, because a faster
/// failure looks like an improvement. So the boundary is asserted from both
/// sides rather than described.
#[tokio::test]
async fn a_terminal_error_short_circuits_while_a_transient_one_takes_the_full_budget() {
    // TERMINAL: an endpoint that does not parse. It never reaches a server, so
    // no mock is needed — and that absence is itself the assertion's substance.
    let client = McClient::new(test_token_receiver());
    let started = std::time::Instant::now();
    let result = client
        .notify_participant_connected("", "meeting-1", "user-1", "mh-1", "conn-1")
        .await;
    let terminal_elapsed = started.elapsed();

    assert!(
        matches!(&result, Err(MhError::McEndpointInvalid(_))),
        "an unparsable endpoint is a registration fault, got: {result:?}"
    );
    assert!(
        terminal_elapsed < Duration::from_secs(1),
        "a terminal error must not pay the backoff: re-parsing the same string cannot succeed, \
         and the connection is held open for the whole of it. Took {terminal_elapsed:?}"
    );

    // TRANSIENT: UNAVAILABLE on every attempt. This is the class @operations
    // keeps slow on purpose, so it must STILL walk the whole retry path.
    let (attempt_tx, mut attempt_rx) = mpsc::channel(8);
    let mc = start_mock_mc_server(
        MockMcServer::new(MockBehavior::FailThenAccept {
            fail_count: u32::MAX,
        })
        .with_connected_tx(attempt_tx),
    )
    .await;
    let mc_url = format!("http://{}", mc.addr);

    let started = std::time::Instant::now();
    let result = client
        .notify_participant_connected(&mc_url, "meeting-1", "user-1", "mh-1", "conn-1")
        .await;
    let transient_elapsed = started.elapsed();

    assert!(
        result.is_err(),
        "an MC that never accepts must eventually fail, got: {result:?}"
    );

    // Count what the SERVER saw, not what the client believes it sent.
    let mut attempts = 0;
    while attempt_rx.try_recv().is_ok() {
        attempts += 1;
    }
    assert_eq!(
        attempts, 3,
        "a transient failure must still be retried the full MAX_RETRY_ATTEMPTS times; fewer \
         means the terminal short-circuit has widened to cover reachability, which shortens the \
         budget operations deliberately keeps long"
    );
    assert!(
        transient_elapsed >= Duration::from_secs(3),
        "the 1s + 2s backoff must still be paid on the transient path — it is the herd rate \
         limiter, and a faster failure here would look like an improvement while removing it. \
         Took {transient_elapsed:?}"
    );
}

/// The RESPONSE survives the retry loop, not just the `Ok`.
///
/// `notify_participant_connected` stopped being an ack when
/// `NotifyParticipantConnectedResponse.sender_id` landed: the retry loop now
/// carries a value the accept path binds a media route to. `assert!(is_ok())`
/// alone cannot see a loop that returns a DEFAULT response after a successful
/// retry — and a defaulted response is `sender_id: 0`, which MH reads as "MC has
/// no answer" and turns into a declined session. That failure would present as
/// "media never starts whenever MC blips", with the RPC counter reading success.
#[tokio::test]
async fn test_retry_returns_the_successful_attempts_response_not_a_default() {
    const ALLOCATED: u32 = 4_097;

    let mc = start_mock_mc_server(
        MockMcServer::new(MockBehavior::FailThenAccept { fail_count: 1 })
            .with_sender_replies(SenderReplies::default().with("user-retry", ALLOCATED)),
    )
    .await;

    let mc_url = format!("http://{}", mc.addr);
    let client = McClient::new(test_token_receiver());

    let response = client
        .notify_participant_connected(&mc_url, "meeting-1", "user-retry", "mh-1", "conn-1")
        .await
        .expect("the second attempt succeeds, so the call must return Ok");

    assert_eq!(
        response.sender_id, ALLOCATED,
        "the ordinal from the attempt that SUCCEEDED must reach the caller; 0 here means the          retry loop discarded the response, which MH would read as `MC has no answer`"
    );
}

#[tokio::test]
async fn test_retry_exhaustion_after_max_attempts() {
    // Fail all 3 attempts (fail_count >= MAX_RETRY_ATTEMPTS)
    let mc = start_mock_mc_server(MockMcServer::new(MockBehavior::FailThenAccept {
        fail_count: 10,
    }))
    .await;

    let mc_url = format!("http://{}", mc.addr);
    let client = McClient::new(test_token_receiver());

    let result = client
        .notify_participant_connected(&mc_url, "meeting-1", "user-1", "mh-1", "conn-1")
        .await;

    assert!(
        matches!(&result, Err(MhError::Grpc(msg)) if msg.contains("MC notification RPC failed")),
        "Expected Grpc error after exhausting retries, got: {result:?}"
    );
}

#[tokio::test]
async fn test_unauthenticated_error_skips_retry() {
    let mc = start_mock_mc_server(MockMcServer::new(MockBehavior::Unauthenticated)).await;

    let mc_url = format!("http://{}", mc.addr);
    let client = McClient::new(test_token_receiver());

    let result = client
        .notify_participant_connected(&mc_url, "meeting-1", "user-1", "mh-1", "conn-1")
        .await;

    // Should fail immediately with JwtValidation (mapped from UNAUTHENTICATED)
    assert!(
        matches!(&result, Err(MhError::JwtValidation(_))),
        "Expected JwtValidation error for auth failure, got: {result:?}"
    );
}

#[tokio::test]
async fn test_permission_denied_skips_retry() {
    let mc = start_mock_mc_server(MockMcServer::new(MockBehavior::PermissionDenied)).await;

    let mc_url = format!("http://{}", mc.addr);
    let client = McClient::new(test_token_receiver());

    let result = client
        .notify_participant_disconnected(&mc_url, "meeting-1", "user-1", "mh-1", "conn-1", 1)
        .await;

    // Should fail immediately with JwtValidation (mapped from PERMISSION_DENIED)
    assert!(
        matches!(&result, Err(MhError::JwtValidation(_))),
        "Expected JwtValidation error for permission denied, got: {result:?}"
    );
}

/// The `declined_mc_auth_rejected` discriminator is a LOG QUERY, and this pins it
/// fail-closed.
///
/// `mh_media_session_starts_total{outcome="declined_mc_auth_rejected"}` covers two
/// routes that share one counter value: MC refused MH's credential, or MH could
/// not build one. They are separated not by a counter but by a log line — the
/// build route is the only emitter of `"Authorization header parse failed"` on
/// target `mh.grpc.mc_client`. `mh.grpc.gc_client` emits the IDENTICAL message
/// off the same `TokenReceiver` in the same process, so the target qualifier is
/// the whole discriminator, and it is a fail-open coupling (review protocol
/// §Assertion Vacuity, mechanism 4): reword the message, change the target, or
/// let one client log on the other's target, and a responder's query silently
/// returns nothing while the counter reads healthy. This test goes red on any of
/// those drifts, rather than the coupling failing in production.
///
/// Read as source-as-data (the sanctioned in-crate pattern, cf.
/// `media_metrics_integration.rs`): the target is a compile-time `&'static str`
/// on the `error!` macro, which no runtime assertion can read without a tracing
/// subscriber, and this crate has none.
#[test]
fn the_mc_auth_build_failure_log_is_pinned_to_its_message_and_target() {
    let read = |rel: &str| {
        std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(rel))
            .expect("the discriminator-pin test must be able to read the mh-service source file")
    };

    let mc_client = read("src/grpc/mc_client.rs");
    assert!(
        mc_client.contains("Authorization header parse failed"),
        "the build-failure message is the needle a responder greps; rewording it silently \
         breaks the declined_mc_auth_rejected discriminator"
    );
    assert!(
        mc_client.contains(r#"target: "mh.grpc.mc_client""#),
        "mc_client must log on its own target"
    );
    assert!(
        !mc_client.contains("mh.grpc.gc_client"),
        "mc_client must log ONLY on mh.grpc.mc_client; a message landing on another target \
         would put the build-failure line outside the responder's query"
    );

    let gc_client = read("src/grpc/gc_client.rs");
    assert!(
        gc_client.contains("Authorization header parse failed"),
        "the discriminator's whole premise is that gc_client emits the IDENTICAL message; if \
         gc_client's wording drifts, the target qualifier stops being load-bearing and this \
         test should be revisited"
    );
    assert!(
        gc_client.contains(r#"target: "mh.grpc.gc_client""#)
            && !gc_client.contains("mh.grpc.mc_client"),
        "gc_client emitting on the mc_client target would collapse the discriminator — the \
         same message on the same target from two sources"
    );
}

/// Every retry of one `NotifyParticipantConnected` carries the SAME
/// `connection_id` the caller passed, and a disconnect carries the value it was
/// handed — the client never mints, rewrites or drops it.
///
/// MC keys live connectivity on `(handler, connection_id)` and tombstones an id
/// once its disconnect lands (`internal.proto`). An id that drifted between
/// attempts would leave MC holding an entry the session's disconnect can never
/// retire — phantom connectivity, i.e. media edges placed on a handler the
/// participant is not on. So the assertion is over EVERY attempt the mock saw,
/// not just the one that succeeded, and it first proves there WERE several
/// attempts (a single-attempt run would pass vacuously).
#[tokio::test]
async fn test_every_retry_and_the_disconnect_carry_the_callers_connection_id() {
    // Deliberately unlike every other string in the fixture.
    const CONNECTION_ID: &str = "conn-4c1d-only-this-session";
    const FAIL_FIRST: u32 = 2;

    let (connected_tx, mut connected_rx) = mpsc::channel(8);
    let (disconnected_tx, mut disconnected_rx) = mpsc::channel(8);
    let mc = start_mock_mc_server(
        MockMcServer::new(MockBehavior::FailThenAccept {
            fail_count: FAIL_FIRST,
        })
        .with_connected_tx(connected_tx)
        .with_disconnected_tx(disconnected_tx),
    )
    .await;

    let mc_url = format!("http://{}", mc.addr);
    let client = McClient::new(test_token_receiver());

    client
        .notify_participant_connected(&mc_url, "meeting-1", "user-1", "mh-1", CONNECTION_ID)
        .await
        .expect("the attempt after the transient failures succeeds");
    client
        .notify_participant_disconnected(&mc_url, "meeting-1", "user-1", "mh-1", CONNECTION_ID, 1)
        .await
        .expect("the mock accepts once past its failure budget");

    let mut connected_ids = Vec::new();
    while let Ok(req) = connected_rx.try_recv() {
        connected_ids.push(req.connection_id);
    }
    assert_eq!(
        connected_ids.len(),
        usize::try_from(FAIL_FIRST + 1).unwrap(),
        "precondition: the mock must have seen every attempt (the failures plus the success), \
         or the per-attempt assertion below is vacuous"
    );
    for (attempt, id) in connected_ids.iter().enumerate() {
        assert_eq!(
            id, CONNECTION_ID,
            "attempt {attempt} carried a different connection_id; every retry must carry the \
             caller's value so MC's entry and the session's disconnect name the same session"
        );
    }

    let disconnect = disconnected_rx
        .try_recv()
        .expect("the disconnect must have reached the mock");
    assert_eq!(
        disconnect.connection_id, CONNECTION_ID,
        "the disconnect must carry the id the caller passed, or MC cannot retire the entry the \
         Connected created"
    );
}
