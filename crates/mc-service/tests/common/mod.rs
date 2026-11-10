//! Shared test infrastructure for `mc-service` integration tests.
//!
//! - [`SeededHandoffs`] / [`MockMhRegistrationClient`]: the GC hand-offs a
//!   test created meetings with, and an in-memory MH registration mock, used
//!   by both the accept-loop rig and the legacy join-flow tests. Per @dry-reviewer, these live next to their consumers
//!   (in `tests/common/`) rather than in `mc-test-utils`, mirroring MH's
//!   `tests/common/mock_mc.rs` pattern.
//!
//! Submodules:
//! - [`accept_loop_rig`]: byte-identical wrapper over
//!   `WebTransportServer::bind() → accept_loop()` for component-tier metric
//!   tests (ADR-0032 Step 3 §Deliverable 3).

#![allow(dead_code)]
#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

pub mod accept_loop_rig;
pub mod media_session;
pub mod otel_capture;

use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ::common::jwt::JwksClient;
use ::common::secret::SecretBox;
use mc_service::actors::{ActorMetrics, ControllerMetrics, MeetingControllerActorHandle};
use mc_service::auth::McJwtValidator;
use mc_service::errors::McError;
use mc_service::grpc::{MeetingProgramming, MhRegistrationClient};
use mc_service::media_routing::MeetingHandlers;
use mc_service::media_routing::PolicyGenerations;
use mc_service::redis::MhEndpointInfo;
use mc_test_utils::jwt_test::{mount_jwks_mock, TestKeypair};
use tokio::sync::Notify;
use wiremock::MockServer;

// =============================================================================
// SeededHandoffs — the GC hand-off each test meeting was created with.
//
// NOT a store MC reads: a meeting's handler set is installed at actor creation
// (ADR-0039 §3) and no join reads anything. This is the test's own record, so
// helpers like `media_session::registered_handlers` can name what was seeded.
// =============================================================================

#[derive(Default)]
pub struct SeededHandoffs {
    data: Mutex<std::collections::HashMap<String, Vec<MhEndpointInfo>>>,
}

impl SeededHandoffs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&self, meeting_id: &str, handlers: Vec<MhEndpointInfo>) {
        self.data
            .lock()
            .expect("SeededHandoffs mutex poisoned")
            .insert(meeting_id.to_string(), handlers);
    }

    pub fn get(&self, meeting_id: &str) -> Option<Vec<MhEndpointInfo>> {
        self.data
            .lock()
            .expect("SeededHandoffs mutex poisoned")
            .get(meeting_id)
            .cloned()
    }
}

/// THE one way the suite builds a meeting's handler set: through the
/// production hand-off mapping (`handler_set_from_handoff`), never a copy.
///
/// # Panics
///
/// On an empty or duplicate list — a fixture defect.
pub fn handler_set(handlers: &[MhEndpointInfo]) -> MeetingHandlers {
    mc_service::grpc::mc_service::handler_set_from_handoff(handlers)
        .expect("fixture hand-off is non-empty and duplicate-free")
}

/// The single default handler (`mh-test-1`) as a handler set — the set
/// [`standalone_join_media`] tests create their meetings with.
pub fn default_handler_set() -> MeetingHandlers {
    handler_set(&[mh_handler("mh-test-1")])
}

// =============================================================================
// MockMhRegistrationClient — records `register_meeting` calls; configurable result.
// =============================================================================

#[derive(Debug, Clone)]
pub struct RegisterMeetingCall {
    pub mh_grpc_endpoint: String,
    pub expected_handler_id: String,
    pub meeting_id: String,
    pub mc_id: String,
    pub mc_grpc_endpoint: String,
    /// The generation MC derived for this push (ADR-0036 §8). Recorded so a
    /// test can assert MC never sends 0 and that an unchanged assignment
    /// re-asserts at the same number.
    pub policy_generation: u64,
    /// How many egress streams the pushed policy carried.
    pub egress_stream_count: usize,
    /// The pushed policy itself, for per-handler edge assertions.
    pub assignment: mc_service::media_routing::HandlerAssignment,
}

