//! Message types for actor communication (ADR-0023 Section 2).
//!
//! All inter-actor communication uses strongly-typed message passing via `tokio::sync::mpsc`.
//! Response patterns use `tokio::sync::oneshot` for request-reply semantics.

use super::participant::ParticipantActorHandle;
use crate::errors::McError;
use std::time::Duration;
use tokio::sync::oneshot;

/// Messages sent to `MeetingControllerActor`.
#[derive(Debug)]
pub enum ControllerMessage {
    /// Create a new meeting actor for the given meeting ID.
    CreateMeeting {
        meeting_id: String,
        /// Response channel for the meeting actor handle or error.
        respond_to: oneshot::Sender<Result<(), McError>>,
    },

    /// Get a handle to an existing meeting actor.
    GetMeeting {
        meeting_id: String,
        /// Response channel for the meeting actor handle or error.
        respond_to: oneshot::Sender<Result<MeetingInfo, McError>>,
    },

    /// Get a handle to a live meeting actor.
    ///
    /// Distinct from [`ControllerMessage::GetMeeting`], which returns a state
    /// SNAPSHOT: this returns the handle itself, so a caller holding it can talk
    /// to the meeting directly without a controller hop per message.
    ///
    /// Taken ONCE per WebTransport connection, at join. The post-join dispatch
    /// path then reaches meeting state without routing through the controller's
    /// single mailbox, which would otherwise make the controller a serialization
    /// point for every client signalling message in the process.
    GetMeetingHandle {
        meeting_id: String,
        /// Response channel for the live handle, or `MeetingNotFound`.
        respond_to: oneshot::Sender<Result<super::meeting::MeetingActorHandle, McError>>,
    },

    /// Remove a meeting (called when all participants leave or meeting ends).
    RemoveMeeting {
        meeting_id: String,
        /// Response channel for confirmation.
        respond_to: oneshot::Sender<Result<(), McError>>,
    },

    /// Create a meeting with test-only construction overrides (sender-id
    /// cursor, flush bound). **Test builds only.**
    #[cfg(feature = "test-seams")]
    CreateMeetingWithSeams {
        meeting_id: String,
        seams: super::meeting_media::MeetingSeams,
        respond_to: oneshot::Sender<Result<(), McError>>,
    },

    /// Get current status of all meetings (for health checks).
    GetStatus {
        /// Response channel for controller status.
        respond_to: oneshot::Sender<ControllerStatus>,
    },

    /// Fire-and-forget: route a new connection to the correct meeting.
    ///
    /// The controller looks up the meeting and forwards as `ConnectionJoin`.
    /// The result is sent back to the connection actor via `respond_to`.
    JoinConnection {
        meeting_id: String,
        connection_id: String,
        user_id: String,
        participant_id: String,
        /// The joiner's Ed25519 identity signing public key, already parsed at
        /// the WebTransport trust boundary. `None` means **no key published** —
        /// a legitimate, contract-defined state, not a failure. Typed
        /// `Option<IdentityPublicKey>` rather than `Vec<u8>` so a wrong-length
        /// key is structurally unable to reach the meeting actor: parse, don't
        /// validate.
        identity_public_key: Option<crate::media_admission::IdentityPublicKey>,
        /// Registered display name from the validated meeting-token claim
        /// (already length-bounded at the connection trust boundary). Empty
        /// string means the claim carried no name → a generic label is used.
        display_name: String,
        is_host: bool,
        /// Sender for writing framed protobuf bytes to the WebTransport stream.
        stream_tx: tokio::sync::mpsc::Sender<bytes::Bytes>,
        /// Media-routing inputs: server dependencies and the meeting's handler
        /// set as read for this join (frozen by the actor at the first join).
        media: super::meeting_media::JoinMedia,
        /// Response channel sent back to the WebTransport connection actor.
        respond_to: oneshot::Sender<Result<JoinResult, McError>>,
    },

    /// Initiate graceful shutdown (SIGTERM received).
    Shutdown {
        /// Deadline for shutdown.
        deadline: Duration,
        /// Response channel for confirmation.
        respond_to: oneshot::Sender<Result<(), McError>>,
    },
}

