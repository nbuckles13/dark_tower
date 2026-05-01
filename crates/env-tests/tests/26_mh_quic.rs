//! P1 Tests: MH QUIC Connection Flow (R-33)
//!
//! End-to-end tests validating the client→MH WebTransport path through the full
//! AC→GC→MC→MH stack. Models the patterns established in `24_join_flow.rs`.
//!
//! # Scenarios (R-33)
//!
//! 1. `test_mh_url_present_in_join_response` — `JoinResponse.media_servers`
//!    populated with non-empty WebTransport URLs.
//! 2. `test_mh_accepts_valid_meeting_jwt` — valid JWT → connection held open.
//! 3. `test_mh_rejects_forged_jwt` / `test_mh_rejects_oversized_jwt` —
//!    rejection observable as peer-close on the recv stream.
//! 4. `test_mh_connect_increments_mc_notification_metric_connected` —
//!    Prometheus `mc_mh_notifications_received_total{event_type="connected"}`
//!    delta ≥ 1 after a successful MH connect.
//! 5. `test_mh_disconnect_increments_mc_notification_metric_disconnected` —
//!    same counter with `event_type="disconnected"` after clean close.
//! 6. `test_mh_disconnects_unregistered_meeting_after_timeout` — `#[ignore]`d
//!    stub. Authoritative coverage at component tier:
//!    `crates/mh-service/tests/webtransport_integration.rs::provisional_connection_kicked_after_register_meeting_timeout`.
//!    Cannot run from env-tests because (a) AC's signing key is not exposed, so
//!    we cannot mint a JWT for an unregistered `meeting_id`; (b) MC fires
//!    `RegisterMeeting` to all assigned MHs after first join, so no MH stays
//!    unregistered for a real meeting; (c) shortening the timeout requires
//!    infra changes that would create a dev-vs-prod behavioral gap.
//!
//! # Prerequisites
//!
//! - Kind cluster with AC, GC, MC, MH-0, MH-1 deployed; port-forwards active.
//! - MH WebTransport endpoints reachable on the host via Kind NodePort
//!   (see `infra/kind/scripts/setup.sh`'s ConfigMap patching of
//!   `MH_WEBTRANSPORT_ADVERTISE_ADDRESS`).
//! - Test data seeded (`devtest` organization).
//!
//! # TLS
//!
//! Both MC and MH WebTransport endpoints use self-signed dev certs generated
//! by `scripts/generate-dev-certs.sh` at Kind setup time. Tests use
//! `with_no_cert_validation()` for the same reason as `connect_mc()` in
//! `24_join_flow.rs`: the dev CA cert is not committed to the repo.
//!
//! # Wire format
//!
//! MH expects the FIRST framed message on a bidi stream to be a typed
//! `MhClientMessage{ConnectRequest{join_token: <JWT>}}` protobuf envelope,
//! 4-byte big-endian length prefix + encoded bytes. Mirrors MC's
//! `ClientMessage{JoinRequest{...}}` discipline. Source of truth:
//! `crates/mh-service/src/webtransport/connection.rs` Step 3 region.

#![cfg(feature = "flows")]

use bytes::{BufMut, BytesMut};
use env_tests::cluster::ClusterConnection;
use env_tests::fixtures::auth_client::UserRegistrationRequest;
use env_tests::fixtures::gc_client::{CreateMeetingRequest, GcClient, JoinMeetingResponse};
use env_tests::fixtures::{AuthClient, PrometheusClient};
use prost::Message;
use std::time::Duration;
use tokio::sync::OnceCell;

// ============================================================================
// Test infrastructure
// ============================================================================

/// Shared cluster connection (initialized once, reused across all tests).
static CLUSTER: OnceCell<ClusterConnection> = OnceCell::const_new();

/// Shared test user (cuts AC registrations under the 5/hour rate limit).
static SHARED_USER: OnceCell<(String, String)> = OnceCell::const_new();

