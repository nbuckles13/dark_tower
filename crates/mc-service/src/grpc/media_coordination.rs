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
//! resolved by exact match against the meeting's frozen set. They are recorded
//! in ONE place, the meeting actor, in the same turn that answers the sender
//! binding (`MeetingActorHandle::media_connected`). There is no registry beside
//! it. The client-reported `MediaConnectionUpdate` map is never consulted.
//!
//! **Hazard, stated at the ingestion site (Lead ruling R3):** connectivity is
//! edge-triggered and NOTHING re-asserts it. A Disconnected that is never sent
//! (MH crash, OOM, kill; `MeetingReleased` by design; retries exhausted on a
//! live MH) leaves MC believing the participant is still on that handler, so MC
//! keeps placing its edges there — **sticky silence that presents as healthy**,
//! with no event that repairs it until story 4's handler-restart detection and
//! re-assert land (`docs/TODO.md` §Media Path Obligations). The reverse
//! direction is closed: a stale Disconnected from an old session cannot remove
//! a newer live session, because connectivity is keyed by `connection_id`.
//!
//! # RPCs
//!
//! - `NotifyParticipantConnected` — MH informs MC that a participant has
//!   established a WebTransport connection to the MH.
//! - `NotifyParticipantDisconnected` — MH informs MC that a participant's
//!   WebTransport connection to the MH has dropped.
//!
//! # Security
//!
//! Authentication is handled by `McAuthLayer` (applied at the server level in main.rs).
//! This handler only needs to validate request field constraints.
//! Generic error messages prevent information leakage (ADR-0003).

use crate::actors::messages::SenderLookup;
use crate::actors::MeetingControllerActorHandle;
use crate::media_admission::SenderBindingOutcome;
use crate::media_routing::Unapplied;
use crate::observability::metrics;
use proto_gen::dark_tower::internal::v1::media_coordination_service_server::MediaCoordinationService;
use proto_gen::dark_tower::internal::v1::{
    NotifyParticipantConnectedRequest, NotifyParticipantConnectedResponse,
    NotifyParticipantDisconnectedRequest, NotifyParticipantDisconnectedResponse,
};
use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::{debug, info, instrument};

/// Maximum ID field length (bytes), for every id field on these RPCs —
/// `meeting_id`, `participant_id`, `handler_id` and `connection_id`. One bound,
/// one home (it used to live in the retired `mh_connection_registry.rs`).
pub const MAX_ID_LENGTH: usize = 256;

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
}

impl McMediaCoordinationService {
    /// Create a new media coordination service.
    #[must_use]
    pub fn new(controller: Arc<MeetingControllerActorHandle>) -> Self {
        Self { controller }
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
            // The actor went away between the handle lookup and the query.
            Err(_) => (
                0,
                SenderBindingOutcome::MeetingUnknown,
                Some(Unapplied::MeetingUnknown),
            ),
        }
    }
}

/// Validate that an ID field is non-empty and within length bounds.
fn validate_id_field(value: &str, field_name: &str) -> Result<(), Status> {
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

/// Validate the OPTIONAL `connection_id`: empty is allowed (a legacy MH that
/// predates the field), a non-empty value is length-bounded.
///
/// **Deliberately a separate function, never an `allow_empty` flag on
/// [`validate_id_field`]** (security): a permissive variant of the shared
/// helper is one careless call away from letting an empty `participant_id`
/// reach `sub` resolution. `tests::identity_fields_still_reject_empty` exists
/// to fail if the two are ever unified. The value is opaque: never parsed, no
/// shape asserted.
fn validate_optional_id_field(value: &str, field_name: &str) -> Result<(), Status> {
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
        validate_optional_id_field(&inner.connection_id, "connection_id")?;

        metrics::record_mh_notification("connected");
        if inner.connection_id.is_empty() {
            metrics::record_notification_without_connection_id();
        }

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
        validate_optional_id_field(&inner.connection_id, "connection_id")?;

        metrics::record_mh_notification("disconnected");
        if inner.connection_id.is_empty() {
            metrics::record_notification_without_connection_id();
        }

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
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A controller with no meetings. Unit tests here exercise request
    /// VALIDATION and the no-meeting arms; the resolution and connectivity arms
    /// need a real meeting actor and live in
    /// `tests/media_coordination_integration.rs` and
    /// `tests/slot_placement_integration.rs`.
    fn create_service() -> McMediaCoordinationService {
        McMediaCoordinationService::new(Arc::new(MeetingControllerActorHandle::new(
            "mc-media-coord-unit".to_string(),
            crate::actors::ActorMetrics::new(),
            crate::actors::ControllerMetrics::new(),
            ::common::secret::SecretBox::new(Box::new(vec![0u8; 32])),
            Arc::new(crate::media_routing::PolicyGenerations::new()),
            crate::media_admission::fixtures::kek_lifecycle(),
        )))
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

    /// Security: the permissive optional validator must never leak into the
    /// identity fields. Fails if someone unifies the two helpers.
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

    #[tokio::test]
    async fn an_empty_connection_id_is_accepted_as_legacy() {
        let svc = create_service();
        assert!(svc
            .notify_participant_connected(connected("m", "p", "h", ""))
            .await
            .is_ok());
        assert!(svc
            .notify_participant_disconnected(disconnected("m", "p", "h", ""))
            .await
            .is_ok());
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
    fn optional_connection_id_bounds() {
        assert!(validate_optional_id_field("", "connection_id").is_ok());
        assert!(validate_optional_id_field(&"x".repeat(MAX_ID_LENGTH), "connection_id").is_ok());
        assert!(
            validate_optional_id_field(&"x".repeat(MAX_ID_LENGTH + 1), "connection_id").is_err()
        );
    }

    #[test]
    fn required_id_bounds() {
        assert!(validate_id_field("", "f").is_err());
        assert!(validate_id_field(&"x".repeat(MAX_ID_LENGTH), "f").is_ok());
        assert!(validate_id_field(&"x".repeat(MAX_ID_LENGTH + 1), "f").is_err());
        assert!(validate_id_field("meeting-123", "f").is_ok());
    }
}
