//! Meeting Controller gRPC service implementation.
//!
//! Implements the `GlobalControllerService` trait for MC registration,
//! heartbeat, meeting-end notification and the `AcquireMediaHandler`
//! replacement grant (ADR-0039 §3; logic in `services::mh_replacement`).
//!
//! # Security
//!
//! - All requests require JWT authentication (enforced by auth interceptor)
//! - Input validation performed on all requests
//! - Generic error messages returned to prevent information leakage

use crate::auth::Claims;
use crate::observability::metrics::{self, GcView, ReplacementTrigger, UsageField, UsageSource};
use crate::repositories::{
    AssignmentId, EndAssignmentOutcome, HealthStatus, MeetingControllersRepository,
};
use crate::routes::AppState;
use crate::services::mh_replacement::{self, Rejection, Report, Validated};
use crate::services::weighted_pick::{SelectionRng, SystemSelectionRng};
use crate::services::McAssignmentService;
use proto_gen::dark_tower::internal::v1::global_controller_service_server::GlobalControllerService;
use proto_gen::dark_tower::internal::v1::{
    AcquireMediaHandlerRequest, AcquireMediaHandlerResponse, ComprehensiveHeartbeatRequest,
    ComprehensiveHeartbeatResponse, FastHeartbeatRequest, FastHeartbeatResponse, MhAssignment,
    NotifyMeetingEndedOutcome, NotifyMeetingEndedRequest, NotifyMeetingEndedResponse,
    RegisterMcRequest, RegisterMcResponse,
};
use std::sync::Arc;
use std::time::Instant;
use tonic::{Request, Response, Status};
use tracing::instrument;

/// Default fast heartbeat interval in milliseconds (10 seconds), pushed to
/// every MC in `RegisterMcResponse`. `McAssignmentConfig` refuses an MC
/// staleness threshold that does not exceed it.
pub(crate) const DEFAULT_FAST_HEARTBEAT_INTERVAL_MS: u64 = 10_000;

/// Default comprehensive heartbeat interval in milliseconds (30 seconds).
const DEFAULT_COMPREHENSIVE_HEARTBEAT_INTERVAL_MS: u64 = 30_000;

/// Maximum allowed controller ID length.
const MAX_CONTROLLER_ID_LENGTH: usize = 255;

/// Maximum allowed meeting ID length.
const MAX_MEETING_ID_LENGTH: usize = 255;

/// Meeting Controller gRPC service.
///
/// Handles registration and heartbeat requests from Meeting Controllers.
pub struct McService {
    state: Arc<AppState>,
    /// Draw source for `AcquireMediaHandler` selection: the system CSPRNG in
    /// production; a seeded source only via [`Self::with_selection_rng`].
    selection_rng: Arc<dyn SelectionRng>,
}

impl McService {
    /// Create a new MC service with the given application state.
    pub fn new(state: Arc<AppState>) -> Self {
        Self::with_selection_rng(state, Arc::new(SystemSelectionRng))
    }

    /// TEST SEAM: inject the selection draw source, so a test can prove that
    /// `AcquireMediaHandler` selection is identical with and without
    /// replacement evidence (ADR-0039 §9, seeded RNG). Production uses
    /// [`Self::new`].
    pub fn with_selection_rng(state: Arc<AppState>, selection_rng: Arc<dyn SelectionRng>) -> Self {
        Self {
            state,
            selection_rng,
        }
    }

