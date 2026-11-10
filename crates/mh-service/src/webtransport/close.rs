//! How MH ends a client WebTransport session — the ONE home of ADR-0039 §7's
//! close-code contract on the MH side.
//!
//! # The invariant
//!
//! Every session MH ends **on purpose** (declined, released, or — from MH-3 —
//! shed at the drain deadline) closes with exactly one numeric `MhCloseCode`,
//! carried as a WebTransport **session** close: one `CLOSE_WEBTRANSPORT_SESSION`
//! capsule with an empty message on the CONNECT stream, then FIN. That is the
//! only close a browser reports as `WebTransportCloseInfo.closeCode`; a QUIC
//! `CONNECTION_CLOSE` carrying the number would reach a browser as transport
//! loss. Every OTHER end MH initiates closes QUIC explicitly with `H3_NO_ERROR`
//! and sends no capsule and no CONNECT FIN ([`end_without_code`]), so "no code"
//! keeps meaning transport loss.
//!
//! # The security conditions, by construction
//!
//! 1. **A code other than DECLINED only after the credential fully validated.**
//!    The only constructor of a non-DECLINED [`SessionClose`] is
//!    [`ValidatedSession::close`], and the only constructor of a
//!    [`ValidatedSession`] is [`validate`], which runs the token validation
//!    itself. A pre-validation site can only name [`SessionClose::declined`].
//! 2. **Only about the caller's own meeting.** A [`ValidatedSession`] carries the
//!    validated token's meeting; nothing here takes a meeting from anywhere else.
//! 3. **Nothing distinguishes token-failure kinds to the peer.** Every
//!    validation failure is the single DECLINED, with no reason bytes, and no
//!    kind-dependent delay is added here. The kind is in the server log only;
//!    on the meeting-token path `mh_jwt_validations_total` carries
//!    `failure_reason="validation_failed"` for every kind (story 4 MH-1 splits it
//!    for `media_dial`, per the ADR-0039 telemetry contract).
//!
//! # The one numeric conversion
//!
//! [`CloseCode::wire`] is the only place a code becomes a number, and it reads
//! the number from the generated `MhCloseCode` (`signaling.proto` is the single
//! source of truth for the values; `proto-gen`'s round-trip test pins them).
//! [`CloseCode`] has no zero variant, so "never send `UNSPECIFIED`" is a property
//! of the type, not of a test.

use crate::auth::MhJwtValidator;
use crate::config::CLOSE_SESSION_LINGER;
use crate::errors::MhError;
use crate::observability::metrics::{self, MediaSessionStartOutcome};
use crate::routing::MeetingKey;
use proto_gen::dark_tower::signaling::v1::MhCloseCode;
use tracing::{debug, info, warn};
use wtransport::proto::error::ErrorCode;
use wtransport::{Connection, VarInt};

/// The codes MH can send. No zero variant: `MH_CLOSE_CODE_UNSPECIFIED` is
/// unrepresentable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseCode {
    /// The token's audience is another handler (MH-1; no emitter yet).
    WrongHandler,
    /// MH refused the sender binding MC returned: it is held by another
    /// participant. The client retries once its old session ages out.
    BindingConflict,
    /// MH could not reach MC to bind the connection. Retried by the client.
    McUnavailable,
    /// MH no longer serves this meeting (released, removed, or shed).
    HandlerReleased,
    /// The credential was refused, or the session cannot be served. Means
    /// "token dead", not "handler dead".
    Declined,
}

impl CloseCode {
    /// Every code, in proto order. The zero-init and the label tests iterate
    /// this; the length is the catalogued cardinality of the `code` label.
    pub const ALL: [Self; 5] = [
        Self::WrongHandler,
        Self::BindingConflict,
        Self::McUnavailable,
        Self::HandlerReleased,
        Self::Declined,
    ];

    /// The generated proto value — the one place this module touches it.
    #[must_use]
    pub const fn proto(self) -> MhCloseCode {
        match self {
            Self::WrongHandler => MhCloseCode::WrongHandler,
            Self::BindingConflict => MhCloseCode::BindingConflict,
            Self::McUnavailable => MhCloseCode::McUnavailable,
            Self::HandlerReleased => MhCloseCode::HandlerReleased,
            Self::Declined => MhCloseCode::Declined,
        }
    }

