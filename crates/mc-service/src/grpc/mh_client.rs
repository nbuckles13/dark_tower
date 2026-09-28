//! Media Handler gRPC Client.
//!
//! Provides a client for MC->MH communication:
//! - `RegisterMeeting` - Program an MH with a meeting's forwarding policy
//!   (ADR-0036 §7, §8)
//! - `EndMeeting` - Release an ended meeting on an MH (story 2 R-20). Ordering
//!   (drain every push first) is the caller's obligation; see
//!   `media_routing::teardown`.
//!
//! # Security
//!
//! - OAuth 2.0 tokens authenticate MC to MH (via TokenReceiver)
//! - Tokens are automatically refreshed by TokenManager background task
//! - Each call creates a new Channel (MH endpoints vary per meeting). **This is
//!   pre-existing and its cost is recorded rather than defended**: a `Channel`
//!   per call means one fresh TCP connection and TLS handshake per RPC, against
//!   a set of distinct endpoints that is the handler FLEET (2), not the meeting
//!   count — so the stated reason is weaker than it reads. The peak is
//!   `MC_MAX_MEETINGS` x handlers-per-meeting (2), and up to twice that while a
//!   teardown's quiesce has timed out and a push worker overlaps its release:
//!   ~4,000 concurrent connects at the default cap. Measured headroom on the
//!   deployed pod is `nofile` = 1,048,576 (containerd default; no limit is set
//!   in `infra/services/mc-service/**`), so exhaustion is not reachable — which
//!   is why this is recorded, not fixed here. **Teardown concurrency is
//!   CORRELATED, unlike push concurrency**: pushes follow independent roster
//!   events, whereas one unreachable handler or one drain ends many meetings
//!   into the same window, so the same arithmetic peak is far likelier to be
//!   approached. **The remedy is channel REUSE, not a concurrency cap** — one
//!   `Channel` per endpoint, multiplexed over HTTP/2, lowers both peaks and
//!   lengthens nothing, where a semaphore lowers one and lengthens the teardown
//!   fence (and so the rejoin hold) for every queued meeting. Spun out with the
//!   numbers in `docs/TODO.md` ("MC opens a fresh gRPC channel per MH call").
//! - **No key material of any kind is read, logged, or placed on the request.**
//!   The request carries meeting/MC identity, edges, per-egress behaviours and a
//!   generation — never a KEK, an identity key or a nonce (ADR-0036 §4, §11).
//!
//! # Connection Pattern
//!
//! Unlike GcClient (singleton channel), MhClient creates a Channel per call
//! because different meetings may be assigned to different MH instances.

use crate::errors::McError;
use crate::media_routing::teardown::{EndMeetingCall, EndMeetingFailure, EndMeetingFailureKind};
use crate::media_routing::{
    self, EgressStreamPlan, HandlerAssignment, PolicyPushOutcome, PushDisposition, PushExpectation,
};
use crate::observability::metrics::{record_media_policy_push, record_register_meeting};
use common::observability::labels::KEY_CUSTODY_OPERATOR;
use common::secret::ExposeSecret;
use common::token_manager::TokenReceiver;
use proto_gen::dark_tower::internal::v1::media_handler_service_client::MediaHandlerServiceClient;
use proto_gen::dark_tower::internal::v1::{
    CandidateSource, EgressStream, EndMeetingRequest, MutedSource, RegisterMeetingRequest,
    RegisterMeetingResponse, SubscriberSlot,
};
use proto_gen::dark_tower::signaling::v1::TransportMode;
use std::num::NonZeroU64;
use std::pin::Pin;
use std::time::{Duration, Instant};
use tonic::transport::Channel;
use tonic::transport::Endpoint;
use tonic::Request;
use tracing::{debug, error, info, instrument, warn};

/// Default timeout for MH RPC calls.
///
/// `pub(crate)` because the teardown's quiesce bound is DERIVED from it
/// (`media_routing::teardown::PUSH_QUIESCE_BOUND`): one in-flight attempt is
/// bounded by this plus [`MH_CONNECT_TIMEOUT`], so changing either moves the
/// bound with it instead of leaving a stale copy.
pub(crate) const MH_RPC_TIMEOUT: Duration = Duration::from_secs(10);

