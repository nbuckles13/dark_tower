//! Meeting Controller gRPC service implementation.
//!
//! Implements the `GlobalControllerService` trait for MC registration and heartbeat.
//!
//! # Security
//!
//! - All requests require JWT authentication (enforced by auth interceptor)
//! - Input validation performed on all requests
//! - Generic error messages returned to prevent information leakage

use crate::repositories::{HealthStatus, MeetingControllersRepository};
use crate::routes::AppState;
use crate::services::McAssignmentService;
use proto_gen::internal::global_controller_service_server::GlobalControllerService;
use proto_gen::internal::{
    ComprehensiveHeartbeatRequest, FastHeartbeatRequest, HeartbeatResponse,
    NotifyMeetingEndedRequest, NotifyMeetingEndedResponse, RegisterMcRequest, RegisterMcResponse,
};
use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::instrument;

/// Default fast heartbeat interval in milliseconds (10 seconds).
const DEFAULT_FAST_HEARTBEAT_INTERVAL_MS: u64 = 10_000;

/// Default comprehensive heartbeat interval in milliseconds (30 seconds).
const DEFAULT_COMPREHENSIVE_HEARTBEAT_INTERVAL_MS: u64 = 30_000;

/// Maximum allowed controller ID length.
const MAX_CONTROLLER_ID_LENGTH: usize = 255;

/// Maximum allowed meeting ID length.
const MAX_MEETING_ID_LENGTH: usize = 255;

/// Maximum allowed region length.
const MAX_REGION_LENGTH: usize = 50;

/// Maximum allowed endpoint length.
const MAX_ENDPOINT_LENGTH: usize = 255;

/// Meeting Controller gRPC service.
///
/// Handles registration and heartbeat requests from Meeting Controllers.
pub struct McService {
    state: Arc<AppState>,
}

impl McService {
    /// Create a new MC service with the given application state.
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    /// Validate a controller ID.
    #[expect(
        clippy::result_large_err,
        reason = "Status is the standard gRPC error type"
    )]
    fn validate_controller_id(id: &str) -> Result<(), Status> {
        if id.is_empty() {
            return Err(Status::invalid_argument("controller_id is required"));
        }
        if id.len() > MAX_CONTROLLER_ID_LENGTH {
            return Err(Status::invalid_argument("controller_id is too long"));
        }
        // Allow alphanumeric, hyphens, and underscores
        if !id
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return Err(Status::invalid_argument(
                "controller_id contains invalid characters",
            ));
        }
        Ok(())
    }

    /// Validate a meeting ID.
    #[expect(
        clippy::result_large_err,
        reason = "Status is the standard gRPC error type"
    )]
    fn validate_meeting_id(id: &str) -> Result<(), Status> {
        if id.is_empty() {
            return Err(Status::invalid_argument("meeting_id is required"));
        }
        if id.len() > MAX_MEETING_ID_LENGTH {
            return Err(Status::invalid_argument("meeting_id is too long"));
        }
        // Allow alphanumeric, hyphens, and underscores (same as controller_id)
        if !id
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return Err(Status::invalid_argument(
                "meeting_id contains invalid characters",
            ));
        }
        Ok(())
    }

    /// Validate a region.
    #[expect(
        clippy::result_large_err,
        reason = "Status is the standard gRPC error type"
    )]
    fn validate_region(region: &str) -> Result<(), Status> {
        if region.is_empty() {
            return Err(Status::invalid_argument("region is required"));
        }
        if region.len() > MAX_REGION_LENGTH {
            return Err(Status::invalid_argument("region is too long"));
        }
        Ok(())
    }

    /// Validate an endpoint URL.
    #[expect(
        clippy::result_large_err,
        reason = "Status is the standard gRPC error type"
    )]
    fn validate_endpoint(endpoint: &str, field_name: &str) -> Result<(), Status> {
        if endpoint.is_empty() {
            return Err(Status::invalid_argument(format!(
                "{} is required",
                field_name
            )));
        }
        if endpoint.len() > MAX_ENDPOINT_LENGTH {
            return Err(Status::invalid_argument(format!(
                "{} is too long",
                field_name
            )));
        }
        // Basic URL format validation
        if !endpoint.starts_with("http://") && !endpoint.starts_with("https://") {
            return Err(Status::invalid_argument(format!(
                "{} must be a valid URL",
                field_name
            )));
        }
        Ok(())
    }

    /// Validate capacity values.
    #[expect(
        clippy::result_large_err,
        reason = "Status is the standard gRPC error type"
    )]
    fn validate_capacity(max_meetings: u32, max_participants: u32) -> Result<(), Status> {
        if max_meetings == 0 {
            return Err(Status::invalid_argument(
                "max_meetings must be greater than 0",
            ));
        }
        if max_participants == 0 {
            return Err(Status::invalid_argument(
                "max_participants must be greater than 0",
            ));
        }
        Ok(())
    }
}

