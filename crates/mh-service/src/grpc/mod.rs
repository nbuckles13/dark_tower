//! gRPC service implementations for Media Handler.
//!
//! This module provides:
//! - `gc_client` - Client for MH→GC communication (registration, load reports)
//! - `mh_service` - Server for MC→MH communication (register, route, telemetry) — stub
//! - `auth_interceptor` - Authorization validation for incoming MC requests
//!
//! # Architecture
//!
//! ```text
//! MH → GC: RegisterMH, SendLoadReport
//! MC → MH: Register, RouteMedia, StreamTelemetry (requires authorization)
//! ```
//!
//! # Security
//!
//! All incoming gRPC requests from MC must pass through the [`MhAuthInterceptor`]
//! which validates authorization headers. This provides defense-in-depth beyond
//! transport-level security.

pub mod auth_interceptor;
pub mod gc_client;
pub mod mh_service;

pub use auth_interceptor::MhAuthInterceptor;
pub use gc_client::GcClient;
pub use mh_service::MhMediaService;