/// Messages sent to `MeetingActor`.
#[derive(Debug)]
pub enum MeetingMessage {
    /// A new connection wants to join this meeting.
    ConnectionJoin {
        connection_id: String,
        user_id: String,
        participant_id: String,
        /// See `ControllerMessage::JoinConnection::identity_public_key`.
        identity_public_key: Option<crate::media_admission::IdentityPublicKey>,
        /// Registered display name from the validated meeting-token claim
        /// (already length-bounded at the connection trust boundary). Empty
        /// string means the claim carried no name → a generic label is used.
        display_name: String,
        /// Whether this participant has host privileges.
        is_host: bool,
        /// Sender for writing framed protobuf bytes to the WebTransport stream.
        stream_tx: Option<tokio::sync::mpsc::Sender<bytes::Bytes>>,
        /// Media-routing inputs (see `ControllerMessage::JoinConnection`).
        media: super::meeting_media::JoinMedia,
        /// Response channel for join result.
        respond_to: oneshot::Sender<Result<JoinResult, McError>>,
    },

    /// A connection has disconnected (may reconnect within grace period).
    ///
    /// `cause` carries the transport-authenticated close classification so the
    /// meeting can decide between an immediate roster removal (clean client
    /// close) and the ADR-0023 grace period (abrupt/ambiguous loss).
    ConnectionDisconnected {
        connection_id: String,
        participant_id: String,
        cause: DisconnectCause,
    },

    /// A connection is attempting to reconnect.
    ConnectionReconnect {
        connection_id: String,
        correlation_id: String,
        binding_token: String,
        /// Response channel for reconnect result.
        respond_to: oneshot::Sender<Result<ReconnectResult, McError>>,
    },

    /// A participant is leaving the meeting (explicit leave, not disconnect).
    ParticipantLeave {
        participant_id: String,
        /// Response channel for confirmation.
        respond_to: oneshot::Sender<Result<(), McError>>,
    },

    /// Forward a signaling message from a connection.
    SignalingMessage {
        participant_id: String,
        message: SignalingPayload,
    },

    /// Get current meeting state (for debugging/health).
    GetState {
        /// Response channel for meeting state.
        respond_to: oneshot::Sender<MeetingState>,
    },

    /// An MH reports a participant's media connection (ADR-0036 §2, §4, §9):
    /// resolve the connection's `sender_id` AND record the connectivity, in ONE
    /// actor turn, so the binding MC answers and the routing input it records
    /// can never name different roster entries.
    ///
    /// # Why this is keyed on `user_id` and not `participant_id`
    ///
    /// **This is an identity-translation seam and the reason is not obvious, so
    /// it is stated here rather than left to be re-derived.** MH names the
    /// connecting party by the validated meeting token's `sub`
    /// (`mh-service/src/webtransport/connection.rs`), which is a **contract
    /// MUST** — that provenance is the entire defence against a client
    /// asserting someone else's identity (@security S1). MC, however, mints a
    /// **fresh `Uuid` per join** as its `participant_id` and stores the token
    /// `sub` as the participant's `user_id`. The two namespaces are disjoint,
    /// and MH has never seen MC's UUID. **Keying this lookup on MC's
    /// `participant_id` resolves nothing, ever** — the defect Gate 2 attempt 1
    /// of the binding contract found.
    ///
    /// Deliberately **not** served by [`Self::GetState`], which clones the whole
    /// roster to answer a one-field question on a per-connection path (§11
    /// read-surface discipline).
    ///
    /// Scoped to THIS meeting: sender ids are per-meeting ordinals, so a
    /// cross-meeting answer is unrepresentable rather than merely unlikely.
    MediaConnected {
        /// The token `sub` MH reported.
        user_id: String,
        /// The MH-asserted handler id; resolved by exact match against the
        /// meeting's frozen set, never trusted as a string.
        handler_id: String,
        /// MH's opaque per-session connection id (empty = legacy MH).
        connection_id: String,
        /// The binding answer and, separately, why connectivity was not
        /// recorded (if it was not). The binding is answered even when
        /// connectivity is not (e.g. `handler_not_in_set`), so MH's binding
        /// contract is unchanged.
        respond_to: oneshot::Sender<(SenderLookup, Option<crate::media_routing::Unapplied>)>,
    },