async fn cluster() -> &'static ClusterConnection {
    CLUSTER
        .get_or_init(|| async {
            let cluster = ClusterConnection::new()
                .await
                .expect("Failed to connect to cluster - ensure port-forwards are running");
            cluster
                .check_ac_health()
                .await
                .expect("AC service must be running for MH QUIC tests");
            cluster
                .check_gc_health()
                .await
                .expect("GC service must be running for MH QUIC tests");
            cluster
        })
        .await
}

async fn shared_user(cluster: &ClusterConnection) -> &'static (String, String) {
    SHARED_USER
        .get_or_init(|| async {
            let auth_client = AuthClient::new(&cluster.ac_base_url);
            register_test_user(&auth_client, "MH QUIC Shared User").await
        })
        .await
}

/// Register a test user via AC and return `(access_token, display_name)`.
async fn register_test_user(auth_client: &AuthClient, display_name: &str) -> (String, String) {
    let request = UserRegistrationRequest::unique(display_name);
    let display = request.display_name.clone();
    let response = auth_client
        .register_user(&request)
        .await
        .expect("AC should register test user");
    (response.access_token, display)
}

/// Create a meeting and join via GC, returning the join response (which
/// contains the meeting JWT and the assigned MC's WebTransport URL).
///
/// The caller is responsible for choosing what to do next: open the MC
/// WebTransport (so `media_servers` is populated and `RegisterMeeting` fires
/// to all assigned MHs) or skip directly to MH.
async fn gc_create_and_join(
    cluster: &ClusterConnection,
    user_token: &str,
    meeting_name: &str,
) -> JoinMeetingResponse {
    let gc_client = GcClient::new(&cluster.gc_base_url);

    let create_request = CreateMeetingRequest::new(meeting_name);
    let created = gc_client
        .create_meeting(user_token, &create_request)
        .await
        .expect("GC should create meeting");

    gc_client
        .join_meeting(&created.meeting_code, user_token)
        .await
        .expect("GC should issue meeting token + MC assignment")
}

/// Connect a wtransport client to a WebTransport URL.
///
/// Uses `with_no_cert_validation()` for Kind's self-signed dev certs.
/// Mirrors `connect_mc()` in `24_join_flow.rs`.
async fn connect_wt(url: &str) -> wtransport::Connection {
    let client_config = wtransport::ClientConfig::builder()
        .with_bind_default()
        .with_no_cert_validation()
        .build();

    let client = wtransport::Endpoint::client(client_config).expect("create WebTransport client");
    client
        .connect(url)
        .await
        .unwrap_or_else(|e| panic!("connect to WebTransport at {url} failed: {e}"))
}

/// Encode `jwt` as the typed `MhClientMessage{ConnectRequest{join_token}}`
/// envelope and frame it (4-byte BE length + encoded payload).
///
/// MH's wire format on the first message of the bidi stream is a typed
/// protobuf envelope, mirroring MC's `ClientMessage{JoinRequest{...}}`.
/// For negative tests that need to send malformed bytes (e.g., the oversized
/// payload tests below), the bytes are wrapped in the same envelope and the
/// validator/decoder observes the failure mode that's intended.
fn encode_jwt_frame(jwt: &str) -> Vec<u8> {
    use proto_gen::signaling::{mh_client_message, MhClientMessage, MhConnectRequest};

    let envelope = MhClientMessage {
        message: Some(mh_client_message::Message::ConnectRequest(
            MhConnectRequest {
                join_token: jwt.to_string(),
            },
        )),
    };
    let encoded = envelope.encode_to_vec();

    let len = u32::try_from(encoded.len()).expect("encoded envelope length must fit in u32");
    let mut frame = BytesMut::with_capacity(4 + encoded.len());
    frame.put_u32(len);
    frame.put_slice(&encoded);
    frame.to_vec()
}

