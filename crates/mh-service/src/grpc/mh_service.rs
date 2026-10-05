//! `MediaHandlerService` gRPC server implementation.
//!
//! Implements the MC→MH gRPC service from `internal.proto`.
//!
//! **Forwarding policy gains fields, not sibling RPCs** (ADR-0036 §8):
//! `RegisterMeeting` *is* the MC→MH control plane. Meeting lifecycle teardown is
//! a distinct concern with its own RPC, `EndMeeting` (story 2 R-20), implemented
//! here: release on the registering MC's `mc_id`, idempotent acknowledgement for
//! an unknown meeting, `FAILED_PRECONDITION` on a mismatch. The `Register`,
//! `RouteMedia` and `StreamTelemetry` stubs were retired with the 2026-09-01
//! `internal.proto` reshape; see the tombstone block in that file.
//!
//! # What this handler does
//!
//! 1. validates the request, rejecting the **whole** registration on any
//!    structural fault — never last-write-wins, never partial application;
//! 2. upserts the meeting and drains/promotes pending WebTransport connections
//!    (unchanged; a re-assert actively rescues clients in the provisional
//!    window, which is what §8 relies on for restart recovery);
//! 3. hands the validated policy to the session actor over the **config-apply**
//!    mailbox, separate from the bounded connection-lifecycle mailbox;
//! 4. answers with what MH's live forward path **actually reflects**.
//!
//! # The response is read from the live snapshot, never from the request
//!
//! `applied_generation` and `transport_mode` are read from
//! [`SessionManagerHandle::routing_snapshot`] *after* the apply resolves — on
//! every path, including mailbox-full, timeout and apply failure. `req` is not
//! in scope at the point the response is built.
//!
//! That is deliberate structure rather than discipline. ADR-0036 §8: MC sends
//! generation 7, MH enqueues and returns success, the mailbox is full or the
//! apply errors, and *MC believes MH runs 7 while MH runs 4* — a partial
//! blackhole with every liveness signal green. `internal.proto` puts it
//! bluntly: an implementer who writes `applied_generation: req.policy_generation`
//! "has deleted the feature while leaving the field". Here there is no wire
//! from the request to the response for that value to travel along.
//!
//! # Security
//!
//! All incoming requests are validated by `MhAuthLayer` before reaching these
//! handlers. Reject messages and log lines on this path carry **counts and
//! bounded static reasons only** — never a `sender_id`, `slot_id`,
//! `egress_stream_id` or meeting id. ADR-0036 §11 bars per-stream identity from
//! telemetry, and `internal.proto` flags this request as "the obvious new back
//! door for it now that `StreamTelemetry` is gone".
//!
//! No key material of any kind crosses this contract (ADR-0036 §4): MH is
//! keyless, and logs here carry `key_custody=operator`.

use std::time::{Duration, Instant};

use crate::config::PolicyLimits;
use crate::observability::metrics::{self, GrpcMethod, MeetingTeardownOutcome, PolicyApplyOutcome};
use crate::routing::MeetingPolicy;
use crate::session::{
    ApplyFailure, ApplyOutcome, EndMeetingFailure, MeetingRegistration, RegisterRefusal,
    SessionManagerHandle,
};
use common::observability::labels::KEY_CUSTODY_OPERATOR;
use proto_gen::dark_tower::internal::v1::media_handler_service_server::MediaHandlerService;
use proto_gen::dark_tower::internal::v1::{
    EndMeetingRequest, EndMeetingResponse, RegisterMeetingRequest, RegisterMeetingResponse,
};
use proto_gen::dark_tower::signaling::v1::TransportMode;
use tonic::{Request, Response, Status};
use tracing::instrument;

/// Maximum allowed length for `meeting_id` and `mc_id` fields.
/// Prevents `HashMap` key bloat from malicious or buggy callers.
const MAX_ID_LENGTH: usize = 256;

/// Maximum allowed length for `mc_grpc_endpoint`.
/// 2048 bytes is generous for any legitimate gRPC endpoint URL.
const MAX_ENDPOINT_LENGTH: usize = 2048;

/// Media Handler gRPC service.
pub struct MhMediaService {
    session_manager: SessionManagerHandle,
    /// This handler's identity, as it identifies itself to MC.
    ///
    /// The **same** value `GcClient` registers with GC, so MC can compare it
    /// against the `MhAssignment.mh_id` it dialed and fail loud on a mismatch.
    /// Never derived from a pod name or any other infrastructure topology.
    handler_id: String,
    /// Sampled once at process start and carried unchanged for the process's
    /// life, so MC can detect a restart by inequality.
    process_start_epoch_ms: u64,
    /// Bounds applied to the request before any routing table is built.
    policy_limits: PolicyLimits,
}

impl MhMediaService {
    /// Create a new media handler service.
    ///
    /// `process_start_epoch_ms` must come from **one** call to
    /// `process::sample_process_start_epoch_ms` made at process start. Sampling
    /// it per request makes MC's restart detector fire on every call; deriving
    /// it from pod name or config makes it never fire at all.
    #[must_use]
    pub fn new(
        session_manager: SessionManagerHandle,
        handler_id: String,
        process_start_epoch_ms: u64,
        policy_limits: PolicyLimits,
    ) -> Self {
        Self {
            session_manager,
            handler_id,
            process_start_epoch_ms,
            policy_limits,
        }
    }

    // There is deliberately no `Default` impl. A handler with no identity and
    // no process-start epoch cannot answer `RegisterMeeting` truthfully, and a
    // default would have to invent both — inventing exactly the two values MC
    // uses to detect a misrouted handler and a restart.

    /// Build the response from MH's own live state.
    ///
    /// The only place `applied_generation` and `transport_mode` come from.
    fn respond(&self, meeting: &crate::routing::MeetingKey) -> RegisterMeetingResponse {
        let snapshot = self.session_manager.routing_snapshot();
        RegisterMeetingResponse {
            // "Received and parsed." That is all it means. MC must not treat
            // it as success; only `applied_generation` matching what it sent
            // is success.
            accepted: true,
            applied_generation: snapshot.generation_for(meeting),
            handler_id: self.handler_id.clone(),
            process_start_epoch_ms: self.process_start_epoch_ms,
            transport_mode: snapshot
                .transport_mode_for(meeting)
                .unwrap_or(TransportMode::Unspecified) as i32,
        }
    }
}