    /// The number on the wire (the capsule's u32 code), read from the proto.
    ///
    /// Every value is positive — a COMPILE-TIME fact (the `const` assertion
    /// below `CloseCode`), so the conversion cannot fail and the fallback is
    /// unreachable.
    #[must_use]
    pub fn wire(self) -> u32 {
        u32::try_from(i32::from(self.proto())).unwrap_or(u32::MAX)
    }

    /// The `code` label on `mh_webtransport_session_closes_total`. The SAME
    /// tokens the client uses on `dt_client_mh_connection_total{outcome}`, so
    /// MH-sent and client-observed closes join (the proto name, lowercased,
    /// prefix stripped — pinned by test).
    #[must_use]
    pub const fn as_label(self) -> &'static str {
        match self {
            Self::WrongHandler => "wrong_handler",
            Self::BindingConflict => "binding_conflict",
            Self::McUnavailable => "mc_unavailable",
            Self::HandlerReleased => "handler_released",
            Self::Declined => "declined",
        }
    }
}

// Every code's proto value is positive, checked at compile time: a zero or a
// negative proto value for any `CloseCode` fails the build, so `wire()` never
// takes its fallback. Destructuring pins the arity to `ALL`'s.
const _: () = {
    let [a, b, c, d, e] = CloseCode::ALL;
    assert!(
        a.proto() as i32 > 0
            && b.proto() as i32 > 0
            && c.proto() as i32 > 0
            && d.proto() as i32 > 0
            && e.proto() as i32 > 0,
        "every CloseCode must map to a positive MhCloseCode"
    );
};

/// Why a media-session start was declined — the decline-only subset of
/// [`MediaSessionStartOutcome`]. Cannot express "started", so a decline path
/// can never be handed the success outcome, and the code mapping needs no arm
/// for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartDecline {
    NoSenderBinding,
    SenderBindingOutOfRange,
    SenderBindingConflict,
    McUnavailable,
    McAuthRejected,
    McEndpointUnknown,
}

impl StartDecline {
    /// Every decline. A test proves this is exactly the non-`Started` image of
    /// [`MediaSessionStartOutcome::ALL`] through [`Self::from_outcome`].
    pub const ALL: [Self; 6] = [
        Self::NoSenderBinding,
        Self::SenderBindingOutOfRange,
        Self::SenderBindingConflict,
        Self::McUnavailable,
        Self::McAuthRejected,
        Self::McEndpointUnknown,
    ];

    /// The counted outcome on `mh_media_session_starts_total`.
    #[must_use]
    pub const fn outcome(self) -> MediaSessionStartOutcome {
        match self {
            Self::NoSenderBinding => MediaSessionStartOutcome::DeclinedNoSenderBinding,
            Self::SenderBindingOutOfRange => {
                MediaSessionStartOutcome::DeclinedSenderBindingOutOfRange
            }
            Self::SenderBindingConflict => MediaSessionStartOutcome::DeclinedSenderBindingConflict,
            Self::McUnavailable => MediaSessionStartOutcome::DeclinedMcUnavailable,
            Self::McAuthRejected => MediaSessionStartOutcome::DeclinedMcAuthRejected,
            Self::McEndpointUnknown => MediaSessionStartOutcome::DeclinedMcEndpointUnknown,
        }
    }

    /// The reverse map, wildcard-free: a new [`MediaSessionStartOutcome`]
    /// variant does not compile until it is classified here, so the decline set
    /// and the outcome enum cannot drift apart.
    #[must_use]
    pub const fn from_outcome(outcome: MediaSessionStartOutcome) -> Option<Self> {
        match outcome {
            MediaSessionStartOutcome::Started => None,
            MediaSessionStartOutcome::DeclinedNoSenderBinding => Some(Self::NoSenderBinding),
            MediaSessionStartOutcome::DeclinedSenderBindingOutOfRange => {
                Some(Self::SenderBindingOutOfRange)
            }
            MediaSessionStartOutcome::DeclinedSenderBindingConflict => {
                Some(Self::SenderBindingConflict)
            }
            MediaSessionStartOutcome::DeclinedMcUnavailable => Some(Self::McUnavailable),
            MediaSessionStartOutcome::DeclinedMcAuthRejected => Some(Self::McAuthRejected),
            MediaSessionStartOutcome::DeclinedMcEndpointUnknown => Some(Self::McEndpointUnknown),
        }
    }
}

