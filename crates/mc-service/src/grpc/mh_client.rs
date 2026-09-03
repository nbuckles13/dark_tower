//! Media Handler gRPC Client.
//!
//! Provides a client for MC->MH communication:
//! - `RegisterMeeting` - Program an MH with a meeting's forwarding policy
//!   (ADR-0036 §7, §8)
//!
//! # Security
//!
//! - OAuth 2.0 tokens authenticate MC to MH (via TokenReceiver)
//! - Tokens are automatically refreshed by TokenManager background task
//! - Each call creates a new Channel (MH endpoints vary per meeting)
//! - **No key material of any kind is read, logged, or placed on the request.**
//!   The request carries meeting/MC identity, edges, per-egress behaviours and a
//!   generation — never a KEK, an identity key or a nonce (ADR-0036 §4, §11).
//!
//! # Connection Pattern
//!
//! Unlike GcClient (singleton channel), MhClient creates a Channel per call
//! because different meetings may be assigned to different MH instances.

use crate::errors::McError;
use crate::media_routing::{
    self, EgressStreamPlan, HandlerAssignment, PolicyPushOutcome, PushDisposition, PushExpectation,
};
use crate::observability::metrics::{record_media_policy_push, record_register_meeting};
use common::observability::labels::KEY_CUSTODY_OPERATOR;
use common::secret::ExposeSecret;
use common::token_manager::TokenReceiver;
use proto_gen::dark_tower::internal::v1::media_handler_service_client::MediaHandlerServiceClient;
use proto_gen::dark_tower::internal::v1::{
    CandidateSource, EgressStream, RegisterMeetingRequest, RegisterMeetingResponse, SubscriberSlot,
};
use proto_gen::dark_tower::signaling::v1::TransportMode;
use std::num::NonZeroU64;
use std::pin::Pin;
use std::time::{Duration, Instant};
use tonic::transport::Endpoint;
use tonic::Request;
use tracing::{debug, error, instrument, warn};

/// Default timeout for MH RPC calls.
const MH_RPC_TIMEOUT: Duration = Duration::from_secs(10);

/// Default connect timeout for MH.
const MH_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Everything one `RegisterMeeting` push carries.
///
/// One borrowed struct rather than seven positional `&str`s: the call now
/// carries an assignment and a generation alongside the identity scalars, and a
/// seven-argument call where five are strings is a swap waiting to happen.
#[derive(Debug, Clone, Copy)]
pub struct MeetingProgramming<'a> {
    /// gRPC endpoint of the target MH.
    pub mh_grpc_endpoint: &'a str,
    /// The handler MC believes it is dialing (`MhAssignment.mh_id` from the
    /// Redis assignment snapshot). Compared against the `handler_id` the
    /// handler asserts in its reply.
    pub expected_handler_id: &'a str,
    /// Meeting being programmed.
    pub meeting_id: &'a str,
    /// This MC's identifier.
    pub mc_id: &'a str,
    /// This MC's gRPC endpoint, for MH->MC callbacks.
    pub mc_grpc_endpoint: &'a str,
    /// The forwarding policy this handler is to install.
    pub assignment: &'a HandlerAssignment,
    /// The generation that policy is pushed under.
    ///
    /// Computed **once per push by the caller**, never inside this module: a
    /// generation recomputed per retry attempt would break ADR-0036 §8's
    /// "an unchanged policy carries the same number".
    pub policy_generation: NonZeroU64,
}

/// Trait for MC->MH meeting registration.
///
/// Abstraction over the gRPC call used to program MH instances with a meeting's
/// forwarding policy. Production code uses `MhClient`; tests can inject a mock
/// to verify call arguments and simulate failures.
pub trait MhRegistrationClient: Send + Sync {
    /// Program a meeting's forwarding policy onto an MH instance.
    fn register_meeting<'a>(
        &'a self,
        programming: &'a MeetingProgramming<'a>,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), McError>> + Send + 'a>>;
}