/// One MC→MH call, in ARRIVAL order across both RPCs — so "no
/// `RegisterMeeting` after the first `EndMeeting`" is asserted directly
/// rather than inferred from two independent counts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MhEvent {
    /// A `RegisterMeeting` push: (meeting, handler endpoint, generation).
    Register {
        meeting_id: String,
        mh_grpc_endpoint: String,
        policy_generation: u64,
    },
    /// An `EndMeeting` release.
    End(mc_service::media_routing::teardown::EndMeetingCall),
}

pub struct MockMhRegistrationClient {
    calls: Mutex<Vec<RegisterMeetingCall>>,
    /// Every call to either RPC, interleaved in arrival order.
    events: Mutex<Vec<MhEvent>>,
    /// If set, `register_meeting()` returns this result.
    result: Result<(), McError>,
    /// Notified once per `register_meeting()` call, after `calls` is updated.
    /// Lets tests await deterministic fan-out without polling sleeps.
    call_notify: Arc<Notify>,
    /// While `Some`, every `register_meeting()` is RECORDED on arrival and
    /// then held (not answered) until [`Self::release_pushes`]: an attempt
    /// genuinely IN FLIGHT, so a test can count what the push worker issues
    /// while one is outstanding (the client-mute churn bound).
    hold: Mutex<Option<Arc<tokio::sync::Semaphore>>>,
    /// Per-handler holds (keyed by `expected_handler_id`): a push to a held
    /// handler is recorded and then held until [`Self::release_handler`].
    handler_holds: Mutex<std::collections::HashMap<String, Arc<tokio::sync::Semaphore>>>,
    /// Handlers whose pushes FAIL (keyed by `expected_handler_id`).
    failing_handlers: Mutex<std::collections::HashSet<String>>,
}

impl MockMhRegistrationClient {
    pub fn new() -> Self {
        Self::with_result(Ok(()))
    }

    pub fn with_error(err: McError) -> Self {
        Self::with_result(Err(err))
    }

