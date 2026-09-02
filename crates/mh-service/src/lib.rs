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
//! ADR-0036 §8 control plane — records the meeting, promotes pending
//! WebTransport connections, and **applies forwarding policy**: it validates
//! the request, hands it to the session actor over a config-apply mailbox
//! separate from the connection-lifecycle mailbox, publishes a lock-free
//! [`routing`] snapshot, and answers with the generation MH's live forward path
//! actually reflects.
//!
//! No media forwarding is performed yet — the snapshot is published and
//! deliberately unconsumed until the forward path lands at story task 16, in
//! the same posture as the [`transport`] seam below.
//!
//! Two contract obligations are **deliberately not** implemented here, both
//! with their enforcement owner named at the code site: rejecting
//! `policy_generation == 0` (story task 13 — enforcing it before MC emits >= 1
//! would blackhole every meeting for the whole window), and binding a
//! `sender_id` to a live connection (story task 16).
//!
//! The ADR-0036 §10 per-connection transport seam ([`transport`]) has landed
//! and is deliberately unconsumed: no production type implements it yet. The
//! real wtransport implementation and the forward path that runs against it are
//! story tasks 12 and 16 respectively.
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
pub mod process;
pub mod routing;
pub mod session;
pub mod transport;
pub mod webtransport;
