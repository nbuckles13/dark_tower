//! `MediaHandlerService` gRPC server implementation (stub).
//!
//! Implements the MC→MH gRPC service from `internal.proto`.
//! This is a stub that accepts all calls and returns success responses
//! to unblock end-to-end join flow testing.
//!
//! # Security
//!
//! All incoming requests are validated by `MhAuthInterceptor` before
//! reaching these handlers.

use crate::observability::metrics;
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

/// Stub implementation of the Media Handler gRPC service.
///
/// Accepts all MC→MH calls and returns success responses.
/// No real media handling is performed.
pub struct MhMediaService;

impl MhMediaService {
    /// Create a new stub media handler service.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Default for MhMediaService {
    fn default() -> Self {
        Self::new()
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

    /// Register a meeting with the media handler (stub).
    ///
    /// Returns accepted without performing any real registration.
    #[instrument(skip_all)]
    async fn register_meeting(
        &self,
        request: Request<RegisterMeetingRequest>,
    ) -> Result<Response<RegisterMeetingResponse>, Status> {
        let req = request.into_inner();

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

        tracing::info!(
            target: "mh.grpc.service",
            meeting_id = %req.meeting_id,
            mc_id = %req.mc_id,
            "Meeting registered (stub)"
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