    fn with_result(result: Result<(), McError>) -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            events: Mutex::new(Vec::new()),
            result,
            call_notify: Arc::new(Notify::new()),
            hold: Mutex::new(None),
            handler_holds: Mutex::new(std::collections::HashMap::new()),
            failing_handlers: Mutex::new(std::collections::HashSet::new()),
        }
    }

    /// Hold every push to `handler_id` (recorded, not answered) until
    /// [`Self::release_handler`] — a per-handler delayed ack.
    pub fn hold_handler(&self, handler_id: &str) {
        self.handler_holds
            .lock()
            .expect("handler_holds mutex poisoned")
            .insert(
                handler_id.to_string(),
                Arc::new(tokio::sync::Semaphore::new(0)),
            );
    }

    /// Answer every held push to `handler_id` and stop holding it.
    pub fn release_handler(&self, handler_id: &str) {
        if let Some(gate) = self
            .handler_holds
            .lock()
            .expect("handler_holds mutex poisoned")
            .remove(handler_id)
        {
            gate.close();
        }
    }

    /// Make every push to `handler_id` fail (a handler that never acks).
    pub fn fail_handler(&self, handler_id: &str) {
        self.failing_handlers
            .lock()
            .expect("failing_handlers mutex poisoned")
            .insert(handler_id.to_string());
    }

    /// Hold every `register_meeting()` from now on (recorded, not answered)
    /// until [`Self::release_pushes`].
    pub fn hold_pushes(&self) {
        *self.hold.lock().expect("hold mutex poisoned") =
            Some(Arc::new(tokio::sync::Semaphore::new(0)));
    }

    /// Answer every held push and stop holding.
    pub fn release_pushes(&self) {
        if let Some(gate) = self.hold.lock().expect("hold mutex poisoned").take() {
            gate.close();
        }
    }

    /// Every call to either RPC, in arrival order.
    pub fn events(&self) -> Vec<MhEvent> {
        self.events
            .lock()
            .expect("MockMhRegistrationClient mutex poisoned")
            .clone()
    }

    /// The `EndMeeting` calls, in arrival order.
    pub fn end_calls(&self) -> Vec<mc_service::media_routing::teardown::EndMeetingCall> {
        self.events()
            .into_iter()
            .filter_map(|e| match e {
                MhEvent::End(call) => Some(call),
                MhEvent::Register { .. } => None,
            })
            .collect()
    }

    /// Wait until at least `expected` `EndMeeting` calls have landed.
    pub async fn wait_for_end_calls(
        &self,
        expected: usize,
        timeout: Duration,
    ) -> Vec<mc_service::media_routing::teardown::EndMeetingCall> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let ends = self.end_calls();
            if ends.len() >= expected {
                return ends;
            }
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                panic!(
                    "Timeout waiting for {expected} end_meeting() calls; got {} ({ends:?})",
                    ends.len()
                );
            }
            let _ = tokio::time::timeout(remaining, self.call_notify.notified()).await;
        }
    }

    pub fn calls(&self) -> Vec<RegisterMeetingCall> {
        self.calls
            .lock()
            .expect("MockMhRegistrationClient mutex poisoned")
            .clone()
    }

    /// Wait for at least `expected` calls to land within `timeout`.
    /// Returns the recorded calls on success; panics on timeout.
    ///
    /// `Notify` stores at most one permit, so under fast fan-out late
    /// notifications can coalesce. Correctness relies on the length re-check
    /// at the top of the loop, which observes the cumulative state.
    pub async fn wait_for_calls(
        &self,
        expected: usize,
        timeout: Duration,
    ) -> Vec<RegisterMeetingCall> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if self.calls().len() >= expected {
                return self.calls();
            }
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                let current = self.calls();
                panic!(
                    "Timeout waiting for {} register_meeting() calls; got {} ({:?})",
                    expected,
                    current.len(),
                    current,
                );
            }
            let _ = tokio::time::timeout(remaining, self.call_notify.notified()).await;
        }
    }
}

impl Default for MockMhRegistrationClient {
    fn default() -> Self {
        Self::new()
    }
}

impl MhRegistrationClient for MockMhRegistrationClient {
    fn register_meeting<'a>(
        &'a self,
        programming: &'a MeetingProgramming<'a>,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), McError>> + Send + 'a>> {
        self.calls
            .lock()
            .expect("MockMhRegistrationClient mutex poisoned")
            .push(RegisterMeetingCall {
                mh_grpc_endpoint: programming.mh_grpc_endpoint.to_string(),
                expected_handler_id: programming.expected_handler_id.to_string(),
                meeting_id: programming.meeting_id.to_string(),
                mc_id: programming.mc_id.to_string(),
                mc_grpc_endpoint: programming.mc_grpc_endpoint.to_string(),
                policy_generation: programming.policy_generation.get(),
                egress_stream_count: programming.assignment.egress_streams.len(),
                assignment: programming.assignment.clone(),
            });
        self.events
            .lock()
            .expect("MockMhRegistrationClient mutex poisoned")
            .push(MhEvent::Register {
                meeting_id: programming.meeting_id.to_string(),
                mh_grpc_endpoint: programming.mh_grpc_endpoint.to_string(),
                policy_generation: programming.policy_generation.get(),
            });
        self.call_notify.notify_one();
        let failing = self
            .failing_handlers
            .lock()
            .expect("failing_handlers mutex poisoned")
            .contains(programming.expected_handler_id);
        let result = match &self.result {
            Ok(()) if failing => Err(McError::Grpc("handler down (fixture)".to_string())),
            Ok(()) => Ok(()),
            Err(e) => Err(McError::Grpc(e.to_string())),
        };
        let gate = self.hold.lock().expect("hold mutex poisoned").clone();
        let handler_gate = self
            .handler_holds
            .lock()
            .expect("handler_holds mutex poisoned")
            .get(programming.expected_handler_id)
            .cloned();
        Box::pin(async move {
            for gate in [gate, handler_gate].into_iter().flatten() {
                // Closed by `release_pushes` / `release_handler`: `acquire`
                // then fails, which is the release signal; no permit is ever
                // granted while held.
                let _ = gate.acquire().await;
            }
            result
        })
    }

    fn end_meeting<'a>(
        &'a self,
        call: &'a mc_service::media_routing::teardown::EndMeetingCall,
    ) -> Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<(), mc_service::media_routing::teardown::EndMeetingFailure>,
                > + Send
                + 'a,
        >,
    > {
        self.events
            .lock()
            .expect("MockMhRegistrationClient mutex poisoned")
            .push(MhEvent::End(call.clone()));
        self.call_notify.notify_one();
        Box::pin(async { Ok(()) })
    }
}

