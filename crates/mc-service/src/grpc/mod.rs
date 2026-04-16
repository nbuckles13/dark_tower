//! gRPC service implementations for Meeting Controller.
//!
//! This module provides:
//! - `gc_client` - Client for MC→GC communication (registration, heartbeat)
//! - `mc_service` - Server for GC→MC communication (meeting assignment)
//! - `mh_client` - Client for MC→MH communication (RegisterMeeting)
//! - `media_coordination` - Server for MH→MC communication (participant notifications)
//! - `auth_interceptor` - Authorization validation for incoming requests
//!
//! # Architecture (ADR-0023 Phase 6c)
//!
//! ```text
//! MC → GC: RegisterMC, FastHeartbeat, ComprehensiveHeartbeat
//! GC → MC: AssignMeetingWithMh (requires authorization)
//! MC → MH: RegisterMeeting (authenticated via OAuth token)
//! MH → MC: NotifyParticipantConnected/Disconnected (requires McAuthLayer, R-22)
//! ```
//!
//! # Security
//!
//! - All incoming gRPC calls pass through [`McAuthLayer`] (JWKS-based two-layer validation, ADR-0003)

pub mod auth_interceptor;
pub mod gc_client;
pub mod mc_service;
pub mod media_coordination;
pub mod mh_client;

pub use auth_interceptor::McAuthLayer;
pub use gc_client::GcClient;
pub use mc_service::McAssignmentService;
pub use media_coordination::McMediaCoordinationService;
pub use mh_client::MhClient;
