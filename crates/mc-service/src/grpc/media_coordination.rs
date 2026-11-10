//! Media Coordination gRPC Service (R-15).
//!
//! Implements the `MediaCoordinationService` that Media Handlers call to notify
//! MC of participant connection/disconnection events.
//!
//! # These notifications ARE the connectivity source (ADR-0036 §9)
//!
//! Since story 2 task 20 a participant hears a sender if and only if the two
//! share a connected handler, and "connected" means exactly what these RPCs
//! report — server-observed, identity from the validated token `sub`, handler
//! resolved by exact match against the meeting's handler set. They are recorded
//! in ONE place, the meeting actor, in the same turn that answers the sender
//! binding (`MeetingActorHandle::media_connected`). There is no registry beside
//! it. The client-reported `MediaConnectionUpdate` map is never consulted.
//!
//! **Lost Disconnecteds (ADR-0039 §6).** A Disconnected a LIVE handler never
//! delivered is healed by that handler's heartbeat: the remove-only reconcile
//! retires keys absent from the heartbeat's `connection_ids` and bound before
//! its previous heartbeat. A heartbeat never ADDS connectivity — the sender-id
//! bind below is the only add path. A crashed handler sends no heartbeat
//! either; its keys go when handler-loss detection removes it (MC-B2). The
//! reverse direction is closed: a stale Disconnected from an old session cannot
//! remove a newer live session, because connectivity is keyed by
//! `connection_id`.
//!
//! # RPCs
//!
//! - `NotifyParticipantConnected` — MH informs MC that a participant has
//!   established a WebTransport connection to the MH.
//! - `NotifyParticipantDisconnected` — MH informs MC that a participant's
//!   WebTransport connection to the MH has dropped.
//! - `MeetingHeartbeat` — one per (meeting, handler) per interval (ADR-0039
//!   §1/§2/§6): MC answers the verdict from the owning meeting actor's state.
//!
//! # Security
//!
//! Authentication is handled by `McAuthLayer` (applied at the server level in main.rs).
//! This handler only needs to validate request field constraints.
//! Generic error messages prevent information leakage (ADR-0003).

use crate::actors::messages::SenderLookup;
use crate::actors::metrics::ActorType;
use crate::actors::{HeartbeatRouteError, MeetingControllerActorHandle};
use crate::errors::McError;
use crate::media_admission::SenderBindingOutcome;
use crate::media_routing::connectivity::MAX_LIVE_MEDIA_CONNECTIONS_PER_HANDLER;
use crate::media_routing::{
    HandlerHeartbeat, HeartbeatAnomaly, HeartbeatOutcome, HeartbeatTiming, HeartbeatVerdict,
    Unapplied,
};
use crate::observability::metrics;
use proto_gen::dark_tower::internal::v1::media_coordination_service_server::MediaCoordinationService;
use proto_gen::dark_tower::internal::v1::{
    ApplyRefusal, MeetingHeartbeatRequest, MeetingHeartbeatResponse,
    NotifyParticipantConnectedRequest, NotifyParticipantConnectedResponse,
    NotifyParticipantDisconnectedRequest, NotifyParticipantDisconnectedResponse, ServingState,
};
use std::sync::Arc;
use tokio::time::Instant;
use tonic::{Request, Response, Status};
use tracing::{debug, info, instrument};

/// Maximum ID field length (bytes), for every id field on these RPCs —
/// `meeting_id`, `participant_id`, `handler_id` and `connection_id`. One bound,
/// one home (it used to live in the retired `mh_connection_registry.rs`).
pub const MAX_ID_LENGTH: usize = 256;

/// What the heartbeat intake needs beyond the controller (ADR-0039 §1/§2).
#[derive(Debug, Clone)]
pub struct HeartbeatIntake {
    /// This MC's id: the OWNER CHECK compares the heartbeat's `mc_id` to it.
    mc_id: String,
    /// N and its derivations (the actor-send and reply budgets).
    timing: HeartbeatTiming,
    /// The receiver's `connection_ids` bound (see [`Self::new`]).
    max_connection_ids: usize,
}