    /// An MH reports a participant's media connection closed. Removes only that
    /// connection's key, and only from the participant `user_id` resolves to
    /// (security S12: no lookup across participants).
    MediaDisconnected {
        /// The token `sub` MH reported.
        user_id: String,
        /// The MH-asserted handler id (exact-match resolved).
        handler_id: String,
        /// MH's opaque per-session connection id (empty = legacy MH).
        connection_id: String,
        /// `None` if applied, else why not.
        respond_to: oneshot::Sender<Option<crate::media_routing::Unapplied>>,
    },

    /// Register a participant's validated receive-capability declaration
    /// (ADR-0036 §6), making the actor the source of truth for slot demand.
    ///
    /// Only ACCEPTED declarations arrive here: every rejection (cap, budget,
    /// malformed) is decided connection-side without touching the actor, and
    /// an identical re-declaration is short-circuited there too. The actor
    /// resizes the participant's slots, re-renders, re-pushes and re-emits.
    RegisterReceiveCapability {
        participant_id: String,
        declaration: crate::media_signaling::ReceiveCapabilityDeclaration,
        /// `Ok` once the declaration is registered (and a first flush ran).
        respond_to: oneshot::Sender<Result<(), McError>>,
    },

    /// Update participant mute status (self-mute, informational).
    UpdateSelfMute {
        participant_id: String,
        audio_muted: bool,
        video_muted: bool,
    },

    /// Server-mutes a participant by meeting policy (enforced at MH ingress,
    /// ADR-0036 §5). "Host mute" is avoided as a term: it presumes a role
    /// model this system has not defined.
    ServerMute {
        target_participant_id: String,
        muted_by: String,
        audio_muted: bool,
        video_muted: bool,
        /// Response channel for confirmation.
        respond_to: oneshot::Sender<Result<(), McError>>,
    },

    /// End the meeting (called by host or system).
    EndMeeting {
        reason: String,
        /// Response channel for confirmation.
        respond_to: oneshot::Sender<Result<(), McError>>,
    },
}

/// Messages sent to `ParticipantActor`.
#[derive(Debug)]
pub enum ParticipantMessage {
    /// Send a signaling message to the connected client.
    Send { message: SignalingPayload },

    /// Notify participant of a state change.
    ParticipantUpdate { update: ParticipantStateUpdate },

    /// Record per-MH connection statuses reported by the client (R-60).
    ///
    /// Each entry is `(truncated mh_url, bounded status)`. Truncation of all
    /// client-controlled strings (the key and the value's fields) happens ONCE
    /// at the connection trust boundary before this message is built, so the
    /// actor can never hold an untruncated field.
    RecordMhStatuses {
        statuses: Vec<(String, BoundedMhStatus)>,
    },

    /// Close the participant actor gracefully.
    Close { reason: String },

    /// Ping the participant actor to check liveness.
    Ping { respond_to: oneshot::Sender<()> },
}

/// Per-MH connection state reported by the client in a `MediaConnectionUpdate`
/// (R-60). Bounded, low-cardinality domain enum (decoupled from the wire proto)
/// — drives the `mc_participant_mh_status_total{state}` metric label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MhState {
    /// Client reports the MH media connection is established.
    Connected,
    /// Client reports the MH media connection failed.
    Failed,
    /// Client reports the MH media connection was disconnected.
    Disconnected,
    /// Proto3 default / unknown wire value (allowlist-clamp catch-all).
    Unspecified,
}

impl MhState {
    /// Lowercase, bounded metric-label form (exactly 4 possible values).
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Connected => "connected",
            Self::Failed => "failed",
            Self::Disconnected => "disconnected",
            Self::Unspecified => "unspecified",
        }
    }
}

/// Bounded per-MH status stored on the `ParticipantActor` (R-60).
///
/// All client-controlled strings are truncated (≤256 bytes, char boundary) at
/// the connection boundary before construction — this struct, by design, only
/// ever holds bounded data, so the actor cannot leak an untruncated field.
#[derive(Debug, Clone)]
pub struct BoundedMhStatus {
    /// Reported connection state (bounded domain enum).
    pub state: MhState,
    /// Optional truncated failure reason (client-controlled, ≤256 bytes).
    pub failure_reason: Option<String>,
    /// Optional truncated failure code (client-controlled, ≤256 bytes).
    pub failure_code: Option<String>,
}

// ----------------------------------------------------------------------------
// Supporting Types
// ----------------------------------------------------------------------------