// =============================================================================
// Shared bring-up — single source of truth for the MC component-test stack.
//
// `TestServer::start` (`tests/join_tests.rs`) and `start_rig`
// (`tests/webtransport_accept_loop_integration.rs`) previously each built the
// same wiremock-JWKS + JwtValidator + ActorMetrics + ControllerHandle + mock
// stores stack inline (~25 LoC duplicated). Per @dry-reviewer F-DRY-1, those
// bring-ups now both call `build_test_stack` and `seed_meeting_with_mh` and
// reduce to thin wrappers around `AcceptLoopRig::start_with`.
// =============================================================================

pub struct TestStackHandles {
    pub controller_handle: Arc<MeetingControllerActorHandle>,
    pub jwt_validator: Arc<McJwtValidator>,
    /// The GC hand-off each seeded meeting was created with.
    pub handoffs: Arc<SeededHandoffs>,
    pub mh_reg_client: Arc<MockMhRegistrationClient>,
    pub mock_server: MockServer,
    pub keypair: TestKeypair,
}

/// Builds the wiremock-JWKS + `McJwtValidator` + actor-handle + mock-stores
/// stack used by both the accept-loop rig and the legacy join-flow tests.
///
/// `keypair_label` is the only meaningful axis of variation across callers
/// (test logs cite the label on JWT-validation paths). Everything else is
/// fixed: 300s clock skew on the validator, zero-byte master secret,
/// fresh `PolicyGenerations`, mc_id `"mc-test"`.
pub async fn build_test_stack(keypair_label: &str) -> TestStackHandles {
    build_test_stack_with_notifier(
        keypair_label,
        Arc::new(mc_service::actors::controller::NoMeetingEndedNotifier),
    )
    .await
}

/// [`build_test_stack`], telling `meeting_ended` when a meeting has ended and
/// its handlers are released — the seam that makes MC's GC notification
/// testable without a GC.
pub async fn build_test_stack_with_notifier(
    keypair_label: &str,
    meeting_ended: Arc<dyn mc_service::actors::controller::MeetingEndedNotifier>,
) -> TestStackHandles {
    let mock_server = MockServer::start().await;
    let keypair = TestKeypair::new(42, keypair_label);
    let jwks_url = mount_jwks_mock(&mock_server, &keypair).await;

    let jwks_client = Arc::new(JwksClient::new(jwks_url).expect("JwksClient::new"));
    let jwt_validator = Arc::new(McJwtValidator::new(jwks_client, 300));

    let master_secret = SecretBox::new(Box::new(vec![0u8; 32]));
    let metrics = ActorMetrics::new();
    let controller_metrics = ControllerMetrics::new();
    let controller_handle = Arc::new(MeetingControllerActorHandle::with_meeting_ended_notifier(
        "mc-test".to_string(),
        metrics,
        controller_metrics,
        master_secret,
        Arc::new(PolicyGenerations::new()),
        mc_test_utils::kek::kek_lifecycle(),
        meeting_ended,
    ));

    let handoffs = Arc::new(SeededHandoffs::new());
    let mh_reg_client: Arc<MockMhRegistrationClient> = Arc::new(MockMhRegistrationClient::new());

    TestStackHandles {
        controller_handle,
        jwt_validator,
        handoffs,
        mh_reg_client,
        mock_server,
        keypair,
    }
}