/// Open a bidi stream on `conn` and write the JWT frame on the send side.
///
/// Returns the live streams so the caller can control read timing and the
/// disconnect.
///
/// IMPORTANT — held-open assertion warning: MH closes its send-half of the
/// JWT-carrier bidi stream immediately after `accept_bi()` (see
/// `crates/mh-service/src/webtransport/connection.rs:163`, where the
/// SendStream is bound to `_` and dropped). From the client's view, `recv`
/// on this stream sees `Ok(None)` (clean end-of-stream) almost instantly,
/// even though the WebTransport SESSION remains alive. Don't use this recv
/// stream as a held-open signal — assert on `conn.closed()` instead.
/// The recv stream IS still useful for negative tests: peer close on the
/// session causes the recv to error/finish, which is observable here.
async fn send_jwt_on_bi_stream(
    conn: &wtransport::Connection,
    jwt: &str,
) -> (
    wtransport::stream::SendStream,
    wtransport::stream::RecvStream,
) {
    let (mut send, recv) = conn
        .open_bi()
        .await
        .expect("open bi stream")
        .await
        .expect("bi stream ready");
    let frame = encode_jwt_frame(jwt);
    send.write_all(&frame).await.expect("write JWT frame");
    (send, recv)
}

// ----------------------------------------------------------------------------
// Negative-test helpers (security guidance: never include the JWT in panics).
// ----------------------------------------------------------------------------

/// Truncate a JWT to the first 16 chars + ellipsis for safe inclusion in
/// assertion failure messages. Per @security plan-stage guidance: real meeting
/// JWTs carry PII in their claims (`participant_id`, `sub`); even forged tokens
/// echo attacker-supplied content. Don't normalize logging full tokens.
fn jwt_preview(jwt: &[u8]) -> String {
    let s = std::str::from_utf8(jwt).unwrap_or("<non-utf8>");
    if s.len() <= 16 {
        format!("{s}...")
    } else {
        format!("{}...", &s[..16])
    }
}

/// Connect to MH and send a bad JWT, then assert the SESSION closes within
/// a bounded window. Mirrors `test_mh_accepts_valid_meeting_jwt`'s held-open
/// assertion inverted: success here = `conn.closed()` resolves observably.
///
/// **Why session-level, not stream-level**: per the warning on
/// `send_jwt_on_bi_stream`, MH's `accept_bi()` binds the SendStream half to
/// `_` and drops it immediately — so the bidi recv stream sees `Ok(None)`
/// on EVERY connection path (accept, forged reject, oversized reject, …).
/// Stream-level end-of-stream is invariant across outcomes and would silently
/// pass even if MH started accepting bad JWTs. Only `conn.closed()`
/// distinguishes rejection (session terminated by MH) from acceptance
/// (session held open for media frames).
async fn assert_mh_rejects(mh_url: &str, jwt: &str) {
    let conn = connect_wt(mh_url).await;
    let (_send, _recv) = send_jwt_on_bi_stream(&conn, jwt).await;

    let close_outcome = tokio::time::timeout(Duration::from_secs(5), conn.closed()).await;

    assert!(
        close_outcome.is_ok(),
        "MH did not close the WebTransport session within 5s — JWT was \
         not rejected (jwt prefix: {})",
        jwt_preview(jwt.as_bytes()),
    );
}

// ----------------------------------------------------------------------------
// Prometheus delta helpers for tests 4 & 5.
// ----------------------------------------------------------------------------

