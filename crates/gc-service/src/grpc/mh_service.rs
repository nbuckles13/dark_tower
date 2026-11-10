//! gRPC service for Media Handler registration and load reports.
//!
//! Implements the MediaHandlerRegistryService from internal.proto.
//! MHs call these RPCs to register with GC and send periodic load reports.
//!
//! # Security
//!
//! - All RPCs require JWT authentication via the auth layer
//! - Handler IDs are validated for format
//! - Sensitive fields are not logged

use crate::config::MhRegistryConfig;
use crate::mh_endpoint_policy::{EndpointField, MhEndpointAllowlist, RejectedComponent};
use crate::observability::metrics::{self, MhRegistrationOutcome};
use crate::repositories::{
    HealthStatus, MediaHandlersRepository, MhRegistrationDecision, NewMhRegistration,
};
use chrono::Utc;
use proto_gen::dark_tower::internal::v1::{
    media_handler_registry_service_server::MediaHandlerRegistryService, RegisterMhRequest,
    RegisterMhResponse, SendLoadReportRequest, SendLoadReportResponse,
};
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tonic::{Request, Response, Status};
use tracing::instrument;

/// Maximum allowed handler ID length.
const MAX_HANDLER_ID_LENGTH: usize = 255;

/// Maximum allowed region length.
const MAX_REGION_LENGTH: usize = 50;

/// gRPC service for MH registration and load reports.
pub struct MhService {
    pool: Arc<PgPool>,
    /// Load-report interval pushed in `RegisterMHResponse`, in milliseconds.
    /// GC is its single source (`GC_MH_LOAD_REPORT_INTERVAL_MS`); it is the
    /// same value the staleness bound derives from.
    load_report_interval_ms: u64,
    /// The ONE MH staleness bound (`MhRegistryConfig::staleness_bound`),
    /// used by the registration rule's "stale" check (arm 2b).
    staleness_bound: Duration,
    /// RegisterMH endpoint allowlists (arm 0).
    endpoint_allowlist: MhEndpointAllowlist,
}

impl MhService {
    /// Create a new MH service from GC's validated MH config.
    pub fn new(
        pool: Arc<PgPool>,
        registry: &MhRegistryConfig,
        endpoint_allowlist: MhEndpointAllowlist,
    ) -> Self {
        Self {
            pool,
            load_report_interval_ms: registry.load_report_interval_ms(),
            staleness_bound: registry.staleness_bound(),
            endpoint_allowlist,
        }
    }

    /// Validate a handler ID.
    ///
    /// Handler IDs must be non-empty, at most 255 characters, and contain
    /// only alphanumeric characters, hyphens, and underscores.
    fn validate_handler_id(id: &str) -> Result<(), Status> {
        if id.is_empty() {
            return Err(Status::invalid_argument("handler_id is required"));
        }
        if id.len() > MAX_HANDLER_ID_LENGTH {
            return Err(Status::invalid_argument("handler_id is too long"));
        }
        // Allow alphanumeric, hyphens, and underscores
        if !id
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return Err(Status::invalid_argument(
                "handler_id contains invalid characters",
            ));
        }
        Ok(())
    }

    /// Validate a region.
    fn validate_region(region: &str) -> Result<(), Status> {
        if region.is_empty() {
            return Err(Status::invalid_argument("region is required"));
        }
        if region.len() > MAX_REGION_LENGTH {
            return Err(Status::invalid_argument("region is too long"));
        }
        Ok(())
    }

    /// Arm 0 of the RegisterMH rule: shape bounds, then the per-zone
    /// allowlists. A failure is counted (`invalid_endpoint`) and logged here;
    /// the Status names the field and component, never the allowlist.
    fn check_endpoints(&self, req: &RegisterMhRequest) -> Result<(), Status> {
        for (field, endpoint) in [
            (EndpointField::WebTransport, &req.webtransport_endpoint),
            (EndpointField::Grpc, &req.grpc_endpoint),
        ] {
            if let Err(status) = super::validate_endpoint_bounds(endpoint, field.field_name()) {
                metrics::record_mh_registration(MhRegistrationOutcome::InvalidEndpoint);
                tracing::warn!(
                    target: "gc.grpc.mh_service",
                    handler_id = %req.handler_id,
                    field = field.field_name(),
                    component = RejectedComponent::Syntax.as_str(),
                    reason = %status.message(),
                    "MH registration rejected: invalid endpoint"
                );
                return Err(status);
            }
        }
        self.endpoint_allowlist
            .check(&req.webtransport_endpoint, &req.grpc_endpoint)
            .map_err(|rejection| {
                metrics::record_mh_registration(MhRegistrationOutcome::InvalidEndpoint);
                // Endpoints are bounded by MAX_ENDPOINT_LENGTH and logged with
                // Debug (`?`) so control characters cannot forge log lines.
                tracing::warn!(
                    target: "gc.grpc.mh_service",
                    handler_id = %req.handler_id,
                    field = rejection.field.field_name(),
                    component = rejection.component.as_str(),
                    webtransport_endpoint = ?req.webtransport_endpoint,
                    grpc_endpoint = ?req.grpc_endpoint,
                    "MH registration rejected: endpoint outside its zone allowlist"
                );
                Status::invalid_argument(rejection.to_string())
            })
    }
}