/// Build the wire `EgressStream` messages for one handler's policy.
///
/// # Sibling fixture home, deliberately NOT shared (D-12)
///
/// `crates/mh-test-utils/src/media_policy.rs` builds the same `EgressStream` /
/// `RegisterMeetingRequest` literal (`egress`, `loopback_egress`,
/// `register_request`) for MH's own suite. The two are **deliberately not
/// collapsed into one home**: `mh-test-utils` has a path dependency on
/// `mh-service`, so importing it from MC's dev-dependencies would pull
/// `mh-service` into MC's dev graph — the same ADR-0028 layering inversion the
/// DRY index already records for `env-tests/src/fixtures/media.rs` versus
/// `mc-test-utils/src/media.rs`.
///
/// **Their agreement is not load-bearing, which is why this is a boundary note
/// and not an `ANCHOR (DRY):`.** MH must accept a *range* of well-formed
/// registrations — `MeetingPolicy::from_request` enforces identifier widths,
/// uniqueness of `egress_stream_id` and `(sender_id, slot_id)`, a specified and
/// homogeneous transport mode, and the configured count bounds — not the one
/// shape MC happens to emit today. Two sites making their own decision under one
/// rule is not a single source of truth, and an anchor here would assert an
/// equality neither side owes and go stale the first time either legitimately
/// diverges.
fn build_egress_streams(assignment: &HandlerAssignment) -> Vec<EgressStream> {
    assignment
        .egress_streams
        .iter()
        .map(|plan| {
            let EgressStreamPlan {
                egress_stream_id,
                subscriber,
                slot_id,
                candidate_sources,
                stream_number,
                priority_group,
                supersede_on_independent_frame,
                transport_mode,
            } = plan;
            EgressStream {
                egress_stream_id: *egress_stream_id,
                subscriber: Some(SubscriberSlot {
                    sender_id: u32::from(subscriber.get().get()),
                    slot_id: u32::from(*slot_id),
                }),
                candidate_sources: candidate_sources
                    .iter()
                    .map(|sender| CandidateSource {
                        sender_id: u32::from(sender.get().get()),
                        stream_number: u32::from(*stream_number),
                    })
                    .collect(),
                priority_group: *priority_group,
                supersede_on_independent_frame: *supersede_on_independent_frame,
                transport_mode: *transport_mode as i32,
            }
        })
        .collect()
}

/// The transport mode MC declares for this push.
///
/// `internal.proto` echoes ONE transport mode for the whole registration, which
/// is unambiguous only while a meeting's egress streams are homogeneous — true
/// this story (audio, datagram) and false when video lands. MC's assignment
/// produces homogeneous modes by construction, so the declared mode is the
/// first stream's.
///
/// An **empty** policy names no mode. `Unspecified` is correct there and is not
/// a fail-open: an empty policy also means MH installs no egress stream, so the
/// echo has nothing to disagree with, and a handler that installed the empty
/// policy echoes `Unspecified` back.
fn declared_transport_mode(assignment: &HandlerAssignment) -> TransportMode {
    assignment
        .egress_streams
        .first()
        .map_or(TransportMode::Unspecified, |s| s.transport_mode)
}

/// MH client for RegisterMeeting RPCs.
///
/// Holds a `TokenReceiver` for Bearer auth. Creates a new gRPC channel
/// per call since MH endpoints vary per meeting assignment.
pub struct MhClient {
    /// Token receiver for dynamically refreshed OAuth tokens.
    token_rx: TokenReceiver,
}

impl MhClient {
    /// Create a new MH client.
    ///
    /// # Arguments
    ///
    /// * `token_rx` - Token receiver for dynamically refreshed OAuth tokens
    #[must_use]
    pub fn new(token_rx: TokenReceiver) -> Self {
        Self { token_rx }
    }

