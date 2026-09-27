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
//! 7. (Retired at story 2 task 6.) The single-participant loopback
//!    forward test is superseded: loopback is removed (R-3), so a lone sender
//!    now gets nothing back by design. The R-15 composition proof — real MC
//!    join programs real MH and a real v2 frame crosses real QUIC rewritten
//!    only in its relay region — moved to
//!    `27_mc_slot_placement.rs::test_multi_party_slot_placement_across_two_handlers`,
//!    where two DIFFERENT senders make the binding assertion discriminating.
//! 9. `test_mc_programs_live_handler_with_confirmed_forwarding_policy` —
//!    `mc_media_policy_pushes_total{outcome="match"}` delta >= 1 after a real
//!    join, with the four non-`match` outcome series flat. The positive
//!    composition counterpart to MH's negative unit gates (ADR-0036 §8).
//! 10. `test_server_muted_sender_is_dropped_at_mh_ingress_and_survives_reassert`
//!     — S4 (story 2 R-9): server mute dropped at MH ingress under
//!     `server_muted`, source-only, surviving a re-assert, lifted in one
//!     generation.
//! 11. `test_two_concurrent_senders_into_one_receiver_are_never_misattributed`
//!     — S5 (R-5): every frame from two concurrent senders lands on the slot
//!     the receiver's own assignments name for its sender.
//! 12. `test_edge_churn_at_a_new_generation_does_not_stall_unrelated_egress`
//!     — runtime edge churn (R-4) does not stall an unrelated flow.
//!
//! S4 and the churn test program MH DIRECTLY through the Layer-7 per-pod gRPC
//! forwards (`env_tests::fixtures::mh_grpc`), because MC sends no server mute
//! before story 2 task 12 — see `Injection` for the footprint and recovery.
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
//! `with_no_cert_validation()` via the shared `fixtures::mc_session::connect_wt`:
//! the dev CA cert is not committed to the repo.
//!
//! # Wire format
//!
//! MH expects the FIRST framed message on a bidi stream to be a typed
//! `MhClientMessage{ConnectRequest{join_token: <JWT>}}` protobuf envelope,
//! 4-byte big-endian length prefix + encoded bytes. Mirrors MC's
//! `ClientMessage{JoinRequest{...}}` discipline. Source of truth:
//! `crates/mh-service/src/webtransport/connection.rs` Step 3 region.

#![cfg(feature = "flows")]