#[tonic::async_trait]
impl MediaHandlerService for MhMediaService {
    /// Register a meeting and apply its forwarding policy (ADR-0036 §8).
    #[instrument(skip_all)]
    async fn register_meeting(
        &self,
        request: Request<RegisterMeetingRequest>,
    ) -> Result<Response<RegisterMeetingResponse>, Status> {
        let req = request.into_inner();

        // -------------------------------------------------------------------
        // Pre-boundary checks: caller identity and reachability scalars only.
        //
        // These read `meeting_id` / `mc_id` / `mc_grpc_endpoint` and never
        // touch `egress_streams`, `selection_rules` or `policy_generation`, so
        // they are NOT policy outcomes and record no
        // `mh_media_policy_applies_total` sample. "This caller's endpoint is
        // malformed" and "MC's assignment computation produced a policy MH will
        // not apply" have different owners and different 3am remedies; sharing
        // a series would also make that counter's sum unusable as a
        // denominator.
        // -------------------------------------------------------------------
        if let Err(reason) = validate_caller_ids(&req.meeting_id, &req.mc_id) {
            metrics::record_grpc_request(GrpcMethod::RegisterMeeting, "error");
            return Err(Status::invalid_argument(reason));
        }
        if req.mc_grpc_endpoint.is_empty() {
            metrics::record_grpc_request(GrpcMethod::RegisterMeeting, "error");
            return Err(Status::invalid_argument("mc_grpc_endpoint is required"));
        }
        if req.mc_grpc_endpoint.len() > MAX_ENDPOINT_LENGTH {
            metrics::record_grpc_request(GrpcMethod::RegisterMeeting, "error");
            return Err(Status::invalid_argument(
                "mc_grpc_endpoint exceeds maximum length",
            ));
        }
        if !req.mc_grpc_endpoint.starts_with("http://")
            && !req.mc_grpc_endpoint.starts_with("https://")
            && !req.mc_grpc_endpoint.starts_with("grpc://")
        {
            metrics::record_grpc_request(GrpcMethod::RegisterMeeting, "error");
            return Err(Status::invalid_argument(
                "mc_grpc_endpoint must use http://, https://, or grpc:// scheme",
            ));
        }

        // -------------------------------------------------------------------
        // Policy boundary. Everything below reads policy-bearing fields, so
        // every terminal path from here records exactly one outcome.
        //
        // Validation runs BEFORE any state is touched: no upsert, no promotion,
        // no apply, no snapshot swap. A rejected registration therefore never
        // regresses live state — an already-registered meeting keeps its
        // registration, its installed snapshot and its active connections.
        // -------------------------------------------------------------------
        let policy = match MeetingPolicy::from_request(&req, &self.policy_limits) {
            Ok(policy) => policy,
            Err(rejection) => {
                metrics::record_media_policy_apply(PolicyApplyOutcome::RejectedInvalid);
                metrics::record_grpc_request(GrpcMethod::RegisterMeeting, "error");
                tracing::warn!(
                    target: "mh.grpc.service",
                    key_custody = KEY_CUSTODY_OPERATOR,
                    reason = rejection.reason(),
                    egress_stream_count = req.egress_streams.len(),
                    "Rejecting whole registration; live policy unchanged"
                );
                return Err(Status::invalid_argument(rejection.reason()));
            }
        };

        // The registration itself is accepted even when the policy will not be
        // installed: MH's handler upserts and drains/promotes pending
        // connections, and §8 relies on that to rescue clients sitting in the
        // provisional window after a restart.
        //
        // ORDER IS LOAD-BEARING for `EndMeeting`: this upsert runs after
        // validation and BEFORE the apply, so ANY validation-passing
        // registration rebinds the meeting's `mc_id` whatever its generation
        // outcome. A failover MC re-registering below the applied generation
        // (refused as stale) must still take over ownership, or it could never
        // release the meeting (`internal.proto`, `EndMeetingRequest`). Do not
        // move this behind `apply_policy`.
        let promoted = match self
            .session_manager
            .register_meeting(
                req.meeting_id.clone(),
                MeetingRegistration {
                    mc_id: req.mc_id.clone(),
                    mc_grpc_endpoint: req.mc_grpc_endpoint.clone(),
                    registered_at: Instant::now(),
                },
                self.policy_limits.max_registered_meetings,
            )
            .await
        {
            Ok(promoted) => promoted,
            // Refused whole: nothing upserted, promoted or applied, so this is
            // the ONLY policy outcome this registration records — never also
            // `no_generation` or an apply outcome — and the request is an error,
            // not a success. The actor logged the WARN with the counts.
            Err(RegisterRefusal::MeetingCapReached { held, cap }) => {
                metrics::record_media_policy_apply(PolicyApplyOutcome::RejectedMeetingCap);
                metrics::record_grpc_request(GrpcMethod::RegisterMeeting, "error");
                return Err(Status::resource_exhausted(format!(
                    "media handler holds its maximum of {cap} registered meetings \
                     ({held} held); new meeting refused"
                )));
            }
        };

        let declared_mode = policy.transport_mode;
        let received_generation = policy.generation;
        let meeting = policy.meeting.clone();

        let outcome = if received_generation == 0 {
            // ORDERING CONSTRAINT — DO NOT TURN THIS INTO A REJECTION HERE.
            //
            // `internal.proto` makes 0 invalid and requires MH to reject it,
            // but its precondition is MC emitting >= 1, which lands at **story
            // task 13**. Enforcing the rejection before that rejects EVERY
            // registration MC sends for the whole window: no meeting is ever
            // registered, every client is provisionally accepted and kicked at
            // the registration timeout — ADR-0036 §8's opening paragraph almost
            // verbatim, "a permanent media blackhole for that meeting until it
            // emptied, reached through an ordinary rolling deploy". Enforcing
            // this MUST out of order reintroduces the exact failure §8 exists
            // to eliminate, through the field added to prevent it.
            //
            // The obvious workaround — have MC send 1 now — is WRONG and is
            // recorded so it is not rediscovered as a fix: MH would apply
            // "1 = no edges", and task 13's first real assignment is also
            // naturally 1, so MH would no-op the real policy. That trades a
            // loud outage for a silent one.
            //
            // So: install nothing, advance nothing, reject nothing. The
            // registration still upserts and still promotes pending
            // connections. `applied_generation` stays at whatever is genuinely
            // installed — which for an already-programmed meeting is its prior
            // generation, NOT 0.
            if !policy.edges.is_empty() {
                tracing::warn!(
                    target: "mh.grpc.service",
                    key_custody = KEY_CUSTODY_OPERATOR,
                    egress_stream_count = policy.edges.len(),
                    "Registration carries policy but names no policy_generation; \
                     nothing installed (see internal.proto ordering constraint)"
                );
            }
            PolicyApplyOutcome::NoGeneration
        } else {
            self.session_manager
                .apply_policy(
                    policy,
                    self.policy_limits.max_total_egress_edges,
                    Duration::from_millis(self.policy_limits.policy_apply_timeout_ms),
                )
                .await
                .into()
        };

        let response = self.respond(&meeting);

        // ADR-0036 §8's two-ends-must-agree check, MH's half. The cautionary
        // precedent §8 cites is a capacity value advertised to GC and enforced
        // nowhere — declared in one place, assumed in another, verified
        // nowhere. Compared against what MH actually installed, never against
        // what the request asked for.
        //
        // Gated on `Applied`, and that gate is load-bearing rather than
        // tidiness. On every other outcome MH deliberately installed nothing,
        // so `applied_mode` is the PRIOR generation's mode (or `UNSPECIFIED`
        // when nothing was ever installed) and a "disagrees" line would be
        // comparing two different generations' modes and reporting the
        // difference as a contract fault. That fires on exactly the paths an
        // operator is already reading logs on — mailbox-full, apply-timeout,
        // stale re-assert, and the whole task-11→13 gen-0 window — naming a
        // transport-mode disagreement as the cause of an incident whose real
        // cause is one WARN above it, with a completely different remedy. Each
        // of those outcomes reports itself; this line exists for the one case
        // nothing else can see.
        //
        // What survives the gate is the case worth the noise: an idempotent
        // re-assert at an ALREADY-INSTALLED generation whose content changed
        // transport mode. MH honours the generation and does not swap
        // (ADR-0036 §8), so it reports `applied` while running a different mode
        // from the one MC just declared — a genuine two-ends divergence that no
        // outcome label distinguishes. A fresh install cannot reach here: it
        // installs the mode it just parsed.
        let applied_mode =
            TransportMode::try_from(response.transport_mode).unwrap_or(TransportMode::Unspecified);
        if outcome == PolicyApplyOutcome::Applied {
            if let Some(declared) = declared_mode {
                if declared != applied_mode {
                    tracing::warn!(
                        target: "mh.grpc.service",
                        key_custody = KEY_CUSTODY_OPERATOR,
                        declared_transport_mode = declared.as_str_name(),
                        applied_transport_mode = applied_mode.as_str_name(),
                        received_generation,
                        applied_generation = response.applied_generation,
                        "MC declared a transport mode that disagrees with the one MH \
                         has installed at this generation"
                    );
                }
            }
        }

        metrics::record_media_policy_apply(outcome);
        metrics::record_grpc_request(GrpcMethod::RegisterMeeting, "success");

        tracing::info!(
            target: "mh.grpc.service",
            key_custody = KEY_CUSTODY_OPERATOR,
            meeting_id = %req.meeting_id,
            mc_id = %req.mc_id,
            promoted_pending_count = promoted.len(),
            policy_apply_outcome = outcome.as_label(),
            received_generation,
            applied_generation = response.applied_generation,
            "Meeting registered"
        );

        Ok(Response::new(response))
    }