    /// Program a meeting's forwarding policy onto an MH instance, and confirm it.
    ///
    /// Creates a new gRPC channel to the specified MH endpoint, sends a
    /// `RegisterMeeting` RPC carrying the edge set, the per-egress behaviours
    /// and the derived `policy_generation`, then classifies the reply
    /// ([`media_routing::evaluate`]) and **fails loud** on anything that does
    /// not prove the policy is live.
    ///
    /// `accepted == true` is deliberately not treated as success: MH's handler
    /// returns before the apply completes, so only
    /// `applied_generation == policy_generation` proves programming.
    ///
    /// # Errors
    ///
    /// Returns `McError::Config` if the endpoint is invalid.
    /// Returns `McError::Grpc` if the connection or RPC fails.
    /// Returns `McError::MediaPolicyDivergence` if the reply did not confirm —
    /// see [`crate::media_routing::PushDisposition`] for which of those the
    /// caller should retry.
    #[instrument(
        skip_all,
        fields(meeting_id = %programming.meeting_id),
        target = "mc.grpc.mh_client"
    )]
    pub async fn register_meeting(
        &self,
        programming: &MeetingProgramming<'_>,
    ) -> Result<(), McError> {
        let meeting_id = programming.meeting_id;

        // Create channel to the specific MH endpoint
        let channel = Endpoint::from_shared(programming.mh_grpc_endpoint.to_string())
            .map_err(|e| {
                error!(
                    target: "mc.grpc.mh_client",
                    error = %e,
                    "Invalid MH endpoint"
                );
                McError::Config(format!("Invalid MH endpoint: {e}"))
            })?
            .connect_timeout(MH_CONNECT_TIMEOUT)
            .timeout(MH_RPC_TIMEOUT)
            .connect()
            .await
            .map_err(|e| {
                warn!(
                    target: "mc.grpc.mh_client",
                    error = %e,
                    meeting_id = %meeting_id,
                    "Failed to connect to MH"
                );
                McError::Grpc(format!("Failed to connect to MH: {e}"))
            })?;

        // R-56: inject the active W3C trace context into outbound MH metadata.
        let mut client = MediaHandlerServiceClient::with_interceptor(
            channel,
            common::observability::otel_grpc::client_interceptor(),
        );

        let declared_mode = declared_transport_mode(programming.assignment);
        let request = RegisterMeetingRequest {
            meeting_id: meeting_id.to_string(),
            mc_id: programming.mc_id.to_string(),
            mc_grpc_endpoint: programming.mc_grpc_endpoint.to_string(),
            egress_streams: build_egress_streams(programming.assignment),
            // `SelectionRules` is a deliberately-empty named message, and its
            // PRESENCE distinguishes "MC declared no rules" from "MC did not
            // speak about rules". This story declares none — ranking, debounce,
            // churn limits and per-subscriber exclusions are §7's story-5 work
            // — so `None` is the accurate statement, not a stub left unfilled.
            selection_rules: None,
            policy_generation: programming.policy_generation.get(),
        };

        let grpc_request = self.add_auth(request)?;

        let start = Instant::now();
        match client.register_meeting(grpc_request).await {
            Ok(response) => {
                let duration = start.elapsed();
                let inner = response.into_inner();
                self.confirm(programming, &inner, declared_mode, duration)
            }
            Err(e) => {
                let duration = start.elapsed();
                record_register_meeting("error", duration);

                warn!(
                    target: "mc.grpc.mh_client",
                    error = %e,
                    meeting_id = %meeting_id,
                    "RegisterMeeting RPC failed"
                );
                Err(McError::Grpc(format!("RegisterMeeting RPC failed: {e}")))
            }
        }
    }

    /// Classify the reply, emit both media-path metrics, log, and decide.
    ///
    /// Metrics are emitted on **every** outcome including `match`, so the
    /// counter's sum is a usable denominator.
    fn confirm(
        &self,
        programming: &MeetingProgramming<'_>,
        response: &RegisterMeetingResponse,
        declared_mode: TransportMode,
        duration: Duration,
    ) -> Result<(), McError> {
        let expectation = PushExpectation {
            handler_id: programming.expected_handler_id,
            policy_generation: programming.policy_generation,
            transport_mode: declared_mode,
        };
        let outcome = media_routing::evaluate(&expectation, response);

        record_media_policy_push(
            outcome,
            programming.policy_generation,
            response.applied_generation,
        );
        // Keyed on the DISPOSITION, not on `== Match`. `status="success"` on this
        // metric means "the meeting is programmed", which is exactly what
        // `PushDisposition::Programmed` asserts — and `handler_id_mismatch` is
        // `Programmed` (see `confirm::PolicyPushOutcome::disposition`). Comparing
        // against `Match` here would record a failure on every ordinary MH pod
        // restart for a push that succeeded and returned `Ok`, reintroducing on
        // this pre-existing binary series the exact false positive OPS-17/OPS-18
        // disarmed on `mc_media_policy_pushes_total` — and here with no yellow,
        // no caveat and no `outcome!~` escape available, because the series has
        // only success/error. One expression, so the two cannot drift.
        record_register_meeting(
            if outcome.disposition() == PushDisposition::Programmed {
                "success"
            } else {
                "error"
            },
            duration,
        );

        if outcome == PolicyPushOutcome::Match {
            debug!(
                target: "mc.grpc.mh_client",
                meeting_id = %programming.meeting_id,
                key_custody = KEY_CUSTODY_OPERATOR,
                handler_id = %response.handler_id,
                applied_generation = response.applied_generation,
                process_start_epoch_ms = response.process_start_epoch_ms,
                "MH confirmed the forwarding policy is live"
            );
            return Ok(());
        }

        // EVERY comparison's operands, regardless of which label won. A single
        // bounded `outcome` reports one failure, but a response can be
        // `generation_mismatch` AND `transport_mode_mismatch` at once, and the
        // responder needs both. Generation numbers ride in the LOG LINE, never
        // in a metric label — as labels they would be unbounded, one new series
        // per policy change. No meeting identifier appears on either metric.
        //
        // `process_start_epoch_ms` is carried for the responder only: restart
        // DETECTION is the deferred handler-restart story, so no outcome and no
        // metric hangs off it here, and `0` reads as "restart undetectable".
        let interim_marker = if outcome == PolicyPushOutcome::HandlerIdMismatch {
            // Read this as the expected stable-id-not-yet-deployed case at a
            // glance; see `media_routing::confirm::evaluate`'s precedence note
            // and `2026-09-02-mh-stable-handler-id`.
            " (interim)"
        } else {
            ""
        };
        error!(
            target: "mc.grpc.mh_client",
            key_custody = KEY_CUSTODY_OPERATOR,
            outcome = outcome.label(),
            meeting_id = %programming.meeting_id,
            expected_handler_id = %programming.expected_handler_id,
            echoed_handler_id = %response.handler_id,
            sent_generation = programming.policy_generation.get(),
            applied_generation = response.applied_generation,
            generation_matched = response.applied_generation == programming.policy_generation.get(),
            sent_transport_mode = declared_mode.as_str_name(),
            echoed_transport_mode = TransportMode::try_from(response.transport_mode)
                .unwrap_or(TransportMode::Unspecified)
                .as_str_name(),
            process_start_epoch_ms = response.process_start_epoch_ms,
            accepted = response.accepted,
            "MH did not confirm the forwarding policy{}",
            interim_marker
        );

        match outcome.disposition() {
            // `handler_id_mismatch` lands here: fully observed above, and the
            // meeting IS programmed (the demoted precedence means this arm is
            // only reachable with `applied == sent` and the mode agreeing), so
            // only the abort is suppressed — never the report.
            media_routing::PushDisposition::Programmed => Ok(()),
            media_routing::PushDisposition::Retryable
            | media_routing::PushDisposition::Terminal => {
                Err(McError::MediaPolicyDivergence { outcome })
            }
        }
    }

    /// Add authorization header to a request.
    fn add_auth<T>(&self, request: T) -> Result<Request<T>, McError> {
        let mut grpc_request = Request::new(request);
        let current_token = self.token_rx.token();
        grpc_request.metadata_mut().insert(
            "authorization",
            format!("Bearer {}", current_token.expose_secret())
                .parse()
                .map_err(|e| {
                    error!(
                        target: "mc.grpc.mh_client",
                        error = %e,
                        "Authorization header parse failed"
                    );
                    McError::Config(format!("Authorization header parse failed: {e}"))
                })?,
        );
        Ok(grpc_request)
    }
}

