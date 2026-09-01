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
//! # Current Status: Partial
//!
//! Registers with GC, and `RegisterMeeting` — the single MC→MH RPC, and the
//! ADR-0036 §8 control plane — records the meeting and promotes pending
//! WebTransport connections. It does **not** yet apply forwarding policy, so it
//! reports `applied_generation: 0` ("nothing applied"); that is story task 5.
//! No media forwarding is performed yet.
//!
//! # Architecture (ADR-0010, ADR-0023)
//!
//! ```text
//! MC → MH: RegisterMeeting (gRPC; ADR-0036 §8 control plane)
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
pub mod webtransport;
