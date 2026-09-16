//! Generated Protocol Buffer code for Dark Tower signaling messages.
//!
//! This crate contains the compiled Protocol Buffer definitions used for
//! communication between Dark Tower components.
//!
//! Module paths mirror the proto package hierarchy so the schema version is
//! visible at every consumer use-site: `proto_gen::dark_tower::signaling::v1`,
//! `proto_gen::dark_tower::internal::v1`. No flat re-export aliases — adding
//! a future v2 will not silently rename v1 message semantics for any caller.

#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::doc_markdown)] // Generated code has various doc formatting
#![allow(clippy::default_trait_access)] // Generated code uses Default::default()
#![allow(clippy::too_many_lines)] // Generated code has long functions
#![allow(clippy::struct_excessive_bools)] // Generated protobuf structs may have many bool fields
#![allow(clippy::result_large_err)] // Generated tonic client/server fns return Result<_, tonic::Status>; Status is ~176 bytes and not ours to shrink

// Re-export prost traits for convenience
pub use prost::Message;

// Re-export tonic for gRPC service traits
pub use tonic;

// Generated protobuf modules — paths mirror proto package hierarchy.
pub mod dark_tower {
    pub mod signaling {
        pub mod v1 {
            include!(concat!(env!("OUT_DIR"), "/dark_tower.signaling.v1.rs"));
        }
    }

    pub mod internal {
        pub mod v1 {
            include!(concat!(env!("OUT_DIR"), "/dark_tower.internal.v1.rs"));
        }
    }
}

// ---------------------------------------------------------------------------
// Redacting `Debug` for credential- and key-bearing signalling messages
// ---------------------------------------------------------------------------
//
// prost's derived `Debug` is suppressed for these three messages in
// `build.rs` (`skip_debug`), so the impls below are the ONLY `Debug` they
// have. They are written rather than omitted because the enclosing envelopes
// (`ClientMessage`, `ServerMessage`) derive `Debug` transitively and callers
// legitimately format them — omitting the impl would break those call sites
// and invite someone to remove the `skip_debug` entry instead.
//
// Shape deliberately mirrors `crates/media-protocol/src/frame.rs`'s
// `impl Debug for WrappedTransmitKey`: metadata in the clear, secret bytes as
// `<redacted {N} B>`. One pattern for the whole media path, so a log line
// reads identically across the frame codec and the signalling layer.
//
// SAFETY INVARIANT — why disclosing these lengths is safe. What is printed is
// always a byte LENGTH, never secret content. For `meeting_kek` and
// `binding_token` the length is a protocol constant — 0 when absent, else fixed
// (KEK 32 bytes; `binding_token` is hex(HMAC-SHA256) = 64 chars,
// `crates/mc-service/src/actors/session.rs:57`) — so it is not a function of the
// secret at all. `join_token` is the one field whose length DOES vary with its
// value (a JWT grows with its claims); disclosing it is acceptable because a
// bearer JWT's confidentiality rests on its bytes, not its size, and the coarse
// length narrows nothing usable for an attacker who lacks the token. The rule
// for the next author is STRICTER than these fields need: if you add a field
// whose length could itself narrow the secret's value, render presence
// (`<empty>` / `<present>`), NOT length.
use core::fmt;

/// Length-only redaction placeholder for a secret field. Rendered WITHOUT the
/// quotes a `String` would carry, so every key- and token-bearing field reads
/// identically to `crates/media-protocol/src/frame.rs`'s `WrappedTransmitKey`
/// in a log line — one mechanism, one spelling, for the whole media path.
///
/// `<empty>` distinguishes the not-yet-provisioned state from a populated
/// secret. That distinction is most load-bearing on `meeting_kek`, where an
/// empty value is the documented "not yet provisioned" sentinel (see the field
/// comments in `signaling.proto`), so it must reach that field too — which the
/// earlier inline `format_args!` path did not.
struct RedactedLen(usize);

impl fmt::Debug for RedactedLen {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 == 0 {
            f.write_str("<empty>")
        } else {
            write!(f, "<redacted {} B>", self.0)
        }
    }
}