#[tonic::async_trait]
impl MediaHandlerRegistryService for MhService {
    /// Register a media handler under the RegisterMH endpoint rule (stated
    /// once above `RegisterMHRequest` in internal.proto; ADR-0039 §7).
    ///
    /// Every call past the pre-rule field checks records exactly ONE
    /// `gc_mh_registrations_total` outcome, except a failed registration (a
    /// database failure, or a registry-invariant breach observed under the
    /// lock — a GC bug), which is
    /// `gc_db_queries_total{operation="register_mh_txn",status="error"}` plus
    /// the error log line, and answers INTERNAL. `ok` and `superseded` both answer
    /// `accepted=true`; every reject is a gRPC Status, never `accepted=false`.
    #[instrument(skip_all, fields(handler_id = %request.get_ref().handler_id, region = %request.get_ref().region))]
    async fn register_mh(
        &self,
        request: Request<RegisterMhRequest>,
    ) -> Result<Response<RegisterMhResponse>, Status> {
        let req = request.into_inner();

        // Pre-rule field checks (not counted as a registration outcome).
        Self::validate_handler_id(&req.handler_id)?;
        Self::validate_region(&req.region)?;
        if req.max_streams == 0 {
            return Err(Status::invalid_argument(
                "max_streams must be greater than 0",
            ));
        }
        // `media_handlers.max_streams` is INTEGER. A truncating `as i32` wraps a
        // value above i32::MAX NEGATIVE, and `current_streams < max_streams`
        // then excludes the handler from placement forever — a healthy-looking
        // black hole with no error anywhere. Refuse it loudly instead. (MH
        // bounds its derived ceiling far below this at boot; this is the GC
        // half of the same invariant, for any caller.)
        let max_streams = i32::try_from(req.max_streams)
            .map_err(|_| Status::invalid_argument("max_streams exceeds the supported maximum"))?;

        // Arm 0: allowlists, before any transaction opens.
        self.check_endpoints(&req)?;

        // Arms 1-3: one advisory-locked transaction.
        let decision = MediaHandlersRepository::register_mh(
            &self.pool,
            &NewMhRegistration {
                handler_id: &req.handler_id,
                region: &req.region,
                webtransport_endpoint: &req.webtransport_endpoint,
                grpc_endpoint: &req.grpc_endpoint,
                max_streams,
            },
            self.staleness_bound,
        )
        .await
        .map_err(|e| {
            tracing::error!(target: "gc.grpc.mh_service", error = %e, "Failed to register MH");
            Status::internal("Registration failed")
        })?;

        // The single recording + wire-mapping site.
        match decision {
            MhRegistrationDecision::Registered => {
                metrics::record_mh_registration(MhRegistrationOutcome::Ok);
                tracing::info!(
                    target: "gc.grpc.mh_service",
                    handler_id = %req.handler_id,
                    region = %req.region,
                    "MH registered successfully"
                );
            }
            MhRegistrationDecision::Superseded { matched, ids } => {
                metrics::record_mh_registration(MhRegistrationOutcome::Superseded(matched));
                // Forensic trail for the ping-pong residual: handler identity
                // is self-asserted, so every eviction names both sides.
                tracing::info!(
                    target: "gc.grpc.mh_service",
                    handler_id = %req.handler_id,
                    region = %req.region,
                    superseded_handler_ids = ?ids,
                    r#match = matched.label(),
                    "MH registered, superseding prior registry row(s)"
                );
            }
            MhRegistrationDecision::AddressConflict { conflicting_ids } => {
                metrics::record_mh_registration(MhRegistrationOutcome::AddressConflict);
                tracing::warn!(
                    target: "gc.grpc.mh_service",
                    handler_id = %req.handler_id,
                    conflicting_handler_ids = ?conflicting_ids,
                    webtransport_endpoint = ?req.webtransport_endpoint,
                    grpc_endpoint = ?req.grpc_endpoint,
                    "MH registration rejected: an endpoint is held by a fresh serving MH"
                );
                // Only the literal: never the other row's id or endpoints.
                return Err(Status::failed_precondition("address_conflict"));
            }
            MhRegistrationDecision::EndpointChanged { webtransport, grpc } => {
                metrics::record_mh_registration(MhRegistrationOutcome::EndpointChanged);
                let changed = match (webtransport, grpc) {
                    (true, true) => "both",
                    (true, false) => "webtransport",
                    (false, true) => "grpc",
                    // Unreachable: the repository only returns EndpointChanged
                    // when at least one differs; named, not panicked on.
                    (false, false) => "none",
                };
                tracing::warn!(
                    target: "gc.grpc.mh_service",
                    handler_id = %req.handler_id,
                    changed,
                    webtransport_endpoint = ?req.webtransport_endpoint,
                    grpc_endpoint = ?req.grpc_endpoint,
                    "MH registration rejected: endpoints are fixed for an incarnation's life"
                );
                // Never echoes the stored endpoints.
                return Err(Status::failed_precondition("endpoint_changed"));
            }
        }

        Ok(Response::new(RegisterMhResponse {
            accepted: true,
            message: "Registered successfully".to_string(),
            load_report_interval_ms: self.load_report_interval_ms,
        }))
    }