impl HeartbeatIntake {
    /// Build the intake. The `connection_ids` bound is `max_participants`
    /// (`MC_MAX_PARTICIPANTS`, the MC-WIDE participant total — MC has no
    /// per-meeting participant cap) × [`MAX_LIVE_MEDIA_CONNECTIONS_PER_HANDLER`],
    /// the tightest bound MC holds. A list over it, or with a malformed entry,
    /// SKIPS that heartbeat's reconcile (counted); the verdict and liveness
    /// still apply (security P-1).
    ///
    /// # Errors
    ///
    /// [`McError::Config`] if the bound overflows `usize` (a startup
    /// configuration error).
    pub fn new(
        mc_id: String,
        timing: HeartbeatTiming,
        max_participants: u32,
    ) -> Result<Self, McError> {
        let max_connection_ids = usize::try_from(max_participants)
            .ok()
            .and_then(|p| p.checked_mul(MAX_LIVE_MEDIA_CONNECTIONS_PER_HANDLER))
            .ok_or_else(|| {
                McError::Config("heartbeat connection_ids bound overflows usize".to_string())
            })?;
        Ok(Self {
            mc_id,
            timing,
            max_connection_ids,
        })
    }
}

/// MC Media Coordination gRPC service implementation.
pub struct McMediaCoordinationService {
    /// Route to the meeting actors, which hold the allocated `sender_id`s AND
    /// each participant's observed connectivity.
    ///
    /// The controller is the only way to reach a meeting actor, and a meeting
    /// actor is the only holder of its participants' ordinals. That chain is
    /// what makes a cross-meeting answer unrepresentable: resolution never
    /// consults anything but the one meeting named in the request.
    controller: Arc<MeetingControllerActorHandle>,
    /// Heartbeat intake config.
    intake: HeartbeatIntake,
}

impl McMediaCoordinationService {
    /// Create a new media coordination service.
    #[must_use]
    pub fn new(controller: Arc<MeetingControllerActorHandle>, intake: HeartbeatIntake) -> Self {
        Self { controller, intake }
    }

    /// Route one validated heartbeat to its meeting actor through the
    /// controller (the same path as `Notify*`) and return the verdict.
    ///
    /// Only a DEFINITE "no actor for this meeting" (`MeetingNotFound`) becomes
    /// RELEASED(MEETING_ENDED). Every other failure — the lookup or the
    /// mailbox send outlasting the budget, a closed actor, no reply — is
    /// counted on `mc_messages_dropped_total{actor_type}` and answered
    /// UNAVAILABLE, NEVER RELEASED (security S-2): RELEASED ends the meeting
    /// on MH.
    async fn route_heartbeat(
        &self,
        meeting_id: String,
        heartbeat: HandlerHeartbeat,
        receipt: Instant,
    ) -> Result<HeartbeatVerdict, Status> {
        let send_deadline = receipt + self.intake.timing.send_budget();
        let meeting = match tokio::time::timeout_at(
            send_deadline,
            self.controller.get_meeting_handle(meeting_id),
        )
        .await
        {
            Ok(Ok(meeting)) => meeting,
            Ok(Err(McError::MeetingNotFound(_))) => return Ok(HeartbeatVerdict::MeetingEnded),
            Ok(Err(_)) | Err(_) => {
                metrics::record_message_dropped(ActorType::Controller.as_str());
                return Err(Status::unavailable("Service unavailable"));
            }
        };
        let send_budget = send_deadline.saturating_duration_since(Instant::now());
        match meeting
            .handler_heartbeat(
                heartbeat,
                receipt,
                send_budget,
                self.intake.timing.interval(),
            )
            .await
        {
            Ok(verdict) => Ok(verdict),
            Err(
                HeartbeatRouteError::Dropped
                | HeartbeatRouteError::ActorGone
                | HeartbeatRouteError::ReplyTimedOut,
            ) => {
                metrics::record_message_dropped(ActorType::Meeting.as_str());
                Err(Status::unavailable("Service unavailable"))
            }
        }
    }