impl MhRegistrationClient for MhClient {
    fn register_meeting<'a>(
        &'a self,
        programming: &'a MeetingProgramming<'a>,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), McError>> + Send + 'a>> {
        Box::pin(self.register_meeting(programming))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::media_admission::SenderId;
    use crate::media_routing::{
        compute_assignment, HandlerId, MeetingRoutingInput, RoutingParticipant,
    };
    use std::num::NonZeroU16;

    #[test]
    fn test_mh_rpc_timeout_constants() {
        assert_eq!(MH_RPC_TIMEOUT, Duration::from_secs(10));
        assert_eq!(MH_CONNECT_TIMEOUT, Duration::from_secs(5));
    }

    /// Local, and it CANNOT delegate to `mc_test_utils::media::loopback_assignment`
    /// even though `test_token_receiver` above does exactly that — the two sit
    /// adjacent and the difference is invisible without this note.
    ///
    /// `mc-test-utils` links the plain rlib build of `mc-service`, while this
    /// `#[cfg(test)]` module is the `--test` build: two distinct crate
    /// instances, so `mc-service`'s **own** types do not unify across the
    /// boundary. Delegating yields
    /// `expected assignment::HandlerAssignment, found HandlerAssignment`.
    /// `test_token_receiver` works because it returns `common::token_manager::
    /// TokenReceiver`, a third-crate type, exactly as
    /// `media_admission/mod.rs` can use `mc_test_utils::media`'s `Vec<u8>`.
    ///
    /// Drift risk is low rather than absent: both this and the shared fixture
    /// build their input and call the real [`compute_assignment`], so neither
    /// hand-encodes the policy shape. A note, deliberately not an
    /// `ANCHOR (DRY):` and not a guard.
    fn loopback_assignment() -> HandlerAssignment {
        let handler = HandlerId::new("mh-0");
        let input = MeetingRoutingInput {
            participants: vec![RoutingParticipant {
                sender_id: SenderId::from_nonzero(NonZeroU16::new(7).unwrap()),
                handlers: vec![handler.clone()],
            }],
            handlers: vec![handler.clone()],
        };
        compute_assignment(&input)
            .unwrap()
            .for_handler(&handler)
            .cloned()
            .unwrap()
    }

    fn programming<'a>(assignment: &'a HandlerAssignment) -> MeetingProgramming<'a> {
        MeetingProgramming {
            mh_grpc_endpoint: "http://mh:50051",
            expected_handler_id: "mh-0",
            meeting_id: "meeting-1",
            mc_id: "mc-1",
            mc_grpc_endpoint: "http://mc:50052",
            assignment,
            policy_generation: NonZeroU64::MIN,
        }
    }

    /// The safety net for @dry-8: MC is the producer and satisfies MH's
    /// `MeetingPolicy::from_request` rejections **by construction**, so this
    /// asserts the built request rather than mirroring MH's validator at
    /// runtime. A mirrored validator would be a second copy of MH's rules that
    /// drifts silently; an assertion on the built message fails the build.
    #[test]
    fn built_request_satisfies_every_mh_rejection_by_construction() {
        let assignment = loopback_assignment();
        let streams = build_egress_streams(&assignment);

        assert_eq!(streams.len(), 1);
        let s = &streams[0];

        // Subscriber present, and both identifier widths in range.
        let subscriber = s.subscriber.as_ref().expect("subscriber must be present");
        assert_ne!(subscriber.sender_id, 0, "sender_id 0 is reserved-invalid");
        assert!(u16::try_from(subscriber.sender_id).is_ok());
        assert!(u16::try_from(subscriber.slot_id).is_ok());
        for candidate in &s.candidate_sources {
            assert_ne!(candidate.sender_id, 0);
            assert!(u16::try_from(candidate.sender_id).is_ok());
            assert!(
                u8::try_from(candidate.stream_number).is_ok(),
                "stream_number carries 8-bit key-id semantics"
            );
        }

        // A specified, homogeneous transport mode.
        assert_ne!(
            s.transport_mode,
            TransportMode::Unspecified as i32,
            "UNSPECIFIED is a rejection, not a default"
        );
        let modes: std::collections::HashSet<i32> =
            streams.iter().map(|s| s.transport_mode).collect();
        assert_eq!(modes.len(), 1, "heterogeneous modes are rejected whole");

        // The two uniqueness obligations.
        let ids: std::collections::HashSet<u32> =
            streams.iter().map(|s| s.egress_stream_id).collect();
        assert_eq!(ids.len(), streams.len());
        let slots: std::collections::HashSet<(u32, u32)> = streams
            .iter()
            .filter_map(|s| s.subscriber.as_ref())
            .map(|sub| (sub.sender_id, sub.slot_id))
            .collect();
        assert_eq!(slots.len(), streams.len());
    }

    #[test]
    fn declared_transport_mode_is_datagram_for_the_loopback_policy() {
        assert_eq!(
            declared_transport_mode(&loopback_assignment()),
            TransportMode::Datagram
        );
    }

    /// An empty policy declares no mode — MH installs no egress stream, so
    /// there is nothing for the echo to disagree with.
    #[test]
    fn declared_transport_mode_is_unspecified_for_an_empty_policy() {
        assert_eq!(
            declared_transport_mode(&HandlerAssignment::default()),
            TransportMode::Unspecified
        );
    }

    /// SEC: the request carries identity, edges, behaviours and a generation.
    /// Nothing else — and specifically no key material.
    #[test]
    fn built_request_carries_no_key_material() {
        let assignment = loopback_assignment();
        let p = programming(&assignment);
        let request = RegisterMeetingRequest {
            meeting_id: p.meeting_id.to_string(),
            mc_id: p.mc_id.to_string(),
            mc_grpc_endpoint: p.mc_grpc_endpoint.to_string(),
            egress_streams: build_egress_streams(p.assignment),
            selection_rules: None,
            policy_generation: p.policy_generation.get(),
        };
        // `RegisterMeetingRequest` has no bytes-typed field at all, so this is
        // structural rather than a scan: there is nowhere for a key to go.
        assert_eq!(request.policy_generation, 1);
        assert!(request.selection_rules.is_none());
        assert_eq!(request.egress_streams.len(), 1);
    }

    #[test]
    fn test_mh_client_creation() {
        let token_rx = mc_test_utils::test_token_receiver();
        let _client = MhClient::new(token_rx);
    }

    #[tokio::test]
    async fn test_register_meeting_invalid_endpoint() {
        let client = MhClient::new(mc_test_utils::test_token_receiver());
        let assignment = loopback_assignment();
        let mut p = programming(&assignment);
        p.mh_grpc_endpoint = "";

        let result = client.register_meeting(&p).await;

        assert!(
            matches!(&result, Err(McError::Config(_)) | Err(McError::Grpc(_))),
            "Expected Config or Grpc error, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_register_meeting_unreachable_endpoint() {
        let client = MhClient::new(mc_test_utils::test_token_receiver());
        let assignment = loopback_assignment();
        let mut p = programming(&assignment);
        p.mh_grpc_endpoint = "http://127.0.0.1:59998";

        let result = client.register_meeting(&p).await;

        assert!(
            matches!(&result, Err(McError::Grpc(msg)) if msg.contains("Failed to connect")),
            "Expected Grpc connection error, got: {result:?}"
        );
    }
}
