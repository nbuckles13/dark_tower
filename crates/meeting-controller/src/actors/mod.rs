//! Actor model implementation for Meeting Controller (ADR-0023).
//!
//! This module implements the actor hierarchy defined in ADR-0023 Section 2:
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
//! - **CancellationToken propagation**: Parent actors pass child tokens for graceful shutdown
//! - **Mailbox monitoring**: Depth thresholds with metrics (Meeting: 100/500, Connection: 50/200)
//! - **Message passing**: All inter-actor communication via `tokio::sync::mpsc` channels
//!
//! # Modules
//!
//! - [`controller`] - `MeetingControllerActor` singleton that supervises meetings
//! - [`meeting`] - `MeetingActor` per active meeting, owns meeting state
//! - [`connection`] - `ConnectionActor` per WebTransport connection
//! - [`messages`] - Message types for actor communication
//! - [`metrics`] - Mailbox monitoring and actor metrics
//! - [`session`] - Session binding token generation and validation

pub mod connection;
pub mod controller;
pub mod meeting;
pub mod messages;
pub mod metrics;
pub mod session;

// Re-export primary types
pub use connection::{ConnectionActor, ConnectionActorHandle};
pub use controller::{MeetingControllerActor, MeetingControllerActorHandle};
pub use meeting::{MeetingActor, MeetingActorHandle};
pub use messages::*;
pub use metrics::{ActorMetrics, ControllerMetrics, ControllerMetricsSnapshot, MailboxMonitor};
pub use session::{SessionBindingManager, StoredBinding};