/// A session whose credential fully validated. Obtainable ONLY from
/// [`validate`]; it is what unlocks every code other than DECLINED.
///
/// Holds the token's meeting and subject and nothing else — never the claims
/// or the token, so nothing claim-shaped can reach a log through it.
pub struct ValidatedSession {
    meeting: MeetingKey,
    participant_id: String,
}

impl ValidatedSession {
    /// The validated token's meeting.
    #[must_use]
    pub fn meeting(&self) -> &MeetingKey {
        &self.meeting
    }

    /// The validated token's `meeting_id`.
    #[must_use]
    pub fn meeting_id(&self) -> &str {
        self.meeting.as_str()
    }

    /// The validated token's `sub` — the ONE read of it (see `handle_connection`'s
    /// provenance note): the value MC answers an identity question about and the
    /// value MH binds to a media route.
    #[must_use]
    pub fn participant_id(&self) -> &str {
        &self.participant_id
    }

    /// Builds (does not send) a close for THIS session's meeting, after
    /// validation; `close_session` sends it.
    #[must_use]
    pub const fn closing(&self, cause: ValidatedClose) -> SessionClose {
        SessionClose(Close::Validated(cause))
    }
}

/// Why a VALIDATED session is being closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidatedClose {
    /// The media-session start was declined.
    StartDeclined(StartDecline),
    /// MH released the meeting (`EndMeeting` today; MH-2 adds RELEASED verdicts
    /// and owner silence, MH-3 the drain-deadline shed).
    MeetingReleased,
    /// The token's audience is another handler. Reserved for MH-1's
    /// post-validation `aud` check: present so the mapping is total today, with
    /// no emitter yet.
    WrongHandler,
}

/// One intentional close, ready to be sent by [`close_session`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionClose(Close);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Close {
    /// Any close before (or instead of) a validated session's own cause.
    Declined,
    Validated(ValidatedClose),
}

impl SessionClose {
    /// The generic refusal: every pre-validation exit and every credential
    /// failure, indistinguishable at the peer.
    #[must_use]
    pub const fn declined() -> Self {
        Self(Close::Declined)
    }

    /// Whether the session was declined before any media loop ran, so every
    /// datagram it received is a `no_media_session` drop. Derived from the
    /// close itself (never carried beside it): true exactly for a declined
    /// media-session start. Pre-validation declines have never counted.
    #[must_use]
    pub const fn counts_undelivered_datagrams(self) -> bool {
        matches!(self.0, Close::Validated(ValidatedClose::StartDeclined(_)))
    }

    /// The code this close carries. Total and wildcard-free.
    #[must_use]
    pub const fn code(self) -> CloseCode {
        match self.0 {
            Close::Declined => CloseCode::Declined,
            Close::Validated(ValidatedClose::MeetingReleased) => CloseCode::HandlerReleased,
            Close::Validated(ValidatedClose::WrongHandler) => CloseCode::WrongHandler,
            Close::Validated(ValidatedClose::StartDeclined(decline)) => match decline {
                StartDecline::SenderBindingConflict => CloseCode::BindingConflict,
                StartDecline::McUnavailable => CloseCode::McUnavailable,
                StartDecline::NoSenderBinding
                | StartDecline::SenderBindingOutOfRange
                | StartDecline::McAuthRejected
                | StartDecline::McEndpointUnknown => CloseCode::Declined,
            },
        }
    }
}

/// Validate the connect credential — the ONLY way to obtain a
/// [`ValidatedSession`].
///
/// Records `mh_jwt_validations_total` on both arms and is the single log sink
/// for the failure's detail (`error = %e`, server-side only). The caller closes
/// any `Err` with DECLINED and hands the `MhError` to the accept loop unchanged.
///
/// # Errors
///
/// The validator's `MhError` for any credential failure.
pub async fn validate(
    validator: &MhJwtValidator,
    token: &str,
    connection_id: &str,
) -> Result<ValidatedSession, MhError> {
    match validator.validate_meeting_token(token).await {
        Ok(claims) => {
            metrics::record_jwt_validation("success", "meeting", "none");
            Ok(ValidatedSession {
                meeting: MeetingKey::new(&claims.meeting_id),
                participant_id: claims.sub,
            })
        }
        Err(e) => {
            warn!(
                target: "mh.webtransport.connection",
                connection_id = %connection_id,
                error = %e,
                "JWT validation failed"
            );
            metrics::record_jwt_validation("failure", "meeting", "validation_failed");
            Err(e)
        }
    }
}