    /// Resolve the wire `sender_id` for a connecting participant AND record its
    /// connectivity, in one meeting-actor turn.
    ///
    /// `token_sub` is `NotifyParticipantConnectedRequest.participant_id`, which
    /// by contract carries **the validated meeting token's `sub`** — never a
    /// client-supplied hint (@security S1). MC stores that value as a
    /// participant's `user_id`, so that is what it is resolved against.
    ///
    /// Returns the value to put on the wire, the binding outcome to count, and
    /// why connectivity was not recorded (if it was not). **The only path that
    /// yields a non-zero value is a live, UNAMBIGUOUS roster hit**; every other
    /// path yields `0`, which MH treats as a reject. MC never invents an id:
    /// [`SenderId`](crate::media_admission::SenderId) wraps `NonZeroU16` and its
    /// only production constructor is the per-meeting allocator.
    ///
    /// The two directions of "recorded ⟺ answered" hold by construction:
    /// connectivity is recorded only for a `Found` participant, i.e. only when
    /// this also answers a real ordinal; and an ordinal is answered only for a
    /// roster entry, which is tracked for connectivity by the same actor.
    async fn resolve_and_record(
        &self,
        meeting_id: &str,
        token_sub: &str,
        handler_id: &str,
        connection_id: &str,
    ) -> (u32, SenderBindingOutcome, Option<Unapplied>) {
        // A missing meeting and a dead/closed meeting actor both land here:
        // "MC cannot answer for this meeting", counted as `meeting_unknown`
        // rather than degrading to the participant-level arm.
        let Ok(meeting) = self
            .controller
            .get_meeting_handle(meeting_id.to_string())
            .await
        else {
            return (
                0,
                SenderBindingOutcome::MeetingUnknown,
                Some(Unapplied::MeetingUnknown),
            );
        };

        match meeting
            .media_connected(
                token_sub.to_string(),
                handler_id.to_string(),
                connection_id.to_string(),
            )
            .await
        {
            Ok((SenderLookup::Found(sender_id), unapplied)) => (
                u32::from(sender_id.get().get()),
                SenderBindingOutcome::Resolved,
                unapplied,
            ),
            // Not on the roster: the honest "I do not know this participant".
            Ok((SenderLookup::NotFound, unapplied)) => {
                (0, SenderBindingOutcome::ParticipantUnknown, unapplied)
            }
            // Two participants share this `sub`; MC will not guess.
            Ok((SenderLookup::Ambiguous, unapplied)) => {
                (0, SenderBindingOutcome::UserAmbiguous, unapplied)
            }
            // The binding gate: out-of-set or removed handler; nothing recorded.
            Ok((SenderLookup::Refused, unapplied)) => (0, SenderBindingOutcome::Refused, unapplied),
            // The actor went away between the handle lookup and the query.
            Err(_) => (
                0,
                SenderBindingOutcome::MeetingUnknown,
                Some(Unapplied::MeetingUnknown),
            ),
        }
    }
}

/// The one id-shape rule: non-empty and at most [`MAX_ID_LENGTH`] bytes.
fn id_field_ok(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_ID_LENGTH
}

/// Validate that an ID field is non-empty and within length bounds.
fn validate_id_field(value: &str, field_name: &str) -> Result<(), Status> {
    if id_field_ok(value) {
        return Ok(());
    }
    if value.is_empty() {
        debug!(
            target: "mc.grpc.media_coordination",
            field = field_name,
            "Empty required field"
        );
        return Err(Status::invalid_argument("Invalid request"));
    }

    if value.len() > MAX_ID_LENGTH {
        debug!(
            target: "mc.grpc.media_coordination",
            field = field_name,
            len = value.len(),
            max = MAX_ID_LENGTH,
            "Field exceeds maximum length"
        );
        return Err(Status::invalid_argument("Invalid request"));
    }

    Ok(())
}

/// Screen a heartbeat's `connection_ids` (security P-1): over the bound, or
/// any entry failing the id-shape rule, SKIPS the reconcile for this heartbeat
/// (an empty list goes to the actor plus the reason) — never rejects the
/// heartbeat, because skipping only keeps keys while a missing verdict would
/// read as a dead handler. Entry contents are never logged.
fn screen_connection_ids(ids: Vec<String>, max: usize) -> (Vec<String>, Option<HeartbeatAnomaly>) {
    if ids.len() > max {
        return (Vec::new(), Some(HeartbeatAnomaly::ConnectionIdsOverCap));
    }
    if !ids.iter().all(|id| id_field_ok(id)) {
        return (Vec::new(), Some(HeartbeatAnomaly::ConnectionIdMalformed));
    }
    (ids, None)
}