/// Build a default `MhEndpointInfo` for an MH with the given id.
///
/// Endpoints follow the convention `wt://{mh_id}:4433` /
/// `http://{mh_id}:50053`. Tests that need a different shape can construct
/// `MhEndpointInfo` directly.
pub fn mh_handler(mh_id: &str) -> MhEndpointInfo {
    MhEndpointInfo {
        mh_id: mh_id.to_string(),
        webtransport_endpoint: format!("wt://{mh_id}:4433"),
        grpc_endpoint: format!("http://{mh_id}:50053"),
    }
}

/// Create `meeting_id` on the controller actor with `handlers` as GC's
/// hand-off (ADR-0039 §3: the set is installed at creation), and record it.
///
/// For single-handler scenarios prefer [`seed_meeting_with_mh`].
pub async fn seed_meeting_with_handlers(
    handles: &TestStackHandles,
    meeting_id: &str,
    handlers: Vec<MhEndpointInfo>,
) {
    let set = handler_set(&handlers);
    handles.handoffs.insert(meeting_id, handlers);
    handles
        .controller_handle
        .create_meeting(meeting_id.to_string(), set)
        .await
        .expect("create_meeting");
}

/// The default hand-off (`mh-test-1`) for a meeting the caller creates itself
/// through a different entry point (e.g. the `test-seams` constructor):
/// records it and returns the set to create the meeting with.
pub fn seed_default_handoff(handles: &TestStackHandles, meeting_id: &str) -> MeetingHandlers {
    let handlers = vec![mh_handler("mh-test-1")];
    let set = handler_set(&handlers);
    handles.handoffs.insert(meeting_id, handlers);
    set
}

/// Create `meeting_id` with a single default handler (`mh-test-1`).
///
/// Thin wrapper over [`seed_meeting_with_handlers`] for the common case.
pub async fn seed_meeting_with_mh(handles: &TestStackHandles, meeting_id: &str) {
    seed_meeting_with_handlers(handles, meeting_id, vec![mh_handler("mh-test-1")]).await;
}

/// Media-routing inputs for actor-level joins in the integration suite: the
/// stack's recording MH client. (The meeting's handler set was installed at
/// creation; a join carries none.)
pub fn test_join_media(handles: &TestStackHandles) -> mc_service::actors::JoinMedia {
    join_media_with(Arc::clone(&handles.mh_reg_client) as Arc<dyn MhRegistrationClient>)
}

/// [`test_join_media`] for tests that drive the actor without a
/// [`TestStackHandles`]: a fresh recording MH client (that acks every push).
/// Create the meeting with [`default_handler_set`].
pub fn standalone_join_media() -> mc_service::actors::JoinMedia {
    join_media_with(Arc::new(MockMhRegistrationClient::new()))
}

/// [`standalone_join_media`] with a caller-held MH client (per-handler holds
/// and failures).
pub fn join_media_on(mh_client: Arc<MockMhRegistrationClient>) -> mc_service::actors::JoinMedia {
    join_media_with(mh_client as Arc<dyn MhRegistrationClient>)
}

fn join_media_with(mh_client: Arc<dyn MhRegistrationClient>) -> mc_service::actors::JoinMedia {
    mc_service::actors::JoinMedia {
        deps: Arc::new(mc_service::actors::MediaRoutingDeps {
            mh_client,
            policy_generations: Arc::new(PolicyGenerations::new()),
            mc_id: "mc-test".to_string(),
            mc_grpc_endpoint: "http://mc-test:50052".to_string(),
            stream_policy: mc_service::media_signaling::MediaStreamPolicy::new(
                client_media_config().audio_encoding,
                client_media_config().video_encoding,
            ),
            connect_settle_window: client_media_config().connect_settle_window,
        }),
        // The actor-driven suite joins as an audio publisher with no
        // join-carried declaration (it declares post-join), story 2's shape.
        declared: mc_service::media_kind::ClientMediaStates::live(
            mc_service::media_signaling::PublishSet::of(&[
                mc_service::media_signaling::MediaKindLabel::Audio,
            ]),
        ),
        demand: mc_service::media_signaling::ReceiveCapabilityDeclaration::empty()
            .truncate(client_media_config().receive_slot_caps),
    }
}

