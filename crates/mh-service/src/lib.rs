//! Media Handler (MH) Service Library
//!
//! This library provides the core functionality for the Dark Tower
//! Media Handler - an SFU (Selective Forwarding Unit) responsible for:
//!
//! - Media stream forwarding between participants
//! - Quality adaptation based on receiver bandwidth
//! - WebTransport connection handling for media data
//! - GC registration and load reporting
//!
//! # Current Status: Stub
//!
//! This is a stub implementation that registers with GC and accepts
//! all MC→MH gRPC calls, returning success responses. No real media
//! handling is performed. This unblocks end-to-end join flow testing.
//!
//! # Architecture (ADR-0010, ADR-0023)
//!
//! ```text
//! MC → MH: Register, RouteMedia, StreamTelemetry (gRPC)
//! MH → GC: RegisterMH, SendLoadReport (gRPC)
//! Client → MH: WebTransport media streams (stub: accept + log)
//! ```

#![warn(clippy::pedantic)]

pub mod auth;
pub mod config;
pub mod errors;
pub mod grpc;
pub mod observability;
pub mod session;
pub mod token_refresh_metrics;
pub mod webtransport;