#[tonic::async_trait]
impl GlobalControllerService for McService {
    /// Register a Meeting Controller with the Global Controller.
    ///
    /// Creates or updates the controller registration in the database.
    /// Returns heartbeat intervals for the controller to use.
    #[instrument(skip_all, name = "gc.grpc.register_mc")]
    async fn register_mc(
        &self,
        request: Request<RegisterMcRequest>,
    ) -> Result<Response<RegisterMcResponse>, Status> {
        let req = request.into_inner();

        // Validate request fields
        Self::validate_controller_id(&req.id)?;
        Self::validate_region(&req.region)?;
        Self::validate_endpoint(&req.grpc_endpoint, "grpc_endpoint")?;

        // WebTransport endpoint is optional but validate if present
        if !req.webtransport_endpoint.is_empty() {
            Self::validate_endpoint(&req.webtransport_endpoint, "webtransport_endpoint")?;
        }

        Self::validate_capacity(req.max_meetings, req.max_participants)?;

        // Convert capacity to i32 for database (validated as positive above)
        let max_meetings = i32::try_from(req.max_meetings).map_err(|e| {
            Status::invalid_argument(format!("max_meetings value too large: {}", e))
        })?;
        let max_participants = i32::try_from(req.max_participants).map_err(|e| {
            Status::invalid_argument(format!("max_participants value too large: {}", e))
        })?;

        // Register in database
        let webtransport_endpoint = if req.webtransport_endpoint.is_empty() {
            None
        } else {
            Some(req.webtransport_endpoint.as_str())
        };

        MeetingControllersRepository::register_mc(
            &self.state.pool,
            &req.id,
            &req.region,
            &req.grpc_endpoint,
            webtransport_endpoint,
            max_meetings,
            max_participants,
        )
        .await
        .map_err(|e| {
            tracing::error!(target: "gc.grpc.register_mc", error = %e, "Failed to register MC");
            Status::internal("Registration failed")
        })?;

        tracing::info!(
            target: "gc.grpc.register_mc",
            controller_id = %req.id,
            region = %req.region,
            "MC registered successfully"
        );

        Ok(Response::new(RegisterMcResponse {
            accepted: true,
            message: "Registration successful".to_string(),
            fast_heartbeat_interval_ms: DEFAULT_FAST_HEARTBEAT_INTERVAL_MS,
            comprehensive_heartbeat_interval_ms: DEFAULT_COMPREHENSIVE_HEARTBEAT_INTERVAL_MS,
        }))
    }

    /// Handle fast heartbeat from a Meeting Controller.
    ///
    /// Updates capacity and health status with minimal payload.
    /// Called every 10 seconds by MCs.
    #[instrument(skip_all, name = "gc.grpc.fast_heartbeat")]
    async fn fast_heartbeat(
        &self,
        request: Request<FastHeartbeatRequest>,
    ) -> Result<Response<HeartbeatResponse>, Status> {
        let req = request.into_inner();

        // Validate controller ID
        Self::validate_controller_id(&req.controller_id)?;

        // Extract capacity
        let capacity = req
            .capacity
            .ok_or_else(|| Status::invalid_argument("capacity is required"))?;

        // Convert capacity values, clamping to i32 range
        let current_meetings = i32::try_from(capacity.current_meetings).unwrap_or(i32::MAX);
        let current_participants = i32::try_from(capacity.current_participants).unwrap_or(i32::MAX);

        // Convert health status from proto enum
        let health_status = HealthStatus::from_proto(req.health);

        // Update heartbeat in database
        let updated = MeetingControllersRepository::update_heartbeat(
            &self.state.pool,
            &req.controller_id,
            current_meetings,
            current_participants,
            health_status,
        )
        .await
        .map_err(|e| {
            tracing::error!(target: "gc.grpc.fast_heartbeat", error = %e, "Failed to update heartbeat");
            Status::internal("Heartbeat update failed")
        })?;

        if !updated {
            return Err(Status::not_found("Controller not registered"));
        }

        let timestamp = chrono::Utc::now().timestamp() as u64;

        Ok(Response::new(HeartbeatResponse {
            acknowledged: true,
            timestamp,
        }))
    }