    /// Release a meeting's handler resources (story 2 R-20).
    ///
    /// The normative semantics are on `EndMeetingRequest` in
    /// `proto/dark_tower/internal/v1/internal.proto`; this implements them:
    ///
    /// - unknown or already-released meeting -> `acknowledged: true` (MC calls
    ///   every handler in the assigned set, including ones never registered);
    /// - `mc_id` equal to the registering MC's -> release, `acknowledged: true`;
    /// - `mc_id` MISMATCH -> `FAILED_PRECONDITION`. Never an idempotent accept
    ///   and never `acknowledged: false` (the decoded default, ambiguous with
    ///   the no-op). NOT `PERMISSION_DENIED`: `mc_id` is self-asserted and is
    ///   not an authorization control, and `PERMISSION_DENIED` is what
    ///   `MhAuthLayer` returns for a real authorization rejection.
    ///
    /// Channel authorization is `MhAuthLayer`'s existing two-layer gate (scope
    /// `service.write.mh`, caller `service_type` = meeting-controller for this
    /// service's path prefix) — this method adds no auth of its own.
    ///
    /// Every exit records exactly one `mh_grpc_requests_total{method=
    /// "end_meeting"}` sample, and every exit the actor decided records exactly
    /// one teardown outcome (counted by the actor).
    #[instrument(skip_all)]
    async fn end_meeting(
        &self,
        request: Request<EndMeetingRequest>,
    ) -> Result<Response<EndMeetingResponse>, Status> {
        let req = request.into_inner();

        // Pre-decision checks, before the ids reach the actor or any log line —
        // the SAME validator `register_meeting` uses.
        if let Err(reason) = validate_caller_ids(&req.meeting_id, &req.mc_id) {
            metrics::record_grpc_request(GrpcMethod::EndMeeting, "error");
            return Err(Status::invalid_argument(reason));
        }

        let decided = self
            .session_manager
            .end_meeting(
                req.meeting_id,
                req.mc_id,
                Duration::from_millis(self.policy_limits.policy_apply_timeout_ms),
            )
            .await;

        match decided {
            Ok(MeetingTeardownOutcome::Released | MeetingTeardownOutcome::UnknownMeeting) => {
                metrics::record_grpc_request(GrpcMethod::EndMeeting, "success");
                Ok(Response::new(EndMeetingResponse { acknowledged: true }))
            }
            Ok(MeetingTeardownOutcome::RejectedOwnership) => {
                metrics::record_grpc_request(GrpcMethod::EndMeeting, "error");
                // Generic on the wire: the registered `mc_id` is in the
                // server-side WARN only. Echoing it would hand a scoped-but-wrong
                // caller the exact string a matching forged release needs.
                Err(Status::failed_precondition(
                    "meeting is registered to a different mc_id; re-register before ending it",
                ))
            }
            Err(failure) => {
                metrics::record_grpc_request(GrpcMethod::EndMeeting, "error");
                tracing::warn!(
                    target: "mh.grpc.service",
                    key_custody = KEY_CUSTODY_OPERATOR,
                    reason = failure.reason(),
                    "EndMeeting not decided; nothing released on this attempt"
                );
                Err(match failure {
                    EndMeetingFailure::ActorUnavailable => {
                        Status::unavailable("media handler session actor unavailable")
                    }
                    EndMeetingFailure::Timeout => {
                        Status::unavailable("media handler did not decide EndMeeting in time")
                    }
                })
            }
        }
    }
}

/// The caller-identity id checks, shared by BOTH `MediaHandlerService` RPCs so the
/// rule — presence and [`MAX_ID_LENGTH`] — has one home rather than two copies
/// held together by a comment. Tightening it (byte vs char length, a charset)
/// then reaches the teardown RPC too, which is exactly where a stale or
/// misrouted caller's traffic arrives.
///
/// Reads no policy-bearing field, so a failure is PRE-boundary and records no
/// `mh_media_policy_applies_total` sample; the caller records its own
/// `mh_grpc_requests_total` error, because the `method` label differs.
///
/// Returns the `INVALID_ARGUMENT` message, not a `Status`: the caller builds the
/// status.
fn validate_caller_ids(meeting_id: &str, mc_id: &str) -> Result<(), String> {
    for (field, value) in [("meeting_id", meeting_id), ("mc_id", mc_id)] {
        if value.is_empty() {
            return Err(format!("{field} is required"));
        }
        if value.len() > MAX_ID_LENGTH {
            return Err(format!("{field} exceeds maximum length"));
        }
    }
    Ok(())
}

