//! `MediaHandlerService` gRPC server implementation.
//!
//! Implements the MC→MH gRPC service from `internal.proto`.
//! `register_meeting` is fully integrated with `SessionManagerHandle`;
//! other handlers remain stubs to unblock end-to-end join flow testing.
//!
//! # Security
//!
//! All incoming requests are validated by `MhAuthInterceptor` before
//! reaching these handlers.

use std::time::Instant;

use crate::observability::metrics;
use crate::session::{MeetingRegistration, SessionManagerHandle};
use proto_gen::internal::media_handler_service_server::MediaHandlerService;
use proto_gen::internal::{
    MediaTelemetry, RegisterMeetingRequest, RegisterMeetingResponse, RegisterParticipant,
    RegisterParticipantResponse, RouteMediaCommand, RouteMediaResponse, TelemetryAck,
};
use tonic::{Request, Response, Status, Streaming};
use tracing::instrument;

/// Returns a stub placeholder value for fields that would contain real data
/// in a production implementation.
fn stub_placeholder() -> String {
    String::from("STUB-PLACEHOLDER")
}

/// Maximum allowed length for `meeting_id` and `mc_id` fields.
/// Prevents `HashMap` key bloat from malicious or buggy callers.
const MAX_ID_LENGTH: usize = 256;

/// Maximum allowed length for `mc_grpc_endpoint`.
/// 2048 bytes is generous for any legitimate gRPC endpoint URL.
const MAX_ENDPOINT_LENGTH: usize = 2048;

/// Media Handler gRPC service.
///
/// Handles MC→MH RPCs. `register_meeting` is fully integrated with
/// `SessionManagerHandle`; other handlers remain stubs.
pub struct MhMediaService {
    session_manager: SessionManagerHandle,
}

impl MhMediaService {
    /// Create a new media handler service with the given session manager handle.
    #[must_use]
    pub fn new(session_manager: SessionManagerHandle) -> Self {
        Self { session_manager }
    }
}

impl Default for MhMediaService {
    fn default() -> Self {
        Self::new(SessionManagerHandle::new())
    }
}

#[tonic::async_trait]
impl MediaHandlerService for MhMediaService {
    /// Register a participant with the media handler (stub).
    ///
    /// Returns a stub connection token and media handler URL.
    #[instrument(skip_all)]
    async fn register(
        &self,
        request: Request<RegisterParticipant>,
    ) -> Result<Response<RegisterParticipantResponse>, Status> {
        let req = request.into_inner();

        // Basic validation
        if req.participant_id.is_empty() {
            metrics::record_grpc_request("register", "error");
            return Err(Status::invalid_argument("participant_id is required"));
        }
        if req.meeting_id.is_empty() {
            metrics::record_grpc_request("register", "error");
            return Err(Status::invalid_argument("meeting_id is required"));
        }

        tracing::info!(
            target: "mh.grpc.service",
            stream_count = req.streams.len(),
            "Participant registered (stub)"
        );

        metrics::record_grpc_request("register", "success");

        let stub_response = RegisterParticipantResponse {
            connection_token: stub_placeholder(),
            media_handler_url: "stub://localhost".to_string(),
        };
        Ok(Response::new(stub_response))
    }

    /// Register a meeting with the media handler.
    ///
    /// Called by MC when a participant is assigned to this MH instance.
    /// Stores the meeting registration in `SessionManagerHandle` and promotes
    /// any pending WebTransport connections that were waiting for this meeting.
    #[instrument(skip_all)]
    async fn register_meeting(
        &self,
        request: Request<RegisterMeetingRequest>,
    ) -> Result<Response<RegisterMeetingResponse>, Status> {
        let req = request.into_inner();

        // Validate required fields
        if req.meeting_id.is_empty() {
            metrics::record_grpc_request("register_meeting", "error");
            return Err(Status::invalid_argument("meeting_id is required"));
        }
        if req.mc_id.is_empty() {
            metrics::record_grpc_request("register_meeting", "error");
            return Err(Status::invalid_argument("mc_id is required"));
        }
        if req.mc_grpc_endpoint.is_empty() {
            metrics::record_grpc_request("register_meeting", "error");
            return Err(Status::invalid_argument("mc_grpc_endpoint is required"));
        }

        // Validate field lengths to prevent HashMap key bloat
        if req.meeting_id.len() > MAX_ID_LENGTH {
            metrics::record_grpc_request("register_meeting", "error");
            return Err(Status::invalid_argument(
                "meeting_id exceeds maximum length",
            ));
        }
        if req.mc_id.len() > MAX_ID_LENGTH {
            metrics::record_grpc_request("register_meeting", "error");
            return Err(Status::invalid_argument("mc_id exceeds maximum length"));
        }

        // Validate mc_grpc_endpoint length
        if req.mc_grpc_endpoint.len() > MAX_ENDPOINT_LENGTH {
            metrics::record_grpc_request("register_meeting", "error");
            return Err(Status::invalid_argument(
                "mc_grpc_endpoint exceeds maximum length",
            ));
        }

        // Validate mc_grpc_endpoint scheme
        if !req.mc_grpc_endpoint.starts_with("http://")
            && !req.mc_grpc_endpoint.starts_with("https://")
            && !req.mc_grpc_endpoint.starts_with("grpc://")
        {
            metrics::record_grpc_request("register_meeting", "error");
            return Err(Status::invalid_argument(
                "mc_grpc_endpoint must use http://, https://, or grpc:// scheme",
            ));
        }

        let promoted = self
            .session_manager
            .register_meeting(
                req.meeting_id.clone(),
                MeetingRegistration {
                    mc_id: req.mc_id.clone(),
                    mc_grpc_endpoint: req.mc_grpc_endpoint.clone(),
                    registered_at: Instant::now(),
                },
            )
            .await;

        let promoted_count = promoted.len();

        tracing::info!(
            target: "mh.grpc.service",
            meeting_id = %req.meeting_id,
            mc_id = %req.mc_id,
            promoted_pending_count = promoted_count,
            "Meeting registered"
        );

        tracing::debug!(
            target: "mh.grpc.service",
            meeting_id = %req.meeting_id,
            mc_grpc_endpoint = %req.mc_grpc_endpoint,
            "Meeting registration details"
        );

        metrics::record_grpc_request("register_meeting", "success");

        Ok(Response::new(RegisterMeetingResponse { accepted: true }))
    }