/// Wait for the cluster-wide `mc_mh_notifications_received_total{event_type=...}`
/// counter to stabilize: two consecutive reads (with a Prometheus scrape interval
/// gap) returning the same value. Closes the cross-test race where leftover
/// signals from a predecessor test (under the same `#[serial]` group) might
/// still be in flight to Prometheus when the next test snapshots its baseline.
/// After this returns, baseline-reads are safe.
///
/// Budget: up to 90s. The chain that has to settle is MH's fire-and-forget
/// `tokio::spawn(notify)` → gRPC RPC → MC counter increment → Prometheus scrape
/// (15s SLA). Under cluster load this can exceed the 30s `MetricsScrape`
/// category, so we use a longer custom budget here.
async fn wait_for_notification_counter_stable(prom: &PrometheusClient, event_type: &str) {
    let deadline = std::time::Instant::now() + Duration::from_secs(90);
    loop {
        let v1 = mh_notification_counter(prom, event_type).await;
        // One Prometheus scrape interval (15s SLA) — wait long enough that any
        // outstanding scrape lands between v1 and v2.
        tokio::time::sleep(Duration::from_secs(16)).await;
        let v2 = mh_notification_counter(prom, event_type).await;
        if (v1 - v2).abs() < f64::EPSILON {
            return;
        }
        if std::time::Instant::now() > deadline {
            panic!(
                "mc_mh_notifications_received_total{{event_type=\"{event_type}\"}} \
                 did not stabilize within 90s (last reads: v1={v1}, v2={v2})"
            );
        }
    }
}

