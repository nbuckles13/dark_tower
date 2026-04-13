//! WebTransport server and connection handler for client media connections.
//!
//! This module implements the client-facing WebTransport entry point:
//! - [`server`] - Accept loop with TLS 1.3 termination via `wtransport`
//! - [`connection`] - Per-connection handler: accept session, read meeting JWT,
//!   validate, check registration status, provisional accept with timeout

pub mod connection;
pub mod server;

pub use server::WebTransportServer;