/// Every frame queued so far on a wired participant's outbound channel,
/// decoded, in order — the actor-level suites' one reader of what a client
/// would receive after its `JoinResponse`.
pub fn drain_frames(
    rx: &mut tokio::sync::mpsc::Receiver<bytes::Bytes>,
) -> Vec<proto_gen::dark_tower::signaling::v1::server_message::Message> {
    use prost::Message as _;
    let mut out = Vec::new();
    while let Ok(bytes) = rx.try_recv() {
        if let Some(msg) =
            proto_gen::dark_tower::signaling::v1::ServerMessage::decode(bytes.as_ref())
                .expect("MC writes decodable ServerMessages")
                .message
        {
            out.push(msg);
        }
    }
    out
}

/// A syntactically valid Ed25519 identity public key for join-path tests.
///
/// Re-exported from `mc_test_utils::media` so the 32-byte value has one home
/// across the MC suite rather than one literal per construction site.
///
/// MC checks length only — no curve validation — so a fixed synthetic pattern
/// is sufficient. Passing it asserts nothing about identity: with no `cnf`
/// binding the roster key is trust-on-first-use, which gives same-keyholder
/// consistency and never a verified identity (ADR-0036 §4).
pub fn sample_identity_public_key() -> Vec<u8> {
    mc_test_utils::media::sample_identity_public_key()
}

/// The client media-signalling configuration the suite runs against
/// (ADR-0036 §5, §6).
///
/// Values are spelled out as literals rather than referenced from a `DEFAULT_*`
/// constant. There deliberately are no such constants for the five `MC_*`
/// media-signalling keys — they are required env vars whose documented values
/// live in `infra/services/mc-service/config.env` — and a constant surviving
/// only in a test fixture reads as retired to a grep of the load path while
/// remaining a live second encoding of the value.
///
/// These match the ConfigMap so a test asserting on a directive's encoding
/// parameters is asserting the shipped configuration, not a test-only one.
pub fn client_media_config() -> mc_service::media_signaling::ClientMediaConfig {
    mc_service::media_signaling::ClientMediaConfig {
        // `MC_MAX_AUDIO_RECEIVE_SLOTS` / `MC_MAX_VIDEO_RECEIVE_SLOTS`.
        receive_slot_caps: [8, 4],
        max_receive_capability_declarations: 64,
        audio_encoding: mc_service::media_signaling::AudioEncoding::new(
            proto_gen::dark_tower::signaling::v1::Codec::Opus,
            48_000,
            50,
        )
        .expect("suite audio encoding is in band"),
        // The `media-codec-profile` production video values and
        // `MC_VIDEO_MIN_KEYFRAME_INTERVAL_MS`.
        video_encoding: mc_service::media_signaling::VideoEncoding::new(
            proto_gen::dark_tower::signaling::v1::Codec::Vp8,
            640,
            360,
            30,
            800_000,
            500,
        )
        .expect("suite video encoding is in band"),
        // `MC_MEDIA_CONNECT_SETTLE_MS` as the ConfigMap ships it.
        connect_settle_window: std::time::Duration::from_millis(1500),
    }
}

/// Typed form of [`sample_identity_public_key`], for actor-level call sites
/// that take an `IdentityPublicKey` rather than raw wire bytes.
pub fn test_identity_key() -> Option<mc_service::media_admission::IdentityPublicKey> {
    Some(
        mc_service::media_admission::IdentityPublicKey::try_from_bytes(
            &sample_identity_public_key(),
        )
        .expect("fixture key is exactly 32 bytes"),
    )
}

