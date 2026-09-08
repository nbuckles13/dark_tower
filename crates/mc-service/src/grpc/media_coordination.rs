//! Media Coordination gRPC Service (R-15).
//!
//! Implements the `MediaCoordinationService` that Media Handlers call to notify
//! MC of participant connection/disconnection events.
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
use crate::mh_connection_registry::{MhConnectionRegistry, MAX_ID_LENGTH};
use crate::observability::metrics;
use proto_gen::dark_tower::internal::v1::media_coordination_service_server::MediaCoordinationService;
use proto_gen::dark_tower::internal::v1::{
    NotifyParticipantConnectedRequest, NotifyParticipantConnectedResponse,
    NotifyParticipantDisconnectedRequest, NotifyParticipantDisconnectedResponse,
};
use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::{debug, info, instrument, warn};

/// MC Media Coordination gRPC service implementation.
pub struct McMediaCoordinationService {
    /// Registry tracking participant-to-MH connection state.
    registry: Arc<MhConnectionRegistry>,
    /// Route to the meeting actors, which hold the allocated `sender_id`s.
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
    pub fn new(
        registry: Arc<MhConnectionRegistry>,
        controller: Arc<MeetingControllerActorHandle>,
    ) -> Self {
        Self {
            registry,
            controller,
        }
    }

    /// Resolve the wire `sender_id` for a connecting participant.
    ///
    /// `token_sub` is `NotifyParticipantConnectedRequest.participant_id`, which
    /// by contract carries **the validated meeting token's `sub`** — never a
    /// client-supplied hint (@security S1). MC stores that value as a
    /// participant's `user_id`, so that is what it is resolved against; MC's own
    /// per-join `participant_id` is a UUID MH has never seen. See
    /// `MeetingMessage::GetSenderIdForUser` for the full seam.
    ///
    /// Returns the value to put on the wire and the outcome to count. **The
    /// only path that yields a non-zero value is a live, UNAMBIGUOUS roster
    /// hit**; every other path yields `0`, which MH treats as a reject.
    ///
    /// # MC never invents an id
    ///
    /// There is deliberately no fallback, no default and no "nearest" id. The
    /// type system carries most of this: [`SenderId`](crate::media_admission::SenderId)
    /// wraps `NonZeroU16` and its only production constructor is the per-meeting
    /// allocator, so a fabricated ordinal is *unconstructible* here rather than
    /// merely discouraged. This function's job is to make the *unresolvable*
    /// case expressible, and `0` is how it is expressed.
    ///
    /// # Why a registry refusal declines the binding
    ///
    /// `registered == false` means the per-meeting connection cap was hit and MC
    /// is **not tracking this connection**. Handing back a real ordinal anyway
    /// would let MH forward media for a connection MC has no record of — and
    /// will never send `NotifyParticipantDisconnected` for, because there is no
    /// registry entry to tear down. The two sides would disagree about whether
    /// the connection exists, and nothing would surface the disagreement. So the
    /// honest answer is the same `0` MC gives for any participant it is not
    /// tracking, under its own [`SenderBindingOutcome::RegistryFull`] so
    /// cap exhaustion is never triaged as a join race.
    async fn resolve_sender_binding(
        &self,
        meeting_id: &str,
        token_sub: &str,
        registered: bool,
    ) -> (u32, SenderBindingOutcome) {
        if !registered {
            return (0, SenderBindingOutcome::RegistryFull);
        }

        // A missing meeting and a dead/closed meeting actor both land here.
        // Both mean "MC cannot answer for this meeting", and both are counted
        // as `meeting_unknown` rather than silently degrading to the
        // participant-level arm -- the remedies differ from a join race.
        let Ok(meeting) = self
            .controller
            .get_meeting_handle(meeting_id.to_string())
            .await
        else {
            return (0, SenderBindingOutcome::MeetingUnknown);
        };

        match meeting.get_sender_id_for_user(token_sub.to_string()).await {
            Ok(SenderLookup::Found(sender_id)) => (
                u32::from(sender_id.get().get()),
                SenderBindingOutcome::Resolved,
            ),
            // Not on the roster: the honest "I do not know this participant".
            Ok(SenderLookup::NotFound) => (0, SenderBindingOutcome::ParticipantUnknown),
            // Two participants share this `sub`; the question has no single
            // answer and MC will not guess one.
            Ok(SenderLookup::Ambiguous) => (0, SenderBindingOutcome::UserAmbiguous),
            // The actor went away between the handle lookup and the query. The
            // meeting, not the participant, is what MC cannot answer for.
            Err(_) => (0, SenderBindingOutcome::MeetingUnknown),
        }
    }
}