use env_tests::cluster::ClusterConnection;
use env_tests::fixtures::auth_client::UserRegistrationRequest;
use env_tests::fixtures::gc_client::{CreateMeetingRequest, GcClient, JoinMeetingResponse};
use env_tests::fixtures::mc_session::{self, connect_wt, McSession};
use env_tests::fixtures::media::{
    assert_relayed_on_slot, bind_until_received, bind_until_received_each,
};
use env_tests::fixtures::metrics::poll_until_pinned_instance;
use env_tests::fixtures::metrics::{
    any_instance_exceeds_baseline, format_instance_map, poll_until_any_instance_above,
    poll_until_stable, service_job_scrape_settle, InstanceCounters,
};
use env_tests::fixtures::mh_grpc::{self, Handler, ReleaseOnDrop};
use env_tests::fixtures::participant::{self, Participant};
use env_tests::fixtures::{AuthClient, PrometheusClient};
use proto_gen::dark_tower::internal::v1::media_handler_service_client::MediaHandlerServiceClient;
use proto_gen::dark_tower::internal::v1::{
    CandidateSource, EgressStream, EndMeetingRequest, MutedSource, RegisterMeetingRequest,
    SubscriberSlot,
};
use proto_gen::dark_tower::signaling::v1::TransportMode;
use std::time::{Duration, Instant};
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
) -> (wtransport::SendStream, wtransport::RecvStream) {
    // The open-bi + MH connect-envelope framing lives in the shared fixture
    // (`mc_session::mh_open_connect`); this wrapper survives only to carry the
    // held-open warning above, which is specific to how THIS suite reads the
    // returned recv stream.
    mc_session::mh_open_connect(conn, jwt).await
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
/// # Settle — `service_job_scrape_settle`, STATED HERE FOR BOTH event-visibility waits
///
/// One scrape interval of the job that produces these series plus margin, so
/// any outstanding scrape lands between the two reads. Both waits in this
/// binary read MC counters (`mc-service` job), so both use
/// `SERVICE_JOB_SCRAPE_SETTLE`, obtained through
/// [`service_job_scrape_settle`], which fails loudly if the LIVE per-job
/// `scrape_interval` (`infra/kubernetes/observability/prometheus.yml`) no
/// longer sits below it. This is **not a local decision**: it is the same
/// decision `wait_for_policy_push_counter_stable` makes, which references this
/// paragraph rather than restating it.
///
/// The *derivations* stay separate, because why each site waits genuinely
/// differs: both waits here wait for an in-flight increment to become visible
/// before a baseline read. One VALUE, several reasons — and no default: the
/// value is named at each call site, never adopted by omission.
///
/// Budget 90s — a FAILURE-ONLY ceiling, deliberately not scaled down with the
/// scrape interval: the chain that has to settle is MH's fire-and-forget
/// `tokio::spawn(notify)` -> gRPC RPC -> MC counter increment -> Prometheus
/// scrape, and under cluster load it converges in several non-equal rounds of
/// the settle plus a load allowance. A faster scrape buys more convergence
/// rounds inside the same 90s; shrinking the budget would only convert load
/// variance into failures, and costs nothing on a green run.
/// Returns the STABILISED snapshot, which callers use as their baseline. Reading
/// the counter again afterwards would baseline on a *different*, newer read than
/// the one just proved stable — a small window, but the whole point of the wait
/// is that the baseline is the settled value.
async fn wait_for_notification_counter_stable(
    prom: &PrometheusClient,
    event_type: &str,
) -> InstanceCounters {
    let settle = service_job_scrape_settle(prom).await;
    poll_until_stable(
        prom,
        &notification_promql(event_type),
        settle,
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
/// cannot drift in budget/ordering/message-shape. Budget: 60s, a failure-only
/// ceiling for the MH spawn-task + gRPC + MC handler + scrape chain (the scrape
/// is one `mc-service` job interval; the rest is cluster-load allowance).
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
    // Connection, framing, the `JoinRequest` field list, and the join
    // positive-control (sender_id / media_servers / handler-url present) all
    // live in the shared `mc_session` fixture. The session is dropped when this
    // returns, which is what the `JoinResponse`-only callers want.
    let mut session = McSession::connect(mc_url).await;
    mc_session::mc_join(
        &mut session,
        meeting_id,
        meeting_token,
        participant_name,
        participant_name,
    )
    .await
}

/// Pick ANY assigned handler, taken in ARBITRARY Redis enumeration order — NOT
/// the steered one. `media_servers` is connection bootstrap data and is
/// deliberately unsorted, so its head carries no relationship to placement.
///
/// Legitimate for this suite's callers: they need "some handler on which this
/// meeting is registered", and MC pushes `RegisterMeeting` to EVERY assigned
/// handler, so both answer the question identically. The two answers may
/// nonetheless *appear* to agree on any given run — the steered handler is
/// always the lexicographically smallest assigned `mh_id`, so an arbitrary pick
/// coincides with it some of the time, by accident and never because they are
/// the same question.
///
/// A caller that needs the handler MC DIRECTED a client to send on must read the
/// send directive instead (see `27_mc_slot_placement.rs`). Both callers in this
/// binary share this ONE home so that distinction cannot drift between them.
fn any_registered_mh_url(
    join_response: &proto_gen::dark_tower::signaling::v1::JoinResponse,
) -> String {
    join_response
        .media_servers
        .first()
        .map(|m| m.media_handler_url.clone())
        .filter(|u| !u.is_empty())
        .expect("MC JoinResponse must include at least one non-empty MH URL")
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

    let mh_url = any_registered_mh_url(&join_response);

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
// WebTransport connection to MH, and the notification-counter tests in this
// binary attribute a per-instance counter delta to their OWN connection. A
// concurrent MH connection would put an unearned increment in their window.
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
// This `#[serial]` key is LOAD-BEARING, not tidiness — but NOT for the reason
// the valid-JWT test above gives. Investigated at story 2 task 10: a rejected
// JWT produces NO MH -> MC notification. Evidence from the code path, not an
// observed zero: the oversized envelope is refused in `read_framed_message`
// (Step 3) and a forged token at `validate_meeting_token` (Step 4) of
// `crates/mh-service/src/webtransport/connection.rs::handle_connection`, both
// returning before Step 5; every `notify_participant_*` call site sits at Step 5
// or later. What keeps this test serialized is its GC -> MC JOIN
// (`join_with_registered_mh`): MC then pushes `RegisterMeeting`, incrementing
// `mc_media_policy_pushes_total{outcome="match"}` — the counter
// `test_mc_programs_live_handler_with_confirmed_forwarding_policy` attributes
// to its OWN join. Run concurrently, this test would hand that one an unearned
// rise (a false pass).
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
    // `fake_token` in `24_join_flow.rs::test_mc_rejects_invalid_meeting_token`
    // — exercises MH's signature verification path.
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
// This `#[serial]` key is LOAD-BEARING, not tidiness — but NOT for the reason
// the valid-JWT test above gives. Investigated at story 2 task 10: a rejected
// JWT produces NO MH -> MC notification. Evidence from the code path, not an
// observed zero: the oversized envelope is refused in `read_framed_message`
// (Step 3) and a forged token at `validate_meeting_token` (Step 4) of
// `crates/mh-service/src/webtransport/connection.rs::handle_connection`, both
// returning before Step 5; every `notify_participant_*` call site sits at Step 5
// or later. What keeps this test serialized is its GC -> MC JOIN
// (`join_with_registered_mh`): MC then pushes `RegisterMeeting`, incrementing
// `mc_media_policy_pushes_total{outcome="match"}` — the counter
// `test_mc_programs_live_handler_with_confirmed_forwarding_policy` attributes
// to its OWN join. Run concurrently, this test would hand that one an unearned
// rise (a false pass).
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
    // Prometheus scrape (the `mc-service` job's `scrape_interval`). 60s budget
    // absorbs cluster-load variance.
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
/// actor record → counter → Prometheus scrape (the `mc-service` job's
/// `scrape_interval`).
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
    use proto_gen::dark_tower::signaling::v1::{
        client_message, ClientMessage, ConnectionState, MediaConnectionUpdate, MhConnectionStatus,
    };

    let cluster = cluster().await;
    let prom = PrometheusClient::new(&cluster.prometheus_base_url);
    let auth_client = AuthClient::new(&cluster.ac_base_url);
    let (user_token, display_name) = register_test_user(&auth_client, "MC MediaUpdate User").await;

    let baseline = mc_participant_mh_status_counter(&prom, "connected").await;

    // Real GC→MC join, but keep the session OPEN so we can send a follow-up
    // MediaConnectionUpdate on it (the `mc_join` wrapper drops its session).
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

    let mut session = McSession::connect(&mc_url).await;
    let join_response = mc_session::mc_join(
        &mut session,
        &gc_join.meeting_id.to_string(),
        &gc_join.token,
        &display_name,
        &display_name,
    )
    .await;

    let mh_url = any_registered_mh_url(&join_response);

    // Report that MH as CONNECTED via the post-join plane.
    session
        .write(&ClientMessage {
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
        })
        .await;

    // Keep the session alive long enough for MC's bridge loop to read the frame
    // before we let it drop at end of scope.
    assert_participant_mh_status_increases_past(&prom, "connected", &baseline).await;
    drop(session);
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
/// [`poll_until_stable`] in `env_tests::fixtures::metrics` and **every**
/// caller uses it — which is exactly the condition the deferral was waiting on.
///
/// # The settle and 90s budget here are the SAME decision as the notification
/// path's, not a coincidence
///
/// This wait and [`wait_for_notification_counter_stable`] are one decision at
/// two sites — both wait for an in-flight MC-counter increment to become
/// visible before a baseline read — so the rationale is stated ONCE, there, and
/// this site references it. Do not restate the derivation here in different
/// words; two separate wordings would manufacture the appearance of two
/// independent decisions and license exactly the divergence the pre-extraction
/// clone comment existed to prevent.
///
/// The settle is the shared, explicitly-named `SERVICE_JOB_SCRAPE_SETTLE`
/// (via [`service_job_scrape_settle`]) — a value both sites NAME, never a
/// default `poll_until_stable` supplies.
/// Returns the STABILISED `outcome="match"` snapshot for the caller to baseline
/// on, for the same reason as [`wait_for_notification_counter_stable`].
async fn wait_for_policy_push_counter_stable(prom: &PrometheusClient) -> InstanceCounters {
    let settle = service_job_scrape_settle(prom).await;
    poll_until_stable(
        prom,
        &policy_push_promql(r#"outcome="match""#),
        settle,
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
    // → MH apply → MC's confirm → counter → Prometheus scrape (the
    // `mc-service` job's `scrape_interval`).
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
// Story 2 task 10: server mute at MH ingress (S4), two-sender attribution (S5)
// and runtime edge churn — the multi-party forwarding proof on the live cluster
// (R-4, R-5, R-9, R-33; ADR-0036 §7, §9).
//
// All three open MH sessions for a real GC -> MC meeting, so they share the
// `mh_notifications` serial key: each produces MH -> MC notifications that the
// notification tests above attribute to their own connections.
// ============================================================================

/// One marker per participant role, in the datagram payload (MH is keyless:
/// the payload survives byte-identically, so the marker names the sender).
const MARK_A: u8 = 0xA4;
const MARK_B: u8 = 0xB4;
const MARK_C: u8 = 0xC4;

/// Margin over MC's published connect-settle window before a view must have
/// converged (MH's connect notification, the push, the flush). Same role as
/// `27_mc_slot_placement.rs`'s margin.
const VIEW_MARGIN: Duration = Duration::from_secs(10);

/// Rig bound on one send-until-observed phase. A precondition for unreliable
/// QUIC datagrams, never a performance assertion.
const PHASE_DEADLINE: Duration = Duration::from_secs(30);

/// Bound on a pinned-pod counter becoming visible in Prometheus.
const PINNED_METRIC_BOUND: Duration = Duration::from_secs(90);

/// How far above MC's installed generation the injected policy sits. Small on
/// purpose (@security, @paired-meeting-controller): it only has to stay above
/// MC's own generation, which cannot move during the injection window because
/// nothing structural changes in the meeting. If MC ever out-ran it, MC's
/// next push would install its own (empty) muted set, forwarding would resume,
/// and the mute assertions would go RED — the safe failure direction.
const INJECTION_MARGIN: u64 = 16;

/// Sequence-number floors separating frames by phase, so a frame still queued
/// from an earlier phase can never be read as evidence about a later one.
const SEQ_PRE: u32 = 1;
const SEQ_MUTED: u32 = 1_000_000;
const SEQ_REASSERTED: u32 = 2_000_000;
const SEQ_UNMUTED: u32 = 3_000_000;
const SEQ_CHURN_ADD: u32 = 4_000_000;
const SEQ_CHURN_REMOVE: u32 = 5_000_000;

fn server_muted_promql() -> &'static str {
    r#"sum by (instance) (mh_media_frames_dropped_total{reason="server_muted"})"#
}

/// A marked datagram's `(marker, stream_sequence, relay slot)`.
fn read_frame(raw: &[u8]) -> (u8, u32, u32) {
    let view = media_protocol::codec::decode_datagram(raw)
        .unwrap_or_else(|_| panic!("received a malformed datagram"));
    (
        view.payload().first().copied().unwrap_or(0),
        view.stream_sequence(),
        u32::from(view.stream_id()),
    )
}

/// One MC-shaped egress stream (story 2 task 20 edge model): `source` into
/// `subscriber`'s declared slot at `ordinal`, with MC's id packing
/// `(subscriber << 8) | ordinal`, one candidate at stream 0, priority group 1,
/// datagram transport.
fn mc_shaped_edge(subscriber: u32, ordinal: u32, slot: u32, source: u32) -> EgressStream {
    EgressStream {
        egress_stream_id: (subscriber << 8) | ordinal,
        subscriber: Some(SubscriberSlot {
            sender_id: subscriber,
            slot_id: slot,
        }),
        candidate_sources: vec![CandidateSource {
            sender_id: source,
            stream_number: 0,
        }],
        priority_group: 1,
        supersede_on_independent_frame: false,
        transport_mode: TransportMode::Datagram as i32,
    }
}

/// A test-injected policy on ONE MH pod for a meeting the REAL MC also
/// programs, presenting MC's own `mc_id` and gRPC endpoint so MH's
/// notifications keep reaching MC and no ownership takeover occurs.
///
/// # This deliberately risks an MC-side footprint — do not triage it as MC's bug
///
/// While an injected policy is live, MH's installed generation is ABOVE MC's.
/// MC's floor adoption is closed after its first confirmed push
/// (`crates/mc-service/src/grpc/mh_client.rs`, the comment on the higher-echo
/// case), so if MC pushed for this meeting mid-window it would record
/// `mc_media_policy_pushes_total{outcome="generation_mismatch"}`, write
/// `mc_media_generation_divergence`, and fire `MCMediaGenerationDivergence`
/// (`for: 0m`). The tests keep the window free of structural changes (no
/// joins, leaves or declarations), so MC has nothing to push; and every
/// injection ends with `EndMeeting` on the pod BEFORE any session closes.
///
/// MC re-programs a pod after that `EndMeeting` ONLY when that pod's MC
/// snapshot CHANGES: MC's per-handler worker does not advance the generation
/// for an unchanged snapshot, and a publish at its last confirmed generation
/// sends no RPC (`crates/mc-service/src/media_routing/pusher.rs`). The
/// carrying pod's snapshot does change as the sessions close (edges removed),
/// so it gets a fresh install. A pod whose MC snapshot is unchanged (empty, in
/// these meeting shapes) stays UNREGISTERED for the rest of this test-owned
/// meeting — which is why the meeting is dedicated and disposable. Do not
/// generalise "EndMeeting is recovered by MC's next push" to production: until
/// story 4's re-assert cadence exists, it is false for an unchanged snapshot.
///
/// # If cleanup is skipped
///
/// `ReleaseOnDrop` releases on success and on a panic, but not on SIGKILL or a
/// harness kill. Then this meeting id stays wedged at the injected generation
/// on that pod — every later MC push for it refused as stale — holding its
/// edges and a registered-meeting slot until the pod restarts. The id is a
/// fresh GC meeting per run, so no later run reuses it. Recovery: `EndMeeting`
/// for the id on the pod through the Layer-7 gRPC forward with the MC
/// credential (`env_tests::fixtures::mh_grpc`), or
/// `kubectl rollout restart deployment/mh-0 deployment/mh-1 -n dark-tower`.
struct Injection<'a> {
    handler: &'a Handler,
    client: MediaHandlerServiceClient<tonic::transport::Channel>,
    token: &'a str,
    meeting_id: String,
    mc_id: String,
    mc_grpc_endpoint: String,
    /// The generation the probe in [`Injection::open`] read back on this pod.
    mc_generation: u64,
}

impl<'a> Injection<'a> {
    /// Connect, record for cleanup BEFORE any call, and read MC's installed
    /// generation with a generation-1 EMPTY registration used as a probe.
    ///
    /// On a pod MC has already programmed past generation 1, the probe is
    /// refused as stale without a swap and echoes MC's live generation. But MC
    /// pushes to each handler on its own async worker, and nothing orders this
    /// probe after MC's push to a pod that carries NO edges: there the probe
    /// can INSTALL (empty, generation 1) first. That is harmless ONLY because
    /// this meeting's MC snapshot for such a pod is itself empty at generation
    /// 1 — MC's own push then no-ops as equal and truthfully confirms `match`.
    /// **Do not reuse this helper for a meeting shape where BOTH pods carry
    /// edges**: a probe that won the race would swallow MC's first real push
    /// (equal generation, no swap) while MC recorded `match` for content that
    /// was never installed. The real precondition — that MC HAS programmed the
    /// carrying pod past its pre-edge render — is
    /// [`assert_mc_programmed_a_carrying_pod`], checked across both pods.
    async fn open(
        handler: &'a Handler,
        token: &'a str,
        cleanup: &ReleaseOnDrop,
        owner: &Participant,
    ) -> Injection<'a> {
        let meeting_id = owner.gc.meeting_id.to_string();
        let mc_id = owner.gc.mc_assignment.mc_id.clone();
        let mc_grpc_endpoint = owner.gc.mc_assignment.grpc_endpoint.clone();
        cleanup.record(&handler.grpc_url, &meeting_id, &mc_id);
        let mut injection = Injection {
            handler,
            client: mh_grpc::connect(handler).await,
            token,
            meeting_id,
            mc_id,
            mc_grpc_endpoint,
            mc_generation: 0,
        };
        injection.mc_generation = injection.register(1, Vec::new(), &[]).await;
        injection
    }

    fn generation(&self, step: u64) -> u64 {
        self.mc_generation + INJECTION_MARGIN + step
    }

    async fn register(
        &mut self,
        generation: u64,
        streams: Vec<EgressStream>,
        muted: &[u32],
    ) -> u64 {
        let request = RegisterMeetingRequest {
            meeting_id: self.meeting_id.clone(),
            mc_id: self.mc_id.clone(),
            mc_grpc_endpoint: self.mc_grpc_endpoint.clone(),
            egress_streams: streams,
            selection_rules: None,
            policy_generation: generation,
            server_muted_sources: muted
                .iter()
                .map(|sender_id| MutedSource {
                    sender_id: *sender_id,
                })
                .collect(),
        };
        self.client
            .register_meeting(mh_grpc::authed(self.token, request))
            .await
            .unwrap_or_else(|s| {
                panic!(
                    "INJECT on {}: RegisterMeeting at generation {generation} refused ({:?}): {}",
                    self.handler.name,
                    s.code(),
                    s.message()
                )
            })
            .into_inner()
            .applied_generation
    }

    /// Install at `generation` and require the synchronous echo to match.
    async fn install(&mut self, generation: u64, streams: Vec<EgressStream>, muted: &[u32]) {
        let applied = self.register(generation, streams, muted).await;
        assert_eq!(
            applied, generation,
            "INJECT on {}: MH did not apply generation {generation} (echoed {applied})",
            self.handler.name
        );
    }

    /// Release, then PROVE the injected generation is gone: a generation-1
    /// registration must now echo below it (1 if it installed, or MC's own
    /// fresh generation if MC re-programmed first). Our probe is released
    /// again only if it is what installed.
    async fn release_and_prove(mut self, cleanup: &ReleaseOnDrop) {
        let injected = self.generation(0);
        self.end().await;
        let echoed = self.register(1, Vec::new(), &[]).await;
        assert!(
            echoed < injected,
            "CLEANUP on {}: after EndMeeting the handler still reports generation {echoed} (>= \
             the injected {injected}) — the meeting is wedged; recover with EndMeeting or a pod \
             restart (see Injection's docs)",
            self.handler.name
        );
        if echoed == 1 {
            self.end().await;
        }
        cleanup.forget(&self.meeting_id);
    }

    async fn end(&mut self) {
        let done = self
            .client
            .end_meeting(mh_grpc::authed(
                self.token,
                EndMeetingRequest {
                    meeting_id: self.meeting_id.clone(),
                    mc_id: self.mc_id.clone(),
                },
            ))
            .await
            .unwrap_or_else(|s| {
                panic!(
                    "CLEANUP on {}: EndMeeting refused ({:?}): {}",
                    self.handler.name,
                    s.code(),
                    s.message()
                )
            })
            .into_inner();
        assert!(done.acknowledged, "CLEANUP on {}", self.handler.name);
    }
}

/// PRECONDITION: MC has programmed the pod that carries this meeting's edges.
/// Generation 1 is MC's pre-edge render, so once edges exist the carrying
/// pod's installed generation is at least 2 — a check the probe CAN fail,
/// unlike a per-pod `>= 1` (see [`Injection::open`]).
fn assert_mc_programmed_a_carrying_pod(injections: &[Injection<'_>]) {
    let seen: Vec<(String, u64)> = injections
        .iter()
        .map(|i| (i.handler.name.clone(), i.mc_generation))
        .collect();
    assert!(
        seen.iter().any(|(_, g)| *g >= 2),
        "PRECONDITION: no MH pod reports an MC-programmed generation past the pre-edge render \
         (probe echoes {seen:?}); MC has not installed this meeting's edges yet, so injecting \
         now would race MC's first real push"
    );
}

/// Every participant opens a session to every handler in `urls`, then declares
/// its slots; returns sessions indexed `[participant][url]`.
async fn open_sessions(
    phase: &str,
    ps: &[Participant],
    urls: &[&String],
) -> Vec<std::collections::HashMap<String, mc_session::MhSession>> {
    let mut all = Vec::new();
    for p in ps {
        let mut mine = std::collections::HashMap::new();
        for url in urls {
            mine.insert(
                (*url).clone(),
                participant::reach(phase, p.label, url, &p.token).await,
            );
        }
        all.push(mine);
    }
    all
}

/// Both MH pods Layer 7 exports. Every injection in this file goes to BOTH
/// with the same policy, so no test needs to know which pod serves which
/// WebTransport URL.
fn both_handlers() -> Vec<Handler> {
    let hs = mh_grpc::handlers();
    assert_eq!(hs.len(), 2, "PRECONDITION: Layer 7 exports two MH pods");
    hs
}

/// Drain whatever is queued on `conn` (non-blocking in effect: one short read
/// per call), handing every marked frame to `each`.
async fn drain(conn: &wtransport::Connection, mut each: impl FnMut(u8, u32, u32)) {
    while let Ok(Ok(d)) =
        tokio::time::timeout(Duration::from_millis(50), conn.receive_datagram()).await
    {
        let (marker, seq, slot) = read_frame(&d.payload());
        each(marker, seq, slot);
    }
}

/// S4 (story 2 R-9; ADR-0036 §7): a server-muted sender's datagrams are
/// dropped at MH ingress under `server_muted`; the mute is SOURCE-only (the
/// muted participant still hears its peer); it survives a re-assert at the
/// same and at a new generation; and an unmute restores forwarding within one
/// generation.
///
/// # Evidence, per the env-tests README evidence rule
///
/// - DROPS: `mh_media_frames_dropped_total{reason="server_muted"}` rises past
///   its OWN baseline on EACH pinned pod (A sends on both). No other producer
///   of that series exists before story 2 task 12 (MC sends an empty muted set
///   until then), so the rise is attributable to this test.
/// - NON-FORWARDING, per receiver: B never receives an A-marked frame whose
///   sequence number was sent while the mute was live. This replaces the
///   task text's "forward counter stays flat": a POD-WIDE forward counter
///   staying flat proves nothing while other suites share the pods.
/// - LIVENESS (positive control): A keeps receiving B's frames while muted.
///
/// MC sends no mute before story 2 task 12, so the mute is injected directly
/// (see [`Injection`]); "survives a re-assert" is therefore the TEST's own
/// re-registration, at the same generation and at a new one.
#[tokio::test]
#[serial_test::serial(mh_notifications)]
async fn test_server_muted_sender_is_dropped_at_mh_ingress_and_survives_reassert() {
    let cluster = cluster().await;
    let prom = PrometheusClient::new(&cluster.prometheus_base_url);
    let window = participant::settle_window(cluster).await;
    let auth = AuthClient::new(&cluster.ac_base_url);
    let gc = GcClient::new(&cluster.gc_base_url);
    let mut ps = participant::meeting_of(&auth, &gc, "Server Mute S4", &["A", "B"]).await;
    let (x, y) = participant::two_offered_handlers("S4", &ps);
    let (a, b) = (ps[0].sender_id(), ps[1].sender_id());
    // Declared slot ids from a range disjoint from the sender ids.
    let (a_slot, b_slot) = (7_u32, 513_u32);

    let sessions = open_sessions("S4", &ps, &[&x, &y]).await;
    ps[0].declare(&[a_slot]).await;
    ps[1].declare(&[b_slot]).await;
    let bound = window + VIEW_MARGIN;
    ps[0]
        .converge("S4", bound, |v, d| {
            v.is_some_and(|v| participant::filled_senders(v) == vec![b]) && d.is_some()
        })
        .await;
    ps[1]
        .converge("S4", bound, |v, d| {
            v.is_some_and(|v| participant::filled_senders(v) == vec![a]) && d.is_some()
        })
        .await;

    // PRE-INJECTION positive control: MC's own policy forwards A -> B.
    let b_edge_url = ps[1].slot_url(a);
    let pre = bind_until_received(
        "S4 PRE",
        sessions[1][&b_edge_url].connection(),
        &[(sessions[0][&b_edge_url].connection(), MARK_A)],
        &[],
    )
    .await;
    assert_relayed_on_slot("S4 PRE", &pre[&MARK_A], b_slot, MARK_A, "A");

    // INJECT on both pods: the same A <-> B edges MC would program, plus A muted.
    let handlers = both_handlers();
    let token = mh_grpc::mc_service_token(&cluster.ac_base_url).await;
    let cleanup = ReleaseOnDrop::new(token.clone());
    let edges = || {
        vec![
            mc_shaped_edge(b, 0, b_slot, a),
            mc_shaped_edge(a, 0, a_slot, b),
        ]
    };
    let mut injections = Vec::new();
    for handler in &handlers {
        injections.push(Injection::open(handler, &token, &cleanup, &ps[0]).await);
    }
    assert_mc_programmed_a_carrying_pod(&injections);
    let baselines = prom.instance_counter_map(server_muted_promql()).await;
    for injection in &mut injections {
        let g = injection.generation(0);
        injection.install(g, edges(), &[a]).await;
    }

    // MUTED: A sends on BOTH sessions (so both pods see muted ingress), B sends
    // too. A must keep hearing B; B must never hear a muted-phase A frame.
    let a_conns: Vec<_> = [&x, &y]
        .iter()
        .map(|u| sessions[0][*u].connection())
        .collect();
    let b_conns: Vec<_> = [&x, &y]
        .iter()
        .map(|u| sessions[1][*u].connection())
        .collect();
    let muted_phase = |floor: u32, phase: &'static str| {
        let a_conns = a_conns.clone();
        let b_conns = b_conns.clone();
        async move {
            let deadline = Instant::now() + PHASE_DEADLINE;
            let mut heard_b = false;
            let mut rounds = 0_u32;
            while !heard_b || rounds < 50 {
                assert!(
                    Instant::now() < deadline,
                    "{phase}: within {PHASE_DEADLINE:?} the muted participant A never heard B \
                     (the mute must be SOURCE-only) — check mh_media_frames_dropped_total{{reason}}"
                );
                rounds += 1;
                for conn in &a_conns {
                    conn.send_datagram(env_tests::fixtures::media::audio_datagram(
                        floor + rounds,
                        MARK_A,
                    ))
                    .unwrap_or_else(|e| panic!("{phase}: A could not send: {e}"));
                }
                for conn in &b_conns {
                    conn.send_datagram(env_tests::fixtures::media::audio_datagram(
                        floor + rounds,
                        MARK_B,
                    ))
                    .unwrap_or_else(|e| panic!("{phase}: B could not send: {e}"));
                }
                for conn in &b_conns {
                    drain(conn, |marker, seq, _| {
                        assert!(
                            !(marker == MARK_A && seq > floor),
                            "{phase}: B received A's frame {seq}, sent while A was SERVER-MUTED \
                             — the mute was bypassed"
                        );
                    })
                    .await;
                }
                for conn in &a_conns {
                    drain(conn, |marker, seq, _| {
                        if marker == MARK_B && seq > floor {
                            heard_b = true;
                        }
                    })
                    .await;
                }
            }
        }
    };
    muted_phase(SEQ_MUTED, "S4 MUTED").await;

    // DROPS counted on EACH pod, past its own baseline.
    let mut after_first = Vec::new();
    for handler in &handlers {
        let base = env_tests::fixtures::metrics::instance_for_pod_ip(&baselines, &handler.pod_ip)
            .map_or(0.0, |(_, v)| v);
        after_first.push(
            poll_until_pinned_instance(
                &prom,
                server_muted_promql(),
                &handler.pod_ip,
                PINNED_METRIC_BOUND,
                Duration::from_secs(2),
                "S4 DROPS",
                &format!("> {base} (this pod's own baseline)"),
                |v| v > base,
            )
            .await,
        );
    }

    // RE-ASSERT at the SAME generation (a no-op that must keep the mute), then
    // at a NEW generation still carrying it.
    for injection in &mut injections {
        let g = injection.generation(0);
        injection.install(g, edges(), &[a]).await;
    }
    for injection in &mut injections {
        let g = injection.generation(1);
        injection.install(g, edges(), &[a]).await;
    }
    muted_phase(SEQ_REASSERTED, "S4 REASSERTED").await;
    for (handler, first) in handlers.iter().zip(&after_first) {
        let first = *first;
        poll_until_pinned_instance(
            &prom,
            server_muted_promql(),
            &handler.pod_ip,
            PINNED_METRIC_BOUND,
            Duration::from_secs(2),
            "S4 REASSERT DROPS",
            &format!("> {first} (still dropping after the re-assert)"),
            |v| v > first,
        )
        .await;
    }

    // UNMUTE: the next generation without A. B hears A again, on B's slot.
    for injection in &mut injections {
        let g = injection.generation(2);
        injection.install(g, edges(), &[]).await;
    }
    let deadline = Instant::now() + PHASE_DEADLINE;
    let mut resumed = None;
    let mut seq = SEQ_UNMUTED;
    while resumed.is_none() {
        assert!(
            Instant::now() < deadline,
            "S4 UNMUTED: B did not hear A within {PHASE_DEADLINE:?} of the unmute generation"
        );
        seq += 1;
        for conn in &a_conns {
            conn.send_datagram(env_tests::fixtures::media::audio_datagram(seq, MARK_A))
                .unwrap_or_else(|e| panic!("S4 UNMUTED: A could not send: {e}"));
        }
        for conn in &b_conns {
            drain(conn, |marker, s, slot| {
                if marker == MARK_A && s > SEQ_UNMUTED {
                    resumed = Some(slot);
                }
            })
            .await;
        }
    }
    assert_eq!(
        resumed,
        Some(b_slot),
        "S4 UNMUTED: A's frames land on B's slot"
    );

    // CLEANUP before any session closes, with the release PROVED.
    for injection in injections {
        injection.release_and_prove(&cleanup).await;
    }
    drop(sessions);
}

/// S5 (story 2 R-5, R-30): two REAL senders transmitting CONCURRENTLY into one
/// receiver, and EVERY frame the receiver gets is relayed onto the slot the
/// receiver's OWN `StreamAssignments` names for that frame's sender — never
/// the other sender's. The binding loopback could not prove this (one sender);
/// `27_mc_slot_placement.rs` proves each ordered pair one sender at a time.
///
/// MC programs everything (no injection). All three participants connect to
/// ONE handler, so both of R's edges live on one transport and a mix-up would
/// be observable on it.
#[tokio::test]
#[serial_test::serial(mh_notifications)]
async fn test_two_concurrent_senders_into_one_receiver_are_never_misattributed() {
    let cluster = cluster().await;
    let window = participant::settle_window(cluster).await;
    let auth = AuthClient::new(&cluster.ac_base_url);
    let gc = GcClient::new(&cluster.gc_base_url);
    let mut ps = participant::meeting_of(&auth, &gc, "Two Sender S5", &["R", "A", "C"]).await;
    let (x, _y) = participant::two_offered_handlers("S5", &ps);
    let (a, c) = (ps[1].sender_id(), ps[2].sender_id());

    let sessions = open_sessions("S5", &ps, &[&x]).await;
    // Non-ordinal declared ids, one > 255 (u8-truncation would collide).
    ps[0].declare(&[513, 7]).await;
    ps[1].declare(&[300]).await;
    ps[2].declare(&[260]).await;
    let bound = window + VIEW_MARGIN;
    ps[0]
        .converge("S5", bound, |v, _| {
            v.is_some_and(|v| {
                let mut f = participant::filled_senders(v);
                f.sort_unstable();
                let mut want = vec![a, c];
                want.sort_unstable();
                f == want
            })
        })
        .await;
    let (slot_a, slot_c) = (ps[0].slot_for(a), ps[0].slot_for(c));
    assert_ne!(slot_a, slot_c, "S5 PRECONDITION: distinct slots");
    assert_eq!(ps[0].slot_url(a), x);
    assert_eq!(ps[0].slot_url(c), x);

    let expected = |marker: u8| if marker == MARK_A { slot_a } else { slot_c };
    let first = bind_until_received_each(
        "S5",
        sessions[0][&x].connection(),
        &[
            (sessions[1][&x].connection(), MARK_A),
            (sessions[2][&x].connection(), MARK_C),
        ],
        &[],
        10,
        |marker, raw| {
            let (_, seq, slot) = read_frame(raw);
            assert_eq!(
                slot,
                expected(marker),
                "S5: frame {seq} from sender {marker:#04x} was relayed onto slot {slot}, not the \
                 slot R's own StreamAssignments names for that sender — MIS-ATTRIBUTED"
            );
        },
    )
    .await;
    assert_relayed_on_slot("S5 A", &first[&MARK_A], slot_a, MARK_A, "A");
    assert_relayed_on_slot("S5 C", &first[&MARK_C], slot_c, MARK_C, "C");
    drop(sessions);
}

/// Runtime edge churn (R-4; ADR-0036 §8): edges added and removed at a new
/// generation do not stall an UNRELATED egress flow. Per-meeting routes swap
/// atomically (`ArcSwap`) and per-connection egress queues are untouched by a
/// swap, so A -> B must keep flowing across both swaps: after EACH synchronous
/// apply echo, B receives an A frame sent after that swap. The added edge
/// C -> B must start delivering on its own slot, and once removed, no C frame
/// sent after the removal may arrive.
///
/// Injected directly (see [`Injection`]) because only a direct registration
/// can change ONE edge while leaving every participant's connectivity — and
/// so MC's own view — unchanged. Everyone connects to ONE handler, so all of
/// MC's edges are there and the injection targets both pods identically.
#[tokio::test]
#[serial_test::serial(mh_notifications)]
async fn test_edge_churn_at_a_new_generation_does_not_stall_unrelated_egress() {
    let cluster = cluster().await;
    let window = participant::settle_window(cluster).await;
    let auth = AuthClient::new(&cluster.ac_base_url);
    let gc = GcClient::new(&cluster.gc_base_url);
    let mut ps = participant::meeting_of(&auth, &gc, "Edge Churn", &["A", "B", "C"]).await;
    let (x, _y) = participant::two_offered_handlers("CHURN", &ps);
    let (a, b, c) = (ps[0].sender_id(), ps[1].sender_id(), ps[2].sender_id());
    let (b_slot_0, b_slot_1) = (513_u32, 3_u32);

    let sessions = open_sessions("CHURN", &ps, &[&x]).await;
    ps[0].declare(&[7]).await;
    ps[1].declare(&[b_slot_0, b_slot_1]).await;
    ps[2].declare(&[260]).await;
    let bound = window + VIEW_MARGIN;
    ps[1]
        .converge("CHURN", bound, |v, _| {
            v.is_some_and(|v| participant::filled_senders(v).len() == 2)
        })
        .await;

    let handlers = both_handlers();
    let token = mh_grpc::mc_service_token(&cluster.ac_base_url).await;
    let cleanup = ReleaseOnDrop::new(token.clone());
    let mut injections = Vec::new();
    for handler in &handlers {
        injections.push(Injection::open(handler, &token, &cleanup, &ps[0]).await);
    }
    assert_mc_programmed_a_carrying_pod(&injections);
    let a_to_b = || mc_shaped_edge(b, 0, b_slot_0, a);
    let c_to_b = || mc_shaped_edge(b, 1, b_slot_1, c);
    for injection in &mut injections {
        let g = injection.generation(0);
        injection.install(g, vec![a_to_b()], &[]).await;
    }

    let a_conn = sessions[0][&x].connection();
    let b_conn = sessions[1][&x].connection();
    let c_conn = sessions[2][&x].connection();

    // Send A (and C) until B has an A frame past `floor` and, when `want_c`,
    // a C frame past `floor` on its own slot. Every A frame must be on slot 0;
    // when `!want_c`, no C frame past `floor` may arrive at all.
    let phase = |floor: u32, want_c: bool, name: &'static str| async move {
        let deadline = Instant::now() + PHASE_DEADLINE;
        let (mut got_a, mut got_c) = (false, !want_c);
        let mut seq = floor;
        let mut after_a = 0_u32;
        while !(got_a && got_c && after_a >= 20) {
            assert!(
                Instant::now() < deadline,
                "{name}: within {PHASE_DEADLINE:?} of the swap B did not receive the expected \
                 frames (A after swap: {got_a}, C: {got_c}) — an unrelated flow STALLED across \
                 a policy swap"
            );
            seq += 1;
            a_conn
                .send_datagram(env_tests::fixtures::media::audio_datagram(seq, MARK_A))
                .unwrap_or_else(|e| panic!("{name}: A could not send: {e}"));
            c_conn
                .send_datagram(env_tests::fixtures::media::audio_datagram(seq, MARK_C))
                .unwrap_or_else(|e| panic!("{name}: C could not send: {e}"));
            drain(b_conn, |marker, s, slot| {
                if s <= floor {
                    return;
                }
                if marker == MARK_A {
                    assert_eq!(slot, b_slot_0, "{name}: A's frames stay on B's slot 0");
                    got_a = true;
                }
                if marker == MARK_C {
                    assert!(
                        want_c,
                        "{name}: C frame {s} arrived after C -> B was REMOVED"
                    );
                    assert_eq!(slot, b_slot_1, "{name}: C's frames land on B's slot 1");
                    got_c = true;
                }
            })
            .await;
            if got_a {
                after_a += 1;
            }
        }
    };

    phase(SEQ_PRE, false, "CHURN BASE").await;
    for injection in &mut injections {
        let g = injection.generation(1);
        injection.install(g, vec![a_to_b(), c_to_b()], &[]).await;
    }
    phase(SEQ_CHURN_ADD, true, "CHURN ADD").await;
    for injection in &mut injections {
        let g = injection.generation(2);
        injection.install(g, vec![a_to_b()], &[]).await;
    }
    phase(SEQ_CHURN_REMOVE, false, "CHURN REMOVE").await;

    for injection in injections {
        injection.release_and_prove(&cleanup).await;
    }
    drop(sessions);
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
