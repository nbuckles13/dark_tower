//! WebTransport server and connection handler for client media connections.
//!
//! This module implements the client-facing WebTransport entry point:
//! - [`server`] - Accept loop with TLS 1.3 termination via `wtransport`, and the
//!   explicit ADR-0036 §1 QUIC transport configuration the endpoint is built with
//! - [`connection`] - Per-connection handler: accept session, read meeting JWT,
//!   validate, check registration status, provisional accept with timeout
//! - [`media_transport`] - The REAL implementation of the [`crate::transport`]
//!   seam, wrapping `wtransport` per-connection I/O (ADR-0036 §10). Plumbing
//!   only: the forward path that consumes it lands with story task 16.

pub mod connection;
pub mod media_transport;
pub mod server;

pub use media_transport::{WtMediaTransport, WtSendStream};
pub use server::WebTransportServer;