/// Information about a meeting returned by GetMeeting.
#[derive(Debug, Clone)]
pub struct MeetingInfo {
    /// Meeting ID.
    pub meeting_id: String,
    /// Current participant count.
    pub participant_count: usize,
    /// Meeting creation timestamp.
    pub created_at: i64,
    /// Current fencing generation.
    pub fencing_generation: u64,
}

/// Status of the `MeetingControllerActor`.
#[derive(Debug, Clone)]
pub struct ControllerStatus {
    /// Total active meetings.
    pub meeting_count: usize,
    /// Total active connections across all meetings.
    pub connection_count: usize,
    /// Whether the controller is draining.
    pub is_draining: bool,
    /// Current mailbox depth.
    pub mailbox_depth: usize,
}

/// Outcome of resolving a token `sub` to a per-meeting `sender_id`.
///
/// Three arms, not an `Option`, because **"I do not know this user" and "I know
/// more than one" are different facts with different remedies** and only one of
/// them is a race.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SenderLookup {
    /// Exactly one participant on this meeting's roster carries that `user_id`.
    Found(crate::media_admission::SenderId),
    /// No participant on this meeting's roster carries that `user_id`.
    ///
    /// Usually the benign race: MH's connect notification overtaking MC's join.
    NotFound,
    /// **More than one** participant on this meeting's roster carries that
    /// `user_id`, so the token `sub` does not identify a single sender.
    ///
    /// MC mints a fresh `participant_id` per join and does **not** bar the same
    /// user joining a meeting twice (two devices, or a reconnect that produced a
    /// second roster entry), so this is reachable rather than theoretical.
    ///
    /// **Fail closed: the caller answers `0`.** Picking either candidate would
    /// bind MH's connection to an ordinal that may belong to the user's *other*
    /// participant, and MH would then stamp that participant's `sender_id` onto
    /// this connection's frames — a cross-participant misattribution inside one
    /// user's own identity, which is exactly the class the binding contract
    /// exists to prevent. An arbitrary pick would be right half the time and
    /// undetectable when wrong.
    ///
    /// The ambiguity is **inherent in the contract as ruled**, not a bug in this
    /// lookup: the token `sub` is the only participant identity MH holds, so
    /// MH's question is genuinely ambiguous when a user has two participants.
    /// `connection_id` does NOT resolve it — it tells MH connections apart, not
    /// MC joins. The remedy has one home: `docs/TODO.md` §Media Path
    /// Obligations, "`user_ambiguous` has no operator remedy".
    Ambiguous,
}

/// Result of a successful join.
#[derive(Debug, Clone)]
pub struct JoinResult {
    /// Assigned participant ID.
    pub participant_id: String,
    /// Correlation ID for reconnection.
    pub correlation_id: String,
    /// Binding token for reconnection (HMAC-SHA256).
    pub binding_token: String,
    /// List of other participants in the meeting.
    pub participants: Vec<ParticipantInfo>,
    /// Current fencing generation.
    pub fencing_generation: u64,
    /// The joiner's own allocated sender id (ADR-0036 §2, §4).
    pub sender_id: crate::media_admission::SenderId,
    /// The meeting's current KEK.
    ///
    /// An `Arc` handle, not a copy: carrying this result through the join path
    /// clones no key bytes. `MeetingKek`'s `Debug` redacts, so this struct's
    /// derived `Debug` cannot print it.
    ///
    /// Not a claim that only one copy exists — `build_join_response` copies the
    /// bytes out with `expose().to_vec()` for the one legitimate egress, and
    /// that copy is not zeroized. See `MeetingKeyState::kek`.
    pub meeting_kek: std::sync::Arc<crate::media_admission::MeetingKek>,
    /// Generation of `meeting_kek`. Always 0 in this story — rotation deferred.
    pub kek_generation: u16,
    /// Handle to the spawned ParticipantActor.
    pub participant_handle: ParticipantActorHandle,
    /// The meeting's FROZEN handler set (ADR-0036 §9). `JoinResponse.media_servers`
    /// carries every handler in it: every participant is offered the whole set
    /// and connects to all it can. The frozen value, never the Redis read this
    /// join happened to make, so a divergent registration cannot widen it.
    pub media_handlers: crate::media_routing::MeetingHandlers,
}