/// Decode and check a heartbeat's enums (ADR-0039 §1): `serving_state` must be
/// SERVING or DRAINING (UNSPECIFIED and unknown numbers are rejected),
/// `apply_refusal` a defined value, and `drain_remaining_ms` zero unless
/// DRAINING (DRAINING with 0 = the deadline is reached).
fn validate_heartbeat_enums(
    request: &MeetingHeartbeatRequest,
) -> Result<(ServingState, ApplyRefusal), Status> {
    let invalid = || Status::invalid_argument("Invalid request");
    let serving_state = match ServingState::try_from(request.serving_state) {
        Ok(state @ (ServingState::Serving | ServingState::Draining)) => state,
        Ok(ServingState::Unspecified) | Err(_) => return Err(invalid()),
    };
    let apply_refusal = ApplyRefusal::try_from(request.apply_refusal).map_err(|_| invalid())?;
    if serving_state == ServingState::Serving && request.drain_remaining_ms != 0 {
        return Err(invalid());
    }
    Ok((serving_state, apply_refusal))
}

/// `connection_id` is REQUIRED (ADR-0039 §6): an empty one is refused HERE,
/// at the boundary — answered `sender_id` 0 (Connected) or a no-op
/// (Disconnected), counted as `connection_id_missing`, and the meeting actor
/// never sees it. Returns whether it was refused.
fn refuse_missing_connection_id(connection_id: &str) -> bool {
    if connection_id.is_empty() {
        metrics::record_notification_unapplied(Unapplied::ConnectionIdMissing);
        return true;
    }
    false
}

#[tonic::async_trait]
impl MediaCoordinationService for McMediaCoordinationService {
    /// Handle notification that a participant connected to an MH (R-15).
    #[instrument(skip_all, name = "mc.grpc.media_coordination.connected")]
    async fn notify_participant_connected(
        &self,
        request: Request<NotifyParticipantConnectedRequest>,
    ) -> Result<Response<NotifyParticipantConnectedResponse>, Status> {
        let inner = request.into_inner();

        // Validate all required fields
        validate_id_field(&inner.meeting_id, "meeting_id")?;
        validate_id_field(&inner.participant_id, "participant_id")?;
        validate_id_field(&inner.handler_id, "handler_id")?;

        metrics::record_mh_notification("connected");
        if refuse_missing_connection_id(&inner.connection_id) {
            metrics::record_sender_binding_response(SenderBindingOutcome::Refused);
            return Ok(Response::new(NotifyParticipantConnectedResponse {
                acknowledged: true,
                sender_id: 0,
            }));
        }
        validate_id_field(&inner.connection_id, "connection_id")?;

        let (sender_id, outcome, unapplied) = self
            .resolve_and_record(
                &inner.meeting_id,
                &inner.participant_id,
                &inner.handler_id,
                &inner.connection_id,
            )
            .await;
        metrics::record_sender_binding_response(outcome);
        if let Some(reason) = unapplied {
            metrics::record_notification_unapplied(reason);
        }

        info!(
            target: "mc.grpc.media_coordination",
            meeting_id = %inner.meeting_id,
            participant_id = %inner.participant_id,
            handler_id = %inner.handler_id,
            // Opaque MH session id: a LOG field for correlation, never a metric
            // label (unbounded cardinality).
            connection_id = %inner.connection_id,
            // The OUTCOME, never the id. ADR-0036 §11 bars the `sender_id`
            // value as a per-participant log dimension.
            sender_binding = outcome.label(),
            connectivity = unapplied.map_or("recorded", Unapplied::label),
            "Participant connected to MH"
        );

        Ok(Response::new(NotifyParticipantConnectedResponse {
            // Unchanged meaning: the notification was received and the meeting actor
            // consulted. Deliberately NOT repurposed to mean "and I resolved a
            // sender" -- the two are independent, and MH must read `sender_id`,
            // never infer a binding from `acknowledged`.
            acknowledged: true,
            sender_id,
        }))
    }