/// Bridge the actor's outcome to the metric's bounded label set.
///
/// Two enums rather than one because they answer different questions: the actor
/// reports what it *did with the policy*, the metric reports *where the
/// registration ended up* — including the two outcomes the actor never sees,
/// `RejectedInvalid` (refused before the actor) and `NoGeneration` (never sent
/// to the actor).
impl From<ApplyOutcome> for PolicyApplyOutcome {
    fn from(outcome: ApplyOutcome) -> Self {
        match outcome {
            ApplyOutcome::Applied => Self::Applied,
            ApplyOutcome::RejectedStale => Self::RejectedStale,
            // A capacity refusal is not an MH fault: its own outcome, so the
            // deploy bake gate (which alerts on `apply_failed`) does not abort
            // on expected capacity pressure. Listed explicitly, before the
            // catch-all, so a new capacity-shaped failure is a decision.
            ApplyOutcome::Failed(ApplyFailure::StreamCeilingExceeded) => {
                Self::RejectedStreamCeiling
            }
            ApplyOutcome::Failed(
                ApplyFailure::MailboxFull
                | ApplyFailure::Timeout
                | ApplyFailure::ActorUnavailable
                | ApplyFailure::EdgeCapExceeded,
            ) => Self::ApplyFailed,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::routing::MeetingKey;
    use crate::session::{PendingConnection, SessionManagerActor};
    // One fixture home for all three test sites that build these messages —
    // see `mh_test_utils::media_policy` for why.
    use mh_test_utils::media_policy::{egress, register_request};
    use proto_gen::dark_tower::internal::v1::{CandidateSource, EgressStream, MutedSource};

    use std::sync::Arc;

    const TEST_HANDLER_ID: &str = "mh-test-handler";
    const TEST_EPOCH_MS: u64 = 1_700_000_000_000;

    fn make_service() -> (MhMediaService, SessionManagerHandle) {
        make_service_with_limits(PolicyLimits::for_tests())
    }

    fn make_service_with_limits(limits: PolicyLimits) -> (MhMediaService, SessionManagerHandle) {
        let sm = SessionManagerHandle::new(crate::session::admission::test_admission());
        let svc = MhMediaService::new(
            sm.clone(),
            TEST_HANDLER_ID.to_string(),
            TEST_EPOCH_MS,
            limits,
        );
        (svc, sm)
    }

    /// A service whose actor is built but **never spawned**.
    ///
    /// The config-apply consumer is then *structurally absent* rather than
    /// merely slow, which is what makes the mailbox-full and apply-timeout
    /// gates deterministic instead of a race. The actor is returned so the
    /// caller can drop or run it explicitly.
    fn make_service_with_unspawned_actor(
        limits: PolicyLimits,
    ) -> (MhMediaService, SessionManagerHandle, SessionManagerActor) {
        let (sm, actor) =
            SessionManagerHandle::new_with_parts(crate::session::admission::test_admission());
        let svc = MhMediaService::new(
            sm.clone(),
            TEST_HANDLER_ID.to_string(),
            TEST_EPOCH_MS,
            limits,
        );
        (svc, sm, actor)
    }

    fn policy_for(meeting: &str, generation: u64, streams: Vec<EgressStream>) -> MeetingPolicy {
        MeetingPolicy::from_request(
            &register_request(meeting, generation, streams),
            &PolicyLimits::for_tests(),
        )
        .unwrap()
    }

    fn make_register_request(
        meeting_id: &str,
        mc_id: &str,
        mc_grpc_endpoint: &str,
    ) -> Request<RegisterMeetingRequest> {
        Request::new(RegisterMeetingRequest {
            meeting_id: meeting_id.to_string(),
            mc_id: mc_id.to_string(),
            mc_grpc_endpoint: mc_grpc_endpoint.to_string(),
            egress_streams: Vec::new(),
            selection_rules: None,
            policy_generation: 0,
            server_muted_sources: Vec::new(),
        })
    }

    fn make_policy_request(
        meeting_id: &str,
        generation: u64,
        streams: Vec<EgressStream>,
    ) -> Request<RegisterMeetingRequest> {
        Request::new(register_request(meeting_id, generation, streams))
    }

    // =======================================================================
    // Story 2 R-20: EndMeeting
    // =======================================================================

    fn end_request(meeting_id: &str, mc_id: &str) -> Request<EndMeetingRequest> {
        Request::new(EndMeetingRequest {
            meeting_id: meeting_id.to_string(),
            mc_id: mc_id.to_string(),
        })
    }

    /// Register `meeting_id` under `mc_id` at `generation` with `streams`.
    fn owned_policy_request(
        meeting_id: &str,
        mc_id: &str,
        generation: u64,
        streams: Vec<EgressStream>,
    ) -> Request<RegisterMeetingRequest> {
        let mut req = register_request(meeting_id, generation, streams);
        req.mc_id = mc_id.to_string();
        Request::new(req)
    }

    /// The matching `mc_id` releases EVERYTHING the meeting holds: routes gone,
    /// edges back, generation forgotten, registration gone, its server-muted
    /// set gone with its routes, and its active connections CLOSED and removed
    /// — acknowledged, with the teardown counter and the method label selecting
    /// `released` / `end_meeting`.
    ///
    /// Each released thing is first shown to EXIST (the connection registered
    /// and open, the mute installed), so no "gone" assertion can pass because
    /// the setup never put it there.
    #[tokio::test]
    async fn end_meeting_from_the_registering_mc_releases_everything() {
        use common::observability::testing::MetricAssertion;
        // Snapshot BEFORE building the service: the teardown counters are
        // handles the actor resolves at construction, bound to whichever
        // recorder is current then. A snapshot taken afterwards would read 0
        // whatever the actor did.
        let snap = MetricAssertion::snapshot();
        let (svc, sm) = make_service();
        let key = MeetingKey::new("m-1");
        let muted = crate::routing::SenderId::from_wire(6).unwrap();
        let mut registration = owned_policy_request(
            "m-1",
            "mc-a",
            5,
            vec![egress(1, 5, 0, 6), egress(2, 6, 0, 5)],
        );
        registration.get_mut().server_muted_sources = vec![MutedSource { sender_id: 6 }];
        svc.register_meeting(registration).await.unwrap();

        // An ACTIVE connection on the meeting, whose close token we keep.
        let close = tokio_util::sync::CancellationToken::new();
        sm.add_connection(
            "m-1",
            crate::session::ConnectionEntry {
                connection_id: "conn-1".to_string(),
                participant_id: "user-1".to_string(),
                connected_at: Instant::now(),
                close: close.clone(),
            },
        )
        .await;

        // PRECONDITIONS: everything the release must remove is present.
        let before = sm.routing_snapshot();
        assert_eq!(before.total_edges(), 2);
        assert!(
            before.is_server_muted(&key, muted),
            "control: mute installed"
        );
        assert_eq!(sm.active_connection_count().await, 1, "control: connected");
        assert!(!close.is_cancelled(), "control: connection open");

        let resp = svc
            .end_meeting(end_request("m-1", "mc-a"))
            .await
            .unwrap()
            .into_inner();

        assert!(resp.acknowledged);
        let after = sm.routing_snapshot();
        assert_eq!(after.total_edges(), 0, "edges return to the budget");
        assert!(after.routes_for(&key).is_none());
        assert_eq!(after.generation_for(&key), 0);
        assert!(!sm.is_meeting_registered("m-1").await);
        // The meeting-keyed state beyond routes (task 12 item 3).
        assert!(
            !after.is_server_muted(&key, muted),
            "the muted set goes with the meeting's routes"
        );
        assert!(close.is_cancelled(), "the active connection is closed");
        assert_eq!(
            sm.active_connection_count().await,
            0,
            "the active-connection entry is removed"
        );
        snap.counter("mh_media_meeting_teardowns_total")
            .with_labels(&[("outcome", "released"), ("key_custody", "operator")])
            .assert_delta(1);
        snap.counter("mh_grpc_requests_total")
            .with_labels(&[("method", "end_meeting"), ("status", "success")])
            .assert_delta(1);
        // Exactly the ONE setup registration: the teardown was not also
        // counted as a `RegisterMeeting` receipt.
        snap.counter("mh_grpc_requests_total")
            .with_labels(&[("method", "register_meeting"), ("status", "success")])
            .assert_delta(1);
    }

    /// A different `mc_id` is refused with `FAILED_PRECONDITION` — never
    /// `PERMISSION_DENIED`, never an ack — the status does not echo the owner's
    /// id, and NOTHING is released.
    #[tokio::test]
    async fn end_meeting_from_a_different_mc_is_refused_and_releases_nothing() {
        use common::observability::testing::MetricAssertion;
        // Snapshot BEFORE building the service: the teardown counters are
        // handles the actor resolves at construction, bound to whichever
        // recorder is current then. A snapshot taken afterwards would read 0
        // whatever the actor did.
        let snap = MetricAssertion::snapshot();
        let (svc, sm) = make_service();
        svc.register_meeting(owned_policy_request(
            "m-1",
            "mc-a",
            5,
            vec![egress(1, 5, 0, 6)],
        ))
        .await
        .unwrap();
        let before = sm.routing_snapshot();

        let status = svc
            .end_meeting(end_request("m-1", "mc-b"))
            .await
            .expect_err("a mismatch must never be acknowledged");

        assert_eq!(status.code(), tonic::Code::FailedPrecondition);
        assert_ne!(status.code(), tonic::Code::PermissionDenied);
        assert!(
            !status.message().contains("mc-a"),
            "the status must not disclose the registering mc_id: {}",
            status.message()
        );
        assert!(Arc::ptr_eq(&before, &sm.routing_snapshot()), "no swap");
        assert!(sm.is_meeting_registered("m-1").await);
        snap.counter("mh_media_meeting_teardowns_total")
            .with_labels(&[
                ("outcome", "rejected_ownership"),
                ("key_custody", "operator"),
            ])
            .assert_delta(1);
        snap.counter("mh_media_meeting_teardowns_total")
            .with_labels(&[("outcome", "released"), ("key_custody", "operator")])
            .assert_delta(0);
        snap.counter("mh_grpc_requests_total")
            .with_labels(&[("method", "end_meeting"), ("status", "error")])
            .assert_delta(1);
    }

    /// Unknown and already-released meetings are an acknowledged no-op,
    /// distinguishable from both the release and the reject.
    #[tokio::test]
    async fn end_meeting_for_an_unknown_or_released_meeting_is_an_acknowledged_no_op() {
        use common::observability::testing::MetricAssertion;
        // Snapshot BEFORE building the service: the teardown counters are
        // handles the actor resolves at construction, bound to whichever
        // recorder is current then. A snapshot taken afterwards would read 0
        // whatever the actor did.
        let snap = MetricAssertion::snapshot();
        let (svc, _sm) = make_service();
        svc.register_meeting(owned_policy_request("m-1", "mc-a", 1, vec![]))
            .await
            .unwrap();
        svc.end_meeting(end_request("m-1", "mc-a")).await.unwrap();

        for meeting in ["never-registered", "m-1"] {
            let resp = svc
                .end_meeting(end_request(meeting, "mc-a"))
                .await
                .unwrap()
                .into_inner();
            assert!(resp.acknowledged, "{meeting}: must acknowledge");
        }
        snap.counter("mh_media_meeting_teardowns_total")
            .with_labels(&[("outcome", "unknown_meeting"), ("key_custody", "operator")])
            .assert_delta(2);
        // The setup's one real release, and no more: the two no-ops did not
        // count as releases.
        snap.counter("mh_media_meeting_teardowns_total")
            .with_labels(&[("outcome", "released"), ("key_custody", "operator")])
            .assert_delta(1);
        snap.counter("mh_media_meeting_teardowns_total")
            .with_labels(&[
                ("outcome", "rejected_ownership"),
                ("key_custody", "operator"),
            ])
            .assert_delta(0);
    }

    /// The MC-restart / failover case: a new `mc_id` re-registers at a STALE
    /// generation (refused as stale — no apply), yet that registration still
    /// rebinds ownership, so the new MC can release. If the rebind were gated
    /// on the generation outcome the meeting would be stranded forever.
    #[tokio::test]
    async fn a_stale_generation_reregistration_still_rebinds_ownership_for_release() {
        let (svc, sm) = make_service();
        svc.register_meeting(owned_policy_request(
            "m-1",
            "mc-old",
            9,
            vec![egress(1, 5, 0, 6)],
        ))
        .await
        .unwrap();

        let stale = svc
            .register_meeting(owned_policy_request(
                "m-1",
                "mc-new",
                1,
                vec![egress(1, 5, 0, 6)],
            ))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(
            stale.applied_generation, 9,
            "the stale policy did not apply"
        );

        assert_eq!(
            svc.end_meeting(end_request("m-1", "mc-old"))
                .await
                .unwrap_err()
                .code(),
            tonic::Code::FailedPrecondition,
            "the previous owner no longer owns it"
        );
        assert!(
            svc.end_meeting(end_request("m-1", "mc-new"))
                .await
                .unwrap()
                .into_inner()
                .acknowledged
        );
        assert!(!sm.is_meeting_registered("m-1").await);
    }

    /// Release forgets the applied generation, so a same-id meeting re-created
    /// at generation 1 is a FRESH INSTALL — exactly as on a fresh pod — not a
    /// `rejected_stale` blackhole.
    #[tokio::test]
    async fn release_then_reregister_at_generation_one_installs_fresh() {
        let (svc, _sm) = make_service();
        svc.register_meeting(owned_policy_request(
            "m-1",
            "mc-a",
            7,
            vec![egress(1, 5, 0, 6)],
        ))
        .await
        .unwrap();
        svc.end_meeting(end_request("m-1", "mc-a")).await.unwrap();

        let resp = svc
            .register_meeting(owned_policy_request(
                "m-1",
                "mc-a",
                1,
                vec![egress(1, 5, 0, 6)],
            ))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(resp.applied_generation, 1);
    }

    /// Malformed ids are refused before the actor decides anything: an
    /// `end_meeting` error, and NO teardown outcome.
    #[tokio::test]
    async fn end_meeting_rejects_malformed_ids_without_a_teardown_outcome() {
        use common::observability::testing::MetricAssertion;
        // Snapshot BEFORE building the service: the teardown counters are
        // handles the actor resolves at construction, bound to whichever
        // recorder is current then. A snapshot taken afterwards would read 0
        // whatever the actor did.
        let snap = MetricAssertion::snapshot();
        let (svc, _sm) = make_service();
        let long = "x".repeat(MAX_ID_LENGTH + 1);
        for (meeting, mc) in [
            ("", "mc"),
            ("m", ""),
            (long.as_str(), "mc"),
            ("m", long.as_str()),
        ] {
            assert_eq!(
                svc.end_meeting(end_request(meeting, mc))
                    .await
                    .unwrap_err()
                    .code(),
                tonic::Code::InvalidArgument
            );
        }
        snap.counter("mh_grpc_requests_total")
            .with_labels(&[("method", "end_meeting"), ("status", "error")])
            .assert_delta(4);
        // POSITIVE CONTROL for the zeros below: one well-formed call on the
        // same service DOES reach the actor and IS counted, so the zeros are
        // not an artefact of handles bound to no recorder.
        svc.end_meeting(end_request("unknown", "mc")).await.unwrap();
        for outcome in MeetingTeardownOutcome::ALL {
            let expected = u64::from(outcome == MeetingTeardownOutcome::UnknownMeeting);
            snap.counter("mh_media_meeting_teardowns_total")
                .with_labels(&[("outcome", outcome.as_label()), ("key_custody", "operator")])
                .assert_delta(expected);
        }
    }

    /// A wedged actor yields UNAVAILABLE within the bound, never a hung RPC,
    /// and records no teardown outcome (nothing was decided). Deterministic
    /// because the actor is built and never spawned.
    #[tokio::test]
    async fn end_meeting_against_an_unreachable_actor_is_bounded_unavailable() {
        let limits = PolicyLimits {
            policy_apply_timeout_ms: 5,
            ..PolicyLimits::for_tests()
        };
        let (svc, _sm, actor) = make_service_with_unspawned_actor(limits);
        let status = svc
            .end_meeting(end_request("m-1", "mc-a"))
            .await
            .unwrap_err();
        assert_eq!(status.code(), tonic::Code::Unavailable);
        drop(actor);
    }

    // =======================================================================
    // Story 2 R-21: the registered-meeting cap
    // =======================================================================

    /// A NEW meeting past the cap is refused loudly (`RESOURCE_EXHAUSTED`),
    /// counted under exactly ONE policy outcome (`rejected_meeting_cap`) and a
    /// `register_meeting` ERROR — not the bottom `success`, and not a second
    /// outcome — and nothing is upserted.
    #[tokio::test]
    async fn a_new_meeting_past_the_cap_is_refused_and_counted_once() {
        use common::observability::testing::MetricAssertion;
        let (svc, sm) = make_service_with_limits(PolicyLimits {
            max_registered_meetings: 2,
            ..PolicyLimits::for_tests()
        });
        for meeting in ["m-1", "m-2"] {
            svc.register_meeting(owned_policy_request(meeting, "mc-a", 1, vec![]))
                .await
                .unwrap();
        }

        let snap = MetricAssertion::snapshot();
        let status = svc
            .register_meeting(owned_policy_request("m-3", "mc-a", 1, vec![]))
            .await
            .unwrap_err();

        assert_eq!(status.code(), tonic::Code::ResourceExhausted);
        assert!(!sm.is_meeting_registered("m-3").await);
        snap.counter("mh_media_policy_applies_total")
            .with_labels(&[
                ("outcome", "rejected_meeting_cap"),
                ("key_custody", "operator"),
            ])
            .assert_delta(1);
        for other in PolicyApplyOutcome::ALL {
            if other != PolicyApplyOutcome::RejectedMeetingCap {
                snap.counter("mh_media_policy_applies_total")
                    .with_labels(&[("outcome", other.as_label()), ("key_custody", "operator")])
                    .assert_delta(0);
            }
        }
        snap.counter("mh_grpc_requests_total")
            .with_labels(&[("method", "register_meeting"), ("status", "error")])
            .assert_delta(1);
        snap.counter("mh_grpc_requests_total")
            .with_labels(&[("method", "register_meeting"), ("status", "success")])
            .assert_delta(0);
    }

    /// At the cap, a re-registration of an ALREADY-HELD meeting is admitted:
    /// refusing it would stop a full handler taking policy for meetings it is
    /// already serving. And a release frees a slot.
    #[tokio::test]
    async fn at_the_cap_held_meetings_still_reassert_and_release_frees_a_slot() {
        let (svc, _sm) = make_service_with_limits(PolicyLimits {
            max_registered_meetings: 1,
            ..PolicyLimits::for_tests()
        });
        svc.register_meeting(owned_policy_request(
            "m-1",
            "mc-a",
            1,
            vec![egress(1, 5, 0, 6)],
        ))
        .await
        .unwrap();

        let reassert = svc
            .register_meeting(owned_policy_request(
                "m-1",
                "mc-a",
                2,
                vec![egress(1, 5, 0, 6)],
            ))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(reassert.applied_generation, 2);

        assert_eq!(
            svc.register_meeting(owned_policy_request("m-2", "mc-a", 1, vec![]))
                .await
                .unwrap_err()
                .code(),
            tonic::Code::ResourceExhausted
        );
        svc.end_meeting(end_request("m-1", "mc-a")).await.unwrap();
        assert!(svc
            .register_meeting(owned_policy_request("m-2", "mc-a", 1, vec![]))
            .await
            .is_ok());
    }

    // =======================================================================
    // ADR-0036 §10 Tier-1b control-plane gates
    // =======================================================================

    /// **The negative half of the cadence-convergence gate** — §10's "one gate
    /// in the set that catches a false green": inject an apply failure and
    /// assert the acknowledgement does not advance.
    ///
    /// Driven by the aggregate egress-edge bound: a static integer threshold
    /// with a live actor over the full handler path, so there is no clock, no
    /// timer and no race.
    ///
    /// **Also a deliberately-labelled BACKSTOP test for that bound.** Since
    /// story 2 task 8 the stream-ceiling admission check runs first and config
    /// load refuses a ceiling above the edge bound, so in production this branch
    /// is unreachable. This service is built with a never-binding admission
    /// (ceiling far ABOVE the edge bound of 2) — a state `from_vars` refuses —
    /// precisely so the defense-in-depth branch stays exercised. Do not
    /// "retarget" it onto the ceiling; the ceiling has its own tests. Asserted on the acknowledged generation **in the
    /// response** rather than on MH internals, so it survives refactoring.
    #[tokio::test]
    async fn apply_failure_does_not_advance_the_acknowledged_generation() {
        let limits = PolicyLimits {
            max_total_egress_edges: 2,
            ..PolicyLimits::for_tests()
        };
        let (svc, sm) = make_service_with_limits(limits);

        // Generation 5 installs and is acknowledged.
        let ok = svc
            .register_meeting(make_policy_request(
                "m-1",
                5,
                vec![egress(1, 5, 0, 5), egress(2, 6, 0, 6)],
            ))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(ok.applied_generation, 5);
        let snapshot_before = sm.routing_snapshot();

        // Generation 6 exceeds the aggregate bound, so nothing installs.
        let failed = svc
            .register_meeting(make_policy_request(
                "m-1",
                6,
                vec![egress(1, 5, 0, 5), egress(2, 6, 0, 6), egress(3, 7, 0, 7)],
            ))
            .await
            .unwrap()
            .into_inner();

        // The whole point: the echo must NOT advance to 6.
        assert_eq!(
            failed.applied_generation, 5,
            "a failed apply must report the generation the live forward path still reflects; \
             advancing here is the partial-blackhole-reporting-healthy bug the field exists to \
             catch"
        );
        assert!(failed.accepted, "the registration was received and parsed");
        assert!(
            Arc::ptr_eq(&snapshot_before, &sm.routing_snapshot()),
            "a failed apply must leave the live snapshot untouched — no partial install"
        );
    }

    /// Mailbox-full and apply-timeout are deterministic because **nothing
    /// exists that could drain the mailbox or answer the reply** — not because
    /// the test out-raced a live consumer.
    ///
    /// The actor is built and deliberately never spawned. With a live actor
    /// these two paths are unreachable in either direction: under paused time
    /// the runtime never goes idle while the actor has queued work, so the
    /// clock never auto-advances and the reply always wins; and advancing the
    /// clock manually just races the actor. A gate that depends on winning a
    /// race is a flake, so the consumer is removed instead.
    #[tokio::test]
    async fn apply_failure_is_reachable_when_the_config_mailbox_cannot_drain() {
        use crate::session::CONFIG_APPLY_CHANNEL_BUFFER;

        let (_svc, sm, actor) = make_service_with_unspawned_actor(PolicyLimits::for_tests());
        let limits = PolicyLimits::for_tests();
        let timeout = Duration::from_millis(5);

        // The first send occupies a slot permanently — no consumer exists to
        // free one — so it necessarily times out rather than replying.
        let first = sm
            .apply_policy(
                policy_for("m-fill", 1, vec![egress(1, 5, 0, 5)]),
                limits.max_total_egress_edges,
                timeout,
            )
            .await;
        assert_eq!(
            first,
            ApplyOutcome::Failed(ApplyFailure::Timeout),
            "with no actor to answer, the bounded await must elapse rather than hang"
        );

        // Fill the remaining capacity, then one more: that one cannot be
        // enqueued at all.
        for generation in 2..=u64::try_from(CONFIG_APPLY_CHANNEL_BUFFER).unwrap() {
            let _ = sm
                .apply_policy(
                    policy_for("m-fill", generation, vec![egress(1, 5, 0, 5)]),
                    limits.max_total_egress_edges,
                    timeout,
                )
                .await;
        }
        let overflow = sm
            .apply_policy(
                policy_for("m-fill", 999, vec![egress(1, 5, 0, 5)]),
                limits.max_total_egress_edges,
                timeout,
            )
            .await;
        assert_eq!(
            overflow,
            ApplyOutcome::Failed(ApplyFailure::MailboxFull),
            "a saturated config-apply mailbox must fail fast, never block the handler"
        );

        // Nothing was installed on any of those paths. Read off the snapshot,
        // which is lock-free and needs no actor — the same property that lets
        // the RPC answer truthfully while the actor is wedged.
        assert_eq!(
            sm.routing_snapshot()
                .generation_for(&MeetingKey::new("m-fill")),
            0,
            "nothing was ever installed, so 'nothing applied' is the truth"
        );

        // ADR-0036 §8's separation requirement, asserted rather than assumed:
        // the config-apply mailbox is saturated and the connection-lifecycle
        // mailbox is at FULL headroom. A single shared mailbox would show both
        // at zero, and connection handling would be starved by a policy tick.
        assert_eq!(
            sm.config_apply_capacity(),
            0,
            "config-apply mailbox is full"
        );
        assert_eq!(
            sm.lifecycle_capacity(),
            crate::session::SESSION_CHANNEL_BUFFER,
            "a saturated config-apply mailbox must not consume lifecycle capacity"
        );
        drop(actor);
    }

    /// Generation monotonicity — a stale lower generation is ignored.
    #[tokio::test]
    async fn stale_lower_generation_is_ignored_and_does_not_roll_policy_back() {
        let (svc, sm) = make_service();

        svc.register_meeting(make_policy_request("m-1", 9, vec![egress(1, 5, 0, 5)]))
            .await
            .unwrap();
        let snapshot_before = sm.routing_snapshot();

        let resp = svc
            .register_meeting(make_policy_request(
                "m-1",
                4,
                vec![egress(1, 6, 1, 6), egress(2, 7, 2, 7)],
            ))
            .await
            .unwrap()
            .into_inner();

        assert_eq!(
            resp.applied_generation, 9,
            "reordered or retried delivery must not roll policy back"
        );
        assert!(
            Arc::ptr_eq(&snapshot_before, &sm.routing_snapshot()),
            "a stale generation must not swap the snapshot"
        );
    }

    /// Re-assert idempotency — an identical re-assert is a **true data-plane
    /// no-op**: no snapshot swap and no connection churn.
    ///
    /// Without this, §8's <=10 s cadence produces churn at exactly the cadence
    /// interval — periodic media glitches wearing a configuration disguise.
    #[tokio::test]
    async fn identical_reassert_is_a_true_data_plane_no_op() {
        let (svc, sm) = make_service();

        sm.add_pending_connection(PendingConnection {
            connection_id: "conn-1".to_string(),
            meeting_id: "m-1".to_string(),
            participant_id: "user-1".to_string(),
            connected_at: Instant::now(),
            close: tokio_util::sync::CancellationToken::new(),
        })
        .await;

        let streams = vec![egress(1, 5, 0, 5)];
        svc.register_meeting(make_policy_request("m-1", 3, streams.clone()))
            .await
            .unwrap();

        let snapshot_before = sm.routing_snapshot();
        let connections_before = sm.active_connection_count().await;

        let resp = svc
            .register_meeting(make_policy_request("m-1", 3, streams))
            .await
            .unwrap()
            .into_inner();

        assert_eq!(resp.applied_generation, 3);
        assert!(
            Arc::ptr_eq(&snapshot_before, &sm.routing_snapshot()),
            "an identical re-assert must not swap the snapshot"
        );
        assert_eq!(
            sm.active_connection_count().await,
            connections_before,
            "an identical re-assert must not churn connections"
        );
    }

    /// **The contract-violation detector's positive case.** ADR-0036 §8
    /// guarantees an unchanged `policy_generation` means unchanged policy, and
    /// nothing on MH's side can enforce that on MC — so MH honours the
    /// generation (no swap) and logs loudly. Both halves matter and only the
    /// no-swap half is machine-checkable here.
    ///
    /// Without this, the differing-content sub-path of the `==` arm has zero
    /// coverage: `identical_reassert_is_a_true_data_plane_no_op` only ever
    /// takes the identical branch, so a broken `edges()` comparison or a
    /// broken `routes_for` lookup would make the detector silently never fire,
    /// and an implementation that *swapped* on difference — which is the
    /// intuitive thing to write, and which would churn the data plane at the
    /// cadence interval — would also pass every other test in this file.
    #[tokio::test]
    async fn reassert_at_an_equal_generation_with_different_content_does_not_swap() {
        let (svc, sm) = make_service();

        svc.register_meeting(make_policy_request("m-1", 3, vec![egress(1, 5, 0, 5)]))
            .await
            .unwrap();

        let snapshot_before = sm.routing_snapshot();
        let meeting = MeetingKey::new("m-1");
        assert_eq!(
            snapshot_before.routes_for(&meeting).map(|r| r.edge_count()),
            Some(1)
        );

        // Same generation, genuinely different policy.
        let resp = svc
            .register_meeting(make_policy_request(
                "m-1",
                3,
                vec![egress(1, 5, 0, 5), egress(2, 6, 0, 6)],
            ))
            .await
            .unwrap()
            .into_inner();

        let snapshot_after = sm.routing_snapshot();
        assert!(
            Arc::ptr_eq(&snapshot_before, &snapshot_after),
            "an equal-generation re-assert must not swap even when content differs"
        );
        assert_eq!(
            resp.applied_generation, 3,
            "the echo reports the live generation, which is unchanged"
        );
        assert_eq!(
            snapshot_after.routes_for(&meeting).map(|r| r.edge_count()),
            Some(1),
            "the INSTALLED policy is still the first one; the second was not applied"
        );
    }

    /// The same case one field over: an equal-generation re-assert that
    /// changes `transport_mode`.
    ///
    /// This is the one path on which the §8 two-ends-must-agree WARN can fire,
    /// and it is the reason that WARN is gated on `Applied` rather than
    /// emitted whenever the request's mode differs from the echo. The
    /// machine-checkable half is the echo: `transport_mode` must report what
    /// MH has INSTALLED, never what the request declared. Echoing the request
    /// would make MC's comparison agree with itself and the divergence
    /// undetectable at both ends simultaneously.
    #[tokio::test]
    async fn reassert_at_an_equal_generation_echoes_the_installed_transport_mode() {
        let (svc, sm) = make_service();

        svc.register_meeting(make_policy_request("m-1", 3, vec![egress(1, 5, 0, 5)]))
            .await
            .unwrap();
        let snapshot_before = sm.routing_snapshot();

        let mut changed = egress(1, 5, 0, 5);
        changed.transport_mode = TransportMode::StreamPerGroup as i32;
        let resp = svc
            .register_meeting(make_policy_request("m-1", 3, vec![changed]))
            .await
            .unwrap()
            .into_inner();

        assert_eq!(
            resp.transport_mode,
            TransportMode::Datagram as i32,
            "the echo must report the INSTALLED mode, never the requested one"
        );
        assert_eq!(resp.applied_generation, 3);
        assert!(
            Arc::ptr_eq(&snapshot_before, &sm.routing_snapshot()),
            "a transport-mode change at an unchanged generation must not swap"
        );
        assert_eq!(
            sm.routing_snapshot()
                .transport_mode_for(&MeetingKey::new("m-1")),
            Some(TransportMode::Datagram)
        );
    }

    /// The re-assert double-count guard: an identical re-assert at the
    /// aggregate bound must not trip it.
    ///
    /// A check written as `total + new > bound` instead of
    /// `total - own_current + new > bound` passes every single-meeting test
    /// that registers once, then turns §8's own cadence into a rotating apply
    /// failure once the handler is half full.
    ///
    /// Backstop test for the edge bound (see
    /// `apply_failure_does_not_advance_the_acknowledged_generation`); the
    /// stream-ceiling twin of this property is
    /// `reassert_at_the_stream_ceiling_is_admitted_not_double_counted`.
    #[tokio::test]
    async fn reassert_at_the_aggregate_bound_does_not_double_count_itself() {
        let limits = PolicyLimits {
            max_total_egress_edges: 2,
            ..PolicyLimits::for_tests()
        };
        let (svc, _sm) = make_service_with_limits(limits);
        let streams = vec![egress(1, 5, 0, 5), egress(2, 6, 0, 6)];

        svc.register_meeting(make_policy_request("m-1", 1, streams.clone()))
            .await
            .unwrap();

        // Same edges, higher generation: projects to 2, not 4.
        let resp = svc
            .register_meeting(make_policy_request("m-1", 2, streams))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(
            resp.applied_generation, 2,
            "a re-assert at the bound must not count its own edges twice"
        );
    }

    fn make_service_with_ceiling(stream_ceiling: usize) -> (MhMediaService, SessionManagerHandle) {
        let sm = SessionManagerHandle::new(crate::session::StreamAdmission {
            stream_ceiling,
            rejection_ratio_threshold: 0.05,
        });
        let svc = MhMediaService::new(
            sm.clone(),
            TEST_HANDLER_ID.to_string(),
            TEST_EPOCH_MS,
            PolicyLimits::for_tests(),
        );
        (svc, sm)
    }

    /// Stream admission (story 2 R-19): a registration that would take the
    /// handler's installed egress streams above its ceiling is refused whole,
    /// the echo does not advance, and the outcome is the capacity refusal —
    /// NOT `apply_failed`, which the deploy bake gate treats as a fault.
    #[tokio::test]
    async fn over_ceiling_is_refused_whole_and_the_echo_does_not_advance() {
        use common::observability::testing::MetricAssertion;
        let (svc, sm) = make_service_with_ceiling(2);
        let ok = svc
            .register_meeting(make_policy_request(
                "m-1",
                5,
                vec![egress(1, 5, 0, 6), egress(2, 6, 0, 5)],
            ))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(ok.applied_generation, 5);
        let before = sm.routing_snapshot();

        let snap = MetricAssertion::snapshot();
        let refused = svc
            .register_meeting(make_policy_request("m-2", 1, vec![egress(1, 7, 0, 8)]))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(refused.applied_generation, 0, "nothing installed for m-2");
        assert!(refused.accepted, "the registration itself is accepted");
        assert!(
            Arc::ptr_eq(&before, &sm.routing_snapshot()),
            "no snapshot swap: all-or-none"
        );
        snap.counter("mh_media_policy_applies_total")
            .with_labels(&[
                ("outcome", "rejected_stream_ceiling"),
                ("key_custody", "operator"),
            ])
            .assert_delta(1);
        snap.counter("mh_media_policy_applies_total")
            .with_labels(&[("outcome", "apply_failed"), ("key_custody", "operator")])
            .assert_delta(0);
    }

    /// A re-assert at the ceiling projects its own streams once, not twice,
    /// so a full handler keeps accepting its own meetings' re-asserts.
    #[tokio::test]
    async fn reassert_at_the_stream_ceiling_is_admitted_not_double_counted() {
        let (svc, _sm) = make_service_with_ceiling(2);
        let streams = vec![egress(1, 5, 0, 6), egress(2, 6, 0, 5)];
        svc.register_meeting(make_policy_request("m-1", 1, streams.clone()))
            .await
            .unwrap();
        let resp = svc
            .register_meeting(make_policy_request("m-1", 2, streams))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(resp.applied_generation, 2);
    }

    /// A source that resolves to no connection still advances the echo.
    ///
    /// `internal.proto`: "A PENDING SOURCE DOES NOT HOLD BACK
    /// `applied_generation`, AND MUST NOT." The two are different instruments —
    /// `applied_generation` is per-snapshot ("is this policy installed?"), slot
    /// state is per-edge ("is this edge carrying media?"). Named explicitly so
    /// a later refactor that adds sender→connection binding cannot silently
    /// start gating the echo on connectivity.
    #[tokio::test]
    async fn a_source_with_no_connection_still_advances_applied_generation() {
        let (svc, _sm) = make_service();
        let resp = svc
            .register_meeting(make_policy_request("m-1", 4, vec![egress(1, 5, 0, 5)]))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(resp.applied_generation, 4);
    }

    // =======================================================================
    // Generation 0 — the ordering constraint
    // =======================================================================

    /// Generation 0 must NOT be rejected in this story: MC does not emit >= 1
    /// until story task 13, so rejecting now blackholes every meeting for the
    /// whole window.
    #[tokio::test]
    async fn generation_zero_is_accepted_and_installs_nothing() {
        let (svc, sm) = make_service();
        let snapshot_before = sm.routing_snapshot();

        let resp = svc
            .register_meeting(make_register_request("m-1", "mc-1", "http://mc:50052"))
            .await
            .unwrap()
            .into_inner();

        assert!(
            resp.accepted,
            "generation 0 must not be rejected before task 13"
        );
        assert_eq!(resp.applied_generation, 0, "nothing applied is the truth");
        assert_eq!(resp.transport_mode, TransportMode::Unspecified as i32);
        assert!(
            Arc::ptr_eq(&snapshot_before, &sm.routing_snapshot()),
            "generation 0 must install nothing"
        );
        assert!(
            sm.is_meeting_registered("m-1").await,
            "the registration itself is still accepted, so pending connections are promoted"
        );
    }

    /// **The rollback case.** A meeting already at generation N that receives a
    /// `policy_generation: 0` re-assert — what a rolled-back MC sends — must
    /// report N, not 0.
    ///
    /// The status ("what did I do with this request") and the echo ("what is
    /// live") come from different sources, and this is the only input that
    /// distinguishes a correct implementation from one that couples them: for
    /// the whole pre-task-13 window nothing has ever been applied, so the echo
    /// is legitimately 0 and a coupled implementation passes every test anyone
    /// would naturally write.
    #[tokio::test]
    async fn generation_zero_reassert_still_reports_the_installed_generation() {
        let (svc, _sm) = make_service();

        svc.register_meeting(make_policy_request("m-1", 6, vec![egress(1, 5, 0, 5)]))
            .await
            .unwrap();

        let resp = svc
            .register_meeting(make_policy_request("m-1", 0, vec![]))
            .await
            .unwrap()
            .into_inner();

        assert_eq!(
            resp.applied_generation, 6,
            "a rolled-back MC sending generation 0 must not make MH claim nothing is live; \
             reporting 0 here manufactures a false divergence alarm"
        );
        assert_eq!(
            resp.transport_mode,
            TransportMode::Datagram as i32,
            "the echo reports the mode MH applied, which is still generation 6's"
        );
    }

    // =======================================================================
    // Structural rejects — whole registration, no state mutation
    // =======================================================================

    #[tokio::test]
    async fn structural_reject_mutates_no_state_and_regresses_nothing() {
        let (svc, sm) = make_service();

        svc.register_meeting(make_policy_request("m-1", 2, vec![egress(1, 5, 0, 5)]))
            .await
            .unwrap();
        let snapshot_before = sm.routing_snapshot();
        let endpoint_before = sm.get_mc_endpoint("m-1").await.unwrap();

        // Duplicate egress_stream_id.
        let err = svc
            .register_meeting(make_policy_request(
                "m-1",
                3,
                vec![egress(1, 5, 0, 5), egress(1, 6, 1, 6)],
            ))
            .await
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);

        assert!(
            Arc::ptr_eq(&snapshot_before, &sm.routing_snapshot()),
            "a rejected registration must not touch the live snapshot"
        );
        assert_eq!(
            sm.get_mc_endpoint("m-1").await.unwrap(),
            endpoint_before,
            "a rejected registration must not upsert"
        );
    }

    #[tokio::test]
    async fn structural_rejects_cover_every_tier_a_check() {
        let (svc, _sm) = make_service();

        // duplicate (sender, slot)
        assert_eq!(
            svc.register_meeting(make_policy_request(
                "m",
                1,
                vec![egress(1, 5, 3, 5), egress(2, 5, 3, 6)]
            ))
            .await
            .unwrap_err()
            .code(),
            tonic::Code::InvalidArgument
        );

        // sender_id 0
        assert_eq!(
            svc.register_meeting(make_policy_request("m", 1, vec![egress(1, 0, 0, 5)]))
                .await
                .unwrap_err()
                .code(),
            tonic::Code::InvalidArgument
        );

        // slot_id out of 16-bit range
        assert_eq!(
            svc.register_meeting(make_policy_request("m", 1, vec![egress(1, 5, 65_536, 5)]))
                .await
                .unwrap_err()
                .code(),
            tonic::Code::InvalidArgument
        );

        // stream_number out of 8-bit range
        let mut wide_stream = egress(1, 5, 0, 5);
        wide_stream.candidate_sources[0].stream_number = 256;
        assert_eq!(
            svc.register_meeting(make_policy_request("m", 1, vec![wide_stream]))
                .await
                .unwrap_err()
                .code(),
            tonic::Code::InvalidArgument
        );

        // missing subscriber
        let mut no_sub = egress(1, 5, 0, 5);
        no_sub.subscriber = None;
        assert_eq!(
            svc.register_meeting(make_policy_request("m", 1, vec![no_sub]))
                .await
                .unwrap_err()
                .code(),
            tonic::Code::InvalidArgument
        );
    }

    #[tokio::test]
    async fn per_meeting_and_per_egress_count_bounds_reject() {
        let (svc, _sm) = make_service_with_limits(PolicyLimits {
            max_egress_streams_per_meeting: 1,
            ..PolicyLimits::for_tests()
        });
        assert_eq!(
            svc.register_meeting(make_policy_request(
                "m",
                1,
                vec![egress(1, 5, 0, 5), egress(2, 6, 1, 6)]
            ))
            .await
            .unwrap_err()
            .code(),
            tonic::Code::InvalidArgument
        );

        let (svc, _sm) = make_service_with_limits(PolicyLimits {
            max_candidate_sources_per_egress: 1,
            ..PolicyLimits::for_tests()
        });
        let mut stream = egress(1, 5, 0, 5);
        stream.candidate_sources.push(CandidateSource {
            sender_id: 6,
            stream_number: 1,
        });
        assert_eq!(
            svc.register_meeting(make_policy_request("m", 1, vec![stream]))
                .await
                .unwrap_err()
                .code(),
            tonic::Code::InvalidArgument
        );
    }

    // =======================================================================
    // Transport mode
    // =======================================================================

    #[tokio::test]
    async fn unspecified_transport_mode_rejects_and_never_defaults_to_datagram() {
        let (svc, sm) = make_service();
        let mut stream = egress(1, 5, 0, 5);
        stream.transport_mode = TransportMode::Unspecified as i32;

        let err = svc
            .register_meeting(make_policy_request("m", 1, vec![stream]))
            .await
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert_eq!(
            sm.routing_snapshot()
                .transport_mode_for(&MeetingKey::new("m")),
            None,
            "MH must not fall back to datagram for a stream that names no mode"
        );
    }

    #[tokio::test]
    async fn heterogeneous_transport_modes_reject_rather_than_echoing_one_of_n() {
        let (svc, _sm) = make_service();
        let mut second = egress(2, 6, 0, 6);
        second.transport_mode = TransportMode::StreamPerGroup as i32;
        let err = svc
            .register_meeting(make_policy_request(
                "m",
                1,
                vec![egress(1, 5, 0, 5), second],
            ))
            .await
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn applied_transport_mode_is_echoed() {
        let (svc, _sm) = make_service();
        let mut stream = egress(1, 5, 0, 5);
        stream.transport_mode = TransportMode::StreamPerGroup as i32;
        let resp = svc
            .register_meeting(make_policy_request("m", 1, vec![stream]))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(resp.transport_mode, TransportMode::StreamPerGroup as i32);
    }

    // =======================================================================
    // Response identity fields
    // =======================================================================

    /// Within-process stability of `process_start_epoch_ms`.
    ///
    /// This alone kills the per-call `now()` implementation. The
    /// changes-on-restart half is not constructible until a second process
    /// incarnation shares a pod identity (the handler-restart story).
    #[tokio::test]
    async fn process_start_epoch_is_identical_across_two_calls_in_one_process() {
        let (svc, _sm) = make_service();

        let first = svc
            .register_meeting(make_policy_request("m-1", 1, vec![egress(1, 5, 0, 5)]))
            .await
            .unwrap()
            .into_inner();
        let second = svc
            .register_meeting(make_policy_request("m-2", 1, vec![egress(1, 5, 0, 5)]))
            .await
            .unwrap()
            .into_inner();

        assert_eq!(
            first.process_start_epoch_ms, second.process_start_epoch_ms,
            "two calls in one process must report one incarnation"
        );
        assert_ne!(
            first.process_start_epoch_ms, 0,
            "0 means 'not reported'; a live handler must report its incarnation"
        );
    }

    #[tokio::test]
    async fn handler_id_is_reported_and_is_not_invented() {
        let (svc, _sm) = make_service();
        let resp = svc
            .register_meeting(make_register_request("m-1", "mc-1", "http://mc:50052"))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(resp.handler_id, TEST_HANDLER_ID);
    }

    // =======================================================================
    // Pre-boundary scalar validation (unchanged behaviour)
    // =======================================================================

    #[tokio::test]
    async fn test_register_meeting_valid_request_stores_registration() {
        let (svc, sm) = make_service();

        let resp = svc
            .register_meeting(make_register_request(
                "meeting-1",
                "mc-1",
                "http://mc:50052",
            ))
            .await
            .unwrap();

        assert!(resp.into_inner().accepted);
        assert!(sm.is_meeting_registered("meeting-1").await);
        assert_eq!(
            sm.get_mc_endpoint("meeting-1").await.unwrap(),
            "http://mc:50052"
        );
    }

    #[tokio::test]
    async fn test_register_meeting_empty_meeting_id_rejected() {
        let (svc, _sm) = make_service();
        let err = svc
            .register_meeting(make_register_request("", "mc-1", "http://mc:50052"))
            .await
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("meeting_id"));
    }

    #[tokio::test]
    async fn test_register_meeting_empty_mc_id_rejected() {
        let (svc, _sm) = make_service();
        let err = svc
            .register_meeting(make_register_request("meeting-1", "", "http://mc:50052"))
            .await
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("mc_id"));
    }