/// Result of a successful reconnection.
///
/// **No handler field yet, and that is deliberate** (reconnect is not wired
/// into the WebTransport path; `handle_reconnect` has no caller). When it is:
/// the proto's reconnect response is `JoinResponse`-shaped, and its
/// `media_servers` MUST carry the meeting's FULL frozen `MeetingHandlers` set
/// (ADR-0036 §9) — never the registration's (Redis) handler list, which could
/// have diverged. Mirror `JoinResult::media_handlers`.
#[derive(Debug, Clone)]
pub struct ReconnectResult {
    /// Confirmed participant ID.
    pub participant_id: String,
    /// New correlation ID (rotated).
    pub new_correlation_id: String,
    /// New binding token (rotated).
    pub new_binding_token: String,
    /// Current participant list.
    pub participants: Vec<ParticipantInfo>,
}

/// Information about a participant.
#[derive(Debug, Clone)]
pub struct ParticipantInfo {
    /// Participant ID.
    pub participant_id: String,
    /// User ID (from JWT).
    pub user_id: String,
    /// Display name.
    pub display_name: String,
    /// Whether audio is self-muted.
    pub audio_self_muted: bool,
    /// Whether video is self-muted.
    pub video_self_muted: bool,
    /// Whether audio is server-muted (enforced, ADR-0036 §5).
    pub audio_server_muted: bool,
    /// Whether video is server-muted (enforced, ADR-0036 §5).
    pub video_server_muted: bool,
    /// Connection status.
    pub status: ParticipantStatus,
    /// This participant's per-meeting sender id (ADR-0036 §2, §4).
    pub sender_id: crate::media_admission::SenderId,
    /// This participant's Ed25519 identity signing public key, as presented at
    /// join and republished on the roster.
    ///
    /// `None` means **no key published** — a defined state the wire represents
    /// as empty `bytes`, and one consumers MUST fail closed on (drop frames from
    /// this participant; never fall back to accepting unsigned ones). A
    /// wrong-length key is still structurally unrepresentable here: MC rejects
    /// any length other than 0 or 32 at the trust boundary, so this is
    /// `Option<[u8; 32]>` and never a variable-length blob.
    ///
    /// **Trust on first use.** No `cnf` thumbprint check binds a present key to
    /// the meeting token, so a signature verifying against it proves only
    /// same-keyholder consistency — never a verified identity (ADR-0036 §4).
    pub identity_public_key: Option<crate::media_admission::IdentityPublicKey>,
}

/// Participant connection status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParticipantStatus {
    /// Connected and active.
    Connected,
    /// Disconnected, within grace period.
    Disconnected,
    /// Reconnecting.
    Reconnecting,
}

/// State update for a participant (broadcast to other connections).
#[derive(Debug, Clone)]
pub enum ParticipantStateUpdate {
    /// A participant joined.
    Joined(ParticipantInfo),
    /// A participant left.
    Left {
        participant_id: String,
        reason: LeaveReason,
    },
    /// A participant's mute status changed.
    MuteChanged {
        participant_id: String,
        audio_self_muted: bool,
        video_self_muted: bool,
        audio_server_muted: bool,
        video_server_muted: bool,
    },
    /// A participant disconnected (still in grace period).
    Disconnected { participant_id: String },
    /// A participant reconnected.
    Reconnected { participant_id: String },
}

/// Reason for participant leaving.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaveReason {
    /// Participant chose to leave (explicit leave OR clean transport close).
    Voluntary,
    /// Disconnect grace period expired.
    Timeout,
    /// Removed by host.
    Removed,
    /// Meeting ended.
    MeetingEnded,
}

impl LeaveReason {
    /// Bounded, low-cardinality metric-label form for
    /// `mc_participant_leaves_total{reason}` (exactly 4 values).
    ///
    /// Exhaustive match with NO wildcard: a future variant is a COMPILE error,
    /// not a silently-mislabeled metric.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Voluntary => "voluntary",
            Self::Timeout => "timeout",
            Self::Removed => "removed",
            Self::MeetingEnded => "meeting_ended",
        }
    }
}