    /// Handle notification that a participant disconnected from an MH (R-15).
    #[instrument(skip_all, name = "mc.grpc.media_coordination.disconnected")]
    async fn notify_participant_disconnected(
        &self,
        request: Request<NotifyParticipantDisconnectedRequest>,
    ) -> Result<Response<NotifyParticipantDisconnectedResponse>, Status> {
        let inner = request.into_inner();

        // Validate all required fields
        validate_id_field(&inner.meeting_id, "meeting_id")?;
        validate_id_field(&inner.participant_id, "participant_id")?;
        validate_id_field(&inner.handler_id, "handler_id")?;

        metrics::record_mh_notification("disconnected");
        if refuse_missing_connection_id(&inner.connection_id) {
            return Ok(Response::new(NotifyParticipantDisconnectedResponse {
                acknowledged: true,
            }));
        }
        validate_id_field(&inner.connection_id, "connection_id")?;

        // Unknown meeting: nothing to remove. A disconnect is idempotent and
        // never an error to MH.
        let unapplied = match self
            .controller
            .get_meeting_handle(inner.meeting_id.clone())
            .await
        {
            Ok(meeting) => meeting
                .media_disconnected(
                    inner.participant_id.clone(),
                    inner.handler_id.clone(),
                    inner.connection_id.clone(),
                )
                .await
                .unwrap_or(Some(Unapplied::MeetingUnknown)),
            Err(_) => Some(Unapplied::MeetingUnknown),
        };
        if let Some(reason) = unapplied {
            metrics::record_notification_unapplied(reason);
        }

        info!(
            target: "mc.grpc.media_coordination",
            meeting_id = %inner.meeting_id,
            participant_id = %inner.participant_id,
            handler_id = %inner.handler_id,
            connection_id = %inner.connection_id,
            reason = ?inner.reason,
            connectivity = unapplied.map_or("removed", Unapplied::label),
            "Participant disconnected from MH"
        );

        Ok(Response::new(NotifyParticipantDisconnectedResponse {
            acknowledged: true,
        }))
    }