    #[tokio::test]
    async fn test_register_meeting_empty_endpoint_rejected() {
        let (svc, _sm) = make_service();
        let err = svc
            .register_meeting(make_register_request("meeting-1", "mc-1", ""))
            .await
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("mc_grpc_endpoint"));
    }

    #[tokio::test]
    async fn test_register_meeting_invalid_endpoint_scheme_rejected() {
        let (svc, _sm) = make_service();
        let err = svc
            .register_meeting(make_register_request("meeting-1", "mc-1", "ftp://mc:50052"))
            .await
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("scheme"));
    }

    #[tokio::test]
    async fn test_register_meeting_accepts_valid_schemes() {
        let (svc, _sm) = make_service();
        for (meeting, endpoint) in [
            ("m-1", "http://mc:50052"),
            ("m-2", "https://mc:50052"),
            ("m-3", "grpc://mc:50052"),
        ] {
            assert!(svc
                .register_meeting(make_register_request(meeting, "mc-1", endpoint))
                .await
                .is_ok());
        }
    }

    #[tokio::test]
    async fn test_register_meeting_id_too_long_rejected() {
        let (svc, _sm) = make_service();
        let long_id = "x".repeat(MAX_ID_LENGTH + 1);
        let err = svc
            .register_meeting(make_register_request(&long_id, "mc-1", "http://mc:50052"))
            .await
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("meeting_id"));
    }

    #[tokio::test]
    async fn test_register_meeting_mc_id_too_long_rejected() {
        let (svc, _sm) = make_service();
        let long_id = "x".repeat(MAX_ID_LENGTH + 1);
        let err = svc
            .register_meeting(make_register_request(
                "meeting-1",
                &long_id,
                "http://mc:50052",
            ))
            .await
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("mc_id"));
    }

    #[tokio::test]
    async fn test_register_meeting_endpoint_too_long_rejected() {
        let (svc, _sm) = make_service();
        let long_endpoint = format!("http://{}", "x".repeat(MAX_ENDPOINT_LENGTH));
        let err = svc
            .register_meeting(make_register_request("meeting-1", "mc-1", &long_endpoint))
            .await
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("mc_grpc_endpoint"));
    }

    #[tokio::test]
    async fn test_register_meeting_promotes_pending_connections() {
        let (svc, sm) = make_service();

        for (conn, user) in [("conn-1", "user-1"), ("conn-2", "user-2")] {
            sm.add_pending_connection(PendingConnection {
                connection_id: conn.to_string(),
                meeting_id: "meeting-1".to_string(),
                participant_id: user.to_string(),
                connected_at: Instant::now(),
                close: tokio_util::sync::CancellationToken::new(),
            })
            .await;
        }
        assert_eq!(sm.active_connection_count().await, 0);

        let resp = svc
            .register_meeting(make_register_request(
                "meeting-1",
                "mc-1",
                "http://mc:50052",
            ))
            .await
            .unwrap();

        assert!(resp.into_inner().accepted);
        assert_eq!(sm.active_connection_count().await, 2);
    }

    #[tokio::test]
    async fn test_register_meeting_duplicate_updates_registration() {
        let (svc, sm) = make_service();

        svc.register_meeting(make_register_request(
            "meeting-1",
            "mc-1",
            "http://mc-1:50052",
        ))
        .await
        .unwrap();
        assert_eq!(
            sm.get_mc_endpoint("meeting-1").await.unwrap(),
            "http://mc-1:50052"
        );

        svc.register_meeting(make_register_request(
            "meeting-1",
            "mc-2",
            "http://mc-2:50052",
        ))
        .await
        .unwrap();
        assert_eq!(
            sm.get_mc_endpoint("meeting-1").await.unwrap(),
            "http://mc-2:50052"
        );
    }
}
