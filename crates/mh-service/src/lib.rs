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
//! deliberately unconsumed until the forward path lands at story task 16.
//!
//! QUIC transport parameters are declared explicitly and validated at startup
//! (ADR-0036 §1): five required environment variables, a frames-to-bytes
//! datagram-buffer conversion performed once at config load, declared
//! receive-side flow control, and a shutdown drain window derived from the
//! pod's own termination grace rather than hardcoded. See [`config`].
//!
//! Two contract obligations are **deliberately not** implemented here, both
//! with their enforcement owner named at the code site: rejecting
//! `policy_generation == 0` (story task 13 — enforcing it before MC emits >= 1
//! would blackhole every meeting for the whole window), and binding a
//! `sender_id` to a live connection (story task 16).
//!
//! The ADR-0036 §10 per-connection transport seam ([`transport`]) now has its
//! real implementation — [`webtransport::WtMediaTransport`], wrapping
//! `wtransport` per-connection I/O. It is **plumbing, deliberately unwired**:
//! nothing constructs it yet, and the forward path that consumes it is story
//! task 16. Whoever wires it must construct it only *after*
//! [`webtransport::connection`]'s JWT gate returns, or unauthenticated media
//! I/O would start on an accepted-but-unvalidated session.
//!
//! Its `finished` flag is not incidental: `wtransport` collapses quinn's
//! stream-scoped `ClosedStream` into the same value it uses for connection
//! loss, so the dead-stream / dead-connection distinction the forward path
//! needs — a per-subscriber problem versus a per-participant one — survives
//! only where MH holds the state itself. Found by test, not by reading; see
//! `docs/TODO.md` for the constraint that places on task 16.
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