/// Default connect timeout for MH. See [`MH_RPC_TIMEOUT`] for why it is
/// crate-visible.
pub(crate) const MH_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

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
    /// Set by the pusher iff this `(meeting, handler)` has had NO confirmed
    /// push and NO adopted floor in this process lifetime — the "first confirm
    /// after an MC restart" discriminator.
    ///
    /// When set, a `generation_mismatch` reply whose applied generation is
    /// HIGHER than the one sent is the expected MC-restart case (the handler
    /// still holds the pre-restart MC's number), not a divergence: `confirm`
    /// does not record it on `mc_media_policy_pushes_total`, and the pusher
    /// records exactly one of `mc_media_policy_generation_adoptions_total`
    /// (floor adopted) or `generation_mismatch` (adoption failed, so it still
    /// pages). Never set on a later push, so a higher echo after a confirm —
    /// the `u64::MAX` ratchet wedge, or anything else — still pages.
    pub restart_floor_adoptable: bool,
}

impl MeetingProgramming<'_> {
    /// Whether this reply is the MC-restart floor case (see
    /// [`Self::restart_floor_adoptable`]). One predicate, read by `confirm`
    /// (to not record it) and the pusher (to adopt), so the two cannot drift.
    #[must_use]
    pub fn is_restart_floor(&self, outcome: PolicyPushOutcome, applied_generation: u64) -> bool {
        self.restart_floor_adoptable
            && outcome == PolicyPushOutcome::GenerationMismatch
            && applied_generation > self.policy_generation.get()
    }
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

    /// Release an ended meeting on one MH instance.
    ///
    /// REQUIRED, with no default body: a defaulted `Ok(())` would let a test
    /// double silently skip recording the call, and the "no `EndMeeting` before
    /// pushes stop" assertions would then hold over an empty set.
    ///
    /// `Ok(())` means ACKNOWLEDGED — released, or unknown there (one outcome by
    /// design). Every failure is classified; see [`EndMeetingFailureKind`].
    fn end_meeting<'a>(
        &'a self,
        call: &'a EndMeetingCall,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), EndMeetingFailure>> + Send + 'a>>;
}