/// The contract's NO KEY PUBLISHED state: a participant MC admits without an
/// identity key. Legitimate, not a failure — consumers must fail closed on it.
pub fn absent_identity_key() -> Option<mc_service::media_admission::IdentityPublicKey> {
    None
}

// =============================================================================
// WebTransport client-side framing
// =============================================================================
//
// The signaling wire framing — 4-byte big-endian length prefix, then a
// protobuf message — re-implemented per test file until 2026-09-02. This is
// the shared home; new files MUST use it rather than adding a sixth copy.
//
// `join_tests.rs`, `media_connection_update_integration.rs`,
// `otel_webtransport_integration.rs` and `webtransport_accept_loop_integration.rs`
// still carry their own near-identical copies. Those pre-date this home and are
// tracked for migration in `docs/TODO.md §Cross-Service Duplication`; the
// variations between them are cosmetic (`Vec<u8>` vs `BytesMut`, `expect` vs a
// bounded `Option`) against a wire contract that is identical, which is exactly
// why drift between them would surface as a mysterious test-only decode failure.

/// Length-prefix-frame a `ClientMessage` for the signaling stream.
pub fn encode_framed(msg: &proto_gen::dark_tower::signaling::v1::ClientMessage) -> bytes::BytesMut {
    use bytes::BufMut as _;
    use prost::Message as _;

    let payload = msg.encode_to_vec();
    let mut framed = bytes::BytesMut::with_capacity(4 + payload.len());
    framed.put_u32(u32::try_from(payload.len()).expect("payload fits u32"));
    framed.put_slice(&payload);
    framed
}

/// Read one length-prefixed `ServerMessage` off a signaling stream.
pub async fn read_server_message(
    recv: &mut wtransport::RecvStream,
) -> proto_gen::dark_tower::signaling::v1::ServerMessage {
    use prost::Message as _;

    let mut len_buf = [0u8; 4];
    recv.read_exact(&mut len_buf).await.expect("read length");
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut payload = vec![0u8; len];
    recv.read_exact(&mut payload).await.expect("read payload");
    proto_gen::dark_tower::signaling::v1::ServerMessage::decode(payload.as_slice())
        .expect("decodable ServerMessage")
}

/// Connect a WebTransport client to `url`, skipping cert validation.
pub async fn connect(url: &str) -> wtransport::Connection {
    let config = wtransport::ClientConfig::builder()
        .with_bind_default()
        .with_no_cert_validation()
        .build();
    wtransport::Endpoint::client(config)
        .expect("client endpoint")
        .connect(url)
        .await
        .expect("connect")
}

/// A `MeetingEndedNotifier` that records every meeting id it is told about.
#[derive(Default)]
pub struct RecordingMeetingEnded {
    ended: Mutex<Vec<String>>,
    notify: Notify,
}

impl RecordingMeetingEnded {
    /// Every meeting id reported, in order.
    pub fn ended(&self) -> Vec<String> {
        self.ended
            .lock()
            .expect("RecordingMeetingEnded poisoned")
            .clone()
    }

    /// Wait until at least `expected` reports have landed.
    pub async fn wait_for(&self, expected: usize, timeout: Duration) -> Vec<String> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let ended = self.ended();
            if ended.len() >= expected {
                return ended;
            }
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            assert!(
                !remaining.is_zero(),
                "Timeout waiting for {expected} meeting-ended reports; got {ended:?}"
            );
            let _ = tokio::time::timeout(remaining, self.notify.notified()).await;
        }
    }
}

impl mc_service::actors::controller::MeetingEndedNotifier for RecordingMeetingEnded {
    fn meeting_ended(&self, meeting_id: &str) {
        self.ended
            .lock()
            .expect("RecordingMeetingEnded poisoned")
            .push(meeting_id.to_string());
        self.notify.notify_one();
    }
}