/// Transport-authenticated classification of why a connection dropped.
///
/// Derived by the WebTransport layer from the QUIC/WebTransport session close
/// signal (`wtransport::Connection::closed()` → `ConnectionError`), NEVER from a
/// client-controlled payload. Drives the disconnect handling decision:
///
/// - [`ClientClosed`](Self::ClientClosed): clean peer/application close (browser
///   tab close) → the participant is removed from the roster **immediately**
///   (the ADR-0023 reconnection grace period is skipped — a deliberately closed
///   session is not coming back).
/// - [`ConnectionLost`](Self::ConnectionLost): idle-timeout / abrupt loss →
///   the participant enters the grace period so a genuine transient disconnect
///   can reconnect (ADR-0023).
/// - [`ServerInitiated`](Self::ServerInitiated): we closed it (cancel / drain /
///   explicit-leave already processed) → grace path (a no-op when the
///   participant was already removed by another handler).
///
/// # Fail-safe default
///
/// The `u8` round-trip ([`from_u8`](Self::from_u8) / [`as_u8`](Self::as_u8)) used
/// for the lock-free cause cell decodes any unexpected/uninitialized byte to
/// [`ConnectionLost`](Self::ConnectionLost) — the grace-preserving direction.
/// An unknown byte can therefore NEVER be coerced into the immediate-remove
/// (`ClientClosed`) path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisconnectCause {
    /// Clean client/application close — remove immediately (skip grace).
    ClientClosed,
    /// Idle-timeout / abrupt loss — keep the ADR-0023 grace period.
    ConnectionLost,
    /// Server-initiated close (cancel / drain / already-handled leave).
    ServerInitiated,
}

impl DisconnectCause {
    /// Encode as a `u8` for the lock-free cause cell.
    #[must_use]
    pub fn as_u8(self) -> u8 {
        match self {
            Self::ClientClosed => 1,
            Self::ConnectionLost => 2,
            Self::ServerInitiated => 3,
        }
    }

    /// Decode from a `u8`. Any unexpected/uninitialized byte fails safe to
    /// [`ConnectionLost`](Self::ConnectionLost) (grace-preserving) — an unknown
    /// value is NEVER decoded as the immediate-remove `ClientClosed`.
    #[must_use]
    pub fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::ClientClosed,
            3 => Self::ServerInitiated,
            // 2 (ConnectionLost) and every other byte (incl. the 0 the cell is
            // initialized to) fail safe to the grace-preserving cause.
            _ => Self::ConnectionLost,
        }
    }

    /// Bounded, low-cardinality metric-label form for
    /// `mc_participant_disconnects_total{cause}` (exactly 3 values).
    ///
    /// Exhaustive match with NO wildcard: a future variant is a COMPILE error,
    /// not a silently-mislabeled metric.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::ClientClosed => "client_closed",
            Self::ConnectionLost => "connection_lost",
            Self::ServerInitiated => "server_initiated",
        }
    }
}

/// Current state of a meeting (for debugging/health).
#[derive(Debug, Clone)]
pub struct MeetingState {
    /// Meeting ID.
    pub meeting_id: String,
    /// Current participants.
    pub participants: Vec<ParticipantInfo>,
    /// Current fencing generation.
    pub fencing_generation: u64,
    /// Meeting creation timestamp.
    pub created_at: i64,
    /// Current mailbox depth.
    pub mailbox_depth: usize,
    /// Whether the meeting is shutting down.
    pub is_shutting_down: bool,
}

/// Signaling message payload (wraps various message types).
#[derive(Debug, Clone)]
pub enum SignalingPayload {
    /// Mute update from client.
    MuteUpdate {
        audio_muted: bool,
        video_muted: bool,
    },
    /// Layout subscription request.
    LayoutSubscribe { layout_type: String },
    /// Chat message.
    Chat { content: String },
    /// Generic signaling data (protobuf bytes).
    Raw { message_type: u32, data: Vec<u8> },
}

/// `SignalingPayload::Raw::message_type` for an encoded `ServerMessage`, the
/// only kind MC sends raw today. Named once so the discriminant is not a bare
/// `0` at every send site.
pub const RAW_SERVER_MESSAGE_TYPE: u32 = 0;