    /// Validate an `AcquireMediaHandler` request, cheapest reject first.
    ///
    /// On rejection, returns the bounded status and a [`Report`] holding only
    /// VALIDATED values (a malformed id never reaches a log or a label). The
    /// trigger is projected before any check, so its label is always defined.
    fn validate_acquire(
        req: AcquireMediaHandlerRequest,
        max_exclude_handler_ids: u32,
    ) -> Result<Validated, (Rejection, Report)> {
        let AcquireMediaHandlerRequest {
            meeting_id,
            controller_id,
            exclude_handler_ids,
            trigger,
            replaced_handler_id,
        } = req;

        // Each check runs ONCE; its result feeds both the gate and the Report
        // (which may carry only values that passed).
        let trigger = mh_replacement::project_trigger(trigger);
        let replaced_check = replaced_handler_id
            .as_deref()
            .map(|id| super::validate_handler_id(id, "replaced_handler_id"));
        let controller_check = Self::validate_controller_id(&controller_id);
        let report = Report {
            trigger: trigger.unwrap_or(ReplacementTrigger::Invalid),
            gc_view: GcView::Unevaluated,
            replaced_handler_id: replaced_handler_id
                .clone()
                .filter(|_| matches!(replaced_check, Some(Ok(())))),
            webtransport_endpoint: None,
            controller_id: controller_check.is_ok().then(|| controller_id.clone()),
        };
        let invalid = |status: Status| (Rejection::InvalidRequest(status), report.clone());

        // The cap is on the RAW list length (duplicates count), checked before
        // any per-id work.
        let cap = usize::try_from(max_exclude_handler_ids).unwrap_or(usize::MAX);
        if exclude_handler_ids.len() > cap {
            return Err(invalid(Status::invalid_argument(
                "exclude_handler_ids exceeds max_exclude_handler_ids",
            )));
        }
        Self::validate_meeting_id(&meeting_id).map_err(invalid)?;
        controller_check.map_err(invalid)?;
        for id in &exclude_handler_ids {
            super::validate_handler_id(id, "exclude_handler_ids").map_err(invalid)?;
        }
        if let Some(check) = replaced_check {
            check.map_err(invalid)?;
        }
        let Some(trigger) = trigger else {
            return Err(invalid(Status::invalid_argument(
                "trigger must be HEARTBEAT_TIMEOUT, APPLY_LAG or PLANNED_TRANSFER",
            )));
        };
        Ok(Validated {
            meeting_id,
            controller_id,
            exclude: exclude_handler_ids,
            trigger,
            replaced_handler_id,
        })
    }

    /// Refresh the registered controllers gauge metric.
    ///
    /// Queries the database for current controller counts by status
    /// and updates the `gc_registered_controllers` gauge.
    async fn refresh_controller_metrics(&self) {
        match MeetingControllersRepository::get_controller_counts_by_status(&self.state.pool).await
        {
            Ok(counts) => {
                // Convert to the format expected by the metrics helper
                let counts: Vec<(String, u64)> = counts
                    .into_iter()
                    .map(|(status, count)| (status.as_db_str().to_string(), count as u64))
                    .collect();
                metrics::update_registered_controller_gauges("meeting", &counts);
            }
            Err(e) => {
                tracing::warn!(
                    target: "gc.grpc.mc_service",
                    error = %e,
                    "Failed to refresh controller metrics"
                );
            }
        }
    }