    /// Handle a load report from an MH.
    ///
    /// # Arguments
    ///
    /// * `request` - Load report with current streams and health metrics
    ///
    /// # Returns
    ///
    /// Acknowledgment response.
    #[instrument(skip_all, fields(handler_id = %request.get_ref().handler_id))]
    async fn send_load_report(
        &self,
        request: Request<SendLoadReportRequest>,
    ) -> Result<Response<SendLoadReportResponse>, Status> {
        let req = request.into_inner();

        // Validate handler_id with format checks
        Self::validate_handler_id(&req.handler_id)?;

        // Same INTEGER column shape as `max_streams`, and it FAILS OPEN if
        // wrapped: a negative `current_streams` makes `current_streams <
        // max_streams` always true and sorts the handler as LEAST loaded, so
        // a handler reporting exhaustion would be placed on preferentially.
        // Rejected rather than saturated: MH bounds its own value by its
        // ceiling, so an overflow is a malformed or foreign report, and
        // refusing it beats placing against it.
        let current_streams = i32::try_from(req.current_streams).map_err(|_| {
            Status::invalid_argument("current_streams exceeds the supported maximum")
        })?;

        // Convert health status from proto enum
        let health_status = match req.health {
            0 => HealthStatus::Pending,
            1 => HealthStatus::Healthy,
            2 => HealthStatus::Degraded,
            3 => HealthStatus::Unhealthy,
            4 => HealthStatus::Draining,
            _ => HealthStatus::Pending,
        };

        // Update the load report
        let updated = MediaHandlersRepository::update_load_report(
            &self.pool,
            &req.handler_id,
            current_streams,
            health_status,
            if req.cpu_usage_percent > 0.0 {
                Some(req.cpu_usage_percent)
            } else {
                None
            },
            if req.memory_usage_percent > 0.0 {
                Some(req.memory_usage_percent)
            } else {
                None
            },
            if req.bandwidth_usage_percent > 0.0 {
                Some(req.bandwidth_usage_percent)
            } else {
                None
            },
        )
        .await
        .map_err(|e| {
            tracing::error!(target: "gc.grpc.mh_service", error = %e, "Failed to update load report");
            Status::internal("Load report update failed")
        })?;

        if !updated {
            metrics::record_mh_load_report(metrics::MH_LOAD_REPORT_UNKNOWN_HANDLER);
            tracing::warn!(
                target: "gc.grpc.mh_service",
                handler_id = %req.handler_id,
                "Load report for unknown handler"
            );
            return Err(Status::not_found("Handler not registered"));
        }
        metrics::record_mh_load_report(metrics::MH_LOAD_REPORT_ACCEPTED);

        // Use current timestamp from chrono (consistent with rest of codebase)
        let timestamp = Utc::now().timestamp() as u64;

        Ok(Response::new(SendLoadReportResponse {
            acknowledged: true,
            timestamp,
        }))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    // === Handler ID Validation Tests ===

    #[test]
    fn test_validate_handler_id_valid() {
        assert!(MhService::validate_handler_id("mh-us-east-1-001").is_ok());
        assert!(MhService::validate_handler_id("mh_123").is_ok());
        assert!(MhService::validate_handler_id("MH123").is_ok());
    }

    #[test]
    fn test_validate_handler_id_empty() {
        let err = MhService::validate_handler_id("").unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("required"));
    }