/// `H3_NO_ERROR` (RFC 9114 §8.1), from wtransport-proto rather than retyped. The
/// QUIC close after a session close, and every code-less end. A browser is
/// EXPECTED to read a capsule-less QUIC close as a connection error rather than
/// a close code (unverified for `H3_NO_ERROR`; the story 4 C1 browser residual).
fn h3_no_error() -> VarInt {
    ErrorCode::NoError.to_code()
}

/// Send `close` as a WebTransport session close: one capsule with the code and
/// an empty message, then FIN on the CONNECT stream — THE emission site.
///
/// # ONE deadline over the whole sequence (a denial-of-service bound)
///
/// Writing the capsule waits on the PEER's flow-control credit for the CONNECT
/// stream, and a peer may never grant it while keeping QUIC alive with PINGs.
/// The write and the wait for the peer's reaction are therefore bounded
/// TOGETHER by [`CLOSE_SESSION_LINGER`]: past it MH closes QUIC with
/// `H3_NO_ERROR`, which also fails the stuck write inside the transport.
/// Without that, an unauthenticated client (pre-validation exits close with
/// DECLINED) could hold an `MH_MAX_CONNECTIONS` slot for as long as it liked.
/// The caller holds its slot until this returns; nothing from the peer is read
/// or dispatched here, and the wait depends only on the peer, never on the cause.
///
/// Counted once the capsule was WRITTEN — before the wait for the peer, so a
/// linger expiry still counts. A connection already closed, a capsule that
/// could not be written, or a write that did not finish within the deadline
/// sends no code and is NOT counted.
pub async fn close_session(connection: &Connection, close: SessionClose, connection_id: &str) {
    if connection.quic_connection().close_reason().is_some() {
        debug!(
            target: "mh.webtransport.connection",
            connection_id = %connection_id,
            "Session close skipped: the connection is already closed"
        );
        return;
    }
    let code = close.code();
    let deadline = tokio::time::Instant::now() + CLOSE_SESSION_LINGER;
    match tokio::time::timeout_at(deadline, connection.close_session(code.wire())).await {
        Ok(Ok(())) => metrics::record_session_close(code),
        Ok(Err(e)) => {
            warn!(
                target: "mh.webtransport.connection",
                connection_id = %connection_id,
                code = code.as_label(),
                error = %e,
                "Session close capsule was not sent; closing QUIC without a code"
            );
            connection.close(h3_no_error(), b"");
            return;
        }
        Err(_) => {
            info!(
                target: "mh.webtransport.connection",
                connection_id = %connection_id,
                code = code.as_label(),
                linger_ms = CLOSE_SESSION_LINGER.as_millis(),
                "Session close capsule could not be written within the linger bound (the peer \
                 withheld flow-control credit or went silent); closing QUIC without a code"
            );
            connection.close(h3_no_error(), b"");
            return;
        }
    }
    if let Ok(end) = tokio::time::timeout_at(deadline, connection.closed()).await {
        debug!(
            target: "mh.webtransport.connection",
            connection_id = %connection_id,
            code = code.as_label(),
            error = %end,
            "Session closed with a code"
        );
    } else {
        info!(
            target: "mh.webtransport.connection",
            connection_id = %connection_id,
            code = code.as_label(),
            linger_ms = CLOSE_SESSION_LINGER.as_millis(),
            "Peer did not react to the session close within the linger bound; closing QUIC \
             (the capsule may be lost, and the client then sees transport loss)"
        );
        connection.close(h3_no_error(), b"");
    }
}