/// Map a gRPC status from `EndMeeting` to its contract class
/// (`EndMeetingRequest` in `internal.proto`).
#[must_use]
pub fn classify_end_meeting_status(status: &tonic::Status) -> EndMeetingFailureKind {
    match status.code() {
        tonic::Code::FailedPrecondition => EndMeetingFailureKind::RejectedOwnership,
        tonic::Code::Unimplemented => EndMeetingFailureKind::Unimplemented,
        tonic::Code::InvalidArgument => EndMeetingFailureKind::InvalidArgument,
        // "Nothing released on this attempt" — and a release is idempotent.
        tonic::Code::Unavailable | tonic::Code::DeadlineExceeded => {
            EndMeetingFailureKind::Retryable
        }
        _ => EndMeetingFailureKind::Other,
    }
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
        let channel = connect_handler(programming.mh_grpc_endpoint, meeting_id).await?;

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
            // This handler's server-muted senders, filtered by edge ownership in
            // the render (`HandlerAssignment::server_muted_sources`). A set
            // upstream, so the wire list is duplicate-free and canonically
            // ordered; a change to it changed the assignment, so the generation
            // above already advanced for it.
            server_muted_sources: build_muted_sources(programming.assignment),
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
    /// Metrics are emitted on **every** outcome including `match`, with ONE
    /// exception: the MC-restart floor on a first confirm
    /// ([`MeetingProgramming::restart_floor_adoptable`]) is recorded by the
    /// pusher instead, as exactly one of `mc_media_policy_generation_adoptions_total`
    /// or (adoption failed) `generation_mismatch`. So the usable denominator of
    /// evaluated replies is `sum(mc_media_policy_pushes_total) +
    /// sum(mc_media_policy_generation_adoptions_total)`.
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
        let restart_floor = programming.is_restart_floor(outcome, response.applied_generation);

        // The MC-restart floor is NOT recorded here: the pusher records exactly
        // one of the adoption counter or (if adoption fails) this outcome, in
        // one place, so no scrape can see a mismatch that an adoption then
        // explains. See `MeetingProgramming::restart_floor_adoptable`.
        if !restart_floor {
            record_media_policy_push(
                outcome,
                programming.policy_generation,
                response.applied_generation,
            );
        }
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
        if restart_floor {
            info!(
                target: "mc.grpc.mh_client",
                key_custody = KEY_CUSTODY_OPERATOR,
                meeting_id = %programming.meeting_id,
                sent_generation = programming.policy_generation.get(),
                applied_generation = response.applied_generation,
                process_start_epoch_ms = response.process_start_epoch_ms,
                "Handler holds a higher policy generation on this MC's first push (MC restart \
                 under a live meeting); handing to the pusher to adopt it as a floor"
            );
            return Err(McError::MediaPolicyDivergence {
                outcome,
                applied_generation: response.applied_generation,
            });
        }

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
            | media_routing::PushDisposition::Terminal => Err(McError::MediaPolicyDivergence {
                outcome,
                applied_generation: response.applied_generation,
            }),
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

impl MhClient {
    /// Release an ended meeting on one MH (story 2 R-20).
    ///
    /// Classifies every failure for the caller's retry decision; logs nothing
    /// itself (the teardown logs each terminal outcome once, with its cause).
    ///
    /// # Errors
    ///
    /// [`EndMeetingFailure`] — see [`EndMeetingFailureKind`] for the classes.
    #[instrument(skip_all, fields(meeting_id = %call.meeting_id), target = "mc.grpc.mh_client")]
    pub async fn end_meeting(&self, call: &EndMeetingCall) -> Result<(), EndMeetingFailure> {
        // A connect failure provably released nothing, so it is retryable.
        let channel = connect_handler(&call.mh_grpc_endpoint, &call.meeting_id)
            .await
            .map_err(|e| EndMeetingFailure {
                kind: EndMeetingFailureKind::Retryable,
                detail: e.to_string(),
            })?;
        let mut client = MediaHandlerServiceClient::with_interceptor(
            channel,
            common::observability::otel_grpc::client_interceptor(),
        );
        let request = self
            .add_auth(EndMeetingRequest {
                meeting_id: call.meeting_id.clone(),
                mc_id: call.mc_id.clone(),
            })
            .map_err(|e| EndMeetingFailure {
                kind: EndMeetingFailureKind::Other,
                detail: e.to_string(),
            })?;
        match client.end_meeting(request).await {
            Ok(response) if response.get_ref().acknowledged => Ok(()),
            // `false` is the proto3 default of an empty reply, so it must never
            // read as success (`EndMeetingResponse`).
            Ok(_) => Err(EndMeetingFailure {
                kind: EndMeetingFailureKind::Other,
                detail: "EndMeeting returned acknowledged=false".to_string(),
            }),
            Err(status) => Err(EndMeetingFailure {
                kind: classify_end_meeting_status(&status),
                detail: format!("{:?}: {}", status.code(), status.message()),
            }),
        }
    }
}

/// Open a channel to one MH — the ONE dial path both RPCs share, so the
/// timeouts that the teardown's quiesce bound is derived from cannot diverge
/// between them.
async fn connect_handler(mh_grpc_endpoint: &str, meeting_id: &str) -> Result<Channel, McError> {
    Endpoint::from_shared(mh_grpc_endpoint.to_string())
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
        })
}

/// The wire form of one handler's server-muted set. The set is already
/// deduplicated and ordered upstream; this is a pure mapping.
fn build_muted_sources(assignment: &HandlerAssignment) -> Vec<MutedSource> {
    assignment
        .server_muted_sources
        .iter()
        .map(|sender| MutedSource {
            sender_id: u32::from(sender.get().get()),
        })
        .collect()
}

