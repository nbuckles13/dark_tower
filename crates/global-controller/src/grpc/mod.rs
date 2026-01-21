//! gRPC services for Global Controller.
//!
//! Provides gRPC endpoints for Meeting Controller registration and heartbeat.
//! All gRPC requests require JWT authentication via the auth layer.

pub mod auth_layer;
pub mod mc_service;

#[allow(unused_imports)] // Alternative API
pub use auth_layer::GrpcAuthInterceptor;
pub use mc_service::McService;
