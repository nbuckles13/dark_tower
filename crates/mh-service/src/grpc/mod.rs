//! gRPC service implementations for Media Handler.
//!
//! This module provides:
//! - `gc_client` - Client for MH→GC communication (registration, load reports)
//! - `mc_client` - Client for MH→MC communication (participant notifications)
//! - `mh_service` - Server for MC→MH communication (register, route, telemetry) — stub
//! - `auth_interceptor` - Authorization validation for incoming MC requests
//!
//! # Architecture
//!
//! ```text
//! MH → GC: RegisterMH, SendLoadReport
//! MH → MC: NotifyParticipantConnected, NotifyParticipantDisconnected
//! MC → MH: Register, RouteMedia, StreamTelemetry (requires authorization)
//! ```
//!
//! # Security
//!
//! All incoming gRPC requests from MC must pass through the [`MhAuthLayer`]
//! which validates authorization headers and enforces caller-type routing
//! (ADR-0003). This provides defense-in-depth beyond transport-level security.

pub mod auth_interceptor;
pub mod gc_client;
pub mod mc_client;
pub mod mh_service;

pub use auth_interceptor::MhAuthLayer;
pub use gc_client::GcClient;
pub use mc_client::McClient;
pub use mh_service::MhMediaService;