impl MhRegistrationClient for MhClient {
    fn register_meeting<'a>(
        &'a self,
        programming: &'a MeetingProgramming<'a>,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), McError>> + Send + 'a>> {
        Box::pin(self.register_meeting(programming))
    }

    fn end_meeting<'a>(
        &'a self,
        call: &'a EndMeetingCall,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), EndMeetingFailure>> + Send + 'a>> {
        Box::pin(MhClient::end_meeting(self, call))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::media_admission::SenderId;
    use crate::media_routing::{HandlerId, SlotTable};
    use std::num::NonZeroU16;

    #[test]
    fn test_mh_rpc_timeout_constants() {
        assert_eq!(MH_RPC_TIMEOUT, Duration::from_secs(10));
        assert_eq!(MH_CONNECT_TIMEOUT, Duration::from_secs(5));
    }

    /// Local, and it CANNOT delegate to `mc_test_utils::media::two_party_assignment`
    /// even though `test_token_receiver` above does exactly that — the two sit
    /// adjacent and the difference is invisible without this note.
    ///
    /// `mc-test-utils` links the plain rlib build of `mc-service`, while this
    /// `#[cfg(test)]` module is the `--test` build: two distinct crate
    /// instances, so `mc-service`'s **own** types do not unify across the
    /// boundary. Delegating yields
    /// `expected assignment::HandlerAssignment, found HandlerAssignment`.
    ///
    /// Drift risk is low rather than absent: both this and the shared fixture
    /// render through the real [`SlotTable`], so neither hand-encodes the
    /// policy shape. A note, deliberately not an `ANCHOR (DRY):` and not a guard.
    fn two_party_assignment() -> HandlerAssignment {
        let handler = HandlerId::new("mh-0");
        let set =
            crate::media_routing::MeetingHandlers::new([crate::media_routing::HandlerEndpoint {
                id: handler.clone(),
                webtransport_url: "https://mh-0.example:4434".to_string(),
                grpc_endpoint: "http://mh-0.example:50053".to_string(),
            }])
            .unwrap();
        let mut table = SlotTable::new();
        for n in [7, 8] {
            let sender = SenderId::from_nonzero(NonZeroU16::new(n).unwrap());
            table.admit_on(sender, &set, &["mh-0"]);
            table.set_demand(sender, vec![0]);
        }
        table
            .render([&handler], &std::collections::BTreeSet::new())
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
            restart_floor_adoptable: false,
        }
    }

    /// The safety net for @dry-8: MC is the producer and satisfies MH's
    /// `MeetingPolicy::from_request` rejections **by construction**, so this
    /// asserts the built request rather than mirroring MH's validator at
    /// runtime. A mirrored validator would be a second copy of MH's rules that
    /// drifts silently; an assertion on the built message fails the build.
    #[test]
    fn built_request_satisfies_every_mh_rejection_by_construction() {
        let assignment = two_party_assignment();
        let streams = build_egress_streams(&assignment);

        assert_eq!(streams.len(), 2, "one pinned stream per subscriber");
        let s = &streams[0];
        assert_eq!(s.candidate_sources.len(), 1, "exactly one pinned candidate");
        assert_ne!(
            s.candidate_sources[0].sender_id,
            s.subscriber.as_ref().unwrap().sender_id,
            "no self-edge (story 2 R-3)"
        );

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
    fn declared_transport_mode_is_datagram_for_an_audio_policy() {
        assert_eq!(
            declared_transport_mode(&two_party_assignment()),
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
        let assignment = two_party_assignment();
        let p = programming(&assignment);
        let request = RegisterMeetingRequest {
            meeting_id: p.meeting_id.to_string(),
            mc_id: p.mc_id.to_string(),
            mc_grpc_endpoint: p.mc_grpc_endpoint.to_string(),
            egress_streams: build_egress_streams(p.assignment),
            selection_rules: None,
            policy_generation: p.policy_generation.get(),
            server_muted_sources: Vec::new(),
        };
        // `RegisterMeetingRequest` has no bytes-typed field at all, so this is
        // structural rather than a scan: there is nowhere for a key to go.
        assert_eq!(request.policy_generation, 1);
        assert!(request.selection_rules.is_none());
        assert_eq!(request.egress_streams.len(), 2);
    }

    #[test]
    fn test_mh_client_creation() {
        let token_rx = mc_test_utils::test_token_receiver();
        let _client = MhClient::new(token_rx);
    }

    #[tokio::test]
    async fn test_register_meeting_invalid_endpoint() {
        let client = MhClient::new(mc_test_utils::test_token_receiver());
        let assignment = two_party_assignment();
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
        let assignment = two_party_assignment();
        let mut p = programming(&assignment);
        p.mh_grpc_endpoint = "http://127.0.0.1:59998";

        let result = client.register_meeting(&p).await;

        assert!(
            matches!(&result, Err(McError::Grpc(msg)) if msg.contains("Failed to connect")),
            "Expected Grpc connection error, got: {result:?}"
        );
    }
}