/// End a session MH chose to end WITHOUT a code: closes QUIC with `H3_NO_ERROR`
/// and an empty reason BEFORE the connection is dropped.
///
/// Load-bearing: dropping a live wtransport connection can FIN the CONNECT
/// stream (quinn's `SendStream` drop finishes it) before the implicit QUIC
/// close, and a browser reads a bare CONNECT FIN as a clean close with code 0 —
/// a contract fault, not transport loss. Once QUIC is closed locally, no FIN
/// goes out. A no-op on an already-closed connection.
pub fn end_without_code(connection: &Connection) {
    connection.close(h3_no_error(), b"");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_code_all_maps_one_to_one_onto_every_nonzero_proto_value() {
        // Iterates the PROTO enum's values, so a new MhCloseCode variant fails
        // here until CloseCode carries it.
        // proto3 enum values here are small; the scanned window is bounded and
        // the largest value found must sit well inside it, so a value added
        // past the window fails this test instead of escaping it.
        const WINDOW: std::ops::RangeInclusive<i32> = -64..=64;
        let mut proto_nonzero = Vec::new();
        for value in WINDOW {
            if let Ok(code) = MhCloseCode::try_from(value) {
                if code != MhCloseCode::Unspecified {
                    proto_nonzero.push(code);
                }
            }
        }
        let max = proto_nonzero
            .iter()
            .map(|c| i32::from(*c))
            .max()
            .unwrap_or(0);
        assert!(
            max + 1 < *WINDOW.end(),
            "widen WINDOW: proto values reach {max}, a value past it would go unseen"
        );
        let ours: Vec<MhCloseCode> = CloseCode::ALL.iter().map(|c| c.proto()).collect();
        assert_eq!(
            ours, proto_nonzero,
            "CloseCode::ALL must be every non-zero proto value, in order"
        );
    }

    #[test]
    fn wire_codes_are_the_proto_numbers_and_never_zero() {
        for code in CloseCode::ALL {
            assert_eq!(i64::from(code.wire()), i64::from(i32::from(code.proto())));
            assert_ne!(code.wire(), 0, "{code:?} must never put 0 on the wire");
        }
    }

    #[test]
    fn labels_are_the_proto_names_lowercased_without_prefix() {
        for code in CloseCode::ALL {
            let expected = code
                .proto()
                .as_str_name()
                .strip_prefix("MH_CLOSE_CODE_")
                .expect("proto names carry the enum prefix")
                .to_ascii_lowercase();
            assert_eq!(code.as_label(), expected);
        }
    }

    #[test]
    fn start_declines_are_exactly_the_non_started_outcomes() {
        let mut images = Vec::new();
        for outcome in MediaSessionStartOutcome::ALL {
            match StartDecline::from_outcome(outcome) {
                Some(decline) => {
                    assert_eq!(decline.outcome(), outcome, "round trip for {outcome:?}");
                    images.push(decline);
                }
                None => assert_eq!(outcome, MediaSessionStartOutcome::Started),
            }
        }
        assert_eq!(images, StartDecline::ALL.to_vec());
    }

    /// The task's table, per arm. Expected codes are named by variant, never by
    /// number (the numbers' one pin is proto-gen's round-trip test).
    #[test]
    fn every_close_cause_maps_to_its_code() {
        assert_eq!(SessionClose::declined().code(), CloseCode::Declined);
        let validated = |cause| SessionClose(Close::Validated(cause)).code();
        assert_eq!(
            validated(ValidatedClose::MeetingReleased),
            CloseCode::HandlerReleased
        );
        assert_eq!(
            validated(ValidatedClose::WrongHandler),
            CloseCode::WrongHandler
        );
        // A LITERAL table (the task's mapping), not the implementation's match.
        let table = [
            (StartDecline::NoSenderBinding, CloseCode::Declined),
            (StartDecline::SenderBindingOutOfRange, CloseCode::Declined),
            (
                StartDecline::SenderBindingConflict,
                CloseCode::BindingConflict,
            ),
            (StartDecline::McUnavailable, CloseCode::McUnavailable),
            (StartDecline::McAuthRejected, CloseCode::Declined),
            (StartDecline::McEndpointUnknown, CloseCode::Declined),
        ];
        assert_eq!(
            table.map(|(d, _)| d),
            StartDecline::ALL,
            "the table must cover every decline, in order"
        );
        for (decline, expected) in table {
            assert_eq!(
                validated(ValidatedClose::StartDeclined(decline)),
                expected,
                "{decline:?}"
            );
        }
    }

    #[test]
    fn every_code_is_reachable_from_some_cause() {
        let mut produced: Vec<CloseCode> = vec![
            SessionClose::declined().code(),
            SessionClose(Close::Validated(ValidatedClose::MeetingReleased)).code(),
            SessionClose(Close::Validated(ValidatedClose::WrongHandler)).code(),
        ];
        produced.extend(
            StartDecline::ALL
                .iter()
                .map(|d| SessionClose(Close::Validated(ValidatedClose::StartDeclined(*d))).code()),
        );
        for code in CloseCode::ALL {
            assert!(produced.contains(&code), "{code:?} has no cause");
        }
    }

    /// The EXACT bytes the patched transport writes on the CONNECT stream, typed
    /// from the specs — never captured from the encoder's own output:
    ///
    /// - RFC 9114 §7.2.1 DATA frame: type `0x00`, length varint, payload;
    /// - RFC 9297 §3.2 capsule: type varint, length varint, value;
    /// - draft-ietf-webtrans-http3 §5: `CLOSE_WEBTRANSPORT_SESSION` = 0x2843, value
    ///   = 32-bit application error code (network order) + UTF-8 message;
    /// - RFC 9000 §16 minimal varints: 0x2843 (10307) is in the 2-byte range
    ///   (64..16383), so `0x40 | 0x28, 0x43` = `68 43`; 4 and 7 are 1-byte.
    ///
    /// So: `00` DATA, `07` length (2 + 1 + 4), `68 43` type, `04` length, code
    /// (BE), and NO message bytes.
    #[test]
    fn capsule_bytes_are_the_rfc_encoding() {
        let encode = wtransport::connection::encode_close_session_capsule;
        // HANDLER_RELEASED (4) and DECLINED (5).
        assert_eq!(
            encode(4),
            [0x00, 0x07, 0x68, 0x43, 0x04, 0x00, 0x00, 0x00, 0x04]
        );
        assert_eq!(
            encode(5),
            [0x00, 0x07, 0x68, 0x43, 0x04, 0x00, 0x00, 0x00, 0x05]
        );
        // A code with every byte distinct pins the byte order of the u32 path.
        assert_eq!(
            encode(0x0102_0304),
            [0x00, 0x07, 0x68, 0x43, 0x04, 0x01, 0x02, 0x03, 0x04]
        );
        // Every code we send: the last four bytes are its wire value, big-endian,
        // and the prefix is the fixed frame + capsule header.
        for code in CloseCode::ALL {
            let bytes = encode(code.wire());
            assert_eq!(bytes[..5], [0x00, 0x07, 0x68, 0x43, 0x04], "{code:?}");
            assert_eq!(bytes[5..], code.wire().to_be_bytes(), "{code:?}");
        }
    }

    /// The encoder's output parses back through UPSTREAM's own parser
    /// (wtransport-proto's frame + capsule readers): this ties the vendored
    /// 0x2843 literal to upstream's private constant, and proves the code
    /// survives and the message is empty.
    #[test]
    fn capsule_bytes_round_trip_through_the_upstream_parser() {
        use wtransport::proto::capsule::{capsules::CloseWebTransportSession, Capsule};
        use wtransport::proto::frame::{Frame, FrameKind};
        for code in CloseCode::ALL {
            let bytes = wtransport::connection::encode_close_session_capsule(code.wire());
            let mut reader: &[u8] = &bytes;
            let frame = Frame::read(&mut reader)
                .expect("parses as an HTTP/3 frame")
                .expect("a complete frame");
            assert!(
                reader.is_empty(),
                "{code:?}: exactly one frame, nothing trailing"
            );
            assert!(
                matches!(frame.kind(), FrameKind::Data),
                "{code:?}: a DATA frame"
            );
            let capsule = Capsule::with_frame(&frame).expect("upstream recognises the capsule");
            let close = CloseWebTransportSession::with_capsule(&capsule).expect("a session close");
            assert_eq!(
                close.error_code().into_inner(),
                u64::from(code.wire()),
                "{code:?}"
            );
            assert!(
                close.reason().is_empty(),
                "{code:?}: the message must be empty"
            );
        }
    }

    #[test]
    fn only_a_declined_media_start_counts_undelivered_datagrams() {
        assert!(!SessionClose::declined().counts_undelivered_datagrams());
        assert!(
            !SessionClose(Close::Validated(ValidatedClose::MeetingReleased))
                .counts_undelivered_datagrams()
        );
        for decline in StartDecline::ALL {
            assert!(
                SessionClose(Close::Validated(ValidatedClose::StartDeclined(decline)))
                    .counts_undelivered_datagrams(),
                "{decline:?}"
            );
        }
    }

    #[test]
    fn h3_no_error_is_the_rfc_value() {
        // RFC 9114 §8.1: `H3_NO_ERROR` = 0x0100.
        assert_eq!(h3_no_error().into_inner(), 0x0100);
    }
}
