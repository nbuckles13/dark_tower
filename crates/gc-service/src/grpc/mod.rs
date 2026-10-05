//! gRPC services for Global Controller.
//!
//! Provides gRPC endpoints for Meeting Controller and Media Handler registration.
//! All gRPC requests require JWT authentication via the auth layer.

pub mod auth_layer;
pub mod mc_service;
pub mod mh_service;

pub use mc_service::McService;
pub use mh_service::MhService;

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
