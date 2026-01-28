//! Meeting Controller (MC) Service Library
//!
//! This library provides the core functionality for the Dark Tower
//! Meeting Controller - a stateful WebTransport signaling server responsible for:
//!
//! - Real-time meeting coordination and participant state management
//! - Session binding token pattern for secure session recovery (ADR-0023)
//! - WebTransport connection handling for client signaling
//! - Layout subscription management (virtualized pub/sub)
//! - Media Handler coordination for media routing
//! - Graceful shutdown with meeting migration
//!
//! # Architecture
//!
//! The MC uses an actor model hierarchy (ADR-0023 Section 2):
//!
//! ```text
//! MeetingControllerActor (singleton per MC instance)
//! ├── supervises N MeetingActors
//! │   └── MeetingActor (one per active meeting)
//! │       ├── owns meeting state
//! │       └── supervises N ConnectionActors
//! │           └── ConnectionActor (one per WebTransport connection)
//! └── MhRegistryActor (tracks MH health via heartbeats)
//! ```
//!
//! # Key Design Decisions
//!
//! - **One connection per meeting**: A user in multiple meetings has multiple connections
//! - **Redis for state**: Live meeting state in Redis with sync writes for critical state
//! - **Fencing tokens**: Generation-based fencing prevents split-brain during failover
//! - **Session binding**: HMAC-SHA256 binding tokens with one-time nonces (30s TTL)
//!
//! # Modules
//!
//! - [`actors`] - Actor model implementation (Phase 6b)
//! - [`config`] - Service configuration from environment
//! - [`errors`] - Error types with appropriate error codes
//!
//! # Reference
//!
//! See ADR-0023 (Meeting Controller Architecture) for the full design specification.

pub mod actors;
pub mod config;
pub mod errors;
pub mod grpc;
pub mod redis;

// Future modules (Phase 6d+):
// pub mod handlers;       // WebTransport message handlers
// pub mod signaling;      // Signaling message routing