    /// Handle comprehensive heartbeat from a Meeting Controller.
    ///
    /// Updates capacity, health status, and metrics.
    /// Called every 30 seconds by MCs.
    #[instrument(skip_all, name = "gc.grpc.comprehensive_heartbeat")]
    async fn comprehensive_heartbeat(
        &self,
        request: Request<ComprehensiveHeartbeatRequest>,
    ) -> Result<Response<HeartbeatResponse>, Status> {
        let req = request.into_inner();

        // Validate controller ID
        Self::validate_controller_id(&req.controller_id)?;

        // Extract capacity
        let capacity = req
            .capacity
            .ok_or_else(|| Status::invalid_argument("capacity is required"))?;

        // Convert capacity values
        let current_meetings = i32::try_from(capacity.current_meetings).unwrap_or(i32::MAX);
        let current_participants = i32::try_from(capacity.current_participants).unwrap_or(i32::MAX);

        // Convert health status from proto enum
        let health_status = HealthStatus::from_proto(req.health);

        // Log metrics (could be stored in a time-series database in the future)
        tracing::debug!(
            target: "gc.grpc.comprehensive_heartbeat",
            controller_id = %req.controller_id,
            cpu_usage = req.cpu_usage_percent,
            memory_usage = req.memory_usage_percent,
            current_meetings = current_meetings,
            current_participants = current_participants,
            health = ?health_status,
            "Comprehensive heartbeat received"
        );

        // Update heartbeat in database
        let updated = MeetingControllersRepository::update_heartbeat(
            &self.state.pool,
            &req.controller_id,
            current_meetings,
            current_participants,
            health_status,
        )
        .await
        .map_err(|e| {
            tracing::error!(target: "gc.grpc.comprehensive_heartbeat", error = %e, "Failed to update heartbeat");
            Status::internal("Heartbeat update failed")
        })?;

        if !updated {
            return Err(Status::not_found("Controller not registered"));
        }

        let timestamp = chrono::Utc::now().timestamp() as u64;

        Ok(Response::new(HeartbeatResponse {
            acknowledged: true,
            timestamp,
        }))
    }

