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

use crate::mh_connection_registry::{MhConnectionRegistry, MAX_ID_LENGTH};
use crate::observability::metrics;
use proto_gen::internal::media_coordination_service_server::MediaCoordinationService;
use proto_gen::internal::{
    ParticipantMediaConnected, ParticipantMediaConnectedResponse, ParticipantMediaDisconnected,
    ParticipantMediaDisconnectedResponse,
};
use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::{debug, info, instrument, warn};

/// MC Media Coordination gRPC service implementation.
pub struct McMediaCoordinationService {
    /// Registry tracking participant-to-MH connection state.
    registry: Arc<MhConnectionRegistry>,
}

impl McMediaCoordinationService {
    /// Create a new media coordination service.
    #[must_use]
    pub fn new(registry: Arc<MhConnectionRegistry>) -> Self {
        Self { registry }
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
        request: Request<ParticipantMediaConnected>,
    ) -> Result<Response<ParticipantMediaConnectedResponse>, Status> {
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

        info!(
            target: "mc.grpc.media_coordination",
            meeting_id = %inner.meeting_id,
            participant_id = %inner.participant_id,
            handler_id = %inner.handler_id,
            "Participant connected to MH"
        );

        Ok(Response::new(ParticipantMediaConnectedResponse {
            acknowledged: true,
        }))
    }

    /// Handle notification that a participant disconnected from an MH (R-15).
    #[instrument(skip_all, name = "mc.grpc.media_coordination.disconnected")]
    async fn notify_participant_disconnected(
        &self,
        request: Request<ParticipantMediaDisconnected>,
    ) -> Result<Response<ParticipantMediaDisconnectedResponse>, Status> {
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

        Ok(Response::new(ParticipantMediaDisconnectedResponse {
            acknowledged: true,
        }))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn create_service() -> McMediaCoordinationService {
        McMediaCoordinationService::new(Arc::new(MhConnectionRegistry::new()))
    }

    #[tokio::test]
    async fn test_notify_connected_success() {
        let svc = create_service();

        let request = Request::new(ParticipantMediaConnected {
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
        let svc = McMediaCoordinationService::new(Arc::clone(&registry));

        let request = Request::new(ParticipantMediaConnected {
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

        let request = Request::new(ParticipantMediaConnected {
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

        let request = Request::new(ParticipantMediaConnected {
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

        let request = Request::new(ParticipantMediaConnected {
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

        let request = Request::new(ParticipantMediaConnected {
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
        let svc = McMediaCoordinationService::new(Arc::clone(&registry));

        // First connect
        let connect_req = Request::new(ParticipantMediaConnected {
            meeting_id: "meeting-1".to_string(),
            participant_id: "part-1".to_string(),
            handler_id: "mh-1".to_string(),
        });
        svc.notify_participant_connected(connect_req).await.unwrap();

        // Then disconnect
        let disconnect_req = Request::new(ParticipantMediaDisconnected {
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

    #[tokio::test]
    async fn test_notify_disconnected_unknown_meeting() {
        let svc = create_service();

        let request = Request::new(ParticipantMediaDisconnected {
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

        let request = Request::new(ParticipantMediaDisconnected {
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
