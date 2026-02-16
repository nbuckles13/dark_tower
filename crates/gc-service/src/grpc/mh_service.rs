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

use crate::repositories::{HealthStatus, MediaHandlersRepository};
use chrono::Utc;
use proto_gen::internal::{
    media_handler_registry_service_server::MediaHandlerRegistryService, MhLoadReportRequest,
    MhLoadReportResponse, RegisterMhRequest, RegisterMhResponse,
};
use sqlx::PgPool;
use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::instrument;

/// Default load report interval in milliseconds (10 seconds).
const DEFAULT_LOAD_REPORT_INTERVAL_MS: u64 = 10_000;

/// Maximum allowed handler ID length.
const MAX_HANDLER_ID_LENGTH: usize = 255;

/// Maximum allowed region length.
const MAX_REGION_LENGTH: usize = 50;

/// Maximum allowed endpoint length.
const MAX_ENDPOINT_LENGTH: usize = 255;

/// gRPC service for MH registration and load reports.
pub struct MhService {
    pool: Arc<PgPool>,
}

impl MhService {
    /// Create a new MH service.
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    /// Validate a handler ID.
    ///
    /// Handler IDs must be non-empty, at most 255 characters, and contain
    /// only alphanumeric characters, hyphens, and underscores.
    #[expect(
        clippy::result_large_err,
        reason = "Status is the standard gRPC error type"
    )]
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
    ///
    /// Endpoints must be non-empty, at most 255 characters, and start with
    /// http:// or https://.
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
        // Basic URL format validation - allow http, https, and grpc schemes
        if !endpoint.starts_with("http://")
            && !endpoint.starts_with("https://")
            && !endpoint.starts_with("grpc://")
        {
            return Err(Status::invalid_argument(format!(
                "{} must be a valid URL with http, https, or grpc scheme",
                field_name
            )));
        }
        Ok(())
    }
}

#[tonic::async_trait]
impl MediaHandlerRegistryService for MhService {
    /// Register a new media handler.
    ///
    /// # Arguments
    ///
    /// * `request` - Registration request with handler details
    ///
    /// # Returns
    ///
    /// Registration response with load report interval.
    #[instrument(skip_all, fields(handler_id = %request.get_ref().handler_id, region = %request.get_ref().region))]
    async fn register_mh(
        &self,
        request: Request<RegisterMhRequest>,
    ) -> Result<Response<RegisterMhResponse>, Status> {
        let req = request.into_inner();

        // Validate required fields with format checks
        Self::validate_handler_id(&req.handler_id)?;
        Self::validate_region(&req.region)?;
        Self::validate_endpoint(&req.webtransport_endpoint, "webtransport_endpoint")?;
        Self::validate_endpoint(&req.grpc_endpoint, "grpc_endpoint")?;

        if req.max_streams == 0 {
            return Err(Status::invalid_argument(
                "max_streams must be greater than 0",
            ));
        }

        // Register the handler
        MediaHandlersRepository::register_mh(
            &self.pool,
            &req.handler_id,
            &req.region,
            &req.webtransport_endpoint,
            &req.grpc_endpoint,
            req.max_streams as i32,
        )
        .await
        .map_err(|e| {
            tracing::error!(target: "gc.grpc.mh_service", error = %e, "Failed to register MH");
            Status::internal("Registration failed")
        })?;

        tracing::info!(
            target: "gc.grpc.mh_service",
            handler_id = %req.handler_id,
            region = %req.region,
            "MH registered successfully"
        );

        Ok(Response::new(RegisterMhResponse {
            accepted: true,
            message: "Registered successfully".to_string(),
            load_report_interval_ms: DEFAULT_LOAD_REPORT_INTERVAL_MS,
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
        request: Request<MhLoadReportRequest>,
    ) -> Result<Response<MhLoadReportResponse>, Status> {
        let req = request.into_inner();

        // Validate handler_id with format checks
        Self::validate_handler_id(&req.handler_id)?;

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
            req.current_streams as i32,
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
            tracing::warn!(
                target: "gc.grpc.mh_service",
                handler_id = %req.handler_id,
                "Load report for unknown handler"
            );
            return Err(Status::not_found("Handler not registered"));
        }

        // Use current timestamp from chrono (consistent with rest of codebase)
        let timestamp = Utc::now().timestamp() as u64;

        Ok(Response::new(MhLoadReportResponse {
            acknowledged: true,
            timestamp,
        }))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_default_load_report_interval() {
        assert_eq!(DEFAULT_LOAD_REPORT_INTERVAL_MS, 10_000);
    }

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

    // === Endpoint Validation Tests ===

    #[test]
    fn test_validate_endpoint_valid() {
        assert!(MhService::validate_endpoint("http://localhost:443", "test").is_ok());
        assert!(MhService::validate_endpoint("https://mh.example.com:443", "test").is_ok());
        assert!(MhService::validate_endpoint("grpc://mh:50051", "test").is_ok());
    }

    #[test]
    fn test_validate_endpoint_empty() {
        let err = MhService::validate_endpoint("", "webtransport_endpoint").unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("webtransport_endpoint is required"));
    }

    #[test]
    fn test_validate_endpoint_invalid_scheme() {
        let err = MhService::validate_endpoint("ftp://example.com", "test").unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("valid URL"));
    }

    #[test]
    fn test_validate_endpoint_too_long() {
        let long_endpoint = format!("https://{}", "a".repeat(248));
        let err = MhService::validate_endpoint(&long_endpoint, "test").unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("too long"));
    }