    /// Route media between participants (stub).
    ///
    /// Returns success without performing any routing.
    #[instrument(skip_all)]
    async fn route_media(
        &self,
        request: Request<RouteMediaCommand>,
    ) -> Result<Response<RouteMediaResponse>, Status> {
        let req = request.into_inner();

        tracing::info!(
            target: "mh.grpc.service",
            destination_count = req.destination_participant_ids.len(),
            cascade_count = req.cascade_destinations.len(),
            "RouteMedia received (stub)"
        );

        metrics::record_grpc_request("route_media", "success");

        Ok(Response::new(RouteMediaResponse {
            success: true,
            error_message: String::new(),
        }))
    }

    /// Receive telemetry stream from MC (stub).
    ///
    /// Acknowledges the stream without processing telemetry data.
    #[instrument(skip_all)]
    async fn stream_telemetry(
        &self,
        request: Request<Streaming<MediaTelemetry>>,
    ) -> Result<Response<TelemetryAck>, Status> {
        let mut stream = request.into_inner();

        // Consume the stream (log first message, drain rest)
        let mut count: u64 = 0;
        while let Some(result) = stream_next(&mut stream).await {
            match result {
                Ok(_telemetry) => {
                    if count == 0 {
                        tracing::info!(
                            target: "mh.grpc.service",
                            "StreamTelemetry started (stub)"
                        );
                    }
                    count = count.saturating_add(1);
                }
                Err(e) => {
                    tracing::warn!(
                        target: "mh.grpc.service",
                        error = %e,
                        messages_received = count,
                        "StreamTelemetry error"
                    );
                    metrics::record_grpc_request("stream_telemetry", "error");
                    return Err(e);
                }
            }
        }

        tracing::info!(
            target: "mh.grpc.service",
            messages_received = count,
            "StreamTelemetry completed (stub)"
        );

        metrics::record_grpc_request("stream_telemetry", "success");

        Ok(Response::new(TelemetryAck { received: true }))
    }
}

/// Helper to get next item from a streaming request.
///
/// Wraps `stream.message()` to work with `while let Some` pattern.
async fn stream_next(
    stream: &mut Streaming<MediaTelemetry>,
) -> Option<Result<MediaTelemetry, Status>> {
    match stream.message().await {
        Ok(Some(msg)) => Some(Ok(msg)),
        Ok(None) => None,
        Err(e) => Some(Err(e)),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::session::PendingConnection;

    fn make_service() -> (MhMediaService, SessionManagerHandle) {
        let sm = SessionManagerHandle::new();
        let svc = MhMediaService::new(sm.clone());
        (svc, sm)
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
        })
    }

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

        // http://
        assert!(svc
            .register_meeting(make_register_request("m-1", "mc-1", "http://mc:50052"))
            .await
            .is_ok());

        // https://
        assert!(svc
            .register_meeting(make_register_request("m-2", "mc-1", "https://mc:50052"))
            .await
            .is_ok());

        // grpc://
        assert!(svc
            .register_meeting(make_register_request("m-3", "mc-1", "grpc://mc:50052"))
            .await
            .is_ok());
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

        // Add pending connections before RegisterMeeting
        sm.add_pending_connection(PendingConnection {
            connection_id: "conn-1".to_string(),
            meeting_id: "meeting-1".to_string(),
            participant_id: "user-1".to_string(),
            connected_at: Instant::now(),
        })
        .await;

        sm.add_pending_connection(PendingConnection {
            connection_id: "conn-2".to_string(),
            meeting_id: "meeting-1".to_string(),
            participant_id: "user-2".to_string(),
            connected_at: Instant::now(),
        })
        .await;

        assert_eq!(sm.active_connection_count().await, 0);

        // RegisterMeeting should promote pending connections
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

        // First registration
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

        // Second registration overwrites
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

    #[tokio::test]
    async fn test_default_creates_working_service() {
        let svc = MhMediaService::default();

        let resp = svc
            .register_meeting(make_register_request(
                "meeting-1",
                "mc-1",
                "http://mc:50052",
            ))
            .await
            .unwrap();

        assert!(resp.into_inner().accepted);
    }
}