/// Validate that an ID field is non-empty and within length bounds.
#[allow(clippy::result_large_err)] // tonic::Status is inherently large; standard tonic pattern
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

        debug!(
            target: "mc.grpc.media_coordination",
            meeting_id = %inner.meeting_id,
            participant_id = %inner.participant_id,
            handler_id = %inner.handler_id,
            "Participant connected notification received"
        );

        let added = self
            .registry
            .add_connection(&inner.meeting_id, &inner.participant_id, &inner.handler_id)
            .await;

        if !added {
            warn!(
                target: "mc.grpc.media_coordination",
                meeting_id = %inner.meeting_id,
                "Connection registry limit reached for meeting"
            );
        }

        metrics::record_mh_notification("connected");

        let (sender_id, outcome) = self
            .resolve_sender_binding(&inner.meeting_id, &inner.participant_id, added)
            .await;
        metrics::record_sender_binding_response(outcome);

        info!(
            target: "mc.grpc.media_coordination",
            meeting_id = %inner.meeting_id,
            participant_id = %inner.participant_id,
            handler_id = %inner.handler_id,
            // The OUTCOME, never the id. ADR-0036 §11 bars the `sender_id`
            // value as a per-participant log dimension; the bounded outcome
            // token carries every bit of triage signal the value would.
            sender_binding = outcome.label(),
            "Participant connected to MH"
        );

        Ok(Response::new(NotifyParticipantConnectedResponse {
            // Unchanged meaning: the notification was received and the registry
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

        debug!(
            target: "mc.grpc.media_coordination",
            meeting_id = %inner.meeting_id,
            participant_id = %inner.participant_id,
            handler_id = %inner.handler_id,
            reason = ?inner.reason,
            "Participant disconnected notification received"
        );

        let removed = self
            .registry
            .remove_connection(&inner.meeting_id, &inner.participant_id, &inner.handler_id)
            .await;

        if !removed {
            debug!(
                target: "mc.grpc.media_coordination",
                meeting_id = %inner.meeting_id,
                participant_id = %inner.participant_id,
                handler_id = %inner.handler_id,
                "Connection was not in registry (may have already been removed)"
            );
        }

        metrics::record_mh_notification("disconnected");

        info!(
            target: "mc.grpc.media_coordination",
            meeting_id = %inner.meeting_id,
            participant_id = %inner.participant_id,
            handler_id = %inner.handler_id,
            "Participant disconnected from MH"
        );

        Ok(Response::new(NotifyParticipantDisconnectedResponse {
            acknowledged: true,
        }))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// A controller with no meetings.
    ///
    /// Every unit test in this module exercises request VALIDATION or the
    /// registry, not sender resolution — so an empty controller is the honest
    /// fixture: resolution correctly answers `0`/`meeting_unknown` throughout.
    /// The resolution arms live in
    /// `tests/media_coordination_integration.rs`, where a real meeting actor
    /// can be seeded and the wire value asserted.
    fn test_controller() -> Arc<MeetingControllerActorHandle> {
        Arc::new(MeetingControllerActorHandle::new(
            "mc-media-coord-unit".to_string(),
            crate::actors::ActorMetrics::new(),
            crate::actors::ControllerMetrics::new(),
            ::common::secret::SecretBox::new(Box::new(vec![0u8; 32])),
            Arc::new(MhConnectionRegistry::new()),
            Arc::new(crate::media_routing::PolicyGenerations::new()),
        ))
    }

    fn create_service() -> McMediaCoordinationService {
        McMediaCoordinationService::new(Arc::new(MhConnectionRegistry::new()), test_controller())
    }

    #[tokio::test]
    async fn test_notify_connected_success() {
        let svc = create_service();

        let request = Request::new(NotifyParticipantConnectedRequest {
            meeting_id: "meeting-1".to_string(),
            participant_id: "part-1".to_string(),
            handler_id: "mh-1".to_string(),
        });

        let response = svc.notify_participant_connected(request).await;
        assert!(response.is_ok());
        assert!(response.unwrap().into_inner().acknowledged);
    }

    #[tokio::test]
    async fn test_notify_connected_updates_registry() {
        let registry = Arc::new(MhConnectionRegistry::new());
        let svc = McMediaCoordinationService::new(Arc::clone(&registry), test_controller());

        let request = Request::new(NotifyParticipantConnectedRequest {
            meeting_id: "meeting-1".to_string(),
            participant_id: "part-1".to_string(),
            handler_id: "mh-1".to_string(),
        });

        svc.notify_participant_connected(request).await.unwrap();

        let conns = registry.get_connections("meeting-1", "part-1").await;
        assert_eq!(conns.len(), 1);
        assert_eq!(conns[0].handler_id, "mh-1");
    }

    #[tokio::test]
    async fn test_notify_connected_empty_meeting_id() {
        let svc = create_service();

        let request = Request::new(NotifyParticipantConnectedRequest {
            meeting_id: String::new(),
            participant_id: "part-1".to_string(),
            handler_id: "mh-1".to_string(),
        });

        let result = svc.notify_participant_connected(request).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn test_notify_connected_empty_participant_id() {
        let svc = create_service();

        let request = Request::new(NotifyParticipantConnectedRequest {
            meeting_id: "meeting-1".to_string(),
            participant_id: String::new(),
            handler_id: "mh-1".to_string(),
        });

        let result = svc.notify_participant_connected(request).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn test_notify_connected_empty_handler_id() {
        let svc = create_service();

        let request = Request::new(NotifyParticipantConnectedRequest {
            meeting_id: "meeting-1".to_string(),
            participant_id: "part-1".to_string(),
            handler_id: String::new(),
        });

        let result = svc.notify_participant_connected(request).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn test_notify_connected_oversized_id() {
        let svc = create_service();

        let request = Request::new(NotifyParticipantConnectedRequest {
            meeting_id: "a".repeat(257),
            participant_id: "part-1".to_string(),
            handler_id: "mh-1".to_string(),
        });

        let result = svc.notify_participant_connected(request).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn test_notify_disconnected_success() {
        let registry = Arc::new(MhConnectionRegistry::new());
        let svc = McMediaCoordinationService::new(Arc::clone(&registry), test_controller());

        // First connect
        let connect_req = Request::new(NotifyParticipantConnectedRequest {
            meeting_id: "meeting-1".to_string(),
            participant_id: "part-1".to_string(),
            handler_id: "mh-1".to_string(),
        });
        svc.notify_participant_connected(connect_req).await.unwrap();

        // Then disconnect
        let disconnect_req = Request::new(NotifyParticipantDisconnectedRequest {
            meeting_id: "meeting-1".to_string(),
            participant_id: "part-1".to_string(),
            handler_id: "mh-1".to_string(),
            reason: 1, // CLIENT_CLOSED
        });
        let response = svc.notify_participant_disconnected(disconnect_req).await;
        assert!(response.is_ok());
        assert!(response.unwrap().into_inner().acknowledged);

        // Verify removed from registry
        let conns = registry.get_connections("meeting-1", "part-1").await;
        assert!(conns.is_empty());
    }

    /// Coordination round-trip covering multi-MH and idempotent retry paths.
    ///
    /// Delta vs `test_notify_disconnected_success`:
    /// - Multi-MH per participant (existing test uses single MH per participant)
    /// - Interleaved disconnect: removing one handler must not affect siblings
    ///   (exercises the per-handler `retain` path in
    ///   `MhConnectionRegistry::remove_connection`)
    /// - Idempotent second-disconnect on a tuple already removed: returns Ok
    ///   with `acknowledged=true` (rollback-safe invariant for MH retries /
    ///   late deliveries after meeting teardown — ops concern, R-15 spec)
    #[tokio::test]
    async fn test_coordination_flow_connect_disconnect_round_trip() {
        let registry = Arc::new(MhConnectionRegistry::new());
        let svc = McMediaCoordinationService::new(Arc::clone(&registry), test_controller());

        // Participant connects to two MHs (active/active topology).
        let connect_mh1 = Request::new(NotifyParticipantConnectedRequest {
            meeting_id: "meeting-round-trip".to_string(),
            participant_id: "part-rt".to_string(),
            handler_id: "mh-alpha".to_string(),
        });
        svc.notify_participant_connected(connect_mh1).await.unwrap();

        let connect_mh2 = Request::new(NotifyParticipantConnectedRequest {
            meeting_id: "meeting-round-trip".to_string(),
            participant_id: "part-rt".to_string(),
            handler_id: "mh-beta".to_string(),
        });
        svc.notify_participant_connected(connect_mh2).await.unwrap();

        let conns = registry
            .get_connections("meeting-round-trip", "part-rt")
            .await;
        assert_eq!(conns.len(), 2, "both MH connections should be tracked");

        // Disconnect from mh-alpha only: mh-beta must remain intact (sibling preservation).
        let disconnect_mh1 = Request::new(NotifyParticipantDisconnectedRequest {
            meeting_id: "meeting-round-trip".to_string(),
            participant_id: "part-rt".to_string(),
            handler_id: "mh-alpha".to_string(),
            reason: 1,
        });
        let ack1 = svc
            .notify_participant_disconnected(disconnect_mh1)
            .await
            .unwrap();
        assert!(ack1.into_inner().acknowledged);

        let conns = registry
            .get_connections("meeting-round-trip", "part-rt")
            .await;
        assert_eq!(
            conns.len(),
            1,
            "removing mh-alpha should leave mh-beta tracked"
        );
        assert_eq!(conns[0].handler_id, "mh-beta");

        // Disconnect from mh-beta: participant and meeting should be cleaned up.
        let disconnect_mh2 = Request::new(NotifyParticipantDisconnectedRequest {
            meeting_id: "meeting-round-trip".to_string(),
            participant_id: "part-rt".to_string(),
            handler_id: "mh-beta".to_string(),
            reason: 1,
        });
        let ack2 = svc
            .notify_participant_disconnected(disconnect_mh2)
            .await
            .unwrap();
        assert!(ack2.into_inner().acknowledged);

        assert_eq!(
            registry.meeting_count().await,
            0,
            "meeting should be cleaned up once all connections are dropped"
        );

        // Idempotent retry: MH may resend disconnect for a tuple the registry
        // has already cleared. MC must return Ok, not a gRPC error — a gRPC
        // error would page operators via error-rate alerts.
        let disconnect_retry = Request::new(NotifyParticipantDisconnectedRequest {
            meeting_id: "meeting-round-trip".to_string(),
            participant_id: "part-rt".to_string(),
            handler_id: "mh-beta".to_string(),
            reason: 1,
        });
        let ack_retry = svc
            .notify_participant_disconnected(disconnect_retry)
            .await
            .unwrap();
        assert!(
            ack_retry.into_inner().acknowledged,
            "idempotent re-disconnect must return acknowledged=true"
        );
    }

    #[tokio::test]
    async fn test_notify_disconnected_unknown_meeting() {
        let svc = create_service();

        let request = Request::new(NotifyParticipantDisconnectedRequest {
            meeting_id: "unknown-meeting".to_string(),
            participant_id: "part-1".to_string(),
            handler_id: "mh-1".to_string(),
            reason: 0,
        });

        // Should succeed even if connection wasn't tracked
        let response = svc.notify_participant_disconnected(request).await;
        assert!(response.is_ok());
        assert!(response.unwrap().into_inner().acknowledged);
    }

    #[tokio::test]
    async fn test_notify_disconnected_empty_fields() {
        let svc = create_service();

        let request = Request::new(NotifyParticipantDisconnectedRequest {
            meeting_id: String::new(),
            participant_id: "part-1".to_string(),
            handler_id: "mh-1".to_string(),
            reason: 0,
        });

        let result = svc.notify_participant_disconnected(request).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn test_validate_id_field_empty() {
        let result = validate_id_field("", "test_field");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_validate_id_field_too_long() {
        let result = validate_id_field(&"a".repeat(257), "test_field");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_validate_id_field_valid() {
        let result = validate_id_field("valid-uuid-123", "test_field");
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_validate_id_field_at_max_length() {
        let result = validate_id_field(&"a".repeat(256), "test_field");
        assert!(result.is_ok());
    }
}