    /// Validate a controller ID.
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
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(Status::invalid_argument(
                "controller_id contains invalid characters",
            ));
        }
        Ok(())
    }

    /// Validate a meeting ID.
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
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(Status::invalid_argument(
                "meeting_id contains invalid characters",
            ));
        }
        Ok(())
    }

    /// Validate an endpoint URL.
    ///
    /// SCOPE: only a shape + http/https prefix check. The RegisterMH endpoint
    /// rule (per-zone allowlists, ADR-0039 §7) is deliberately MH-scoped and
    /// does NOT apply here; MC registration carries the same credential-
    /// harvest class (GC dials this `grpc_endpoint` with its bearer) and is
    /// tracked as its own task in docs/TODO.md §Media Path Obligations
    /// ("RegisterMC endpoint allowlist").
    fn validate_endpoint(endpoint: &str, field_name: &str) -> Result<(), Status> {
        super::validate_endpoint_bounds(endpoint, field_name)?;
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
        super::validate_region(&req.region)?;
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

        // Refresh the registered controllers metric
        self.refresh_controller_metrics().await;

        Ok(Response::new(RegisterMcResponse {
            accepted: true,
            message: "Registration successful".to_string(),
            fast_heartbeat_interval_ms: DEFAULT_FAST_HEARTBEAT_INTERVAL_MS,
            comprehensive_heartbeat_interval_ms: DEFAULT_COMPREHENSIVE_HEARTBEAT_INTERVAL_MS,
            // The same config value AcquireMediaHandler enforces.
            max_exclude_handler_ids: self.state.config.mh_registry.max_exclude_handler_ids(),
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
    ) -> Result<Response<FastHeartbeatResponse>, Status> {
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

        // Refresh the registered controllers metric (status may have changed)
        self.refresh_controller_metrics().await;

        let timestamp = chrono::Utc::now().timestamp() as u64;

        Ok(Response::new(FastHeartbeatResponse {
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
    ) -> Result<Response<ComprehensiveHeartbeatResponse>, Status> {
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

        // Usage readings are advisory: an invalid one is discarded field by
        // field (warned, counted) and the heartbeat still applies. Unset = no
        // reading this interval.
        let cpu_usage = super::sanitize_usage_percent(
            UsageSource::Mc,
            UsageField::Cpu,
            &req.controller_id,
            req.cpu_usage_percent,
        );
        let memory_usage = super::sanitize_usage_percent(
            UsageSource::Mc,
            UsageField::Memory,
            &req.controller_id,
            req.memory_usage_percent,
        );

        // Log metrics (could be stored in a time-series database in the future)
        tracing::debug!(
            target: "gc.grpc.comprehensive_heartbeat",
            controller_id = %req.controller_id,
            cpu_usage = ?cpu_usage,
            memory_usage = ?memory_usage,
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

        // Refresh the registered controllers metric (status may have changed)
        self.refresh_controller_metrics().await;

        let timestamp = chrono::Utc::now().timestamp() as u64;

        Ok(Response::new(ComprehensiveHeartbeatResponse {
            acknowledged: true,
            timestamp,
        }))
    }

    /// Grant one replacement media handler (ADR-0039 §3).
    ///
    /// Thin: validate → `mh_replacement::acquire` → record the counter,
    /// histogram and `gc.mh.replacement_report` event ONCE → map to a status.
    /// Every exit, including `INVALID_ARGUMENT`, passes the single recording
    /// site. No retry inside GC; MC owns backoff.
    #[instrument(skip_all, name = "gc.grpc.acquire_media_handler")]
    async fn acquire_media_handler(
        &self,
        request: Request<AcquireMediaHandlerRequest>,
    ) -> Result<Response<AcquireMediaHandlerResponse>, Status> {
        let start = Instant::now();
        // Absent when no auth layer injected claims (test rigs): omitted.
        let caller_sub = request.extensions().get::<Claims>().map(|c| c.sub.clone());
        let req = request.into_inner();
        let registry = &self.state.config.mh_registry;

        let (result, report) = match Self::validate_acquire(req, registry.max_exclude_handler_ids())
        {
            Ok(validated) => {
                mh_replacement::acquire(
                    &self.state.pool,
                    registry,
                    &validated,
                    self.selection_rng.as_ref(),
                )
                .await
            }
            Err((rejection, report)) => (Err(rejection), report),
        };

        let outcome = mh_replacement::outcome(&result);
        metrics::record_mh_replacement(report.trigger, outcome, report.gc_view, start.elapsed());
        mh_replacement::emit_report(&report, outcome, caller_sub.as_deref());

        match result {
            Ok(handler) => Ok(Response::new(AcquireMediaHandlerResponse {
                assignment: Some(MhAssignment::from(handler)),
            })),
            Err(Rejection::InvalidRequest(status)) => Err(status),
            Err(Rejection::NotOwner) => Err(Status::failed_precondition(
                "caller is not the meeting's controller",
            )),
            Err(Rejection::NoCapacity) => {
                Err(Status::resource_exhausted("no media handler available"))
            }
            Err(Rejection::Error(e)) => {
                tracing::error!(
                    target: "gc.grpc.acquire_media_handler",
                    error = %e,
                    "AcquireMediaHandler failed"
                );
                Err(Status::internal("Replacement failed"))
            }
        }
    }

    /// Handle meeting ended notification from a Meeting Controller.
    ///
    /// Called when a meeting ends on MC: the last participant left, or no
    /// participant connected within MC's join window (ADR-0010 §3). The notify
    /// carries no cause; GC handles both the same way.
    ///
    /// FENCED (rules at `NotifyMeetingEndedRequest` in `internal.proto`): GC
    /// ends ONLY the live row naming both `controller_id` and `assignment_id`,
    /// so a notify ends nothing but the incarnation it names, and MC may
    /// re-send it. Every field is REQUIRED (`INVALID_ARGUMENT` otherwise).
    /// `ENDED` and `NOT_CURRENT` are both success: MC stops retrying on either.
    #[instrument(skip_all, name = "gc.grpc.notify_meeting_ended")]
    async fn notify_meeting_ended(
        &self,
        request: Request<NotifyMeetingEndedRequest>,
    ) -> Result<Response<NotifyMeetingEndedResponse>, Status> {
        // Validated service subject from the auth layer (absent only in test
        // rigs without it): logged on a fenced notify for attribution.
        let caller_sub = request.extensions().get::<Claims>().map(|c| c.sub.clone());
        let req = request.into_inner();

        Self::validate_meeting_id(&req.meeting_id)?;
        super::validate_region(&req.region)?;
        Self::validate_controller_id(&req.controller_id)?;
        // One definition of a valid id, shared with MC. The error names the
        // rule broken, never echoes the input.
        let assignment_id = AssignmentId::parse(&req.assignment_id)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;

        let outcome = McAssignmentService::end_assignment(
            &self.state.pool,
            &req.meeting_id,
            &req.region,
            &req.controller_id,
            assignment_id,
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

        metrics::record_meeting_end_notification(outcome);

        // `Ended` is logged at INFO by the service layer (the single record
        // of an end).
        let wire = match outcome {
            EndAssignmentOutcome::Ended => NotifyMeetingEndedOutcome::Ended,
            EndAssignmentOutcome::AlreadyEnded => {
                tracing::debug!(
                    target: "gc.grpc.notify_meeting_ended",
                    meeting_id = %req.meeting_id,
                    region = %req.region,
                    mc_id = %req.controller_id,
                    caller_sub = caller_sub.as_deref().unwrap_or("-"),
                    "No live assignment to end (already ended)"
                );
                NotifyMeetingEndedOutcome::NotCurrent
            }
            EndAssignmentOutcome::Fenced => {
                // No row of THIS incarnation exists: this notify is late (a
                // revive / failover re-point replaced its row) or from an
                // orphan whose row never committed (lost assignment race, or a
                // missed accept->commit budget).
                // Ids are not secrets; logged for correlation with MC's lines.
                tracing::warn!(
                    target: "gc.grpc.notify_meeting_ended",
                    meeting_id = %req.meeting_id,
                    region = %req.region,
                    mc_id = %req.controller_id,
                    assignment_id = %assignment_id,
                    caller_sub = caller_sub.as_deref().unwrap_or("-"),
                    "Meeting-ended notify fenced: no row of this incarnation exists; nothing ended"
                );
                NotifyMeetingEndedOutcome::NotCurrent
            }
        };

        Ok(Response::new(NotifyMeetingEndedResponse {
            outcome: wire.into(),
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

    /// ASCII only: a Unicode homoglyph or fullwidth digit is rejected (these
    /// ids reach the operator-facing replacement evidence log).
    #[test]
    fn test_validate_ids_reject_non_ascii() {
        for id in ["mc-\u{0430}", "\u{FF11}"] {
            let err = McService::validate_controller_id(id).unwrap_err();
            assert!(err.message().contains("invalid characters"), "{id:?}");
            let err = McService::validate_meeting_id(id).unwrap_err();
            assert!(err.message().contains("invalid characters"), "{id:?}");
        }
    }

    #[test]
    fn test_validate_meeting_id_at_1_char() {
        assert!(
            McService::validate_meeting_id("m").is_ok(),
            "Meeting ID with 1 char should pass"
        );
    }
}