    #[test]
    fn test_validate_endpoint_at_255_chars() {
        let endpoint_255 = format!("https://{}", "a".repeat(247));
        assert!(
            MhService::validate_endpoint(&endpoint_255, "test").is_ok(),
            "Endpoint at 255 chars should pass"
        );
    }
}

/// Integration tests requiring database.
#[cfg(test)]
mod integration_tests {
    use super::*;
    use sqlx::PgPool;

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_register_mh_success(pool: PgPool) {
        let service = MhService::new(Arc::new(pool.clone()));

        let request = Request::new(RegisterMhRequest {
            handler_id: "test-mh-001".to_string(),
            region: "us-east-1".to_string(),
            webtransport_endpoint: "https://mh:443".to_string(),
            grpc_endpoint: "grpc://mh:50051".to_string(),
            max_streams: 1000,
        });

        let response = service.register_mh(request).await.unwrap();
        let inner = response.into_inner();

        assert!(inner.accepted);
        assert_eq!(
            inner.load_report_interval_ms,
            DEFAULT_LOAD_REPORT_INTERVAL_MS
        );

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
        let service = MhService::new(Arc::new(pool));

        // Empty handler_id
        let request = Request::new(RegisterMhRequest {
            handler_id: "".to_string(),
            region: "us-east-1".to_string(),
            webtransport_endpoint: "https://mh:443".to_string(),
            grpc_endpoint: "grpc://mh:50051".to_string(),
            max_streams: 1000,
        });
        let result = service.register_mh(request).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);

        // Empty region
        let request = Request::new(RegisterMhRequest {
            handler_id: "test-mh".to_string(),
            region: "".to_string(),
            webtransport_endpoint: "https://mh:443".to_string(),
            grpc_endpoint: "grpc://mh:50051".to_string(),
            max_streams: 1000,
        });
        let result = service.register_mh(request).await;
        assert!(result.is_err());

        // Zero max_streams
        let request = Request::new(RegisterMhRequest {
            handler_id: "test-mh".to_string(),
            region: "us-east-1".to_string(),
            webtransport_endpoint: "https://mh:443".to_string(),
            grpc_endpoint: "grpc://mh:50051".to_string(),
            max_streams: 0,
        });
        let result = service.register_mh(request).await;
        assert!(result.is_err());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_send_load_report_success(pool: PgPool) {
        let service = MhService::new(Arc::new(pool.clone()));

        // First register the handler
        MediaHandlersRepository::register_mh(
            &pool,
            "load-report-mh",
            "us-east-1",
            "https://mh:443",
            "grpc://mh:50051",
            1000,
        )
        .await
        .unwrap();

        // Send load report
        let request = Request::new(MhLoadReportRequest {
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
        let service = MhService::new(Arc::new(pool));

        let request = Request::new(MhLoadReportRequest {
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
        let service = MhService::new(Arc::new(pool));

        let request = Request::new(MhLoadReportRequest {
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