    #[test]
    fn test_validate_handler_id_too_long() {
        let long_id = "a".repeat(256);
        let err = MhService::validate_handler_id(&long_id).unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("too long"));
    }

    #[test]
    fn test_validate_handler_id_at_255_chars() {
        let id_255 = "a".repeat(255);
        assert!(
            MhService::validate_handler_id(&id_255).is_ok(),
            "Handler ID at 255 chars should pass"
        );
    }

    #[test]
    fn test_validate_handler_id_invalid_chars() {
        let err = MhService::validate_handler_id("mh/invalid").unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("invalid characters"));

        let err = MhService::validate_handler_id("mh with space").unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    // === Region Validation Tests ===

    #[test]
    fn test_validate_region_valid() {
        assert!(MhService::validate_region("us-east-1").is_ok());
        assert!(MhService::validate_region("eu-west-1").is_ok());
    }

    #[test]
    fn test_validate_region_empty() {
        let err = MhService::validate_region("").unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn test_validate_region_too_long() {
        let long_region = "r".repeat(51);
        let err = MhService::validate_region(&long_region).unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("too long"));
    }

    #[test]
    fn test_validate_region_at_50_chars() {
        let region_50 = "r".repeat(50);
        assert!(
            MhService::validate_region(&region_50).is_ok(),
            "Region at 50 chars should pass"
        );
    }

    // === Endpoint shape bounds (shared with MC; zone rules are in
    // mh_endpoint_policy.rs) ===

    #[test]
    fn test_endpoint_bounds() {
        let err = super::super::validate_endpoint_bounds("", "webtransport_endpoint").unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("webtransport_endpoint is required"));
        let at_255 = format!("https://{}", "a".repeat(247));
        assert!(super::super::validate_endpoint_bounds(&at_255, "f").is_ok());
        let at_256 = format!("https://{}", "a".repeat(248));
        let err = super::super::validate_endpoint_bounds(&at_256, "f").unwrap_err();
        assert!(err.message().contains("too long"));
    }
}

/// Integration tests requiring database.
#[cfg(test)]
mod integration_tests {
    use super::*;
    use sqlx::PgPool;

    /// Test-local registry config. 2500 ms is deliberately NOT the shipped
    /// interval, so the push test cannot pass against a hardcoded constant.
    fn registry(interval_ms: u64) -> crate::config::MhRegistryConfig {
        crate::config::MhRegistryConfig::new(interval_ms, 3, 3600, 2)
            .expect("valid test registry config")
    }

    /// Test-local allowlist (unit tests here cannot use gc-test-utils, which
    /// links a second copy of this crate). Arm behaviour is tested in
    /// `tests/mh_registration_tests.rs` with the gc-test-utils fixture.
    fn allowlist() -> MhEndpointAllowlist {
        use crate::mh_endpoint_policy as p;
        MhEndpointAllowlist::new(
            p::parse_wt_hosts(".mh.test").unwrap(),
            p::parse_port_set("4440").unwrap(),
            p::parse_scheme_set("grpc").unwrap(),
            p::parse_cidr_set("10.244.0.0/16").unwrap(),
            p::parse_port_set("50053").unwrap(),
        )
    }

    const WT: &str = "https://mh.mh.test:4440";
    const GRPC: &str = "grpc://10.244.0.1:50053";

    fn service(pool: PgPool) -> MhService {
        MhService::new(Arc::new(pool), &registry(2500), allowlist())
    }

