//! gRPC services for Global Controller.
//!
//! Provides gRPC endpoints for Meeting Controller and Media Handler registration.
//! All gRPC requests require JWT authentication via the auth layer.

pub mod auth_layer;
pub mod mc_service;
pub mod mh_service;

#[allow(unused_imports)] // Alternative API
pub use auth_layer::GrpcAuthInterceptor;
pub use mc_service::McService;
pub use mh_service::MhService;
