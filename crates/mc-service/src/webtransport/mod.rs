//! WebTransport server and connection handler for client signaling.
//!
//! This module implements the client-facing WebTransport entry point:
//! - [`server`] - Accept loop with TLS 1.3 termination via `wtransport`
//! - [`connection`] - Per-connection actor: owns streams, sends JoinResponse, runs bridge loop
//! - [`handler`] - Shared protobuf encoding utilities (encode_participant_update, etc.)

pub mod connection;
pub mod handler;
pub mod server;

pub use server::WebTransportServer;