/// Poll until `mc_mh_notifications_received_total{event_type=...}` exceeds
/// `baseline`. Budget: 60s — 2x `MetricsScrape` to absorb the MH spawn-task
/// + gRPC + MC handler + scrape chain under cluster load.
async fn assert_notification_counter_increases_past(
    prom: &PrometheusClient,
    event_type: &str,
    baseline: f64,
) {
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    loop {
        let current = mh_notification_counter(prom, event_type).await;
        if current > baseline {
            return;
        }
        if std::time::Instant::now() > deadline {
            panic!(
                "mc_mh_notifications_received_total{{event_type=\"{event_type}\"}} \
                 did not increase above baseline {baseline} within 60s \
                 (last observed: {current})"
            );
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

/// Read the cluster-wide value of `mc_mh_notifications_received_total` for a
/// specific `event_type` label. Uses `sum(...)` for replica-robustness per
/// @observability plan-stage guidance.
///
/// Returns 0.0 if the counter has not yet been observed (Prometheus returns
/// no result for an empty series).
async fn mh_notification_counter(prom: &PrometheusClient, event_type: &str) -> f64 {
    let promql = format!(
        r#"sum(mc_mh_notifications_received_total{{event_type="{}"}})"#,
        event_type
    );
    let response = match prom.query_promql(&promql).await {
        Ok(r) => r,
        Err(_) => return 0.0,
    };
    response
        .data
        .result
        .first()
        .and_then(|r| r.value.as_ref())
        .and_then(|(_, v)| v.parse::<f64>().ok())
        .unwrap_or(0.0)
}

// ============================================================================
// Scenario 1: media_servers populated in JoinResponse
// ============================================================================

/// Test: GC's join response includes a non-empty `mc_assignment` and, after
/// the client connects to MC and sends a `JoinRequest`, MC's `JoinResponse`
/// includes `media_servers` populated from Redis with non-empty
/// `media_handler_url` values pointing at MH WebTransport endpoints.
///
/// Ground truth: `crates/mc-service/src/webtransport/connection.rs:712-718`
/// populates `media_servers` from `MhAssignmentData.handlers`. In Kind,
/// `infra/kind/scripts/setup.sh:651-655` patches MH ConfigMaps to advertise
/// host-reachable URLs, so the URLs returned here are reachable from the test.
///
/// On-call: if this fails after a deploy, suspect MH ConfigMap advertise
/// address misconfiguration (`MH_WEBTRANSPORT_ADVERTISE_ADDRESS`) or Redis
/// MH-assignment data missing for the meeting.
#[tokio::test]
async fn test_mh_url_present_in_join_response() {
    let cluster = cluster().await;
    let (user_token, display_name) = shared_user(cluster).await.clone();

    let gc_join = gc_create_and_join(cluster, &user_token, "MH URL Present Test").await;

    let mc_url = gc_join
        .mc_assignment
        .webtransport_endpoint
        .as_ref()
        .expect("MC assignment must include webtransport_endpoint");

    let join_response = mc_join(
        mc_url,
        &gc_join.meeting_id.to_string(),
        &gc_join.token,
        &display_name,
    )
    .await;

    assert!(
        !join_response.media_servers.is_empty(),
        "JoinResponse.media_servers must be non-empty (MC populates from MhAssignmentData in Redis)",
    );

    for (idx, server) in join_response.media_servers.iter().enumerate() {
        assert!(
            !server.media_handler_url.is_empty(),
            "media_servers[{}].media_handler_url must be non-empty",
            idx
        );
        assert!(
            server.media_handler_url.starts_with("https://"),
            "media_servers[{}].media_handler_url must use https:// scheme (got: {})",
            idx,
            server.media_handler_url,
        );
    }
}

// ============================================================================
// Scenarios 2 & 3: MH JWT acceptance / rejection
// ============================================================================

/// Send a `JoinRequest` to MC over a fresh WebTransport connection and read
/// the framed `JoinResponse`. Driving a real MC join is required to make MC
/// fire `RegisterMeeting` to every assigned MH (R-12), which is the
/// precondition for MH-side tests that expect a registered meeting.
///
/// Returns the parsed `JoinResponse`; the WebTransport connection is dropped.
async fn mc_join(
    mc_url: &str,
    meeting_id: &str,
    meeting_token: &str,
    participant_name: &str,
) -> proto_gen::signaling::JoinResponse {
    use bytes::{BufMut, BytesMut as MsgBuf};
    use prost::Message;
    use proto_gen::signaling::{
        client_message, server_message, ClientMessage, JoinRequest, ServerMessage,
    };

    let conn = connect_wt(mc_url).await;
    let (mut send, mut recv) = conn
        .open_bi()
        .await
        .expect("open bi stream to MC")
        .await
        .expect("MC bi stream ready");

    let join_msg = ClientMessage {
        message: Some(client_message::Message::JoinRequest(JoinRequest {
            meeting_id: meeting_id.to_string(),
            join_token: meeting_token.to_string(),
            participant_name: participant_name.to_string(),
            capabilities: None,
            correlation_id: String::new(),
            binding_token: String::new(),
        })),
    };
    let encoded = join_msg.encode_to_vec();
    let mut frame = MsgBuf::with_capacity(4 + encoded.len());
    frame.put_u32(encoded.len() as u32);
    frame.put_slice(&encoded);
    send.write_all(&frame)
        .await
        .expect("send JoinRequest to MC");

    let mut len_buf = [0u8; 4];
    tokio::time::timeout(Duration::from_secs(10), recv.read_exact(&mut len_buf))
        .await
        .expect("MC JoinResponse timed out")
        .expect("read MC JoinResponse length");
    let msg_len = u32::from_be_bytes(len_buf) as usize;
    assert!(
        msg_len > 0 && msg_len <= 65536,
        "MC framed JoinResponse length out of range: {msg_len}"
    );
    let mut buf = vec![0u8; msg_len];
    recv.read_exact(&mut buf)
        .await
        .expect("read MC JoinResponse body");
    let server_msg = ServerMessage::decode(buf.as_slice()).expect("decode ServerMessage");

    match server_msg.message {
        Some(server_message::Message::JoinResponse(j)) => j,
        Some(server_message::Message::Error(e)) => panic!(
            "MC returned error instead of JoinResponse: code={} message={}",
            e.code, e.message
        ),
        // Print only the variant name, not the full Debug payload, to avoid
        // echoing potentially-PII-bearing fields (participant names from
        // unexpected ParticipantJoined notifications, etc.) into CI logs.
        Some(_) => panic!("Expected JoinResponse from MC, got a different ServerMessage variant"),
        None => panic!("Expected JoinResponse from MC, got an empty ServerMessage"),
    }
}

/// Drive the full GC→MC join so MC fires `RegisterMeeting` to all assigned MHs,
/// then return the meeting JWT and the first MH WebTransport URL. Used by the
/// MH-side scenarios that need a "registered meeting" precondition.
async fn join_with_registered_mh(
    cluster: &ClusterConnection,
    user_token: &str,
    display_name: &str,
    meeting_name: &str,
) -> (String, String) {
    let gc_join = gc_create_and_join(cluster, user_token, meeting_name).await;
    let mc_url = gc_join
        .mc_assignment
        .webtransport_endpoint
        .clone()
        .expect("MC assignment must include webtransport_endpoint");

    let join_response = mc_join(
        &mc_url,
        &gc_join.meeting_id.to_string(),
        &gc_join.token,
        display_name,
    )
    .await;

    let mh_url = join_response
        .media_servers
        .first()
        .map(|m| m.media_handler_url.clone())
        .filter(|u| !u.is_empty())
        .expect("MC JoinResponse must include at least one non-empty MH URL");

    (gc_join.token, mh_url)
}

/// Test: MH accepts a connection authenticated by a valid meeting JWT for a
/// registered meeting, and holds the WebTransport session open (no immediate
/// disconnect, no session-level close).
///
/// Asserts at the SESSION level via `conn.closed()`, NOT at the JWT-carrier
/// bidi stream level. MH closes its send-half of that bidi stream immediately
/// after accept (see `crates/mh-service/src/webtransport/connection.rs:163`),
/// which surfaces to the client as `Ok(None)` on the recv side — but the
/// session is still alive. The `conn.closed()` future only resolves when the
/// WT session itself closes.
///
/// On-call: if this fails, suspect AC JWKS misconfig on MH (`AC_JWKS_URL`),
/// MH↔AC network policy, or MC→MH `RegisterMeeting` failure (the meeting may
/// be in MH's provisional pool and time out before this test finishes).
#[tokio::test]
async fn test_mh_accepts_valid_meeting_jwt() {
    let cluster = cluster().await;
    let auth_client = AuthClient::new(&cluster.ac_base_url);
    let (user_token, display_name) = register_test_user(&auth_client, "MH Valid JWT User").await;

    let (jwt, mh_url) = join_with_registered_mh(
        cluster,
        &user_token,
        &display_name,
        "MH Valid JWT Test Meeting",
    )
    .await;

    let conn = connect_wt(&mh_url).await;
    let (_send, _recv) = send_jwt_on_bi_stream(&conn, &jwt).await;

    // Held-open invariant: `conn.closed()` resolves with the close reason
    // only after the WebTransport session terminates. If the session is
    // alive and healthy, the future is pending; the timeout firing is the
    // success signal. 2.5s gives generous headroom over network jitter.
    let close_outcome = tokio::time::timeout(Duration::from_millis(2500), conn.closed()).await;

    assert!(
        close_outcome.is_err(),
        "MH closed the WebTransport session within 2.5s of a valid JWT — \
         expected the session to be held open. Likely cause: MC→MH \
         RegisterMeeting missing for this meeting, AC_JWKS misconfig on MH, \
         or TLS/WT framing mismatch. Close reason: {close_outcome:?}",
    );
}

/// Test: MH rejects a structurally-valid JWT with a forged signature.
///
/// Mirrors `test_mc_rejects_invalid_meeting_token` in `24_join_flow.rs`.
/// Exercises MH's signature-verification path (EdDSA via JWKS).
///
/// On-call: if this passes when MH accepts the token (i.e., this test FAILS
/// because we never observed a peer-close), suspect that MH's JWT validator
/// is not enforcing signature verification — security-critical.
#[tokio::test]
async fn test_mh_rejects_forged_jwt() {
    let cluster = cluster().await;
    let auth_client = AuthClient::new(&cluster.ac_base_url);
    let (user_token, display_name) = register_test_user(&auth_client, "MH Forged JWT User").await;

    let (_jwt, mh_url) = join_with_registered_mh(
        cluster,
        &user_token,
        &display_name,
        "MH Forged JWT Test Meeting",
    )
    .await;

    // Structurally-valid JWT with garbage signature. Same constant as
    // 24_join_flow.rs:584 — exercises MH's signature verification path.
    let forged = "eyJhbGciOiJFZERTQSIsInR5cCI6IkpXVCJ9.\
        eyJzdWIiOiJhdHRhY2tlciIsIm1lZXRpbmdfaWQiOiJmYWtlIiwiZXhwIjo5OTk5OTk5OTk5fQ.\
        invalid_signature_that_will_not_verify";

    assert_mh_rejects(&mh_url, forged).await;
}

/// Test: MH rejects an oversized JWT (> `MAX_JWT_SIZE_BYTES` = 8192 in
/// `crates/common/src/jwt.rs:73`).
///
/// 9000 bytes of benign filler ('A' repeats) — well over the 8KB validator
/// cap, well under the 64KB framing cap. Exercises the size check at
/// `MhJwtValidator::validate_meeting_token` before any signature work.
///
/// On-call: if this fails (i.e., MH does NOT close the connection), MH may
/// be allocating per-byte memory before enforcing the size cap — DoS risk.
#[tokio::test]
async fn test_mh_rejects_oversized_jwt() {
    let cluster = cluster().await;
    let auth_client = AuthClient::new(&cluster.ac_base_url);
    let (user_token, display_name) =
        register_test_user(&auth_client, "MH Oversized JWT User").await;

    let (_jwt, mh_url) = join_with_registered_mh(
        cluster,
        &user_token,
        &display_name,
        "MH Oversized JWT Test Meeting",
    )
    .await;

    // Benign filler — explicitly NOT shaped like a real JWT (no dots, no
    // base64 header) so we don't normalize logging realistic-looking tokens.
    let oversized = "A".repeat(9000);
    assert_mh_rejects(&mh_url, &oversized).await;
}

// ============================================================================
// Scenarios 4 & 5: Prometheus delta on mc_mh_notifications_received_total
// ============================================================================

/// Test: After a client opens a WebTransport session to MH with a valid JWT,
/// MC observes `mc_mh_notifications_received_total{event_type="connected"}`
/// increment via Prometheus.
///
/// Validates the MH→MC `NotifyParticipantConnected` plane (R-15, R-16).
/// Asserts a strict-greater-than delta on the cluster-wide counter (the
/// metric is intentionally low-cardinality — only `event_type` label — so we
/// cannot scope per-meeting).
///
/// On-call: if this fails, suspect MH→MC gRPC connectivity (network policy
/// egress :50052), MH OAuth token acquisition from AC, or MC's
/// `MediaCoordinationService.NotifyParticipantConnected` handler.
#[tokio::test]
#[serial_test::serial(mh_notifications)]
async fn test_mh_connect_increments_mc_notification_metric_connected() {
    let cluster = cluster().await;
    let prom = PrometheusClient::new(&cluster.prometheus_base_url);

    let auth_client = AuthClient::new(&cluster.ac_base_url);
    let (user_token, display_name) =
        register_test_user(&auth_client, "MH MC-Metric Connect User").await;

    // Cross-test stabilization (mirrors test 5's pattern): wait for any in-flight
    // connect signal from a sibling test under the same `#[serial(mh_notifications)]`
    // group to finish scraping before we snapshot baseline. Use `sum()` so
    // multi-replica MC scaling returns a single scalar.
    wait_for_notification_counter_stable(&prom, "connected").await;
    let baseline = mh_notification_counter(&prom, "connected").await;

    let (jwt, mh_url) = join_with_registered_mh(
        cluster,
        &user_token,
        &display_name,
        "MH Connect Metric Test Meeting",
    )
    .await;

    let conn = connect_wt(&mh_url).await;
    let (_send, _recv) = send_jwt_on_bi_stream(&conn, &jwt).await;

    // The connect notification fires from MH best-effort fire-and-forget after
    // JWT validation. The chain is MH spawn-task → gRPC to MC → MC counter →
    // Prometheus scrape (15s SLA). 60s budget absorbs cluster-load variance.
    assert_notification_counter_increases_past(&prom, "connected", baseline).await;
}

/// Test: After a client cleanly disconnects a WebTransport session from MH,
/// MC observes `mc_mh_notifications_received_total{event_type="disconnected"}`
/// increment via Prometheus.
///
/// Validates MH→MC `NotifyParticipantDisconnected` (R-17). Same delta shape
/// as the connected test.
///
/// On-call: if this fails, the MH connection-handler cleanup path
/// (`crates/mh-service/src/webtransport/connection.rs:347-377`) may not be
/// reached — check MH logs for "Connection closed and cleaned up".
#[tokio::test]
#[serial_test::serial(mh_notifications)]
async fn test_mh_disconnect_increments_mc_notification_metric_disconnected() {
    let cluster = cluster().await;
    let prom = PrometheusClient::new(&cluster.prometheus_base_url);

    let auth_client = AuthClient::new(&cluster.ac_base_url);
    let (user_token, display_name) =
        register_test_user(&auth_client, "MH MC-Metric Disconnect User").await;

    // Cross-test stabilization: the predecessor test under the same
    // `#[serial(mh_notifications)]` group may have produced a disconnect
    // signal that is still in flight to Prometheus. If we snapshot baseline
    // before that signal lands, the eventual loop below would falsely succeed
    // on the leftover bump. Wait for the counter to settle before snapshotting.
    wait_for_notification_counter_stable(&prom, "disconnected").await;
    let baseline = mh_notification_counter(&prom, "disconnected").await;

    let (jwt, mh_url) = join_with_registered_mh(
        cluster,
        &user_token,
        &display_name,
        "MH Disconnect Metric Test Meeting",
    )
    .await;

    let conn = connect_wt(&mh_url).await;
    let (mut send, _recv) = send_jwt_on_bi_stream(&conn, &jwt).await;

    // Clean close: finish the send stream (produces Ok(None) on the server's
    // recv) and drop the connection. Matches the `ClientClosed` branch in
    // `crates/mh-service/src/webtransport/connection.rs:325`.
    send.finish()
        .await
        .expect("client send.finish() should succeed");
    drop(conn);

    assert_notification_counter_increases_past(&prom, "disconnected", baseline).await;
}

// ============================================================================
// Scenario 6 (R-33 #6): unregistered-meeting timeout — STUB
// ============================================================================

/// R-33 #6 stub — see authoritative coverage at component tier:
/// `crates/mh-service/tests/webtransport_integration.rs::provisional_connection_kicked_after_register_meeting_timeout`.
///
/// That component test runs the real MH `accept_loop` with virtual-time
/// control, asserts the lower-bound (counter still 0 at 800ms) AND
/// upper-bound (counter reaches 1 by 3000ms) on
/// `mh_webtransport_connections_total{status="error"}` and
/// `mh_register_meeting_timeouts_total`, and verifies
/// `active_connection_count == 0` after timeout.
///
/// We cannot run this scenario from env-tests because:
/// 1. AC's signing key is not exposed to env-tests (security boundary —
///    see plan §Q1), so we cannot mint a JWT for an unregistered meeting_id.
/// 2. After first MC join, MC fires `RegisterMeeting` to all assigned MHs
///    (R-12), so for any meeting created via the real GC→MC flow, no MH
///    stays "unregistered".
/// 3. Lowering `MH_REGISTER_MEETING_TIMEOUT_SECONDS` in Kind ConfigMaps
///    would create a dev-vs-prod behavioral gap — rejected by ops at plan
///    review.
///
/// Tracked as Tech Debt in `docs/devloop-outputs/2026-04-30-mh-quic-env-tests/main.md`.
#[tokio::test]
#[ignore = "covered at component tier — see crates/mh-service/tests/webtransport_integration.rs::provisional_connection_kicked_after_register_meeting_timeout"]
async fn test_mh_disconnects_unregistered_meeting_after_timeout() {
    // Intentionally unimplemented. See doc-comment above.
}