    /// Seed a row directly through the repository.
    async fn seed(pool: &PgPool, id: &str) {
        MediaHandlersRepository::register_mh(
            pool,
            &NewMhRegistration {
                handler_id: id,
                region: "us-east-1",
                webtransport_endpoint: WT,
                grpc_endpoint: GRPC,
                max_streams: 1000,
            },
            registry(2500).staleness_bound(),
        )
        .await
        .unwrap();
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_register_mh_success(pool: PgPool) {
        let service = service(pool.clone());

        let request = Request::new(RegisterMhRequest {
            handler_id: "test-mh-001".to_string(),
            region: "us-east-1".to_string(),
            webtransport_endpoint: WT.to_string(),
            grpc_endpoint: GRPC.to_string(),
            max_streams: 1000,
        });

        let response = service.register_mh(request).await.unwrap();
        let inner = response.into_inner();

        assert!(inner.accepted);
        // The configured interval is what MH is told (GC is the single source).
        assert_eq!(inner.load_report_interval_ms, 2500);

        // Verify handler was created
        let handler = MediaHandlersRepository::get_handler(&pool, "test-mh-001")
            .await
            .unwrap();
        assert!(handler.is_some());
        let handler = handler.unwrap();
        assert_eq!(handler.region, "us-east-1");
        assert_eq!(handler.max_streams, 1000);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_register_mh_validation_errors(pool: PgPool) {
        let service = service(pool);

        // Empty handler_id
        let request = Request::new(RegisterMhRequest {
            handler_id: "".to_string(),
            region: "us-east-1".to_string(),
            webtransport_endpoint: WT.to_string(),
            grpc_endpoint: GRPC.to_string(),
            max_streams: 1000,
        });
        let result = service.register_mh(request).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);

        // Empty region
        let request = Request::new(RegisterMhRequest {
            handler_id: "test-mh".to_string(),
            region: "".to_string(),
            webtransport_endpoint: WT.to_string(),
            grpc_endpoint: GRPC.to_string(),
            max_streams: 1000,
        });
        let result = service.register_mh(request).await;
        assert!(result.is_err());

        // Zero max_streams
        let request = Request::new(RegisterMhRequest {
            handler_id: "test-mh".to_string(),
            region: "us-east-1".to_string(),
            webtransport_endpoint: WT.to_string(),
            grpc_endpoint: GRPC.to_string(),
            max_streams: 0,
        });
        let result = service.register_mh(request).await;
        assert!(result.is_err());

        // max_streams above i32::MAX would wrap NEGATIVE in the INTEGER
        // column and exclude the handler from placement forever: refused.
        let request = Request::new(RegisterMhRequest {
            handler_id: "test-mh".to_string(),
            region: "us-east-1".to_string(),
            webtransport_endpoint: WT.to_string(),
            grpc_endpoint: GRPC.to_string(),
            max_streams: u32::try_from(i32::MAX).unwrap() + 1,
        });
        let result = service.register_mh(request).await;
        assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
    }

    /// `current_streams` above i32::MAX would wrap NEGATIVE, making an
    /// exhausted handler sort as the least loaded — fail-open placement.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_send_load_report_rejects_current_streams_above_i32(pool: PgPool) {
        let service = service(pool.clone());
        seed(&pool, "overflow-mh").await;

        let request = Request::new(SendLoadReportRequest {
            handler_id: "overflow-mh".to_string(),
            current_streams: u32::try_from(i32::MAX).unwrap() + 1,
            health: 1,
            cpu_usage_percent: 0.0,
            memory_usage_percent: 0.0,
            bandwidth_usage_percent: 0.0,
        });
        let result = service.send_load_report(request).await;
        assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_send_load_report_success(pool: PgPool) {
        let service = service(pool.clone());

        // First register the handler
        seed(&pool, "load-report-mh").await;

        // Send load report
        let request = Request::new(SendLoadReportRequest {
            handler_id: "load-report-mh".to_string(),
            current_streams: 50,
            health: 1, // Healthy
            cpu_usage_percent: 25.0,
            memory_usage_percent: 40.0,
            bandwidth_usage_percent: 30.0,
        });

        let response = service.send_load_report(request).await.unwrap();
        let inner = response.into_inner();

        assert!(inner.acknowledged);
        assert!(inner.timestamp > 0);

        // Verify handler was updated
        let handler = MediaHandlersRepository::get_handler(&pool, "load-report-mh")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(handler.current_streams, 50);
        assert_eq!(handler.health_status, HealthStatus::Healthy);
        assert_eq!(handler.cpu_usage_percent, Some(25.0));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_send_load_report_unknown_handler(pool: PgPool) {
        let service = service(pool);

        let request = Request::new(SendLoadReportRequest {
            handler_id: "unknown-handler".to_string(),
            current_streams: 50,
            health: 1,
            cpu_usage_percent: 25.0,
            memory_usage_percent: 40.0,
            bandwidth_usage_percent: 30.0,
        });

        let result = service.send_load_report(request).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::NotFound);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_send_load_report_empty_handler_id(pool: PgPool) {
        let service = service(pool);

        let request = Request::new(SendLoadReportRequest {
            handler_id: "".to_string(),
            current_streams: 50,
            health: 1,
            cpu_usage_percent: 25.0,
            memory_usage_percent: 40.0,
            bandwidth_usage_percent: 30.0,
        });

        let result = service.send_load_report(request).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
    }
}
