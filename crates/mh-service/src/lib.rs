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
//! **The forward path now consumes that snapshot** (ADR-0036 §2/§7/§11,
//! [`media`]): three per-connection tasks move opaque, externally-authenticated
//! byte blobs from one transport endpoint to another, mutating only the six
//! relay bytes no signature covers, at an offset the blob's own header
//! determines, under a lock-free table re-read per frame; every
//! resource-consumption decision is taken before any allocation, and every
//! observation goes through a handle resolved before the loop started.
//!
//! **It does not run in production yet, and that is a missing contract rather
//! than missing code.** Forwarding is keyed on `(meeting, sender)`, and nothing
//! carries a participant → `sender_id` association to MH: the meeting JWT has
//! no sender field, `RegisterMeetingRequest` names `sender_id` with no
//! `participant_id`, `NotifyConnectedResponse` answers with a bare
//! acknowledgement, and `MhConnectRequest` carries only a token. Every
//! remaining route is client-asserted and therefore a cross-participant
//! injection primitive, so [`webtransport::connection`] declines to start the
//! media tasks and says so loudly rather than guessing an ordinal. See
//! [`session::SenderBindings`] and `docs/TODO.md` §Media Path Obligations.
//!
//! QUIC transport parameters are declared explicitly and validated at startup
//! (ADR-0036 §1): five required environment variables, a frames-to-bytes
//! datagram-buffer conversion performed once at config load, declared
//! receive-side flow control, and a shutdown drain window derived from the
//! pod's own termination grace rather than hardcoded. See [`config`].
//!
//! One contract obligation is **deliberately not** implemented here, with its
//! enforcement owner named at the code site: rejecting `policy_generation == 0`
//! (story task 13 — enforcing it before MC emits >= 1 would blackhole every
//! meeting for the whole window). The second, binding a `sender_id` to a live
//! connection, is blocked on the contract gap described above rather than
//! deferred.
//!
//! The ADR-0036 §10 per-connection transport seam ([`transport`]) has its real
//! implementation — [`webtransport::WtMediaTransport`], wrapping `wtransport`
//! per-connection I/O — and the forward path now wires it, **after**
//! [`webtransport::connection`]'s JWT gate returns. That ordering is a
//! requirement, not a preference: constructing it earlier would start
//! unauthenticated media I/O on an accepted-but-unvalidated session.
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

// Development-only per-frame media tracing (ADR-0036 §11: "A development-only
// per-frame tracing feature is guarded by a compile error in release builds,
// not by a CI check").
//
// MECHANISM COPIED, REASONING CROSS-REFERENCED, NOT RESTATED. The predicate,
// what supports it, both caveats on `dt-guard release-build-profile`, the named
// residual and the rejected `build.rs` alternative are stated once at
// `crates/mc-service/src/lib.rs` above its own `compile_error!`. Read that
// block; a second copy is the one that drifts, and the drifted copy is
// recognisable because it starts describing the gate as "release builds cannot
// enable it" — which MC's block explicitly forbids. This is the
// `debug_assertions` predicate, nothing more.
//
// The feature is DELIBERATELY NOT named `test-seams`: sharing MC's name would
// imply a relationship that does not exist. MC's seam exposes a sender-id
// exhaustion bypass; this one only widens telemetry.
//
// Consequence of the same self-dev-dependency shape MC records:
// `cargo test --release -p mh-service` trips this, and nothing in the pipeline
// runs it today.
//
// What the facility may emit, even when enabled: per-frame SIZE and the
// bounded reject/drop tokens. Never payload bytes, never the SFrame key id
// (`payload()`'s first 8 bytes), never wrapped key material. Per-frame size is
// the sensitive item under §11 — the time-ordered sequence of sizes for one
// stream is the voice-activity trace — and this facility legitimately emits it,
// which is the whole reason it is gated at all.
#[cfg(all(feature = "per-frame-trace", not(debug_assertions)))]
compile_error!(
    "the `per-frame-trace` feature emits per-frame media telemetry (ADR-0036 §11's \
     voice-activity trace) and must never be enabled in a release build"
);

pub mod auth;
pub mod config;
pub mod errors;
pub mod grpc;
pub mod media;
pub mod observability;
pub mod process;
pub mod routing;
pub mod session;
pub mod transport;
pub mod webtransport;