impl SignalingPayload {
    /// The envelope for one `ServerMessage`: encode it under
    /// [`RAW_SERVER_MESSAGE_TYPE`]. Policy-free by design: each caller keeps
    /// its own trace-context injection point and its own delivery-failure
    /// accounting (they legitimately differ).
    pub fn server_message(message: &proto_gen::dark_tower::signaling::v1::ServerMessage) -> Self {
        use prost::Message as _;
        Self::Raw {
            message_type: RAW_SERVER_MESSAGE_TYPE,
            data: message.encode_to_vec(),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_participant_status_equality() {
        assert_eq!(ParticipantStatus::Connected, ParticipantStatus::Connected);
        assert_ne!(
            ParticipantStatus::Connected,
            ParticipantStatus::Disconnected
        );
    }

    #[test]
    fn test_leave_reason_equality() {
        assert_eq!(LeaveReason::Voluntary, LeaveReason::Voluntary);
        assert_ne!(LeaveReason::Voluntary, LeaveReason::Timeout);
    }

    #[test]
    fn test_leave_reason_labels_bounded() {
        assert_eq!(LeaveReason::Voluntary.label(), "voluntary");
        assert_eq!(LeaveReason::Timeout.label(), "timeout");
        assert_eq!(LeaveReason::Removed.label(), "removed");
        assert_eq!(LeaveReason::MeetingEnded.label(), "meeting_ended");
    }

    #[test]
    fn test_disconnect_cause_labels_bounded() {
        assert_eq!(DisconnectCause::ClientClosed.label(), "client_closed");
        assert_eq!(DisconnectCause::ConnectionLost.label(), "connection_lost");
        assert_eq!(DisconnectCause::ServerInitiated.label(), "server_initiated");
    }

    #[test]
    fn test_disconnect_cause_u8_roundtrip() {
        for cause in [
            DisconnectCause::ClientClosed,
            DisconnectCause::ConnectionLost,
            DisconnectCause::ServerInitiated,
        ] {
            assert_eq!(DisconnectCause::from_u8(cause.as_u8()), cause);
        }
    }

    #[test]
    fn test_disconnect_cause_from_u8_fails_safe_to_connection_lost() {
        // The 0 the atomic cell is initialized to, and every other unexpected
        // byte, MUST decode to the grace-preserving cause — never ClientClosed
        // (immediate remove). This is the fail-safe / anti-spoof-eject guard.
        assert_eq!(DisconnectCause::from_u8(0), DisconnectCause::ConnectionLost);
        for byte in [4u8, 5, 42, 255] {
            assert_eq!(
                DisconnectCause::from_u8(byte),
                DisconnectCause::ConnectionLost,
                "byte {byte} must fail safe to ConnectionLost"
            );
        }
        // Explicitly: no byte decodes to ClientClosed except the exact encoding.
        assert_ne!(DisconnectCause::from_u8(0), DisconnectCause::ClientClosed);
    }

    #[test]
    fn test_controller_status_default_values() {
        let status = ControllerStatus {
            meeting_count: 0,
            connection_count: 0,
            is_draining: false,
            mailbox_depth: 0,
        };
        assert_eq!(status.meeting_count, 0);
        assert!(!status.is_draining);
    }

    #[test]
    fn test_participant_info_clone() {
        let info = ParticipantInfo {
            participant_id: "p1".to_string(),
            user_id: "u1".to_string(),
            display_name: "Test User".to_string(),
            audio_self_muted: false,
            video_self_muted: true,
            audio_server_muted: false,
            video_server_muted: false,
            status: ParticipantStatus::Connected,
            sender_id: crate::media_admission::fixtures::sample_sender_id(),
            identity_public_key: crate::media_admission::fixtures::sample_identity_key(),
        };
        let cloned = info.clone();
        assert_eq!(info.participant_id, cloned.participant_id);
        assert_eq!(info.display_name, cloned.display_name);
    }

    #[test]
    fn test_signaling_payload_variants() {
        let mute = SignalingPayload::MuteUpdate {
            audio_muted: true,
            video_muted: false,
        };
        assert!(matches!(mute, SignalingPayload::MuteUpdate { .. }));

        let layout = SignalingPayload::LayoutSubscribe {
            layout_type: "grid".to_string(),
        };
        assert!(matches!(layout, SignalingPayload::LayoutSubscribe { .. }));

        let chat = SignalingPayload::Chat {
            content: "Hello".to_string(),
        };
        assert!(matches!(chat, SignalingPayload::Chat { .. }));
    }
}
