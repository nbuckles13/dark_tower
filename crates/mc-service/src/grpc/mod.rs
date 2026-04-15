//! gRPC service implementations for Meeting Controller.
//!
//! This module provides:
//! - `gc_client` - Client for MC→GC communication (registration, heartbeat)
//! - `mc_service` - Server for GC→MC communication (meeting assignment)
//! - `mh_client` - Client for MC→MH communication (RegisterMeeting)
//! - `auth_interceptor` - Authorization validation for incoming GC requests
//!
//! # Architecture (ADR-0023 Phase 6c)
//!
//! ```text
//! MC → GC: RegisterMC, FastHeartbeat, ComprehensiveHeartbeat
//! GC → MC: AssignMeetingWithMh (requires authorization)
//! MC → MH: RegisterMeeting (authenticated via OAuth token)
//! ```
//!
//! # Security
//!
//! All incoming gRPC requests from GC must pass through the [`McAuthInterceptor`]
//! which validates authorization headers. This provides defense-in-depth beyond
//! transport-level security.

pub mod auth_interceptor;
pub mod gc_client;
pub mod mc_service;
pub mod mh_client;

pub use auth_interceptor::McAuthInterceptor;
pub use gc_client::GcClient;
pub use mc_service::McAssignmentService;
pub use mh_client::MhClient;