    /// One per-(meeting, handler) heartbeat (ADR-0039 §1/§2/§6).
    ///
    /// Order: stamp the receipt; validate (INVALID_ARGUMENT for malformed ids
    /// or enums); the OWNER CHECK FIRST — a foreign `mc_id` is
    /// FAILED_PRECONDITION with a static message, before any meeting lookup,
    /// so a misrouted heartbeat learns nothing and can never end a live
    /// meeting; then route to the meeting actor. A routing drop is counted on
    /// `mc_messages_dropped_total` and answered UNAVAILABLE. Never logged per
    /// heartbeat; the actor logs transitions.
    #[instrument(skip_all, name = "mc.grpc.media_coordination.heartbeat")]
    async fn meeting_heartbeat(
        &self,
        request: Request<MeetingHeartbeatRequest>,
    ) -> Result<Response<MeetingHeartbeatResponse>, Status> {
        let receipt = Instant::now();
        let inner = request.into_inner();
        validate_id_field(&inner.meeting_id, "meeting_id")?;
        validate_id_field(&inner.mc_id, "mc_id")?;
        validate_id_field(&inner.handler_id, "handler_id")?;
        let (serving_state, apply_refusal) = validate_heartbeat_enums(&inner)?;

        if inner.mc_id != self.intake.mc_id {
            metrics::record_handler_heartbeat(HeartbeatOutcome::OwnerMismatch);
            return Err(Status::failed_precondition("Invalid request"));
        }

        let (connection_ids, reconcile_skip) =
            screen_connection_ids(inner.connection_ids, self.intake.max_connection_ids);
        let heartbeat = HandlerHeartbeat {
            handler_id: inner.handler_id,
            serving_state,
            drain_remaining_ms: inner.drain_remaining_ms,
            applied_generation: inner.applied_generation,
            apply_refusal,
            connection_ids,
            reconcile_skip,
        };
        let verdict = self
            .route_heartbeat(inner.meeting_id, heartbeat, receipt)
            .await?;
        if verdict == HeartbeatVerdict::MeetingEnded {
            // The router's answer; the actor counts every other verdict.
            metrics::record_handler_heartbeat(verdict.outcome());
        }
        let (verdict, cause) = verdict.wire();
        Ok(Response::new(MeetingHeartbeatResponse {
            verdict: verdict.into(),
            cause: cause.into(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A controller with no meetings. Unit tests here exercise request
    /// VALIDATION and the no-meeting arms; the resolution and connectivity arms
    /// need a real meeting actor and live in
    /// `tests/media_coordination_integration.rs` and
    /// `tests/slot_placement_integration.rs`.
    const MC_ID: &str = "mc-media-coord-unit";

    fn create_service() -> McMediaCoordinationService {
        McMediaCoordinationService::new(
            Arc::new(MeetingControllerActorHandle::new(
                MC_ID.to_string(),
                crate::actors::ActorMetrics::new(),
                crate::actors::ControllerMetrics::new(),
                ::common::secret::SecretBox::new(Box::new(vec![0u8; 32])),
                Arc::new(crate::media_routing::PolicyGenerations::new()),
                crate::media_admission::fixtures::kek_lifecycle(),
            )),
            HeartbeatIntake::new(
                MC_ID.to_string(),
                HeartbeatTiming::from_timeout_ms(6_000),
                10,
            )
            .unwrap(),
        )
    }

    fn heartbeat(meeting: &str, mc_id: &str) -> Request<MeetingHeartbeatRequest> {
        Request::new(MeetingHeartbeatRequest {
            meeting_id: meeting.to_string(),
            mc_id: mc_id.to_string(),
            handler_id: "mh-1".to_string(),
            serving_state: ServingState::Serving as i32,
            drain_remaining_ms: 0,
            applied_generation: 0,
            apply_refusal: ApplyRefusal::Unspecified as i32,
            connection_ids: Vec::new(),
        })
    }

    fn connected(
        meeting: &str,
        participant: &str,
        handler: &str,
        conn: &str,
    ) -> Request<NotifyParticipantConnectedRequest> {
        Request::new(NotifyParticipantConnectedRequest {
            meeting_id: meeting.to_string(),
            participant_id: participant.to_string(),
            handler_id: handler.to_string(),
            connection_id: conn.to_string(),
        })
    }

    fn disconnected(
        meeting: &str,
        participant: &str,
        handler: &str,
        conn: &str,
    ) -> Request<NotifyParticipantDisconnectedRequest> {
        Request::new(NotifyParticipantDisconnectedRequest {
            meeting_id: meeting.to_string(),
            participant_id: participant.to_string(),
            handler_id: handler.to_string(),
            reason: 1,
            connection_id: conn.to_string(),
        })
    }

    #[tokio::test]
    async fn an_unknown_meeting_is_acknowledged_with_no_binding() {
        let svc = create_service();
        let response = svc
            .notify_participant_connected(connected("meeting-1", "part-1", "mh-1", "c1"))
            .await
            .unwrap()
            .into_inner();
        assert!(response.acknowledged);
        assert_eq!(response.sender_id, 0, "no meeting, no ordinal, ever");
    }

    #[tokio::test]
    async fn a_disconnect_for_an_unknown_meeting_is_acknowledged() {
        let svc = create_service();
        let response = svc
            .notify_participant_disconnected(disconnected("nope", "part-1", "mh-1", "c1"))
            .await
            .unwrap();
        assert!(
            response.into_inner().acknowledged,
            "idempotent, never an error to MH"
        );
    }

    /// Security: every identity field rejects empty (the empty-`connection_id`
    /// refusal is a separate boundary step and never applies to these).
    #[tokio::test]
    async fn identity_fields_still_reject_empty() {
        let svc = create_service();
        for req in [
            connected("", "p", "h", "c"),
            connected("m", "", "h", "c"),
            connected("m", "p", "", "c"),
        ] {
            let err = svc.notify_participant_connected(req).await.unwrap_err();
            assert_eq!(err.code(), tonic::Code::InvalidArgument);
        }
        for req in [
            disconnected("", "p", "h", "c"),
            disconnected("m", "", "h", "c"),
            disconnected("m", "p", "", "c"),
        ] {
            let err = svc.notify_participant_disconnected(req).await.unwrap_err();
            assert_eq!(err.code(), tonic::Code::InvalidArgument);
        }
    }

    /// Replaces the deleted legacy-mode test: an empty `connection_id` is
    /// REFUSED at the boundary (sender 0, acknowledged) on both events, never
    /// an INVALID_ARGUMENT and never routed.
    #[tokio::test]
    async fn an_empty_connection_id_is_refused_with_sender_zero() {
        let svc = create_service();
        let response = svc
            .notify_participant_connected(connected("m", "p", "h", ""))
            .await
            .unwrap()
            .into_inner();
        assert!(response.acknowledged);
        assert_eq!(response.sender_id, 0);
        assert!(
            svc.notify_participant_disconnected(disconnected("m", "p", "h", ""))
                .await
                .unwrap()
                .into_inner()
                .acknowledged
        );
    }

    /// The owner check runs FIRST: a foreign `mc_id` is FAILED_PRECONDITION
    /// even for a meeting this MC has no actor for (which would otherwise be
    /// RELEASED(MEETING_ENDED)). Positive arm: this MC's id gets the
    /// router's MEETING_ENDED.
    #[tokio::test]
    async fn heartbeat_owner_check_precedes_the_no_actor_answer() {
        use proto_gen::dark_tower::internal::v1::{HeartbeatVerdict as Wire, ReleaseCause};
        let svc = create_service();
        let err = svc
            .meeting_heartbeat(heartbeat("no-such-meeting", "mc-other"))
            .await
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::FailedPrecondition);
        assert_eq!(err.message(), "Invalid request", "static: no id, no state");
        let ok = svc
            .meeting_heartbeat(heartbeat("no-such-meeting", MC_ID))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(ok.verdict, Wire::Released as i32);
        assert_eq!(ok.cause, ReleaseCause::MeetingEnded as i32);
    }

    /// ServingState UNSPECIFIED and an unknown number, an unknown
    /// ApplyRefusal, and drain on SERVING are INVALID_ARGUMENT. Positive arms:
    /// DRAINING with 0 and with a value are accepted.
    #[tokio::test]
    async fn heartbeat_enums_are_validated() {
        let svc = create_service();
        let mut bad = Vec::new();
        for state in [0, 99] {
            let mut r = heartbeat("m", MC_ID);
            r.get_mut().serving_state = state;
            bad.push(r);
        }
        let mut r = heartbeat("m", MC_ID);
        r.get_mut().apply_refusal = 99;
        bad.push(r);
        let mut r = heartbeat("m", MC_ID);
        r.get_mut().drain_remaining_ms = 5;
        bad.push(r);
        for r in bad {
            assert_eq!(
                svc.meeting_heartbeat(r).await.unwrap_err().code(),
                tonic::Code::InvalidArgument
            );
        }
        for drain in [0, 5] {
            let mut r = heartbeat("m", MC_ID);
            r.get_mut().serving_state = ServingState::Draining as i32;
            r.get_mut().drain_remaining_ms = drain;
            assert!(svc.meeting_heartbeat(r).await.is_ok(), "drain={drain}");
        }
    }

    /// P-1: over the bound or malformed -> skip the reconcile, never reject.
    #[test]
    fn connection_ids_are_screened_not_rejected() {
        let ok = vec!["a".to_string(), "b".to_string()];
        assert_eq!(screen_connection_ids(ok.clone(), 2), (ok, None));
        assert_eq!(
            screen_connection_ids(vec!["a".into(), "b".into(), "c".into()], 2),
            (Vec::new(), Some(HeartbeatAnomaly::ConnectionIdsOverCap))
        );
        for bad in [String::new(), "x".repeat(MAX_ID_LENGTH + 1)] {
            assert_eq!(
                screen_connection_ids(vec!["a".into(), bad], 8),
                (Vec::new(), Some(HeartbeatAnomaly::ConnectionIdMalformed))
            );
        }
    }

    #[test]
    fn the_connection_ids_bound_is_mc_wide_participants_times_live_keys() {
        let intake =
            HeartbeatIntake::new("mc".into(), HeartbeatTiming::from_timeout_ms(6000), 10).unwrap();
        assert_eq!(
            intake.max_connection_ids,
            10 * MAX_LIVE_MEDIA_CONNECTIONS_PER_HANDLER
        );
    }

    #[tokio::test]
    async fn oversized_ids_are_rejected_including_connection_id() {
        let svc = create_service();
        let long = "x".repeat(MAX_ID_LENGTH + 1);
        for req in [
            connected(&long, "p", "h", "c"),
            connected("m", &long, "h", "c"),
            connected("m", "p", &long, "c"),
            connected("m", "p", "h", &long),
        ] {
            let err = svc.notify_participant_connected(req).await.unwrap_err();
            assert_eq!(err.code(), tonic::Code::InvalidArgument);
        }
    }

    #[test]
    fn required_id_bounds() {
        assert!(validate_id_field("", "f").is_err());
        assert!(validate_id_field(&"x".repeat(MAX_ID_LENGTH), "f").is_ok());
        assert!(validate_id_field(&"x".repeat(MAX_ID_LENGTH + 1), "f").is_err());
        assert!(validate_id_field("meeting-123", "f").is_ok());
    }
}
