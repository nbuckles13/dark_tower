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
//! 7. `test_mh_forwards_an_audio_datagram_back_to_its_sender` — R-15/R-18
//!    loopback forward path, and it RUNS: the `#[ignore]` came off at story
//!    task 26, whose diagnosis found no MH receive-path defect to fix (MH was
//!    byte-identical since task 24 and this test passed unmodified). It proves
//!    COMPOSITION: real MC join programs real MH, MC's
//!    `NotifyParticipantConnectedResponse` carries an ordinal MH accepts, and a
//!    real v2 frame returns over real QUIC rewritten only in its relay region.
//!    Being a single-participant loopback it CANNOT prove the binding is the
//!    right one — see its own doc comment. That is
//!    `crates/mh-service/tests/media_session_binding_integration.rs`
//!    (two participants, two meetings); forward-path mechanics stay in
//!    `crates/mh-service/tests/media_forward_integration.rs`.
//! 9. `test_mc_programs_live_handler_with_confirmed_forwarding_policy` —
//!    `mc_media_policy_pushes_total{outcome="match"}` delta >= 1 after a real
//!    join, with the four non-`match` outcome series flat. The positive
//!    composition counterpart to MH's negative unit gates (ADR-0036 §8).
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
use env_tests::fixtures::media::sample_identity_public_key;
use env_tests::fixtures::metrics::{
    any_instance_exceeds_baseline, format_instance_map, poll_until_any_instance_above,
    poll_until_exactly_one_instance_rose, poll_until_instance_above, poll_until_stable,
    InstanceCounters,
};
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
    use proto_gen::dark_tower::signaling::v1::{
        mh_client_message, MhClientMessage, MhConnectRequest,
    };

    let envelope = MhClientMessage {
        message: Some(mh_client_message::Message::ConnectRequest(
            MhConnectRequest {
                join_token: jwt.to_string(),
            },
        )),
        trace_parent: String::new(),
        trace_state: String::new(),
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

/// The per-instance PromQL for `mc_mh_notifications_received_total` — the ONE
/// string source shared by the stabilize loop and the delta assert, so the two
/// cannot disagree on the query.
///
/// See [`InstanceCounters`] for why the grouping label is `instance` (pod
/// IP:port, fresh on every rollover) and the two traps that keep it
/// load-bearing (the `pod` label lives only in the logs/Loki pipeline; a future
/// `labelmap` in the metrics scrape config would silently break this). Reads
/// through this query FAIL LOUDLY on a Prometheus error (naming the PromQL +
/// error); an empty result (counter not yet observed) is a legitimate empty map
/// — a per-pod zero distinct from a failed query (docs/TODO.md §Env-Test
/// Resilience, defect #2).
fn notification_promql(event_type: &str) -> String {
    format!(
        r#"sum by (instance) (mc_mh_notifications_received_total{{event_type="{event_type}"}})"#
    )
}

/// Wait for the PER-INSTANCE `mc_mh_notifications_received_total{event_type=...}`
/// snapshot to stabilize: two consecutive reads (a Prometheus scrape interval
/// apart) returning the SAME per-instance map. Closes the cross-test race where
/// leftover signals from a predecessor test (under the same `#[serial]` group)
/// might still be in flight to Prometheus when the next test snapshots its
/// baseline. After this returns, baseline-reads are safe.
///
/// The loop is the shared [`poll_until_stable`]; the two numbers below are this
/// caller's own decisions, argued here. **Read that helper's doc before changing
/// either** — the settle interval in particular is a correctness precondition,
/// not a budget, and shortening it makes the stabilize vacuous rather than fast.
///
/// # Settle 16s — STATED HERE FOR BOTH event-visibility waits
///
/// One Prometheus scrape interval (15s SLA) plus margin, so any outstanding
/// scrape lands between the two reads. This is **not a local decision**: it is
/// the same decision `wait_for_policy_push_counter_stable` makes, which
/// references this paragraph rather than restating it. Editing it changes both.
///
/// **Shared drift obligation — ONE trigger, three reasons.** All THREE 16s values
/// in this binary derive from the same 15s scrape SLA, so a change to that SLA
/// invalidates all three and they must move together. (It is also one of the
/// three triggers the extraction deferral recorded — see
/// `wait_for_policy_push_counter_stable`.) The *derivations* stay separate,
/// because why each site waits genuinely differs and collapsing them would be a
/// false SSoT: these two wait for an in-flight increment to become visible; the
/// media-forward path's baseline-freshness gate waits for baseline completeness.
///
/// **Re-check the freshness gate FIRST, not last.** Its failure mode under a
/// stale value is the worst of the three: too small a settle there puts both
/// reads inside one scrape, `instance_maps_equal` compares them equal, and
/// `poll_until_stable` returns having observed nothing — silently restoring the
/// false-positive instance identification the gate exists to prevent. Here, too
/// small a settle merely weakens a precondition and shows up as flake.
///
/// Budget 90s: the chain that has to settle is MH's fire-and-forget
/// `tokio::spawn(notify)` -> gRPC RPC -> MC counter increment -> Prometheus
/// scrape (15s SLA). Under cluster load this can exceed the 30s `MetricsScrape`
/// category, so a longer custom budget is used here.
/// Returns the STABILISED snapshot, which callers use as their baseline. Reading
/// the counter again afterwards would baseline on a *different*, newer read than
/// the one just proved stable — a small window, but the whole point of the wait
/// is that the baseline is the settled value.
async fn wait_for_notification_counter_stable(
    prom: &PrometheusClient,
    event_type: &str,
) -> InstanceCounters {
    poll_until_stable(
        prom,
        &notification_promql(event_type),
        Duration::from_secs(16),
        Duration::from_secs(90),
        |v1, v2| {
            format!(
                "mc_mh_notifications_received_total{{event_type=\"{event_type}\"}} per-instance \
                 snapshot did not stabilize within 90s (last reads: v1={}, v2={})",
                format_instance_map(v1),
                format_instance_map(v2),
            )
        },
    )
    .await
}

/// Poll until SOME currently-present instance's
/// `mc_mh_notifications_received_total{event_type=...}` value exceeds its OWN
/// `baseline` value. Robust to a pod rollover (a fresh post-rollover pod passes
/// once its value exceeds zero; an expired old-pod series can't inflate anything
/// because the comparison never sums across pods). The loop itself is the shared
/// [`poll_until_any_instance_above`] so this and the participant-status assert
/// cannot drift in budget/ordering/message-shape. Budget: 60s — 2x
/// `MetricsScrape` for the MH spawn-task + gRPC + MC handler + scrape chain.
async fn assert_notification_counter_increases_past(
    prom: &PrometheusClient,
    event_type: &str,
    baseline: &InstanceCounters,
) {
    poll_until_any_instance_above(
        prom,
        &notification_promql(event_type),
        baseline,
        Duration::from_secs(60),
        Duration::from_secs(2),
        |current| {
            format!(
                "mc_mh_notifications_received_total{{event_type=\"{event_type}\"}} did not increase \
                 past baseline on any instance within 60s (baseline: {}, last observed: {})",
                format_instance_map(baseline),
                format_instance_map(current),
            )
        },
    )
    .await;
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
) -> proto_gen::dark_tower::signaling::v1::JoinResponse {
    mc_join_session(mc_url, meeting_id, meeting_token, participant_name)
        .await
        .join_response
}

/// A live post-join MC session: the WebTransport connection and its bidi stream
/// stay OPEN, so post-join signalling (ADR-0036 §6 `ReceiveCapability` in,
/// §5 `SendDirective` + `StreamAssignments` out) can be driven over it.
///
/// [`mc_join`] is the thin wrapper for callers that only want the
/// `JoinResponse`; dropping this struct closes the session, which is what that
/// wrapper does.
struct McSession {
    _conn: wtransport::Connection,
    send: wtransport::SendStream,
    recv: wtransport::RecvStream,
    join_response: proto_gen::dark_tower::signaling::v1::JoinResponse,
}

impl McSession {
    /// Write a length-prefixed `ClientMessage`.
    async fn write(&mut self, message: &proto_gen::dark_tower::signaling::v1::ClientMessage) {
        let encoded = message.encode_to_vec();
        let mut frame = BytesMut::with_capacity(4 + encoded.len());
        frame.put_u32(u32::try_from(encoded.len()).expect("message fits a u32 length prefix"));
        frame.put_slice(&encoded);
        self.send
            .write_all(&frame)
            .await
            .expect("write ClientMessage to MC");
    }

    /// Read the next length-prefixed `ServerMessage`.
    async fn read(&mut self) -> proto_gen::dark_tower::signaling::v1::ServerMessage {
        read_framed_server_message(&mut self.recv).await
    }
}

/// Read one length-prefixed `ServerMessage` off an MC bidi stream.
///
/// Local to this binary rather than in `env_tests::fixtures`: `proto-gen`,
/// `wtransport`, `prost` and `bytes` are `[dev-dependencies]` of `env-tests`, so
/// the lib target cannot name `ServerMessage` at all. Promoting them to fix that
/// would make every consumer of the lib link `wtransport` — a real cost for a
/// structural-duplication concern. The extraction is filed under
/// `docs/TODO.md` §Cross-Service Duplication, owned by @test, along with that
/// reachability constraint so nobody "fixes" it by promoting the dependencies.
async fn read_framed_server_message(
    recv: &mut wtransport::RecvStream,
) -> proto_gen::dark_tower::signaling::v1::ServerMessage {
    use proto_gen::dark_tower::signaling::v1::ServerMessage;

    let mut len_buf = [0u8; 4];
    tokio::time::timeout(Duration::from_secs(10), recv.read_exact(&mut len_buf))
        .await
        .expect("MC ServerMessage timed out")
        .expect("read MC ServerMessage length");
    let msg_len = u32::from_be_bytes(len_buf) as usize;
    assert!(
        msg_len > 0 && msg_len <= 65536,
        "MC framed ServerMessage length out of range: {msg_len}"
    );
    let mut buf = vec![0u8; msg_len];
    recv.read_exact(&mut buf)
        .await
        .expect("read MC ServerMessage body");
    ServerMessage::decode(buf.as_slice()).expect("decode ServerMessage")
}

/// Send a `JoinRequest` to MC over a fresh WebTransport connection, read the
/// framed `JoinResponse`, and **keep the session open**. Driving a real MC join
/// is required to make MC fire `RegisterMeeting` to every assigned MH (R-12),
/// which is the precondition for MH-side tests that expect a registered meeting.
async fn mc_join_session(
    mc_url: &str,
    meeting_id: &str,
    meeting_token: &str,
    participant_name: &str,
) -> McSession {
    use proto_gen::dark_tower::signaling::v1::{
        client_message, server_message, ClientMessage, JoinRequest,
    };

    let conn = connect_wt(mc_url).await;
    let (send, recv) = conn
        .open_bi()
        .await
        .expect("open bi stream to MC")
        .await
        .expect("MC bi stream ready");

    let mut session = McSession {
        _conn: conn,
        send,
        recv,
        // Placeholder replaced immediately below; never observed.
        join_response: proto_gen::dark_tower::signaling::v1::JoinResponse::default(),
    };

    session
        .write(&ClientMessage {
            message: Some(client_message::Message::JoinRequest(JoinRequest {
                meeting_id: meeting_id.to_string(),
                join_token: meeting_token.to_string(),
                participant_name: participant_name.to_string(),
                capabilities: None,
                correlation_id: String::new(),
                binding_token: String::new(),
                // ADR-0036 §4: raw Ed25519 identity signing public key. A valid
                // 32-byte fixture — this test exercises the join path, not
                // attribution, and MC performs no attestation check this story.
                identity_public_key: sample_identity_public_key(),
            })),
            trace_parent: String::new(),
            trace_state: String::new(),
        })
        .await;

    let server_msg = session.read().await;
    session.join_response = match server_msg.message {
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
    };
    session
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

    // ANY assigned handler, taken in ARBITRARY Redis enumeration order — NOT the
    // steered one. `media_servers` is connection bootstrap data and is
    // deliberately unsorted, so its head carries no relationship to placement.
    //
    // Legitimate here: this caller needs "some handler on which this meeting is
    // registered", and MC pushes `RegisterMeeting` to EVERY assigned handler, so
    // both answer the question identically. The two answers may nonetheless
    // *appear* to agree on any given run — the steered handler is always the
    // lexicographically smallest assigned `mh_id`, so an arbitrary pick
    // coincides with it some of the time, by accident and never because they are
    // the same question.
    //
    // A caller that needs the handler MC DIRECTED a client to send on must read
    // the send directive instead — see `follow_mc_steering`.
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
// This `#[serial]` key is LOAD-BEARING, not tidiness. This test opens a
// WebTransport connection to MH, which increments
// `mh_webtransport_connections_total` — the counter
// `test_mh_forwards_an_audio_datagram_back_to_its_sender` uses to identify the
// instance MC steered its client to. Running concurrently would put this
// connection's increment in that test's risen set, and it would either trip the
// ambiguity panic or (with our increment scraped and theirs not yet) name the
// WRONG instance, so the policy-apply gate would then fail about a handler
// nothing was ever steered to. Removing this attribute silently unsounds that
// probe. See that test's STEP 2 for the full argument.
#[serial_test::serial(mh_notifications)]
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
// This `#[serial]` key is LOAD-BEARING, not tidiness. This test opens a
// WebTransport connection to MH, which increments
// `mh_webtransport_connections_total` — the counter
// `test_mh_forwards_an_audio_datagram_back_to_its_sender` uses to identify the
// instance MC steered its client to. Running concurrently would put this
// connection's increment in that test's risen set, and it would either trip the
// ambiguity panic or (with our increment scraped and theirs not yet) name the
// WRONG instance, so the policy-apply gate would then fail about a handler
// nothing was ever steered to. Removing this attribute silently unsounds that
// probe. See that test's STEP 2 for the full argument.
#[serial_test::serial(mh_notifications)]
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
// This `#[serial]` key is LOAD-BEARING, not tidiness. This test opens a
// WebTransport connection to MH, which increments
// `mh_webtransport_connections_total` — the counter
// `test_mh_forwards_an_audio_datagram_back_to_its_sender` uses to identify the
// instance MC steered its client to. Running concurrently would put this
// connection's increment in that test's risen set, and it would either trip the
// ambiguity panic or (with our increment scraped and theirs not yet) name the
// WRONG instance, so the policy-apply gate would then fail about a handler
// nothing was ever steered to. Removing this attribute silently unsounds that
// probe. See that test's STEP 2 for the full argument.
#[serial_test::serial(mh_notifications)]
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
    // group to finish scraping before we snapshot baseline. Baseline is a
    // per-instance map (`sum by (instance)`), robust to a mid-assertion pod
    // rollover — see the helper docs.
    let baseline = wait_for_notification_counter_stable(&prom, "connected").await;

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
    assert_notification_counter_increases_past(&prom, "connected", &baseline).await;
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
    let baseline = wait_for_notification_counter_stable(&prom, "disconnected").await;

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

    assert_notification_counter_increases_past(&prom, "disconnected", &baseline).await;
}

// ============================================================================
// Scenario 7 (R-60 MC half): MediaConnectionUpdate → mc_participant_mh_status_total
// ============================================================================

/// The per-instance PromQL for `mc_participant_mh_status_total` — the ONE string
/// source shared by the reader and the delta assert.
fn participant_mh_status_promql(state: &str) -> String {
    format!(r#"sum by (instance) (mc_participant_mh_status_total{{state="{state}"}})"#)
}

/// Read a PER-INSTANCE snapshot of `mc_participant_mh_status_total` for a
/// specific `state` label, via `sum by (instance)(...)`. Mirrors
/// [`notification_promql`]; see [`InstanceCounters`] for the `instance`
/// grouping rationale + the two traps. FAILS LOUDLY on a Prometheus query
/// error; an empty result (state not yet observed) is a legitimate empty map
/// (per-pod zero), distinct from a failed query.
async fn mc_participant_mh_status_counter(
    prom: &PrometheusClient,
    state: &str,
) -> InstanceCounters {
    prom.instance_counter_map(&participant_mh_status_promql(state))
        .await
}

/// Poll until SOME currently-present instance's
/// `mc_participant_mh_status_total{state=...}` value exceeds its OWN `baseline`
/// value — robust to a pod rollover, via the shared
/// [`poll_until_any_instance_above`] (same loop as
/// [`assert_notification_counter_increases_past`], so the two cannot drift).
/// Budget: 60s — the chain is WT frame → MC bridge-loop decode → participant
/// actor record → counter → Prometheus scrape (15s SLA).
async fn assert_participant_mh_status_increases_past(
    prom: &PrometheusClient,
    state: &str,
    baseline: &InstanceCounters,
) {
    poll_until_any_instance_above(
        prom,
        &participant_mh_status_promql(state),
        baseline,
        Duration::from_secs(60),
        Duration::from_secs(2),
        |current| {
            format!(
                "mc_participant_mh_status_total{{state=\"{state}\"}} did not increase past baseline \
                 on any instance within 60s (baseline: {}, last observed: {})",
                format_instance_map(baseline),
                format_instance_map(current),
            )
        },
    )
    .await;
}

/// Test: after a client joins MC and sends a
/// `ClientMessage{MediaConnectionUpdate}` reporting a `CONNECTED` MH, MC
/// records it on the participant actor and
/// `mc_participant_mh_status_total{state="connected"}` increments (R-60 MC
/// half — the post-join client→MC reporting plane).
///
/// This is the cluster-tier companion to the component-tier seam coverage in
/// `crates/mc-service/tests/media_connection_update_integration.rs` (which
/// asserts the same metric through the real `handle_client_message` decode
/// path) and the deterministic actor + cap coverage in
/// `crates/mc-service/src/actors/participant.rs::tests`.
///
/// On-call: if this fails, suspect MC's post-join `handle_client_message`
/// dispatch (`crates/mc-service/src/webtransport/connection.rs`), the
/// `ParticipantActor::record_mh_statuses` path, or Prometheus scrape of MC.
#[tokio::test]
#[serial_test::serial(mc_participant_mh_status)]
async fn test_mc_media_connection_update_increments_participant_mh_status_metric() {
    use bytes::{BufMut, BytesMut as MsgBuf};
    use prost::Message;
    use proto_gen::dark_tower::signaling::v1::{
        client_message, server_message, ClientMessage, ConnectionState, JoinRequest,
        MediaConnectionUpdate, MhConnectionStatus, ServerMessage,
    };

    let cluster = cluster().await;
    let prom = PrometheusClient::new(&cluster.prometheus_base_url);
    let auth_client = AuthClient::new(&cluster.ac_base_url);
    let (user_token, display_name) = register_test_user(&auth_client, "MC MediaUpdate User").await;

    let baseline = mc_participant_mh_status_counter(&prom, "connected").await;

    // Real GC→MC join, but keep the bidi stream open so we can send a
    // follow-up MediaConnectionUpdate on it (mc_join drops the connection).
    let gc_join = gc_create_and_join(
        cluster,
        &user_token,
        "MC MediaConnectionUpdate Metric Meeting",
    )
    .await;
    let mc_url = gc_join
        .mc_assignment
        .webtransport_endpoint
        .clone()
        .expect("MC assignment must include webtransport_endpoint");

    let conn = connect_wt(&mc_url).await;
    let (mut send, mut recv) = conn
        .open_bi()
        .await
        .expect("open bi stream to MC")
        .await
        .expect("MC bi stream ready");

    let join_msg = ClientMessage {
        message: Some(client_message::Message::JoinRequest(JoinRequest {
            meeting_id: gc_join.meeting_id.to_string(),
            join_token: gc_join.token.clone(),
            participant_name: display_name.clone(),
            capabilities: None,
            correlation_id: String::new(),
            binding_token: String::new(),
            // ADR-0036 §4: raw Ed25519 identity signing public key. A valid
            // 32-byte fixture — this test exercises the join path, not
            // attribution, and MC performs no attestation check this story.
            identity_public_key: sample_identity_public_key(),
        })),
        trace_parent: String::new(),
        trace_state: String::new(),
    };
    let encoded = join_msg.encode_to_vec();
    let mut frame = MsgBuf::with_capacity(4 + encoded.len());
    frame.put_u32(encoded.len() as u32);
    frame.put_slice(&encoded);
    send.write_all(&frame)
        .await
        .expect("send JoinRequest to MC");

    // Read the JoinResponse to learn the assigned MH URL(s).
    let mut len_buf = [0u8; 4];
    tokio::time::timeout(Duration::from_secs(10), recv.read_exact(&mut len_buf))
        .await
        .expect("MC JoinResponse timed out")
        .expect("read MC JoinResponse length");
    let msg_len = u32::from_be_bytes(len_buf) as usize;
    assert!(
        msg_len > 0 && msg_len <= 65536,
        "framed length out of range"
    );
    let mut buf = vec![0u8; msg_len];
    recv.read_exact(&mut buf)
        .await
        .expect("read JoinResponse body");
    let join_response = match ServerMessage::decode(buf.as_slice())
        .expect("decode ServerMessage")
        .message
    {
        Some(server_message::Message::JoinResponse(j)) => j,
        _ => panic!("expected JoinResponse from MC"),
    };
    // ANY assigned handler, taken in ARBITRARY Redis enumeration order — NOT the
    // steered one. `media_servers` is connection bootstrap data and is
    // deliberately unsorted, so its head carries no relationship to placement.
    //
    // Legitimate here: this caller needs "some handler on which this meeting is
    // registered", and MC pushes `RegisterMeeting` to EVERY assigned handler, so
    // both answer the question identically. The two answers may nonetheless
    // *appear* to agree on any given run — the steered handler is always the
    // lexicographically smallest assigned `mh_id`, so an arbitrary pick
    // coincides with it some of the time, by accident and never because they are
    // the same question.
    //
    // A caller that needs the handler MC DIRECTED a client to send on must read
    // the send directive instead — see `follow_mc_steering`.
    let mh_url = join_response
        .media_servers
        .first()
        .map(|m| m.media_handler_url.clone())
        .filter(|u| !u.is_empty())
        .expect("JoinResponse must include a non-empty MH URL");

    // Report that MH as CONNECTED via the post-join plane.
    let update = ClientMessage {
        message: Some(client_message::Message::MediaConnectionUpdate(
            MediaConnectionUpdate {
                statuses: vec![MhConnectionStatus {
                    mh_url,
                    state: ConnectionState::Connected as i32,
                    failure_reason: None,
                    failure_code: None,
                    observed_at: None,
                }],
            },
        )),
        trace_parent: String::new(),
        trace_state: String::new(),
    };
    let encoded = update.encode_to_vec();
    let mut frame = MsgBuf::with_capacity(4 + encoded.len());
    frame.put_u32(encoded.len() as u32);
    frame.put_slice(&encoded);
    send.write_all(&frame)
        .await
        .expect("send MediaConnectionUpdate to MC");

    // Keep the connection alive long enough for MC's bridge loop to read the
    // frame before we let `conn` drop at end of scope.
    assert_participant_mh_status_increases_past(&prom, "connected", &baseline).await;
    drop(conn);
}

// ============================================================================
// Scenario 8 (R-55/R-56/R-57): end-to-end trace continuity — STUB
// ============================================================================

/// R-55/R-56/R-57 trace-continuity stub — authoritative coverage is at the
/// component tier:
///
/// - `crates/mc-service/tests/otel_grpc_inbound_continuity.rs` — inbound gRPC
///   `traceparent` → `server_interceptor` → handler span reparent.
/// - `crates/mc-service/tests/otel_grpc_outbound_integration.rs` — GcClient +
///   MhClient `client_interceptor` injects the active span's `traceparent`.
/// - `crates/mc-service/tests/otel_webtransport_integration.rs` — WebTransport
///   `ClientMessage.trace_parent` → connection span reparent.
///
/// We cannot assert trace continuity from env-tests because the Kind
/// OTel-collector runs at `verbosity: normal` and does NOT log per-span
/// trace-ids; forcing `detailed` to grep spans out of collector logs would
/// break the PII-minimization control (untrusted browser-supplied `tracestate`
/// would land in collector stdout). Asserting continuity would instead require
/// a queryable trace backend (Tempo/Jaeger), which is out of scope for R-55.
/// Tracked in `docs/TODO.md` (trace-backend breadcrumb).
#[tokio::test]
#[ignore = "covered at component tier — see crates/mc-service/tests/otel_grpc_inbound_continuity.rs, otel_grpc_outbound_integration.rs, otel_webtransport_integration.rs"]
async fn test_mc_trace_continuity_end_to_end() {
    // Intentionally unimplemented. See doc-comment above.
}

// ============================================================================
// Scenario 9: MC programs a LIVE handler and the handler confirms the applied
// generation (ADR-0036 §8)
// ============================================================================

/// The per-instance `PromQL` for `mc_media_policy_pushes_total` — the ONE
/// string source shared by the baseline read, the stabilize loop and the delta
/// assert, mirroring [`notification_promql`], so those three cannot disagree on
/// the query.
///
/// `outcome_selector` is spliced in as a full label matcher (e.g.
/// `outcome="match"` or `outcome!~"match|handler_id_mismatch"`) rather than a
/// bare value, because the positive and negative halves of this test need
/// different matcher OPERATORS over one metric.
fn policy_push_promql(outcome_selector: &str) -> String {
    format!(r#"sum by (instance) (mc_media_policy_pushes_total{{{outcome_selector}}})"#)
}

/// Read a PER-INSTANCE snapshot of `mc_media_policy_pushes_total` for an
/// outcome selector. Same `instance`-grouping rationale and same fail-loud-on-
/// query-error behaviour as [`notification_promql`]-based reads.
async fn policy_push_counter(prom: &PrometheusClient, outcome_selector: &str) -> InstanceCounters {
    prom.instance_counter_map(&policy_push_promql(outcome_selector))
        .await
}

/// Wait for the per-instance `mc_media_policy_pushes_total{outcome="match"}`
/// snapshot to stabilize, in the same two-reads-a-scrape-apart shape as
/// [`wait_for_notification_counter_stable`].
///
/// Needed for the same reason: a sibling test in this binary drives a real join,
/// every real join programs a handler, and a `match` still in flight to
/// Prometheus when this test snapshots its baseline would make the delta assert
/// pass on the predecessor's increment. The `#[serial]` group makes the
/// predecessor *finished*, not *scraped*.
///
/// # The deferred extraction happened, because its own trigger fired
///
/// This was a documented structural clone of `wait_for_notification_counter_stable`,
/// deliberately not extracted on the ground that a helper shaped around one call
/// site would create a shared home its sibling did not use — which reads as done
/// and stops the next reader looking. The recorded trigger was "the next touch of
/// either helper, **a third stability-wait**, or a change to the Prometheus
/// scrape SLA". Story task 25 added the third stability-wait (the media-forward
/// path's baseline-freshness precondition), so the loop is now
/// [`poll_until_stable`] in `env_tests::fixtures::metrics` and **all three**
/// callers use it — which is exactly the condition the deferral was waiting on.
///
/// # The 16s/90s here are the SAME decision as the notification path's, not a
/// coincidence
///
/// This wait and [`wait_for_notification_counter_stable`] are one decision at
/// two sites — both wait for an in-flight increment to become visible before a
/// baseline read — so the rationale is stated ONCE, there, and this site
/// references it. They carry a **shared** drift obligation: a change to the
/// Prometheus scrape SLA invalidates both 16s values together. Do not restate
/// the derivation here in different words; two separate wordings would
/// manufacture the appearance of two independent decisions and license exactly
/// the divergence the pre-extraction clone comment existed to prevent.
///
/// Still no shared constant: `poll_until_stable` takes settle and budget as
/// required parameters with no default, so the values stay visible at the sites
/// that own them.
/// Returns the STABILISED `outcome="match"` snapshot for the caller to baseline
/// on, for the same reason as [`wait_for_notification_counter_stable`].
async fn wait_for_policy_push_counter_stable(prom: &PrometheusClient) -> InstanceCounters {
    poll_until_stable(
        prom,
        &policy_push_promql(r#"outcome="match""#),
        Duration::from_secs(16),
        Duration::from_secs(90),
        |v1, v2| {
            format!(
                "mc_media_policy_pushes_total{{outcome=\"match\"}} per-instance snapshot did \
                 not stabilize within 90s (last reads: v1={}, v2={})",
                format_instance_map(v1),
                format_instance_map(v2),
            )
        },
    )
    .await
}

/// Test: a real join against the Kind cluster makes MC compute the meeting's
/// forwarding assignment and program the assigned **live** MH, and the handler
/// echoes back the exact `policy_generation` MC sent.
///
/// **This is the positive-composition counterpart to MH's negative unit gates**
/// (`crates/mh-service` proves it *rejects* a bad policy; this proves the two
/// services *agree* on a good one). Nothing below stubs either end: MC computes
/// the assignment, the real `RegisterMeeting` RPC carries it, and a real
/// `mh-service` process applies it and answers.
///
/// # Why the verdict is the counter and not the gauge
///
/// `outcome="match"` is reachable **only** when the handler echoed an
/// `applied_generation` equal to what MC sent, with the transport mode agreeing
/// — so a delta >= 1 on that series cannot be produced by an unprogrammed
/// handler, which makes it non-vacuous. `mc_media_generation_divergence` is
/// deliberately NOT asserted here: a gauge that has materialised no series makes
/// absent-versus-zero ambiguous, and an instant read can be masked by an
/// intervening scrape — a vacuous pass or a flake. Its assertion lives in
/// `crates/mc-service/tests/media_policy_push_integration.rs`, where
/// `MetricAssertion` gives deterministic absent-versus-zero.
///
/// # Why the negative guard uses the negated increase-predicate
///
/// `!any_instance_exceeds_baseline(..)` rather than `instance_maps_equal(..)`.
/// A stale old-pod non-`match` series **expiring** between the baseline and
/// current reads makes the maps unequal and would produce a rollover-induced
/// false FAIL; the negated increase-predicate tolerates disappearing series by
/// construction, and an absent series correctly reads as an empty map, i.e. 0.
///
/// `handler_id_mismatch` is NOT excluded from the negative guard here, unlike in
/// the post-deploy checklist and the alert rule: this test drives a *fresh* join
/// against a handler MC has just been assigned, so the ids agree and the outcome
/// must not occur.
///
/// **Why they agree here, when OPS-17 says `handler_id` is per-incarnation.**
/// Both are true and they are not in tension. The id MC compares against is
/// captured into its Redis snapshot at assignment time, from the same MH
/// incarnation that answers this push — no restart intervenes inside a single
/// controlled join, so expected == echoed by construction. OPS-17's staleness
/// bites only *post-restart*, when a frozen snapshot is compared against a new
/// incarnation's fresh id. Fresh-join-agrees and post-restart-diverges are
/// different situations. **Do not "align" this guard with the operational
/// `outcome!~"match|handler_id_mismatch"` expression** — that exclusion exists
/// for the rollout window, which this test does not exercise, and adopting it
/// here would make the test tolerate an outcome that would be a genuine fault
/// (an MH crashloop mid-test). If it does, something is genuinely wrong with this path — the
/// rollout-window reasoning that justifies excluding it operationally does not
/// apply to a single controlled join.
///
/// On-call: if this fails with `no_applied_generation`, MH received the policy
/// and installed nothing — check MH's `mh_media_policy_applies_total{outcome}`
/// and the `mh.session.policy` WARN log. If it fails with
/// `transport_mode_mismatch`, MC and MH are version-skewed on
/// `dark_tower.signaling.v1.TransportMode`.
#[tokio::test]
#[serial_test::serial(mh_notifications)]
async fn test_mc_programs_live_handler_with_confirmed_forwarding_policy() {
    let cluster = cluster().await;
    let prom = PrometheusClient::new(&cluster.prometheus_base_url);

    let auth_client = AuthClient::new(&cluster.ac_base_url);
    let (user_token, display_name) = register_test_user(&auth_client, "MC Policy Push User").await;

    let match_baseline = wait_for_policy_push_counter_stable(&prom).await;
    let non_match_selector = r#"outcome!="match""#;
    let non_match_baseline = policy_push_counter(&prom, non_match_selector).await;

    // A real join: GC creates and assigns, MC admits the participant and — as
    // the first participant — computes the assignment and programs every
    // assigned MH. The MH WebTransport connect is not needed for the push, but
    // driving it keeps this test on the same shape as its siblings and proves
    // the meeting is genuinely usable.
    let (jwt, mh_url) = join_with_registered_mh(
        cluster,
        &user_token,
        &display_name,
        "MC Policy Push Test Meeting",
    )
    .await;
    let conn = connect_wt(&mh_url).await;
    let (_send, _recv) = send_jwt_on_bi_stream(&conn, &jwt).await;

    // Positive: the handler confirmed the generation MC sent.
    // 60s budget — the chain is MC's spawned RegisterMeeting task → gRPC to MH
    // → MH apply → MC's confirm → counter → Prometheus scrape (15s SLA).
    poll_until_any_instance_above(
        &prom,
        &policy_push_promql(r#"outcome="match""#),
        &match_baseline,
        Duration::from_secs(60),
        Duration::from_secs(2),
        |current| {
            format!(
                "mc_media_policy_pushes_total{{outcome=\"match\"}} did not increase past \
                 baseline on any instance within 60s — MC never confirmed that the live handler \
                 applied the generation it sent (baseline: {}, last observed: {}). Check the \
                 non-match series and MC's mc.grpc.mh_client error log for the outcome.",
                format_instance_map(&match_baseline),
                format_instance_map(current),
            )
        },
    )
    .await;

    // Negative guard: no non-`match` outcome moved. Read AFTER the positive so
    // the same push is covered by both halves.
    let non_match_current = policy_push_counter(&prom, non_match_selector).await;
    assert!(
        !any_instance_exceeds_baseline(&non_match_baseline, &non_match_current),
        "a non-match policy-push outcome incremented during a controlled single join \
         (baseline: {}, observed: {}) — the push was classified as something other than \
         `match`; read MC's mc.grpc.mh_client error line for which outcome and both generations",
        format_instance_map(&non_match_baseline),
        format_instance_map(&non_match_current),
    );
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

// ============================================================================
// Scenario C (R-15 / R-18): MH forwards a client's own audio back to it
// ============================================================================

/// A frame-v2 audio datagram, encoded through the production codec.
///
/// **Encoded through `media_protocol::codec::encode_frame`, deliberately.** A
/// hand-rolled builder here would be a fifth home for the frame layout, and an
/// env-test asserting against a frame it built from its own understanding of
/// the wire format is a test that can agree with itself while disagreeing with
/// production. `media-protocol` is a wire-format crate, the exact analogue of
/// `proto-gen` which this suite already links — the suite still links zero
/// service crates, which is the premise of its black-box validation.
///
/// `stream_id` and `hop_sequence` carry publisher-side values, so a relay that
/// failed to rewrite them fails the assertion rather than passing by accident.
/// The payload is opaque bytes and the signature a fixed pattern: MH is keyless
/// and verifies nothing, so all that matters is that both survive the relay
/// byte-identically.
fn audio_datagram(stream_sequence: u32, marker: u8) -> bytes::Bytes {
    use media_protocol::codec::{encode_frame, MediaFrameParts};
    use media_protocol::frame::{FrameFlags, SIGNATURE_SIZE};

    let payload = vec![marker; 160];
    let mut signature = [0_u8; SIGNATURE_SIZE];
    for (index, byte) in signature.iter_mut().enumerate() {
        *byte = u8::try_from(index % 251).unwrap_or(0);
    }
    encode_frame(&MediaFrameParts {
        flags: FrameFlags {
            independently_decodable: true,
            discardable: false,
            key_bearing: false,
        },
        stream_sequence,
        wrapped_transmit_key: None,
        extensions: &[],
        stream_id: 0xFFFF,
        hop_sequence: 0xDEAD_BEEF,
        payload: &payload,
        signature: &signature,
    })
    .expect("fixture frame must encode")
}

/// `sum by (instance)` over MH's policy-apply counter for one outcome.
fn policy_apply_promql(outcome: &str) -> String {
    format!(r#"sum by (instance) (mh_media_policy_applies_total{{outcome="{outcome}"}})"#)
}

/// Follow MC's steering: declare a receive capability and read back the handler
/// MC DIRECTS this client to send to (ADR-0036 §5).
///
/// # Why a declaration is needed at all
///
/// MC emits the send directive from `compose_and_emit`, which runs in response
/// to a `ReceiveCapability` — **not** at join. A client that joins and never
/// declares is never told to send, and the connection stays healthy in every
/// other respect (`docs/TODO.md` §Media Path Obligations records this as the
/// coupling it is). So the fixture declares one audio slot, exactly as the real
/// SDK does before publishing.
///
/// # The expected value is MC's OWN OUTPUT, never recomputed here
///
/// There is deliberately no copy of `edge_handler`'s `shared.sort();
/// shared.first()` in this file, and no fallback to `media_servers.first()`. A
/// fixture that re-implements the placement rule asserts its own copy of the
/// rule rather than what MC actually sent, and stays green through exactly the
/// divergence it exists to catch.
///
/// Returns the directed handler url. Every degenerate shape panics with its own
/// message rather than degrading:
/// - **no directive / no stream** — MC declined to direct this client;
/// - **zero targets** — a specified §5 success ("send nothing"), but not a state
///   this test can proceed from;
/// - **more than one target** — §9 multi-handler send landed, and this fixture
///   must be revisited rather than silently `.first()`-ed.
async fn follow_mc_steering(session: &mut McSession) -> String {
    use proto_gen::dark_tower::signaling::v1::{
        client_message, server_message, ClientMessage, MediaKind, ReceiveCapability, ReceiveSlot,
        SlotState,
    };

    session
        .write(&ClientMessage {
            message: Some(client_message::Message::ReceiveCapability(
                ReceiveCapability {
                    slots: vec![ReceiveSlot {
                        slot_id: 0,
                        media_kind: MediaKind::Audio as i32,
                        pinned_sender_id: None,
                    }],
                },
            )),
            trace_parent: String::new(),
            trace_state: String::new(),
        })
        .await;

    // MC may interleave roster broadcasts, so read until both media messages
    // have arrived rather than assuming the next two frames are ours.
    let mut directive = None;
    let mut assignments = None;
    for _ in 0..8 {
        if directive.is_some() && assignments.is_some() {
            break;
        }
        match session.read().await.message {
            Some(server_message::Message::SendDirective(d)) => directive = Some(d),
            Some(server_message::Message::StreamAssignments(a)) => assignments = Some(a),
            Some(server_message::Message::Error(e)) => panic!(
                "MC rejected the receive capability: code={} message={}",
                e.code, e.message
            ),
            // Dropped silently, and deliberately without logging even the
            // variant name: roster broadcasts legitimately interleave here, and
            // a `ParticipantJoined` payload carries a display name. Nothing to
            // report means nothing to leak.
            _ => {}
        }
    }

    let directive = directive.expect(
        "MC must answer a ReceiveCapability with a SendDirective (ADR-0036 §5); its absence \
         means this client was never told to send, which is silent on the client side",
    );
    let assignments = assignments
        .expect("MC must answer a ReceiveCapability with StreamAssignments (ADR-0036 §6)");

    assert_eq!(
        directive.streams.len(),
        1,
        "expected exactly one publisher stream (main audio) in the send directive"
    );
    let targets = &directive.streams[0].targets;
    assert_eq!(
        targets.len(),
        1,
        "one client, one directed handler is story-1 scope. Zero targets is a specified §5 \
         success (\"send nothing\") that this test cannot proceed from; more than one means \
         ADR-0036 §9 \
         multi-handler send landed and this fixture must be revisited, NOT silently .first()-ed"
    );
    let steered = targets[0].media_handler_url.clone();
    assert!(
        !steered.is_empty(),
        "MC must never emit an empty send-target url; it fails closed on an unresolved handler"
    );

    // Cross-check MC's two client-facing sides against each other on the LIVE
    // cluster. Both are read out of one MeetingAssignment, so a disagreement
    // here means they grew separate answers to "which handler".
    let active: Vec<&str> = assignments
        .assignments
        .iter()
        .filter(|a| a.slot_state == SlotState::Active as i32)
        .map(|a| a.media_handler_url.as_str())
        .collect();
    assert_eq!(
        active,
        vec![steered.as_str()],
        "the send directive and the active stream assignment must name the SAME handler; they are \
         derived from one forwarding assignment and cannot legitimately disagree"
    );

    steered
}

/// `sum by (instance)` over MH's WebTransport connection counter, with **no
/// `status` selector on purpose**.
///
/// The instance that ACCEPTED a connection increments this regardless of what
/// happens downstream (`crates/mh-service/src/webtransport/server.rs` records
/// `accepted` before JWT validation and before the media-session decision), so
/// the identity probe stays valid even when a sender binding is declined. That
/// keeps "I could not identify the steered instance" separable from "the client
/// was refused" — two failures with different owners.
fn mh_connection_promql() -> String {
    "sum by (instance) (mh_webtransport_connections_total)".to_string()
}

/// R-15: a client's uplink audio datagram comes back to it, rewritten only in
/// the relay region, purely from MC-pushed policy.
///
/// # This is R-15's composition proof, and it is NOT the injection proof
///
/// Read what this test can and cannot fail on before treating its green as
/// coverage. It is a **single-participant loopback**, which is the one
/// configuration in which a wrong binding is invisible: with exactly one sender
/// in the meeting, "bind the ordinal MC actually named", "bind the only ordinal
/// in the pushed policy" and "bind a hardcoded 1" all produce byte-identical
/// output. The `stream_id` assertion below is degenerate for the same reason —
/// the subscriber's own receive slot coincides with MC's `MAIN_AUDIO_SLOT_ID`,
/// so `0` is what a correct relay AND several incorrect ones write.
///
/// What this test uniquely proves is **composition**: that the real MC join
/// programs the real MH, that MC's `NotifyParticipantConnectedResponse` carries
/// an ordinal MH accepts, and that the real QUIC datagram path carries a real v2
/// frame back rewritten only in its relay region. That is R-15, and no
/// component-tier test can supply it.
///
/// **What proves the binding is the RIGHT one** is at component tier, where two
/// participants and two meetings can be driven:
/// `crates/mh-service/tests/media_session_binding_integration.rs` —
/// `two_participants_in_one_meeting_each_bind_their_own_ordinal` (per-participant
/// correspondence, which mere distinctness would not catch) and
/// `one_participant_id_in_two_meetings_binds_per_meeting_ordinals` (the
/// cross-tenant arm). Both were run against deliberately wrong bindings and
/// observed to fail before being believed. Forward-path mechanics —
/// relay-region-only rewrite, hop sequencing, the fail-closed trio — remain in
/// `crates/mh-service/tests/media_forward_integration.rs`.
///
/// # Why this was `#[ignore]`d for three sessions, and what was actually wrong
///
/// **Nothing in MH.** The attribute came off at story task 26 after a
/// live-cluster diagnosis found that **there was no MH datagram receive-path
/// defect at all**: `crates/mh-service/` is byte-identical between task 24's
/// commit and task 25's (`git diff cc56d7fc..d090c753 -- crates/mh-service/` is
/// empty), and this test passed against the cluster **unmodified**, three runs
/// of three. Task 25's client steering is what unblocked it.
///
/// The reading that funded task 26 — `mh_media_frames_forwarded_total` at 0 and
/// every `mh_media_frames_dropped_total{reason}` series at 0, therefore
/// "datagrams are not reaching the routing lookup at all" — was taken over a
/// pod lifetime in which this test was `#[ignore]`d, and **this test is the
/// only thing in the tree that sends a media datagram.** Every other scenario in
/// this file opens a WebTransport connection, starts a media session and sends
/// nothing, so `mh_media_session_starts_total{outcome="started"}` climbing
/// beside flat frame series is what a HEALTHY suite looks like. The inference
/// required that a datagram had been sent; none had.
///
/// Measured directly rather than argued: 40 datagrams sent to a handler holding
/// no edge for the sender produce `forwarded{direction="ingress"}` +40 **and**
/// `dropped{reason="no_subscriber"}` +40. The no-route drop the escalation
/// reported missing was never missing.
///
/// # What the diagnosis DID find, and why task 26 was not a no-op
///
/// A real hole, one layer above where anyone was looking. Datagrams arriving on
/// a connection before its ingress loop exists are evicted inside quinn's
/// datagram receive buffer with only a `debug!` and no counter anywhere in MH —
/// 60 datagrams sent on one connection yielded 47 counted and 13 that vanished.
/// An operator could not distinguish "the client sent nothing" from "MH threw it
/// away", which is precisely the distinction three consecutive sessions failed
/// to make. `mh_media_frames_dropped_total{reason="transport_receive_dropped"}`
/// and `{reason="no_media_session"}` are what close it; their component-tier
/// firing paths are in
/// `crates/mh-service/tests/media_session_binding_integration.rs`.
///
/// # Ordering is gated on a metric delta, never a sleep — and on the RIGHT pod
///
/// A datagram racing MC's `RegisterMeeting` is dropped as `no_policy` and the
/// test flakes. The gate is a `mh_media_policy_applies_total{outcome="applied"}`
/// delta: MH increments it only from the live snapshot after the apply, so the
/// delta means the forward path reflects the generation MC sent.
///
/// The gate keys that delta on **the instance this client was steered to**. The
/// previous form used `poll_until_any_instance_above`, which is satisfied by
/// mh-0 applying a policy for a client connected to mh-1 — the gate FIRED
/// correctly and never APPLIED, which is why the mismatch presented as a silent
/// 15s timeout rather than a race, and why it took three sessions to find.
///
/// There is no shared key between the handler url (an MH NodePort advertise
/// address) and the Prometheus `instance` label (pod IP:port), and a
/// port-to-ordinal table would re-encode `infra/services/mh-service/mh-*-configmap.yaml`
/// and fail OPEN. So the instance is **observed, not computed**: the test
/// connects, then asks which instance's connection counter rose. See the three
/// gate steps in the body for what each one proves and how it fails.
#[tokio::test]
#[serial_test::serial(mh_notifications)]
async fn test_mh_forwards_an_audio_datagram_back_to_its_sender() {
    let cluster = cluster().await;
    let prom = PrometheusClient::new(&cluster.prometheus_base_url);
    let auth_client = AuthClient::new(&cluster.ac_base_url);
    let (user_token, display_name) = register_test_user(&auth_client, "MH Forward Path User").await;

    // BOTH baselines are stabilised, and stabilised TOGETHER, before the join.
    //
    // `applied_baseline` needs it for the same reason `connect_baseline` does, on
    // the counter the gate actually decides with: `instances_exceeding_baseline`
    // treats an instance absent from the baseline as 0.0, so a steered instance
    // that was merely unscraped here and reappears carrying its full prior value
    // would satisfy STEP 3 immediately with nothing having been applied. That is
    // a false PASS — silent — and it would hollow out STEP 3's "after baseline"
    // claim rather than merely weakening it.
    //
    // Concurrently, and before the join, because nothing between here and the
    // client's connect opens an MH WebTransport connection — the MC->MH control
    // plane is gRPC — so neither baseline needs to be read late, and the pair
    // costs one settle instead of two.
    //
    // ---- STEP 1 of the three-step ordering gate: make both baselines COMPLETE
    // before reading them. (Steps 2 and 3 are below, after the client connects.)
    //
    // `instances_exceeding_baseline` treats an instance absent from the baseline
    // as baseline 0.0, which is unsound for a pre-existing pod that was merely
    // unscraped and then reappears carrying its full prior value: it shows as a
    // rise nothing earned. That is reachable SINGLE-THREADED, so no amount of
    // serialisation closes it.
    //
    // What two consecutive equal reads a scrape apart deliver — stated at the
    // strength the loop actually has, NOT at "every target has been scraped":
    // every target Prometheus is CURRENTLY REPORTING has settled, so "absent
    // from baseline" means genuinely absent **for anything scraped within the
    // lookback window**, and a later rise within that set is attributable to
    // this test's own action. A target outside that set is slice (d) of the
    // hazard enumeration below and is NOT covered; see `poll_until_stable`'s doc
    // for why the window cannot be widened to reach it. Do not restate the
    // residual here — point at it.
    //
    // Bound to `let` rather than inlined: `tokio::join!` holds its arguments as
    // futures across an await, so a `&format!(...)` temporary inside the macro
    // does not live long enough.
    let applied_promql = policy_apply_promql("applied");
    let connection_promql = mh_connection_promql();
    let (applied_baseline, connect_baseline) = tokio::join!(
        poll_until_stable(
            &prom,
            &applied_promql,
            Duration::from_secs(16),
            Duration::from_secs(90),
            |v1, v2| {
                format!(
                    "mh_media_policy_applies_total{{outcome=\"applied\"}} per-instance snapshot \
                     did not stabilize within 90s, so STEP 3 could not attribute an apply to this \
                     test's join. ENVIRONMENT signal (policy-push churn from outside this test \
                     binary, or a Prometheus scrape problem) — NOT an MC steering defect. Last \
                     reads: v1={}, v2={}",
                    format_instance_map(v1),
                    format_instance_map(v2),
                )
            },
        ),
        poll_until_stable(
            &prom,
            &connection_promql,
            // Correctness precondition, not a budget: one Prometheus scrape interval
            // (15s SLA, infra/kubernetes/observability/prometheus-config.yaml) plus
            // margin. At or below one interval both reads come from the SAME scrape,
            // are trivially identical, and this proves nothing — vacuous rather than
            // merely fast.
            Duration::from_secs(16),
            // 90s is ~5 rounds at a 16s settle, and the rounds are what it is
            // budgeting. A non-equal round is ROUTINE, not interference: the
            // stale-scrape case this exists to catch is absent from read one and
            // present in read two BY CONSTRUCTION, so the success path consumes one
            // round; a second absorbs a single concurrent transient; beyond that,
            // non-convergence is an environment signal rather than a slow answer and
            // more budget buys nothing.
            //
            // **This is NOT `wait_for_notification_counter_stable`'s 90s.** That one
            // budgets a causal chain (`tokio::spawn(notify)` -> gRPC -> MC counter ->
            // scrape); this budgets N rounds of a fixed-cost comparison. Same number,
            // different reasons, opposite drift obligations: if the scrape SLA moved,
            // theirs would grow by the chain's scrape term while this would grow by
            // five times the settle. Neither follows the other, and neither may be
            // hoisted onto the other. (Contrast the 16s values, which DO share one
            // trigger — see `wait_for_notification_counter_stable`.)
            Duration::from_secs(90),
            |v1, v2| {
                format!(
                    "mh_webtransport_connections_total per-instance snapshot did not stabilize \
                 within 90s, so a later rise could not be attributed to this test's own \
                 connection. This is an ENVIRONMENT signal (connection churn from outside this \
                 test binary, or a Prometheus scrape problem) — NOT an MC steering defect, and \
                 NOT the same failure as 'could not identify the steered instance'. Last reads: \
                 v1={}, v2={}",
                    format_instance_map(v1),
                    format_instance_map(v2),
                )
            },
        ),
    );

    // The real MC join: MC admits the participant, allocates the sender id,
    // computes the loopback assignment with the general N=1 algorithm and
    // programs every assigned MH. Deliberately NOT a hand-built
    // `RegisterMeeting`: env-tests hold no MH gRPC client and no MC->MH
    // credential, and `mh_test_utils::media_policy::egress` hardcodes
    // `priority_group: 0` while MC emits the assigned value — so a
    // fixture-built env-test would assert a shape MC cannot produce.
    let gc_join = gc_create_and_join(cluster, &user_token, "MH Forward Path Meeting").await;
    let mc_url = gc_join
        .mc_assignment
        .webtransport_endpoint
        .clone()
        .expect("MC assignment must include webtransport_endpoint");
    let mut session = mc_join_session(
        &mc_url,
        &gc_join.meeting_id.to_string(),
        &gc_join.token,
        &display_name,
    )
    .await;
    let sender_id = session
        .join_response
        .sender_id
        .expect("MC must allocate a sender_id for the joiner (ADR-0036 §2/§4)");

    // Follow MC's DIRECTIVE, not `media_servers.first()`. `media_servers` stays
    // in scope only as the bootstrap set MC offered; the handler this client
    // sends to is the one MC's forwarding assignment placed its edges on.
    assert!(
        !session.join_response.media_servers.is_empty(),
        "MC JoinResponse must include the meeting's bootstrap handler set"
    );
    let mh_url = follow_mc_steering(&mut session).await;

    // ------------------------------------------------------------------
    // The ordering gate, in three steps with three distinct reason tokens.
    //
    // Steps 1 and 2 rest on DISJOINT controls over ONE hazard, and no control
    // closes it alone. The slices, stated exhaustively so the enumeration cannot
    // be mistaken for coverage:
    //   (a) in-suite concurrency — closed by this test's `#[serial]` key (the
    //       three MH-connecting tests above carry it for exactly this reason);
    //   (b) a transient empty result in the baseline read — closed by step 1;
    //   (c) a connector from OUTSIDE this test binary — closed by neither;
    //   (d) an MH target unscraped beyond Prometheus's `query.lookback-delta`
    //       (5m default, unset in `prometheus-config.yaml`) — closed by NEITHER,
    //       and step 1 structurally cannot close it: such a target is absent
    //       from BOTH stabilise reads, so the maps compare equal and step 1
    //       returns having never observed it. If it is then re-scraped during
    //       step 2 while our own increment has not yet landed, it presents as
    //       exactly one unearned riser and step 2 names the WRONG instance —
    //       the ambiguity panic does not fire, because there is no ambiguity to
    //       see. See `poll_until_stable`'s doc for why the window cannot be
    //       widened to cover it, and for the `up{job=...}` check that would.
    // They are not belt-and-braces; do not remove any of them as redundant, and
    // do not read this list as closed.
    //
    // This orchestration RUNS as of story task 26 — the `#[ignore]` is gone and
    // Layer 7 invokes it in the default set. It was written unexercised, so its
    // first real exercise was that task's DoD run; the reason as much decision
    // logic as possible still lives in `env_tests::fixtures::metrics`
    // (`instances_exceeding_baseline` and friends) is that those predicates are
    // unit-tested, while this body can only be exercised against a live
    // cluster.
    // ------------------------------------------------------------------

    // STEP 1 was performed ABOVE, before the MC join — see the `tokio::join!`
    // block there for what it establishes and what it does not.
    //
    // Connect to the handler MC DIRECTED us to. This is the action STEP 2 below
    // attributes; the two are split only because the connection must exist
    // before any instance can be observed serving it.
    let conn = connect_wt(&mh_url).await;
    let (_send, _recv) = send_jwt_on_bi_stream(&conn, &gc_join.token).await;

    // STEP 2 — identify the steered instance by OBSERVATION, and fail closed on
    // ambiguity. This is also the positive control: it cannot pass unless this
    // test's own connection was seen somewhere, so a wholesale-empty Prometheus
    // reading is structurally distinguishable from "present but not incremented".
    //
    // Never `.first()` of the risen set: silently picking one would rebuild the
    // very defect this task removed, one layer up.
    let steered_instance = poll_until_exactly_one_instance_rose(
        &prom,
        &mh_connection_promql(),
        &connect_baseline,
        // 3x the 15s `scrape_interval`
        // (infra/kubernetes/observability/prometheus-config.yaml), so a single
        // slow or missed scrape does not spend the budget and panic with a
        // message that reads as a steering defect.
        Duration::from_secs(45),
        Duration::from_secs(2),
        |current| {
            format!(
                "could not identify the steered MH instance: no instance's \
                 mh_webtransport_connections_total rose above its own baseline within 45s of \
                 connecting to {mh_url}. Prometheus scrapes mh-service every 15s \
                 (infra/kubernetes/observability/prometheus-config.yaml), so check scrape lag and \
                 target health before suspecting MC steering. This is NOT the policy-apply gate \
                 below. Baseline: {}, last observed: {}",
                format_instance_map(&connect_baseline),
                format_instance_map(current),
            )
        },
        |risen, current| {
            format!(
                "could not identify the steered MH instance: {} instances rose above their own \
                 baseline after connecting to {mh_url}, so a rise cannot be attributed to this \
                 test's connection. The likely cause is another actor connecting to MH within \
                 the scrape window — test ISOLATION, not budget: this test's #[serial] key \
                 covers only this binary. Do NOT widen the budget and do NOT pick one of the \
                 risen set. Risen: {:?}, baseline: {}, last observed: {}",
                risen.len(),
                risen,
                format_instance_map(&connect_baseline),
                format_instance_map(current),
            )
        },
    )
    .await;

    // STEP 3 — the policy-apply delta, on THAT instance.
    //
    // What this proves: SOME policy application succeeded on the instance this
    // client is connected to, after baseline. What it does NOT prove: that it
    // was THIS meeting's policy — `mh_media_policy_applies_total` carries no
    // meeting identifier, and correctly so (ADR-0036 §11 forbids one on any
    // metric, flatly). That gap is a deliberate consequence of the telemetry
    // rule, not an oversight to be "fixed" with a label. Instance-keying removes
    // one confound (the wrong pod), not both.
    poll_until_instance_above(
        &prom,
        &policy_apply_promql("applied"),
        &applied_baseline,
        &steered_instance,
        Duration::from_secs(60),
        Duration::from_secs(2),
        |current| {
            format!(
                "the STEERED instance {steered_instance} (serving {mh_url}) did not increase \
                 mh_media_policy_applies_total{{outcome=\"applied\"}} past its OWN baseline \
                 within 60s. MC never applied a policy on the handler it directed this client to, \
                 so any datagram sent now would be correctly dropped as `no_policy`. This is a \
                 steering / policy-push question, DISTINCT from the two \
                 instance-identification failures above. Baseline: {}, last observed: {}",
                format_instance_map(&applied_baseline),
                format_instance_map(current),
            )
        },
    )
    .await;

    // QUIC datagrams are unreliable, so delivery is a rig precondition driven
    // by repetition, not an assertion about the transport. The ASSERTION is
    // about what comes back.
    let returned = {
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        loop {
            let _ = conn.send_datagram(audio_datagram(1, 0x5A));
            if let Ok(Ok(datagram)) =
                tokio::time::timeout(Duration::from_millis(250), conn.receive_datagram()).await
            {
                break datagram.payload();
            }
            assert!(
                std::time::Instant::now() < deadline,
                "MH returned no datagram within 15s. The forwarding policy applied, so check \
                 MH's mh_media_frames_dropped_total{{reason}} series: `no_subscriber` means \
                 MC's assignment carries no edge for this sender, `no_local_subscriber` means \
                 the subscriber is not connected to this handler, and a flat drop series with \
                 a flat forwarded series means the forward path never started."
            );
        }
    };

    // The relay region — and only the relay region — is rewritten. `stream_id`
    // must be the SUBSCRIBER's slot, not the publisher-side sentinel the
    // fixture sent.
    let view = media_protocol::codec::decode_datagram(&returned)
        .expect("the returned datagram must be a well-formed frame-v2 datagram");
    // `assert_eq!`, not `assert_ne!` against the sentinel: "was rewritten off
    // 0xFFFF" also passes for a rewrite to the WRONG slot. The loopback
    // assignment gives the subscriber its own main-audio slot, which MC's
    // `MAIN_AUDIO_SLOT_ID` fixes at 0. That const is MC-internal and not
    // importable from a black-box env-test, so the literal plus this comment is
    // the strongest available form.
    assert_eq!(
        view.stream_id(),
        0,
        "MH must rewrite the relay region's stream id to the subscriber's own main-audio slot \
         (MC's MAIN_AUDIO_SLOT_ID = 0); a different value means the relay rewrote to the wrong \
         slot, and the publisher-side sentinel 0xFFFF means it did not rewrite at all"
    );
    assert_ne!(
        view.hop_sequence(),
        0xDEAD_BEEF,
        "MH must write its OWN downlink hop sequence; the publisher's uplink value survived"
    );

    let original = audio_datagram(1, 0x5A);
    let sent =
        media_protocol::codec::decode_datagram(&original).expect("the fixture frame must decode");
    assert_eq!(
        view.publisher_region(),
        sent.publisher_region(),
        "the publisher region is signed end to end: a relay that alters one byte of it silences \
         the sender at every receiver, and MH — being keyless — counts nothing for it"
    );
    assert_eq!(
        view.payload(),
        sent.payload(),
        "the payload must be opaque to MH"
    );
    assert_eq!(
        view.signature(),
        sent.signature(),
        "the signature must be untouched"
    );

    // No tokens, JWTs or key material in any assertion message above; the
    // sender id is used only to prove MC allocated one.
    assert!(sender_id > 0, "sender_id 0 is never valid (ADR-0036 §2)");
}