    /// Handle meeting ended notification from a Meeting Controller.
    ///
    /// Called when a meeting ends (last participant leaves). Marks the
    /// assignment as ended (soft delete) for audit trail.
    #[instrument(skip_all, name = "gc.grpc.notify_meeting_ended")]
    async fn notify_meeting_ended(
        &self,
        request: Request<NotifyMeetingEndedRequest>,
    ) -> Result<Response<NotifyMeetingEndedResponse>, Status> {
        let req = request.into_inner();

        // Validate meeting_id (including character validation)
        Self::validate_meeting_id(&req.meeting_id)?;

        // Validate region
        Self::validate_region(&req.region)?;

        // End the assignment
        let count = McAssignmentService::end_assignment(
            &self.state.pool,
            &req.meeting_id,
            Some(&req.region),
        )
        .await
        .map_err(|e| {
            tracing::error!(
                target: "gc.grpc.notify_meeting_ended",
                error = %e,
                meeting_id = %req.meeting_id,
                region = %req.region,
                "Failed to end meeting assignment"
            );
            Status::internal("Failed to end meeting assignment")
        })?;

        // Note: Service layer logs with count, so we only log here for debug-level
        // when no assignment was found.
        if count == 0 {
            tracing::debug!(
                target: "gc.grpc.notify_meeting_ended",
                meeting_id = %req.meeting_id,
                region = %req.region,
                "No active assignment found to end (may already be ended)"
            );
        }

        Ok(Response::new(NotifyMeetingEndedResponse {
            acknowledged: true,
        }))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_controller_id_valid() {
        assert!(McService::validate_controller_id("mc-us-east-1-001").is_ok());
        assert!(McService::validate_controller_id("mc_123").is_ok());
        assert!(McService::validate_controller_id("MC123").is_ok());
    }

    #[test]
    fn test_validate_controller_id_empty() {
        let err = McService::validate_controller_id("").unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn test_validate_controller_id_too_long() {
        let long_id = "a".repeat(256);
        let err = McService::validate_controller_id(&long_id).unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn test_validate_controller_id_invalid_chars() {
        let err = McService::validate_controller_id("mc/invalid").unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);

        let err = McService::validate_controller_id("mc with space").unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn test_validate_region_valid() {
        assert!(McService::validate_region("us-east-1").is_ok());
        assert!(McService::validate_region("eu-west-1").is_ok());
    }

    #[test]
    fn test_validate_region_empty() {
        let err = McService::validate_region("").unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn test_validate_endpoint_valid() {
        assert!(McService::validate_endpoint("http://localhost:50051", "test").is_ok());
        assert!(McService::validate_endpoint("https://mc.example.com:443", "test").is_ok());
    }

    #[test]
    fn test_validate_endpoint_empty() {
        let err = McService::validate_endpoint("", "grpc_endpoint").unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("grpc_endpoint is required"));
    }

    #[test]
    fn test_validate_endpoint_invalid_scheme() {
        let err = McService::validate_endpoint("ftp://example.com", "test").unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn test_validate_capacity_valid() {
        assert!(McService::validate_capacity(100, 1000).is_ok());
        assert!(McService::validate_capacity(1, 1).is_ok());
    }

    #[test]
    fn test_validate_capacity_zero_meetings() {
        let err = McService::validate_capacity(0, 1000).unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn test_validate_capacity_zero_participants() {
        let err = McService::validate_capacity(100, 0).unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn test_heartbeat_intervals() {
        assert_eq!(DEFAULT_FAST_HEARTBEAT_INTERVAL_MS, 10_000);
        assert_eq!(DEFAULT_COMPREHENSIVE_HEARTBEAT_INTERVAL_MS, 30_000);
    }

    // === Boundary Tests for Validation Functions ===

    #[test]
    fn test_validate_controller_id_at_255_chars() {
        // Exactly at the limit (255 chars)
        let id_255 = "a".repeat(255);
        assert!(
            McService::validate_controller_id(&id_255).is_ok(),
            "Controller ID at 255 chars should pass"
        );
    }

    #[test]
    fn test_validate_controller_id_at_256_chars() {
        // One over the limit (256 chars)
        let id_256 = "a".repeat(256);
        let err = McService::validate_controller_id(&id_256).unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("too long"));
    }

    #[test]
    fn test_validate_region_at_50_chars() {
        // Exactly at the limit (50 chars)
        let region_50 = "r".repeat(50);
        assert!(
            McService::validate_region(&region_50).is_ok(),
            "Region at 50 chars should pass"
        );
    }

    #[test]
    fn test_validate_region_at_51_chars() {
        // One over the limit (51 chars)
        let region_51 = "r".repeat(51);
        let err = McService::validate_region(&region_51).unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("too long"));
    }

    #[test]
    fn test_validate_endpoint_at_255_chars() {
        // Exactly at the limit (255 chars) - need valid URL scheme
        // "https://" is 8 chars, so 247 more chars needed
        let endpoint_255 = format!("https://{}", "a".repeat(247));
        assert!(
            McService::validate_endpoint(&endpoint_255, "test").is_ok(),
            "Endpoint at 255 chars should pass"
        );
    }

    #[test]
    fn test_validate_endpoint_at_256_chars() {
        // One over the limit (256 chars)
        let endpoint_256 = format!("https://{}", "a".repeat(248));
        let err = McService::validate_endpoint(&endpoint_256, "test").unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("too long"));
    }

    #[test]
    fn test_validate_controller_id_at_1_char() {
        // Minimum valid length (1 char)
        assert!(
            McService::validate_controller_id("a").is_ok(),
            "Controller ID with 1 char should pass"
        );
    }

    #[test]
    fn test_validate_region_at_1_char() {
        // Minimum valid length (1 char)
        assert!(
            McService::validate_region("r").is_ok(),
            "Region with 1 char should pass"
        );
    }

    #[test]
    fn test_validate_endpoint_minimum_valid() {
        // Minimum valid endpoint with scheme
        assert!(
            McService::validate_endpoint("http://a", "test").is_ok(),
            "Minimum valid endpoint should pass"
        );
    }

    // === Meeting ID Validation Tests ===

    #[test]
    fn test_validate_meeting_id_valid() {
        assert!(McService::validate_meeting_id("meeting-123").is_ok());
        assert!(McService::validate_meeting_id("meeting_abc_123").is_ok());
        assert!(McService::validate_meeting_id("MEETING-XYZ").is_ok());
    }

    #[test]
    fn test_validate_meeting_id_empty() {
        let err = McService::validate_meeting_id("").unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("required"));
    }

    #[test]
    fn test_validate_meeting_id_too_long() {
        let long_id = "m".repeat(256);
        let err = McService::validate_meeting_id(&long_id).unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("too long"));
    }

    #[test]
    fn test_validate_meeting_id_at_255_chars() {
        let id_255 = "m".repeat(255);
        assert!(
            McService::validate_meeting_id(&id_255).is_ok(),
            "Meeting ID at 255 chars should pass"
        );
    }

    #[test]
    fn test_validate_meeting_id_invalid_chars() {
        let err = McService::validate_meeting_id("meeting/invalid").unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("invalid characters"));

        let err = McService::validate_meeting_id("meeting with space").unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("invalid characters"));

        let err = McService::validate_meeting_id("meeting@special").unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("invalid characters"));
    }

    #[test]
    fn test_validate_meeting_id_at_1_char() {
        assert!(
            McService::validate_meeting_id("m").is_ok(),
            "Meeting ID with 1 char should pass"
        );
    }
}
