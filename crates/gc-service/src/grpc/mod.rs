//! gRPC services for Global Controller.
//!
//! Provides gRPC endpoints for Meeting Controller and Media Handler registration.
//! All gRPC requests require JWT authentication via the auth layer.

pub mod auth_layer;
pub mod mc_service;
pub mod mh_service;

pub use mc_service::McService;
pub use mh_service::MhService;

use tonic::Status;

/// Maximum allowed registration endpoint length (MC and MH registration;
/// the `VARCHAR(255)` endpoint columns).
pub(crate) const MAX_ENDPOINT_LENGTH: usize = 255;

/// The shape check every registration endpoint shares: non-empty and at most
/// [`MAX_ENDPOINT_LENGTH`]. Scheme/host/port rules are per registrant (MH:
/// `crate::mh_endpoint_policy`; MC: `McService::validate_endpoint`).
pub(crate) fn validate_endpoint_bounds(endpoint: &str, field_name: &str) -> Result<(), Status> {
    if endpoint.is_empty() {
        return Err(Status::invalid_argument(format!(
            "{field_name} is required"
        )));
    }
    if endpoint.len() > MAX_ENDPOINT_LENGTH {
        return Err(Status::invalid_argument(format!(
            "{field_name} is too long"
        )));
    }
    Ok(())
}

/// GC's inbound gRPC layer stack: request span → trace-context extraction →
/// `auth`. The ONE place the ordering is decided (see
/// [`common::observability::otel_grpc::inbound_layers`]); `main.rs` and the
/// test rigs both use it.
#[must_use]
pub fn server_layers(
    auth: auth_layer::GrpcAuthLayer,
) -> common::observability::otel_grpc::InboundLayers<
    common::observability::otel_grpc::GrpcTraceLayer,
    auth_layer::GrpcAuthLayer,
> {
    common::observability::otel_grpc::inbound_layers(
        common::observability::otel_grpc::grpc_trace_layer(),
        auth,
    )
}