/// Redacting `Debug`; see the SAFETY INVARIANT block above `RedactedLen`.
impl fmt::Debug for dark_tower::signaling::v1::JoinRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JoinRequest")
            .field("meeting_id", &self.meeting_id)
            // Bearer meeting JWT.
            .field("join_token", &RedactedLen(self.join_token.len()))
            // PII (name is PII_TOKENS_CATEGORY_B); redacted like
            // `crates/common/src/jwt.rs`'s `display_name`.
            .field("participant_name", &format_args!("[REDACTED]"))
            .field("capabilities", &self.capabilities)
            .field("correlation_id", &self.correlation_id)
            // HMAC session-binding credential (ADR-0023).
            .field("binding_token", &RedactedLen(self.binding_token.len()))
            // Public key: not a secret, but length is the useful fact and a
            // 32-byte blob is noise in a log line.
            .field("identity_public_key_len", &self.identity_public_key.len())
            .finish()
    }
}

/// Redacting `Debug`; see the SAFETY INVARIANT block above `RedactedLen`.
impl fmt::Debug for dark_tower::signaling::v1::JoinResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JoinResponse")
            .field("participant_id", &self.participant_id)
            // Rendered as a COUNT, not the list: `Participant` derives `Debug`,
            // so printing the roster would dump every member's `name` (PII,
            // PII_TOKENS_CATEGORY_B — ADR-0011 membership disclosure) and their
            // raw `identity_public_key` bytes. One `{:?}` on a `JoinResponse`
            // would otherwise emit the whole roster's display names in a line.
            .field(
                "existing_participants_count",
                &self.existing_participants.len(),
            )
            .field("media_servers", &self.media_servers)
            // NOT redacted, deliberately: `correlation_id` is an identifier,
            // not a credential — possessing it does not authenticate a
            // reconnect, the HMAC on `binding_token` does — and it is the
            // ADR-0023 reconnect correlation key an operator reaches for first
            // when triaging a failed session recovery.
            .field("correlation_id", &self.correlation_id)
            .field("binding_token", &RedactedLen(self.binding_token.len()))
            .field("sender_id", &self.sender_id)
            // Live meeting KEK (ADR-0036 §4).
            .field("meeting_kek", &RedactedLen(self.meeting_kek.len()))
            .field("kek_generation", &self.kek_generation)
            .finish()
    }
}

/// Redacting `Debug`; see the SAFETY INVARIANT block above `RedactedLen`.
impl fmt::Debug for dark_tower::signaling::v1::MeetingKekUpdate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MeetingKekUpdate")
            .field("meeting_kek", &RedactedLen(self.meeting_kek.len()))
            .field("kek_generation", &self.kek_generation)
            .finish()
    }
}

/// Redacting `Debug`; see the SAFETY INVARIANT block above `RedactedLen`.
/// `join_token` is a bearer meeting JWT (same semantics as
/// `JoinRequest.join_token`), so this envelope carries the same redaction the
/// join path does — otherwise a malformed-envelope `?envelope` log in
/// `crates/mh-service/src/webtransport/connection.rs` prints it.
impl fmt::Debug for dark_tower::signaling::v1::MhConnectRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MhConnectRequest")
            .field("join_token", &RedactedLen(self.join_token.len()))
            .finish()
    }
}

/// Redacting `Debug`; see the SAFETY INVARIANT block above `RedactedLen`.
/// `name` is PII (ADR-0011 membership disclosure) and `identity_public_key` is
/// a stable per-participant identifier. This type is broadcast on every join via
/// `ParticipantJoined`, which rides inside `ServerMessage`'s transitive `Debug`,
/// so redacting only `JoinResponse` would leave the same fields printable one
/// participant at a time. Length only for the key — a 32-byte blob is noise in a
/// log line — matching `JoinRequest`'s self-key rendering.
impl fmt::Debug for dark_tower::signaling::v1::Participant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Participant")
            .field("participant_id", &self.participant_id)
            .field("name", &format_args!("[REDACTED]"))
            .field("streams", &self.streams)
            .field("joined_at", &self.joined_at)
            .field("sender_id", &self.sender_id)
            .field("identity_public_key_len", &self.identity_public_key.len())
            .finish()
    }
}
