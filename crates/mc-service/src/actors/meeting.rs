//! `MeetingActor` - per-meeting actor that owns meeting state (ADR-0023).
//!
//! Each `MeetingActor`:
//! - Owns all state for one meeting (participants, subscriptions, mute status)
//! - Supervises N `ParticipantActor` instances
//! - Handles session binding tokens for reconnection
//! - Coordinates with Redis for persistent state
//!
//! # Participant Disconnect Handling (ADR-0023 Section 1a)
//!
//! When a connection drops:
//! 1. Participant marked as "disconnected" (still visible to others)
//! 2. 30-second grace period for reconnection
//! 3. If not reconnected: participant removed, slots released

use crate::errors::McError;
use crate::media_admission::rotation::{
    collect_push_outcomes, Coalesced, PendingOutcome, RotationReport,
};
use crate::media_admission::{
    AdmissionEpoch, AdmitFailed, HandedOutBindings, IdentityPublicKey, KekLifecycle,
    KekPushOutcome, KekRotationDebounce, RotationTrigger, SenderId,
};

use super::client_mute_fanout::ClientMuteFanout;
use super::meeting_media::{Affected, JoinMedia, MeetingMedia, MutedSourceCensus, RosterEntry};
use super::messages::{
    DisconnectCause, JoinResult, KekPush, LeaveReason, MeetingEndCause, MeetingLifecycleEvent,
    MeetingLifecycleSink, MeetingMessage, MeetingState, ParticipantInfo, ParticipantStateUpdate,
    ParticipantStatus, ReconnectResult, SenderLookup, ServerMuteRefusal, SignalingPayload,
    UnmuteRelay,
};
use super::metrics::{ActorMetrics, ActorType, ControllerMetrics, MailboxMonitor};
use super::participant::{ParticipantActor, ParticipantActorHandle};
use super::session::{SessionBindingManager, StoredBinding};
use crate::media_kind::{
    ClientKindState, ClientMediaStates, KindDecision, MediaStates, MuteChange, NotPublished,
    PublishSet, PublishedMuteChange, RequestedKinds, ServerMuteAttribution,
};
use crate::media_routing::SourceMute;
use crate::media_routing::{ConnectionKey, Unapplied};
use crate::media_signaling::{MediaKindLabel, SlotDemand, SourceMuteView};

use common::secret::SecretBox;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, instrument, warn};

/// Default channel buffer size for the meeting mailbox.
const MEETING_CHANNEL_BUFFER: usize = 500;

/// Grace period for participant reconnection (ADR-0023: 30 seconds).
const DISCONNECT_GRACE_PERIOD: Duration = Duration::from_secs(30);

/// Handle to a `MeetingActor`.
#[derive(Clone, Debug)]
pub struct MeetingActorHandle {
    sender: mpsc::Sender<MeetingMessage>,
    cancel_token: CancellationToken,
    meeting_id: String,
}

impl MeetingActorHandle {
    /// Get the meeting ID.
    #[must_use]
    pub fn meeting_id(&self) -> &str {
        &self.meeting_id
    }

    /// Request a new connection to join this meeting.
    ///
    /// # Arguments
    ///
    /// * `connection_id` - Unique connection identifier
    /// * `user_id` - User ID from JWT
    /// * `participant_id` - Participant ID for this meeting
    /// * `display_name` - Registered display name from the validated meeting-token
    ///   claim (already length-bounded at the connection boundary); empty means the
    ///   claim carried no name and a generic label is applied at the join sink
    /// * `is_host` - Whether this participant has host privileges
    /// * `identity_public_key` - The joiner's Ed25519 identity signing public
    ///   key, parsed at the WebTransport trust boundary; `None` means no key
    ///   published
    /// * `media` - Media-routing inputs; the first join freezes the meeting's
    ///   handler set
    #[expect(
        clippy::too_many_arguments,
        reason = "actor join signature threads the full join tuple (ids + display_name + host flag + identity key + stream); bundling into a JoinConnectionParams struct is a larger cross-message refactor tracked in docs/TODO.md"
    )]
    pub async fn connection_join(
        &self,
        connection_id: String,
        user_id: String,
        participant_id: String,
        display_name: String,
        is_host: bool,
        identity_public_key: Option<IdentityPublicKey>,
        stream_tx: Option<tokio::sync::mpsc::Sender<bytes::Bytes>>,
        media: JoinMedia,
    ) -> Result<JoinResult, McError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(MeetingMessage::ConnectionJoin {
                connection_id,
                user_id,
                participant_id,
                display_name,
                is_host,
                identity_public_key,
                stream_tx,
                media,
                respond_to: tx,
            })
            .await
            .map_err(|e| McError::Internal(format!("channel send failed: {e}")))?;

        rx.await
            .map_err(|e| McError::Internal(format!("response receive failed: {e}")))?
    }

    /// Notify of a connection disconnect.
    ///
    /// `cause` is the transport-authenticated close classification (see
    /// [`DisconnectCause`]). A [`DisconnectCause::ClientClosed`] removes the
    /// participant from the roster immediately; the abrupt/ambiguous causes keep
    /// the ADR-0023 grace period for reconnection.
    pub async fn connection_disconnected(
        &self,
        connection_id: String,
        participant_id: String,
        cause: DisconnectCause,
    ) -> Result<(), McError> {
        self.sender
            .send(MeetingMessage::ConnectionDisconnected {
                connection_id,
                participant_id,
                cause,
            })
            .await
            .map_err(|e| McError::Internal(format!("channel send failed: {e}")))
    }

    /// Attempt to reconnect with binding token, presenting the client's
    /// publish declaration (its kind set compared as a set, a mismatch refused,
    /// story 3 R-2; its client mute adopted) and optionally a receive
    /// declaration (treated as a redeclaration).
    pub async fn connection_reconnect(
        &self,
        connection_id: String,
        correlation_id: String,
        binding_token: String,
        declared: ClientMediaStates,
        demand: Option<SlotDemand>,
    ) -> Result<ReconnectResult, McError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(MeetingMessage::ConnectionReconnect {
                connection_id,
                correlation_id,
                binding_token,
                declared,
                demand,
                respond_to: tx,
            })
            .await
            .map_err(|e| McError::Internal(format!("channel send failed: {e}")))?;

        rx.await
            .map_err(|e| McError::Internal(format!("response receive failed: {e}")))?
    }

    /// Participant leaves the meeting (explicit leave).
    pub async fn participant_leave(&self, participant_id: String) -> Result<(), McError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(MeetingMessage::ParticipantLeave {
                participant_id,
                respond_to: tx,
            })
            .await
            .map_err(|e| McError::Internal(format!("channel send failed: {e}")))?;

        rx.await
            .map_err(|e| McError::Internal(format!("response receive failed: {e}")))?
    }

    /// Forward a signaling message.
    pub async fn signaling_message(
        &self,
        participant_id: String,
        message: SignalingPayload,
    ) -> Result<(), McError> {
        self.sender
            .send(MeetingMessage::SignalingMessage {
                participant_id,
                message,
            })
            .await
            .map_err(|e| McError::Internal(format!("channel send failed: {e}")))
    }

    /// Fire the leave-debounced KEK rotation now, as if W had elapsed.
    /// **Test builds only** — see `MeetingMessage::ForceKekRotation`.
    ///
    /// Returns `true` iff a rotation was pending and was performed. A second
    /// call right after returns `false`, which is how a test proves every
    /// recorded departure folded into ONE rotation without waiting on a timer.
    ///
    /// # Errors
    ///
    /// `McError::Internal` if the actor's mailbox is closed.
    #[cfg(feature = "test-seams")]
    pub async fn force_kek_rotation(&self) -> Result<bool, McError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(MeetingMessage::ForceKekRotation { respond_to: tx })
            .await
            .map_err(|e| McError::Internal(format!("channel send failed: {e}")))?;
        rx.await
            .map_err(|e| McError::Internal(format!("response receive failed: {e}")))
    }

    /// Get current meeting state.
    pub async fn get_state(&self) -> Result<MeetingState, McError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(MeetingMessage::GetState { respond_to: tx })
            .await
            .map_err(|e| McError::Internal(format!("channel send failed: {e}")))?;

        rx.await
            .map_err(|e| McError::Internal(format!("response receive failed: {e}")))
    }

    /// An MH reports a participant's media connection: resolve its `sender_id`
    /// and record the connectivity in one actor turn.
    ///
    /// Returns a [`SenderLookup`]: `Found(sender_id)` for the single roster
    /// match; `NotFound` when no participant on this meeting's roster carries
    /// the `user_id`; `Ambiguous` when more than one does. The caller turns
    /// **both** `NotFound` and `Ambiguous` into the wire value `0` so MH
    /// rejects — **never a licence to invent, default to, or reuse an id**.
    /// Alongside it, the reason connectivity was NOT recorded, if it was not:
    /// connectivity is recorded only for a `Found` participant on a handler of
    /// the meeting's frozen set (see [`MeetingMessage::MediaConnected`]).
    ///
    /// # Errors
    ///
    /// [`McError::Internal`] if the meeting actor is gone. A transport failure
    /// here is **not** a `NotFound`: "the meeting actor died" and "this
    /// participant is not on the roster" have different remedies.
    pub async fn media_connected(
        &self,
        user_id: String,
        handler_id: String,
        connection_id: String,
    ) -> Result<(SenderLookup, Option<Unapplied>), McError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(MeetingMessage::MediaConnected {
                user_id,
                handler_id,
                connection_id,
                respond_to: tx,
            })
            .await
            .map_err(|e| McError::Internal(format!("channel send failed: {e}")))?;
        rx.await
            .map_err(|e| McError::Internal(format!("response receive failed: {e}")))
    }

    /// An MH reports a participant's media connection closed.
    ///
    /// # Errors
    ///
    /// [`McError::Internal`] if the meeting actor is gone.
    pub async fn media_disconnected(
        &self,
        user_id: String,
        handler_id: String,
        connection_id: String,
    ) -> Result<Option<Unapplied>, McError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(MeetingMessage::MediaDisconnected {
                user_id,
                handler_id,
                connection_id,
                respond_to: tx,
            })
            .await
            .map_err(|e| McError::Internal(format!("channel send failed: {e}")))?;
        rx.await
            .map_err(|e| McError::Internal(format!("response receive failed: {e}")))
    }

    /// Register a validated receive-capability declaration (ADR-0036 §6).
    ///
    /// Resolves once the actor has registered the slot demand, re-rendered,
    /// re-published to every handler and run a first bounded flush.
    ///
    /// # Errors
    ///
    /// [`McError::Internal`] if the actor is gone; [`McError::ParticipantNotFound`]
    /// if the participant is no longer on the roster.
    pub async fn register_receive_capability(
        &self,
        participant_id: String,
        declaration: SlotDemand,
    ) -> Result<(), McError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(MeetingMessage::RegisterReceiveCapability {
                participant_id,
                declaration,
                respond_to: tx,
            })
            .await
            .map_err(|e| McError::Internal(format!("channel send failed: {e}")))?;

        rx.await
            .map_err(|e| McError::Internal(format!("response receive failed: {e}")))?
    }

    /// Record a participant's own client mute, per kind (story 3 R-17): only
    /// the kinds `change` names move. `participant_id` MUST be the
    /// authenticated connection's own id.
    ///
    /// Returns the per-kind decision, or `None` if the participant is no
    /// longer on the roster.
    ///
    /// # Errors
    ///
    /// [`McError::Internal`] if the actor could not be reached.
    pub async fn update_self_mute(
        &self,
        participant_id: String,
        change: PublishedMuteChange,
    ) -> Result<Option<Result<KindDecision<ClientKindState>, NotPublished>>, McError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(MeetingMessage::UpdateSelfMute {
                participant_id,
                change,
                respond_to: tx,
            })
            .await
            .map_err(|e| McError::Internal(format!("channel send failed: {e}")))?;
        rx.await
            .map_err(|e| McError::Internal(format!("response receive failed: {e}")))
    }

    /// Apply or lift a server mute by meeting policy (enforced at MH ingress,
    /// ADR-0036 §5, §7). "Host mute" is avoided as a term: it presumes a role
    /// model this system has not defined.
    ///
    /// `requester` MUST be the participant id of the authenticated connection
    /// making the request — see `MeetingMessage::ServerMute`.
    ///
    /// # Errors
    ///
    /// The outer [`McError`] means the actor could not be reached; the inner
    /// result is the actor's decision.
    pub async fn server_mute(
        &self,
        target_participant_id: String,
        requester: String,
        change: MuteChange,
    ) -> Result<Result<KindDecision, ServerMuteRefusal>, McError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(MeetingMessage::ServerMute {
                target_participant_id,
                requester,
                change,
                respond_to: tx,
            })
            .await
            .map_err(|e| McError::Internal(format!("channel send failed: {e}")))?;

        rx.await
            .map_err(|e| McError::Internal(format!("response receive failed: {e}")))
    }

    /// A server-muted participant asks the host(s) to lift its mute. NOTIFIES;
    /// never clears. `participant_id` MUST be the authenticated connection's.
    ///
    /// # Errors
    ///
    /// [`McError::Internal`] if the actor could not be reached.
    pub async fn request_unmute(
        &self,
        participant_id: String,
        kinds: RequestedKinds,
    ) -> Result<UnmuteRelay, McError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(MeetingMessage::RequestUnmute {
                participant_id,
                kinds,
                respond_to: tx,
            })
            .await
            .map_err(|e| McError::Internal(format!("channel send failed: {e}")))?;
        rx.await
            .map_err(|e| McError::Internal(format!("response receive failed: {e}")))
    }

    /// Close the MC-side meeting. Runs the MH-side release (`EndMeeting` on
    /// every handler of the frozen set, after its pushes drain) as the actor
    /// exits. NOT the MH `EndMeeting` RPC itself.
    pub async fn close_meeting(&self, reason: String) -> Result<(), McError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(MeetingMessage::CloseMeeting {
                reason,
                respond_to: tx,
            })
            .await
            .map_err(|e| McError::Internal(format!("channel send failed: {e}")))?;

        rx.await
            .map_err(|e| McError::Internal(format!("response receive failed: {e}")))?
    }

    /// Cancel the meeting actor.
    pub fn cancel(&self) {
        self.cancel_token.cancel();
    }

    /// Check if the actor is cancelled.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancel_token.is_cancelled()
    }

    /// Get a child token for connection actors.
    #[must_use]
    pub fn child_token(&self) -> CancellationToken {
        self.cancel_token.child_token()
    }
}

/// Participant state within a meeting.
#[derive(Debug)]
struct Participant {
    /// Participant ID.
    participant_id: String,
    /// User ID (from JWT).
    user_id: String,
    /// Display name.
    display_name: String,
    /// Correlation ID for reconnection.
    correlation_id: String,
    /// Current participant actor handle (if connected).
    connection: Option<ParticipantActorHandle>,
    /// Connection status.
    status: ParticipantStatus,
    /// Timestamp when disconnected (for grace period).
    disconnected_at: Option<Instant>,
    /// The full per-kind mute state, both halves (ADR-0036 §5 amendment
    /// 2026-10-25):
    /// - `media.client`: the publish set and client mute in one value per
    ///   kind, definite from admission (the join's `published_kinds`). Its
    ///   publish set is FIXED for the membership (story 3 R-2); its client
    ///   mute is written ONLY through [`ClientMediaStates::apply`] (a
    ///   `MuteRequest`, or a reconnect's declaration). Media policy reads it
    ///   only through [`ClientMediaStates::effective`].
    /// - `media.server`: server mute and who IMPOSED it, one store
    ///   ([`ServerMuteAttribution`]). Written ONLY by `handle_server_mute`.
    media: MediaStates,
    /// Whether this participant has host privileges.
    is_host: bool,
    /// Per-meeting sender id, allocated once at admission (ADR-0036 §2, §4).
    ///
    /// Held on the participant rather than recomputed, which is what makes
    /// reconnect continuity fall out for free: a reconnecting participant is
    /// still in the roster and still holds this, so the allocator is never
    /// consulted and no id is recycled.
    sender_id: SenderId,
    /// Ed25519 identity signing public key, as presented at join. `None` means
    /// **no key published** — a defined state, not a failure.
    ///
    /// Trust on first use — no `cnf` binding. Same-keyholder consistency only,
    /// never a verified identity (ADR-0036 §4).
    identity_public_key: Option<IdentityPublicKey>,
}

impl Participant {
    /// This participant's per-kind mute flags, explicitly projected (never the
    /// roster entry itself, which also carries identity material).
    fn source_mute(&self) -> SourceMute {
        // The ONE place each half is built from its own roster fields: typed,
        // so the two cannot be swapped into the directive's server input.
        SourceMute::new(
            self.media.server.muted_kinds(),
            self.media.client.effective(),
        )
    }

    /// The fixed publish set (derived from the client state).
    fn publishes(&self) -> PublishSet {
        self.media.client.publish_set()
    }

    fn to_info(&self) -> ParticipantInfo {
        ParticipantInfo {
            participant_id: self.participant_id.clone(),
            user_id: self.user_id.clone(),
            display_name: self.display_name.clone(),
            media: self.media.clone(),
            status: self.status,
            sender_id: self.sender_id,
            identity_public_key: self.identity_public_key,
        }
    }
}

/// Managed connection state.
struct ManagedConnection {
    /// Handle to the participant actor.
    #[allow(dead_code)] // Used for signaling
    handle: ParticipantActorHandle,
    /// Join handle for monitoring.
    task_handle: JoinHandle<()>,
    /// Associated participant ID.
    participant_id: String,
}

/// The `MeetingActor` implementation.
pub struct MeetingActor {
    /// Meeting ID.
    meeting_id: String,
    /// Message receiver.
    receiver: mpsc::Receiver<MeetingMessage>,
    /// Cancellation token (child of controller's token).
    cancel_token: CancellationToken,
    /// Participants by ID.
    participants: HashMap<String, Participant>,
    /// Connections by ID.
    connections: HashMap<String, ManagedConnection>,
    /// Correlation ID to participant ID mapping.
    correlation_to_participant: HashMap<String, String>,
    /// Session binding manager for token generation/validation (ADR-0023).
    binding_manager: SessionBindingManager,
    /// Stored bindings by correlation ID.
    stored_bindings: HashMap<String, StoredBinding>,
    /// Current fencing generation.
    fencing_generation: u64,
    /// Meeting creation timestamp.
    created_at: i64,
    /// Whether the meeting is shutting down.
    is_shutting_down: bool,
    /// Shared actor metrics.
    metrics: Arc<ActorMetrics>,
    /// Controller metrics for GC heartbeat reporting (participant count).
    controller_metrics: Arc<ControllerMetrics>,
    /// Mailbox monitor.
    mailbox: MailboxMonitor,
    /// Handle to self, for passing to child ParticipantActors.
    self_handle: MeetingActorHandle,
    /// The meeting KEK, its generation, and the `sender_id` namespace — held as
    /// ONE value, because the namespace may be reclaimed only together with a
    /// new KEK (ADR-0036 §4; story 2 R-16). There is deliberately no separate
    /// allocator field here to reseat.
    ///
    /// Generated at meeting-actor creation, held only here, never persisted and
    /// never logged. Dies with the actor.
    admission: AdmissionEpoch,
    /// Every `sender_id` MC has answered a media handler's binding with and not
    /// yet seen released. Part of the epoch-reset exclusion set, because MH
    /// holds a binding until the CONNECTION closes, which can outlive the
    /// participant's roster entry (Gate-3 F-1).
    mh_bindings: HandedOutBindings,
    /// The leave debounce: at most one leave-triggered rotation per W, measured
    /// from the OLDEST un-rotated removal.
    kek_debounce: KekRotationDebounce,
    /// W, the overdue threshold, and the fleet-gauge registry this meeting
    /// reports into. Evicted when the actor exits.
    kek_lifecycle: Arc<KekLifecycle>,
    /// One long-lived CSPRNG handle for every rotation (ring recommends reuse).
    rng: ring::rand::SystemRandom,
    /// Test seam: never auto-fire the rotation; `ForceKekRotation` drives it.
    #[cfg(feature = "test-seams")]
    kek_debounce_manual: bool,
    /// The per-meeting bound on the client-mute roster fan-out: pending
    /// subjects (FIFO) and the bucket they wait on. Actor state, so it dies
    /// with the loop -- see [`Self::flush_client_mute`].
    client_mute_fanout: ClientMuteFanout,
    /// Join-order slots and shared-handler edges, observed connectivity, push workers and per-participant
    /// views (ADR-0036 §5/§6/§8/§9). Its push workers are HANDED OFF to the
    /// teardown on a clean exit and aborted with it on an unclean one.
    media: MeetingMedia,
    /// Set when the meeting has ended and the actor should leave its loop
    /// through the clean exit path (which releases the handlers).
    end_cause: Option<MeetingEndCause>,
    /// Where this actor reports its lifecycle; `None` for an actor spawned
    /// outside a controller (tests), which still tears down, unobserved.
    wiring: Option<MeetingWiring>,
}

/// What a controller-managed meeting actor is wired to.
#[derive(Debug, Clone)]
pub struct MeetingWiring {
    /// The controller's lifecycle channel (`Ended`, `TeardownComplete`).
    pub lifecycle: MeetingLifecycleSink,
    /// The pod's server-mute census.
    pub muted_census: Arc<MutedSourceCensus>,
    /// The process's in-flight teardown count, which a graceful shutdown
    /// waits on so a handed-off release is not dropped with the runtime.
    pub teardowns: Arc<crate::media_routing::teardown::TeardownTracker>,
}

/// Reports `TeardownComplete` when dropped — so it is sent on EVERY exit of the
/// teardown task, a panic included. The controller's fence depends on this
/// message; a completion sent only on the happy path would make one panicked
/// teardown a permanently unjoinable meeting id.
struct TeardownDone {
    lifecycle: Option<MeetingLifecycleSink>,
    meeting_id: String,
    cause: MeetingEndCause,
}

impl Drop for TeardownDone {
    fn drop(&mut self) {
        if let Some(lifecycle) = &self.lifecycle {
            // The controller being gone (MC exiting) is the only way this
            // fails, and then there is no fence to lift.
            let _ = lifecycle.send(MeetingLifecycleEvent::TeardownComplete {
                meeting_id: std::mem::take(&mut self.meeting_id),
                cause: self.cause,
            });
        }
    }
}

impl MeetingActor {
    /// Spawn a new meeting actor.
    ///
    /// Returns a handle and the task join handle.
    ///
    /// # Arguments
    ///
    /// * `meeting_id` - Unique meeting identifier
    /// * `cancel_token` - Cancellation token (child of controller's token)
    /// * `metrics` - Shared actor metrics
    /// * `controller_metrics` - Controller metrics for GC heartbeat reporting (participant count)
    /// * `master_secret` - Master secret for HKDF key derivation (ADR-0023). Wrapped in
    ///   SecretBox to ensure secure memory handling (zeroization on drop, redacted Debug).
    /// * `kek_lifecycle` - W, the overdue threshold and the fleet-gauge registry
    ///   (ADR-0036 §4 Rotation). This meeting reports its rotation state into it.
    /// # Errors
    ///
    /// [`McError::Internal`] if the system CSPRNG cannot produce the meeting
    /// KEK. **Fail closed**: the meeting is not created. There is deliberately
    /// no fallback RNG and no default key — a predictable KEK would silently
    /// defeat every confidentiality property ADR-0036 §4 claims.
    pub fn spawn(
        meeting_id: String,
        cancel_token: CancellationToken,
        metrics: Arc<ActorMetrics>,
        controller_metrics: Arc<ControllerMetrics>,
        master_secret: SecretBox<Vec<u8>>,
        kek_lifecycle: Arc<KekLifecycle>,
    ) -> Result<(MeetingActorHandle, JoinHandle<()>), McError> {
        Self::spawn_inner(
            meeting_id,
            cancel_token,
            metrics,
            controller_metrics,
            master_secret,
            kek_lifecycle,
            AdmissionEpoch::generate,
            |_| {},
        )
    }

    /// Spawn a CONTROLLER-MANAGED meeting actor: as [`Self::spawn`], plus the
    /// lifecycle channel and the server-mute census.
    ///
    /// # Errors
    ///
    /// As [`Self::spawn`].
    pub(crate) fn spawn_managed(
        meeting_id: String,
        cancel_token: CancellationToken,
        metrics: Arc<ActorMetrics>,
        controller_metrics: Arc<ControllerMetrics>,
        master_secret: SecretBox<Vec<u8>>,
        kek_lifecycle: Arc<KekLifecycle>,
        wiring: MeetingWiring,
    ) -> Result<(MeetingActorHandle, JoinHandle<()>), McError> {
        Self::spawn_inner(
            meeting_id,
            cancel_token,
            metrics,
            controller_metrics,
            master_secret,
            kek_lifecycle,
            AdmissionEpoch::generate,
            move |actor| actor.wiring = Some(wiring),
        )
    }

    /// Shared construction for [`Self::spawn`] and the `test-seams` variant.
    ///
    /// The admission constructor and the actor hook are parameters so the test
    /// seams thread their overrides without duplicating this body — a forked
    /// copy would drift from the real construction path and the seam tests
    /// would stop testing production behaviour. Production passes
    /// `AdmissionEpoch::generate` and a no-op hook.
    #[expect(
        clippy::too_many_arguments,
        reason = "shared body for spawn and spawn_with_seams; the two hooks are what keep the seam path identical to production"
    )]
    fn spawn_inner(
        meeting_id: String,
        cancel_token: CancellationToken,
        metrics: Arc<ActorMetrics>,
        controller_metrics: Arc<ControllerMetrics>,
        master_secret: SecretBox<Vec<u8>>,
        kek_lifecycle: Arc<KekLifecycle>,
        build_admission: impl FnOnce(
            &ring::rand::SystemRandom,
        ) -> Result<
            AdmissionEpoch,
            crate::media_admission::KekGenerationFailed,
        >,
        configure: impl FnOnce(&mut Self),
    ) -> Result<(MeetingActorHandle, JoinHandle<()>), McError> {
        let (sender, receiver) = mpsc::channel(MEETING_CHANNEL_BUFFER);

        // Build the handle first so we can give the actor a clone of it
        let handle = MeetingActorHandle {
            sender,
            cancel_token: cancel_token.clone(),
            meeting_id: meeting_id.clone(),
        };

        // ADR-0036 §4: the meeting KEK is generated at meeting-actor creation,
        // random, held only in memory, never persisted and never logged.
        let rng = ring::rand::SystemRandom::new();
        let admission = build_admission(&rng).map_err(|_| {
            // No key material in the error, and none in any log this
            // produces — the failure is that there IS no key.
            McError::Internal("Failed to initialise meeting key material".to_string())
        })?;

        crate::observability::metrics::record_meeting_kek_generated(
            RotationTrigger::MeetingCreated,
        );

        // `key_custody=operator` (ADR-0036 §4, §11): media is encrypted between
        // clients; MH, transport and storage cannot read it; MC can. Never
        // described as end-to-end or zero-trust, and never a boolean.
        info!(
            target: "mc.actor.meeting",
            meeting_id = %meeting_id,
            key_custody = crate::observability::metrics::KEY_CUSTODY_OPERATOR,
            "Meeting key material provisioned"
        );

        let media = MeetingMedia::new(meeting_id.clone());
        let kek_debounce = KekRotationDebounce::new(kek_lifecycle.window());

        let mut actor = Self {
            meeting_id: meeting_id.clone(),
            receiver,
            cancel_token,
            participants: HashMap::new(),
            connections: HashMap::new(),
            correlation_to_participant: HashMap::new(),
            binding_manager: SessionBindingManager::new(master_secret),
            stored_bindings: HashMap::new(),
            fencing_generation: 1,
            created_at: chrono::Utc::now().timestamp(),
            is_shutting_down: false,
            metrics,
            controller_metrics,
            mailbox: MailboxMonitor::new(ActorType::Meeting, &meeting_id),
            self_handle: handle.clone(),
            admission,
            mh_bindings: HandedOutBindings::default(),
            kek_debounce,
            kek_lifecycle,
            rng,
            #[cfg(feature = "test-seams")]
            kek_debounce_manual: false,
            client_mute_fanout: ClientMuteFanout::new(Instant::now()),
            media,
            end_cause: None,
            wiring: None,
        };
        configure(&mut actor);

        let task_handle = tokio::spawn(actor.run());

        Ok((handle, task_handle))
    }

    /// Spawn a meeting actor with test-only construction overrides.
    /// **Test builds only.**
    ///
    /// Exists so seam-dependent behaviour runs through the real join path:
    /// sender-id exhaustion and the KEK-epoch reset without performing 65535
    /// joins, an observable flush deferral (a small flush bound), and a
    /// manually-fired rotation so coalescing holds structurally rather than by
    /// racing a real window. There is no handler placement seam: partial
    /// connectivity is produced by reporting (or not) real handler connections.
    /// Compiled only under the non-default `test-seams` feature, and a release
    /// build with that feature on fails to compile (see `lib.rs`).
    ///
    /// This IS the bypass for the namespace guard. It must never be reachable
    /// in production.
    ///
    /// # Errors
    ///
    /// As [`MeetingActor::spawn`].
    #[cfg(feature = "test-seams")]
    pub fn spawn_with_seams(
        meeting_id: String,
        cancel_token: CancellationToken,
        metrics: Arc<ActorMetrics>,
        controller_metrics: Arc<ControllerMetrics>,
        master_secret: SecretBox<Vec<u8>>,
        kek_lifecycle: Arc<KekLifecycle>,
        seams: &super::meeting_media::MeetingSeams,
    ) -> Result<(MeetingActorHandle, JoinHandle<()>), McError> {
        Self::spawn_with_seams_wired(
            meeting_id,
            cancel_token,
            metrics,
            controller_metrics,
            master_secret,
            kek_lifecycle,
            seams,
            None,
        )
    }

    /// [`Self::spawn_with_seams`] for a controller-managed actor. **Test builds
    /// only.**
    #[cfg(feature = "test-seams")]
    #[expect(
        clippy::too_many_arguments,
        reason = "spawn_with_seams' tuple plus the controller wiring; one body keeps the seam path identical to production"
    )]
    pub(crate) fn spawn_with_seams_wired(
        meeting_id: String,
        cancel_token: CancellationToken,
        metrics: Arc<ActorMetrics>,
        controller_metrics: Arc<ControllerMetrics>,
        master_secret: SecretBox<Vec<u8>>,
        kek_lifecycle: Arc<KekLifecycle>,
        seams: &super::meeting_media::MeetingSeams,
        wiring: Option<MeetingWiring>,
    ) -> Result<(MeetingActorHandle, JoinHandle<()>), McError> {
        let sender_ids = match seams.sender_id_cursor {
            Some(cursor) => crate::media_admission::SenderIdAllocator::resuming_from(
                cursor,
                std::collections::HashSet::new(),
            ),
            None => crate::media_admission::SenderIdAllocator::new(),
        };
        let reset_cursor = seams.epoch_reset_cursor;
        let manual = seams.kek_debounce_manual;
        Self::spawn_inner(
            meeting_id,
            cancel_token,
            metrics,
            controller_metrics,
            master_secret,
            kek_lifecycle,
            |rng| AdmissionEpoch::with_seams(rng, sender_ids, reset_cursor),
            |actor| {
                actor.media.apply_seams(seams);
                actor.kek_debounce_manual = manual;
                actor.wiring = wiring;
            },
        )
    }

    /// Run the actor message loop.
    #[instrument(skip_all, name = "mc.actor.meeting", fields(meeting_id = %self.meeting_id))]
    async fn run(mut self) {
        info!(
            target: "mc.actor.meeting",
            meeting_id = %self.meeting_id,
            "MeetingActor started"
        );

        // Create interval for checking disconnect grace periods
        let mut grace_check = tokio::time::interval(Duration::from_secs(5));

        loop {
            // The meeting ended during the previous turn (emptied, or closed):
            // leave through the clean exit path below, which releases the
            // handlers. Checked BEFORE any other work, so nothing is admitted
            // after the ending turn — a join already queued sees `Draining`.
            if self.end_cause.is_some() {
                break;
            }

            // Check for terminated connection actors
            self.check_connection_health().await;
            let settle_wake = self.media.next_settle_wake();
            let kek_wake = self.kek_wake();
            let fanout_wake = self.client_mute_fanout.next_wake(Instant::now());

            tokio::select! {
                // Handle cancellation
                () = self.cancel_token.cancelled() => {
                    info!(
                        target: "mc.actor.meeting",
                        meeting_id = %self.meeting_id,
                        "MeetingActor received cancellation signal"
                    );
                    self.end_cause.get_or_insert(MeetingEndCause::Shutdown);
                    self.graceful_shutdown().await;
                    break;
                }

                // Check disconnect grace periods
                _ = grace_check.tick() => {
                    self.check_disconnect_timeouts().await;
                }

                // An establishing participant's settle window is due
                // (`media_routing/connectivity.rs`). A tokio timer, so paused
                // test time drives it deterministically.
                () = async {
                    match settle_wake {
                        Some(at) => tokio::time::sleep_until(at).await,
                        None => std::future::pending().await,
                    }
                } => {
                    self.settle_due().await;
                }

                // The leave-debounced KEK rotation is due (ADR-0036 §4). A
                // tokio timer, so paused test time drives it deterministically.
                //
                // Deliberately NOT gated on `is_shutting_down` (story 2 task 9,
                // security F-5): a pending rotation may be abandoned only on the
                // EXIT path — the cancel arm above, which breaks the loop. While
                // the actor still serves participants the rotation must still
                // fire, or the W bound would lapse for that window with pending
                // state already cleared and nothing paging.
                () = async {
                    match kek_wake {
                        Some(at) => tokio::time::sleep_until(at).await,
                        None => std::future::pending().await,
                    }
                } => {
                    self.rotate_due(Instant::now()).await;
                }

                // Client-mute fan-outs deferred by the per-meeting bucket: the
                // next token is earned. A tokio timer built per iteration, like
                // the two above, so it dies with the loop (no wake after exit)
                // and paused test time drives it.
                () = async {
                    match fanout_wake {
                        Some(at) => tokio::time::sleep_until(at).await,
                        None => std::future::pending().await,
                    }
                } => {
                    self.flush_client_mute().await;
                }

                // Deferred slot-view work from an earlier turn (the per-turn
                // flush bound). Interleaves with mailbox traffic rather than
                // starving it; never drops a view.
                () = std::future::ready(()), if self.media.has_dirty() => {
                    self.flush_views().await;
                }

                // Handle messages
                msg = self.receiver.recv() => {
                    match msg {
                        Some(message) => {
                            self.mailbox.record_enqueue();
                            self.handle_message(message).await;
                            self.mailbox.record_dequeue();
                            self.metrics.record_message_processed();
                        }
                        None => {
                            info!(
                                target: "mc.actor.meeting",
                                meeting_id = %self.meeting_id,
                                "MeetingActor channel closed, exiting"
                            );
                            break;
                        }
                    }
                }
            }
        }

        // A meeting that ended by emptying or closing has not yet drained its
        // connection actors (the shutdown path above already did).
        let cause = self.end_cause.unwrap_or(MeetingEndCause::Shutdown);
        if cause != MeetingEndCause::Shutdown {
            self.graceful_shutdown().await;
        }
        self.hand_off_teardown(cause);

        // One of the two teardown evictions (the controller's `remove_meeting`
        // is the other, for an actor that panicked before reaching here), so a
        // dead meeting cannot pin the fleet gauges.
        //
        // A pending rotation is DROPPED here, not flushed, and that is sound:
        // the KEK is never persisted and dies with this actor, so the end of the
        // actor is itself a rotation for every future joiner.
        self.kek_lifecycle.remove_meeting(&self.meeting_id).await;

        info!(
            target: "mc.actor.meeting",
            meeting_id = %self.meeting_id,
            participants = self.participants.len(),
            messages_processed = self.mailbox.messages_processed(),
            "MeetingActor stopped"
        );
    }

    /// When the leave-debounced rotation should fire, if one is pending.
    fn kek_wake(&self) -> Option<Instant> {
        #[cfg(feature = "test-seams")]
        if self.kek_debounce_manual {
            return None;
        }
        self.kek_debounce.next_wake()
    }

    /// Handle a single message.
    async fn handle_message(&mut self, message: MeetingMessage) {
        match message {
            MeetingMessage::ConnectionJoin {
                connection_id,
                user_id,
                participant_id,
                display_name,
                is_host,
                identity_public_key,
                stream_tx,
                media,
                respond_to,
            } => {
                let result = self
                    .handle_join(
                        connection_id,
                        user_id,
                        participant_id,
                        display_name,
                        is_host,
                        identity_public_key,
                        stream_tx,
                        media,
                    )
                    .await;
                let _ = respond_to.send(result);
            }

            MeetingMessage::ConnectionDisconnected {
                connection_id,
                participant_id,
                cause,
            } => {
                self.handle_disconnect(&connection_id, &participant_id, cause)
                    .await;
            }

            MeetingMessage::ConnectionReconnect {
                connection_id,
                correlation_id,
                binding_token,
                declared,
                demand,
                respond_to,
            } => {
                let result = self
                    .handle_reconnect(
                        connection_id,
                        correlation_id,
                        binding_token,
                        declared,
                        demand,
                    )
                    .await;
                let _ = respond_to.send(result);
            }

            MeetingMessage::ParticipantLeave {
                participant_id,
                respond_to,
            } => {
                let result = self.handle_leave(&participant_id).await;
                let _ = respond_to.send(result);
            }

            MeetingMessage::SignalingMessage {
                participant_id,
                message,
            } => {
                self.handle_signaling(&participant_id, message).await;
            }

            MeetingMessage::GetState { respond_to } => {
                let state = self.get_state();
                let _ = respond_to.send(state);
            }

            MeetingMessage::MediaConnected {
                user_id,
                handler_id,
                connection_id,
                respond_to,
            } => {
                let result = self
                    .handle_media_connected(&user_id, &handler_id, connection_id)
                    .await;
                let _ = respond_to.send(result);
            }

            MeetingMessage::MediaDisconnected {
                user_id,
                handler_id,
                connection_id,
                respond_to,
            } => {
                let result = self
                    .handle_media_disconnected(&user_id, &handler_id, connection_id)
                    .await;
                let _ = respond_to.send(result);
            }

            MeetingMessage::RegisterReceiveCapability {
                participant_id,
                declaration,
                respond_to,
            } => {
                let result = self
                    .handle_receive_capability(&participant_id, declaration)
                    .await;
                let _ = respond_to.send(result);
            }

            MeetingMessage::UpdateSelfMute {
                participant_id,
                change,
                respond_to,
            } => {
                let decision = self.handle_self_mute(&participant_id, change).await;
                let _ = respond_to.send(decision);
            }

            MeetingMessage::ServerMute {
                target_participant_id,
                requester,
                change,
                respond_to,
            } => {
                let result = self
                    .handle_server_mute(&target_participant_id, &requester, change)
                    .await;
                let _ = respond_to.send(result);
            }

            MeetingMessage::RequestUnmute {
                participant_id,
                kinds,
                respond_to,
            } => {
                let relay = self.handle_request_unmute(&participant_id, kinds).await;
                let _ = respond_to.send(relay);
            }

            MeetingMessage::CloseMeeting { reason, respond_to } => {
                let result = self.handle_close_meeting(&reason).await;
                let _ = respond_to.send(result);
            }

            #[cfg(feature = "test-seams")]
            MeetingMessage::ForceKekRotation { respond_to } => {
                // Fire as if W had elapsed since the oldest removal.
                let now = self
                    .kek_debounce
                    .next_wake()
                    .map_or_else(Instant::now, |due| due.max(Instant::now()));
                let rotated = self.rotate_due(now).await;
                let _ = respond_to.send(rotated);
            }
        }
    }

    /// Handle a new connection joining.
    ///
    /// Generates secure binding tokens per ADR-0023 Section 1:
    /// - Correlation ID (UUIDv7)
    /// - Binding token via HMAC-SHA256(meeting_key, correlation_id || participant_id || nonce)
    #[instrument(skip_all, fields(meeting_id = %self.meeting_id))]
    #[expect(
        clippy::too_many_arguments,
        reason = "actor join signature threads the full join tuple (ids + display_name + host flag + identity key + stream); bundling into a JoinConnectionParams struct is a larger cross-message refactor tracked in docs/TODO.md"
    )]
    async fn handle_join(
        &mut self,
        connection_id: String,
        user_id: String,
        participant_id: String,
        display_name: String,
        is_host: bool,
        identity_public_key: Option<IdentityPublicKey>,
        stream_tx: Option<tokio::sync::mpsc::Sender<bytes::Bytes>>,
        media: JoinMedia,
    ) -> Result<JoinResult, McError> {
        if self.is_shutting_down {
            return Err(McError::Draining);
        }

        // Check if participant already exists (don't include participant_id in error - MINOR-002)
        if self.participants.contains_key(&participant_id) {
            return Err(McError::Conflict(
                "Participant already in meeting".to_string(),
            ));
        }

        // ADR-0036 §2/§4, invariant R-35: allocate the sender id BEFORE any
        // other admission state is built, so a failure costs nothing and leaves
        // nothing to unwind.
        //
        // At the wall this no longer refuses the joiner (story 2 R-16): the
        // admission epoch rotates the KEK and reissues from a fresh namespace
        // whose exclusion snapshot is every id bound right now — live AND
        // grace-period members, from the same roster map the KEK push goes to
        // (security D-1). The reset is atomic and immediate, exempt from the
        // leave debounce: debouncing it would stall joins for up to W.
        let rng = &self.rng;
        let participants = &self.participants;
        let mh_bindings = &self.mh_bindings;
        let admitted = self
            .admission
            .admit(rng, || {
                // Rostered ids (grace included), PLUS ids a media handler may
                // still hold for a participant already off the roster (F-1).
                participants
                    .values()
                    .map(|p| p.sender_id)
                    .chain(mh_bindings.held())
                    .collect()
            })
            .map_err(|failed| {
                // Fail closed: the joiner is refused and nothing about the key
                // state or namespace changed. Both arms are near-unreachable;
                // both are loud.
                match failed {
                    AdmitFailed::Rotation(reason) => {
                        crate::observability::metrics::record_kek_rotation_failure(reason);
                        error!(
                            target: "mc.actor.meeting",
                            reason = reason.label(),
                            key_custody = crate::observability::metrics::KEY_CUSTODY_OPERATOR,
                            "sender_id namespace exhausted and the KEK rotation that must accompany an epoch reset failed; refusing admission"
                        );
                    }
                    AdmitFailed::NoAllocatableId => error!(
                        target: "mc.actor.meeting",
                        bound = participants.len(),
                        "sender_id namespace exhausted and a fresh epoch had nothing allocatable after excluding the bound ids; refusing admission"
                    ),
                }
                McError::Internal("Failed to admit participant to the media path".to_string())
            })?;
        let allocation = admitted.allocation;
        let sender_id = allocation.sender_id;

        if admitted.epoch_reset {
            // The reset rotated the KEK, so it covers any departures still
            // waiting on the leave debounce too.
            let absorbed = self.kek_debounce.absorb();
            self.kek_lifecycle.set_pending(&self.meeting_id, None).await;
            crate::observability::metrics::record_meeting_kek_generated(
                RotationTrigger::SenderSpaceExhausted,
            );
            // The runbook (mc-incident-response.md Scenario 8, root cause 11)
            // greps the stem `sender_id namespace`, which this message shares
            // with the high-watermark record below. Keep that stem contiguous at
            // the START of a single-line literal: a `\`-continued literal split
            // inside it still reads correctly in the log but the grep matches
            // nothing — and nothing reads as "not this cause".
            warn!(
                target: "mc.actor.meeting",
                generation = self.admission.keys().generation(),
                departures_absorbed = absorbed.map_or(0, |c| c.leaves),
                bound = self.participants.len(),
                meeting_age_seconds = chrono::Utc::now().timestamp() - self.created_at,
                "sender_id namespace exhausted; KEK epoch reset and reissue"
            );
            // Incumbents get the new KEK BEFORE the joiner's ParticipantJoined
            // is broadcast. The joiner is not yet on the roster, so it is not a
            // recipient; it receives this KEK in its own JoinResponse.
            self.push_kek_update(RotationReport {
                trigger: RotationTrigger::SenderSpaceExhausted,
                coalesced_leaves: absorbed.map_or(0, |c| c.leaves),
                generation: self.admission.keys().generation(),
                started: Instant::now(),
            })
            .await;
        }
        self.kek_lifecycle
            .set_sender_ids_issued(&self.meeting_id, self.admission.sender_ids_issued())
            .await;

        // Media admission, BEFORE any other admission state is built, so a
        // failure leaves nothing to unwind (the allocated sender id is simply
        // never used). The first join freezes the meeting's handler set. The
        // joiner is bound to no handler: it is offered the whole set, and
        // becomes routable only when the handlers report its connections
        // (ADR-0036 §9).
        let declared = media.declared;
        let publishes = declared.publish_set();
        let demand = media.demand.clone();
        self.media.install(media);
        self.media.admit(&participant_id, sender_id, publishes)?;
        // The join carries the initial declaration (story 3 R-2): registered
        // in THIS turn, before the reconcile below, so the joiner's first
        // directive and assignments are queued behind its JoinResponse.
        self.media.declare(&participant_id, demand)?;
        let media_handlers =
            self.media.frozen_handlers().cloned().ok_or_else(|| {
                McError::MhAssignmentMissing("no handler set installed".to_string())
            })?;

        // `high_watermark_crossed` is the allocator's own one-shot latch (true on
        // exactly the crossing allocation and never again per allocator), so
        // there is no second guard here — one latch, unit-tested in
        // `sender_id.rs`.
        if allocation.high_watermark_crossed {
            // A LEADING indicator: it precedes a recoverable epoch reset, and
            // re-arms each epoch because each reset installs a fresh allocator.
            // The action it prompts is to investigate the driver (a flapping
            // client or a scripted join loop), not to end the meeting.
            // Carries no sender_id value.
            //
            // The runbook greps this message's `sender_id namespace` stem — keep
            // it contiguous at the start of a single-line literal (see the
            // epoch-reset record above).
            warn!(
                target: "mc.actor.meeting",
                cursor_consumed = self.admission.sender_ids_issued(),
                namespace_remaining = self.admission.sender_ids_remaining(),
                meeting_age_seconds = chrono::Utc::now().timestamp() - self.created_at,
                "sender_id namespace past high watermark"
            );
        }

        debug!(
            target: "mc.actor.meeting",
            "Participant joining"
        );

        // Generate correlation ID and binding token (ADR-0023 Section 1)
        let correlation_id = StoredBinding::generate_correlation_id();
        let (binding_token, nonce) =
            self.binding_manager
                .generate_token(&self.meeting_id, &correlation_id, &participant_id);

        // Store the binding for reconnection validation
        let stored_binding = StoredBinding::new(
            correlation_id.clone(),
            participant_id.clone(),
            user_id.clone(),
            nonce,
            binding_token.clone(),
        );
        self.stored_bindings
            .insert(correlation_id.clone(), stored_binding);

        // Create participant actor with stream + meeting handle for disconnect notification
        let connection_token = self.cancel_token.child_token();
        let (conn_handle, conn_task) = ParticipantActor::spawn_inner(
            connection_id.clone(),
            participant_id.clone(),
            self.meeting_id.clone(),
            connection_token,
            Arc::clone(&self.metrics),
            stream_tx,
            Some(self.self_handle.clone()),
        );

        // Store connection
        self.connections.insert(
            connection_id.clone(),
            ManagedConnection {
                handle: conn_handle.clone(),
                task_handle: conn_task,
                participant_id: participant_id.clone(),
            },
        );

        // Prefer the token's display_name; fall back to a generic label only when
        // the claim is genuinely absent (empty). The claim was already length-bounded
        // at the connection trust boundary, so it is used verbatim here. Record the
        // resolution outcome (bounded label, never the name value) so the empty-claim
        // degradation is observable on the MC consumer side ("fail loudly").
        let display_name = if display_name.is_empty() {
            crate::observability::metrics::record_display_name_resolution("fallback");
            format!("Participant {}", self.participants.len() + 1)
        } else {
            crate::observability::metrics::record_display_name_resolution("present");
            display_name
        };
        let conn_handle_for_result = conn_handle.clone();
        let participant = Participant {
            participant_id: participant_id.clone(),
            user_id: user_id.clone(),
            display_name,
            correlation_id: correlation_id.clone(),
            connection: Some(conn_handle),
            status: ParticipantStatus::Connected,
            disconnected_at: None,
            media: MediaStates {
                // Definite from admission (ADR-0036 §5 amendment 2026-10-25):
                // the declaration names each published kind's client mute, and
                // every other kind is NOT_PUBLISHED, so peers never see an
                // unknown state and nothing is fanned out for this join -- its
                // state rides its own `ParticipantJoined`. A joined-muted kind is
                // PAUSED at MH from the first render below.
                client: declared,
                // A fresh JOIN starts server-unmuted — this is what makes a
                // server mute not survive a rejoin. A RECONNECT reuses this
                // roster entry and keeps it. No side table is needed (R-10).
                server: ServerMuteAttribution::none(),
            },
            is_host,
            sender_id,
            identity_public_key,
        };

        let participant_info = participant.to_info();

        self.participants
            .insert(participant_id.clone(), participant);
        self.correlation_to_participant
            .insert(correlation_id.clone(), participant_id.clone());

        self.metrics.connection_created();
        self.controller_metrics.increment_participants();

        // Get list of other participants
        let participants: Vec<ParticipantInfo> = self
            .participants
            .values()
            .filter(|p| p.participant_id != participant_id)
            .map(Participant::to_info)
            .collect();

        // Broadcast join to other participants
        self.broadcast_update(
            Some(&participant_id),
            ParticipantStateUpdate::Joined(participant_info),
        )
        .await;

        info!(
            target: "mc.actor.meeting",
            total_participants = self.participants.len(),
            "Participant joined"
        );

        // Structural change: re-render, re-push every handler (the first join
        // registers every handler, possibly with an empty snapshot), and
        // re-emit changed views to every declared participant — the joiner
        // included, whose join carried its declaration.
        self.reconcile_media(Affected::All).await;
        self.flush_views().await;

        Ok(JoinResult {
            participant_id,
            correlation_id,
            binding_token,
            participants,
            fencing_generation: self.fencing_generation,
            sender_id,
            // Arc clone: a handle, never a copy of the key bytes. The CURRENT
            // key — after an epoch reset on this very join, the new one.
            meeting_kek: Arc::clone(self.admission.keys().kek()),
            kek_generation: self.admission.keys().generation(),
            kek_rotation_debounce_seconds: self.kek_lifecycle.window_seconds(),
            participant_handle: conn_handle_for_result,
            media_handlers,
        })
    }

    /// Handle connection disconnect.
    ///
    /// `cause` (transport-authenticated; see [`DisconnectCause`]) selects the
    /// path:
    /// - [`DisconnectCause::ClientClosed`] — a clean tab-close: remove the
    ///   participant from the roster IMMEDIATELY and broadcast
    ///   `ParticipantLeft{Voluntary}` (the ADR-0023 grace period is skipped — a
    ///   deliberately closed session is not reconnecting).
    /// - [`DisconnectCause::ConnectionLost`] / [`DisconnectCause::ServerInitiated`]
    ///   — abrupt/ambiguous: mark the participant `Disconnected` and start the
    ///   grace period so a genuine transient disconnect can reconnect.
    ///
    /// Idempotent under the double-notify path (the `ParticipantActor` self-notify
    /// plus [`check_connection_health`](Self::check_connection_health)): once the
    /// participant is removed (`ClientClosed`) or already in grace
    /// (`ConnectionLost`/`ServerInitiated`), a second call is a no-op. Both arms
    /// gate `record_participant_disconnect` on a live→dead transition
    /// (`status != Disconnected`), so the physical drop is counted exactly once
    /// even when a racing inline `ConnectionLost` marks the participant
    /// `Disconnected` before the queued `ClientClosed` is processed — the disconnect
    /// counter is never double-incremented, and the leave counter fires once via the
    /// single `remove_and_broadcast_left` choke-point.
    async fn handle_disconnect(
        &mut self,
        connection_id: &str,
        participant_id: &str,
        cause: DisconnectCause,
    ) {
        debug!(
            target: "mc.actor.meeting",
            meeting_id = %self.meeting_id,
            participant_id = %participant_id,
            connection_id = %connection_id,
            cause = %cause.label(),
            "Connection disconnected"
        );

        // Remove connection (once).
        if let Some(conn) = self.connections.remove(connection_id) {
            // Wait briefly for task to complete
            let _ = tokio::time::timeout(Duration::from_millis(100), conn.task_handle).await;
            self.metrics.connection_closed();
        }

        // Look up current participant status; if already gone (removed/left),
        // there is nothing to do — keeps the double-notify path idempotent.
        let Some(status) = self.participants.get(participant_id).map(|p| p.status) else {
            return;
        };

        match cause {
            DisconnectCause::ClientClosed => {
                // Clean close — the user closed the tab. Remove immediately and
                // broadcast ParticipantLeft{Voluntary}; skip the grace period.
                //
                // Count the physical drop ONLY on a live→dead transition, matching
                // the ConnectionLost arm below. If the participant is already
                // Disconnected, a racing inline ConnectionLost (from
                // check_connection_health observing the finished conn task before
                // this queued ClientClosed is processed) already counted the drop —
                // re-counting here would double-increment
                // mc_participant_disconnects_total for one departure and skew the
                // reconnect-rate identity. We still remove + broadcast Left below
                // (skip grace); only the counter is gated.
                if status != ParticipantStatus::Disconnected {
                    crate::observability::metrics::record_participant_disconnect(cause.label());
                }
                info!(
                    target: "mc.actor.meeting",
                    meeting_id = %self.meeting_id,
                    participant_id = %participant_id,
                    "Clean transport close — removing participant immediately (grace skipped)"
                );
                self.remove_and_broadcast_left(participant_id, LeaveReason::Voluntary)
                    .await;
            }
            DisconnectCause::ConnectionLost | DisconnectCause::ServerInitiated => {
                // Abrupt / ambiguous loss. Start the ADR-0023 grace period so a
                // genuine transient disconnect can reconnect. Idempotent: if the
                // participant is already in grace, do nothing (no double count).
                if status == ParticipantStatus::Disconnected {
                    return;
                }
                crate::observability::metrics::record_participant_disconnect(cause.label());
                if let Some(participant) = self.participants.get_mut(participant_id) {
                    participant.status = ParticipantStatus::Disconnected;
                    participant.disconnected_at = Some(Instant::now());
                    participant.connection = None;
                }

                // Broadcast disconnect to other participants (informational; not
                // serialized to the wire — the roster is only removed on grace
                // expiry via check_disconnect_timeouts).
                self.broadcast_update(
                    Some(participant_id),
                    ParticipantStateUpdate::Disconnected {
                        participant_id: participant_id.to_string(),
                    },
                )
                .await;

                info!(
                    target: "mc.actor.meeting",
                    meeting_id = %self.meeting_id,
                    participant_id = %participant_id,
                    "Participant disconnected, grace period started"
                );
            }
        }
    }

    /// Remove a participant from the roster and broadcast `ParticipantLeft`.
    ///
    /// The SINGLE roster-removal choke-point (ADR-0032 metric-path completeness):
    /// every removal — clean-close, grace-timeout, explicit leave — flows through
    /// here, so `mc_participant_leaves_total{reason}` is emitted exactly once per
    /// removal and can never be broadcast-without-metric. Cleans up the
    /// correlation/binding maps, cancels any live connection, decrements the GC
    /// participant count, records the metric, then broadcasts.
    async fn remove_and_broadcast_left(&mut self, participant_id: &str, reason: LeaveReason) {
        let Some(participant) = self.participants.remove(participant_id) else {
            return;
        };

        // Remove correlation and binding mappings (session recovery cleanup).
        self.correlation_to_participant
            .remove(&participant.correlation_id);
        self.stored_bindings.remove(&participant.correlation_id);

        // Cancel the connection if still active.
        if let Some(conn_handle) = &participant.connection {
            conn_handle.cancel();
        }

        // Decrement participant count for GC heartbeat reporting.
        self.controller_metrics.decrement_participants();

        // Record the leave (bounded reason label) at the single choke-point.
        crate::observability::metrics::record_participant_leave(reason.label());

        // A leaver owes no client-mute fan-out: nobody may hear about it after
        // its `ParticipantLeft`.
        self.client_mute_fanout.remove(participant_id);

        // Broadcast leave to the remaining participants.
        self.broadcast_update(
            Some(participant_id),
            ParticipantStateUpdate::Left {
                participant_id: participant_id.to_string(),
                reason,
            },
        )
        .await;

        // Structural change, on the SAME choke point every removal takes
        // (explicit leave, clean close, grace expiry): free only the leaver's
        // slots, refill earliest-first, re-push, re-emit.
        self.media.remove(participant_id);
        self.reconcile_media(Affected::All).await;
        self.flush_views().await;

        // KEK rotation on leave (ADR-0036 §4; story 2 R-12), scheduled on the
        // SAME choke point, AFTER the removal above. When it fires, the new KEK
        // goes to `self.participants` as it stands then — which no longer holds
        // this leaver — so the leaver is unreachable BY CONSTRUCTION, never by a
        // filter. Debounced from the OLDEST un-rotated removal; a later removal
        // never moves that anchor.
        self.kek_debounce.record_removal(Instant::now());
        self.kek_lifecycle
            .set_pending(&self.meeting_id, self.kek_debounce.pending_since())
            .await;

        // The mute census follows the roster: a removed server-muted
        // participant is no longer counted.
        self.publish_mute_census();

        // The last participant is gone: the meeting has ENDED (ADR-0010 §3,
        // "last participant leaves, MC notifies GC"). On THIS choke point, so
        // every removal path ends it identically — an explicit leave, a clean
        // close, a grace expiry. A participant still in its grace window is on
        // the roster, so a reconnect inside the window never reaches here.
        //
        // No new admission from this turn on (`Draining`), and the loop leaves
        // through the clean exit path, which drains the push workers and
        // releases every handler (`media_routing::teardown`). The pending KEK
        // rotation just recorded is dropped with the actor, which is sound: the
        // KEK dies with it, so the end of the actor is itself a rotation.
        if self.participants.is_empty() && self.end_cause.is_none() {
            info!(
                target: "mc.actor.meeting",
                meeting_id = %self.meeting_id,
                "Last participant left; ending the meeting"
            );
            self.is_shutting_down = true;
            self.end_cause = Some(MeetingEndCause::Empty);
        }
    }

    /// Perform the leave-debounced rotation if it is due. Returns whether a
    /// rotation happened.
    ///
    /// Pending age is cleared AT ROTATION, not when every push succeeds
    /// (security S-3b): one wedged participant must not pin the gauge and page
    /// forever. Undelivered pushes are the push counter's job.
    async fn rotate_due(&mut self, now: Instant) -> bool {
        let Some(coalesced) = self.kek_debounce.take_due(now) else {
            return false;
        };
        if let Err(reason) = self.admission.rotate(&self.rng) {
            self.rotation_failed(coalesced, reason, now);
            return false;
        }
        self.kek_lifecycle.set_pending(&self.meeting_id, None).await;
        crate::observability::metrics::record_meeting_kek_generated(
            RotationTrigger::ParticipantLeft,
        );
        crate::observability::metrics::record_kek_rotation_coalesced_leaves(coalesced.leaves);
        self.push_kek_update(RotationReport {
            trigger: RotationTrigger::ParticipantLeft,
            coalesced_leaves: coalesced.leaves,
            generation: self.admission.keys().generation(),
            started: now,
        })
        .await;
        true
    }

    /// A leave rotation failed. Keep the departures pending with their
    /// ORIGINAL anchor, so the published pending age keeps growing and
    /// `MCKekRotationOverdue` fires; retry after W rather than spinning.
    fn rotation_failed(
        &mut self,
        coalesced: Coalesced,
        reason: crate::media_admission::KekRotationFailed,
        now: Instant,
    ) {
        crate::observability::metrics::record_kek_rotation_failure(reason);
        self.kek_debounce.defer_after_failure(coalesced, now);
        // ERROR, not WARN: the exposure is real while this persists. Every
        // departed participant still holds a KEK that opens all current media,
        // with no forward bound — the W bound is SUSPENDED, not delayed
        // (security F-3). No key material here, nor any participant id.
        error!(
            target: "mc.kek.lifecycle",
            reason = reason.label(),
            departures_pending = coalesced.leaves,
            key_custody = crate::observability::metrics::KEY_CUSTODY_OPERATOR,
            "Meeting KEK rotation failed; departed participants retain a working KEK until it succeeds. Retrying after W."
        );
    }

    /// Push the CURRENT KEK to every rostered participant, and hand their
    /// outcomes to a detached collector.
    ///
    /// Exactly one outcome per rostered recipient. A participant with no live
    /// connection (inside grace) is `participant_gone` without a send; the rest
    /// get a `KekUpdate` and answer on a oneshot. The actor never awaits those
    /// answers — the collector does, bounded — so a participant blocked on this
    /// actor's mailbox cannot deadlock it.
    ///
    /// Stated residual (security S-3a): a member whose push is not delivered
    /// never rotates its own transmit keys, so a departed participant keeps
    /// opening THAT member's media past W. `mc_meeting_kek_pushes_total` and
    /// `MCKekPushFailureRate` are the only control.
    async fn push_kek_update(&self, report: RotationReport) {
        let push = KekPush {
            kek: Arc::clone(self.admission.keys().kek()),
            generation: self.admission.keys().generation(),
            kek_rotation_debounce_seconds: self.kek_lifecycle.window_seconds(),
        };
        let mut pending = Vec::with_capacity(self.participants.len());
        for participant in self.participants.values() {
            let outcome = match &participant.connection {
                None => PendingOutcome::Decided(KekPushOutcome::ParticipantGone),
                Some(conn) => match conn.send_kek_update(push.clone()).await {
                    Ok(rx) => PendingOutcome::Awaiting(rx),
                    Err(_) => PendingOutcome::Decided(KekPushOutcome::ActorUnavailable),
                },
            };
            pending.push(outcome);
        }
        // `push` (and its Arc) drops here; each queued message holds its own
        // clone until the participant actor encodes it.
        drop(push);
        tokio::spawn(tracing::Instrument::in_current_span(collect_push_outcomes(
            pending, report,
        )));
    }

    /// Handle reconnection attempt.
    ///
    /// Validates binding token per ADR-0023 Section 1:
    /// 1. Correlation ID exists
    /// 2. Binding token HMAC verification (constant-time)
    /// 3. Token not expired (30s TTL)
    ///
    /// On success, rotates correlation ID and binding token.
    #[instrument(skip_all, fields(meeting_id = %self.meeting_id))]
    async fn handle_reconnect(
        &mut self,
        connection_id: String,
        correlation_id: String,
        binding_token: String,
        declared: ClientMediaStates,
        demand: Option<SlotDemand>,
    ) -> Result<ReconnectResult, McError> {
        let publishes = declared.publish_set();
        // Find stored binding by correlation ID
        let stored_binding =
            self.stored_bindings
                .get(&correlation_id)
                .ok_or(McError::SessionBinding(
                    crate::errors::SessionBindingError::SessionNotFound,
                ))?;

        // Check if binding has expired (ADR-0023: 30s TTL)
        if stored_binding.is_expired() {
            // Remove expired binding
            self.stored_bindings.remove(&correlation_id);
            return Err(McError::SessionBinding(
                crate::errors::SessionBindingError::TokenExpired,
            ));
        }

        // Validate binding token via HMAC-SHA256 (MAJOR-003 fix)
        let is_valid = self.binding_manager.validate_token(
            &self.meeting_id,
            &correlation_id,
            &stored_binding.participant_id,
            &stored_binding.nonce,
            &binding_token,
        );

        if !is_valid {
            warn!(
                target: "mc.actor.meeting",
                "Invalid binding token on reconnect attempt"
            );
            return Err(McError::SessionBinding(
                crate::errors::SessionBindingError::InvalidToken,
            ));
        }

        // Find participant by correlation ID
        let participant_id = self
            .correlation_to_participant
            .get(&correlation_id)
            .ok_or(McError::SessionBinding(
                crate::errors::SessionBindingError::SessionNotFound,
            ))?
            .clone();

        let participant =
            self.participants
                .get_mut(&participant_id)
                .ok_or(McError::SessionBinding(
                    crate::errors::SessionBindingError::SessionNotFound,
                ))?;

        // Story 3 R-2: the publish set is FIXED for the membership. A
        // reconnect presenting a different SET is refused — generically,
        // counted, BEFORE any binding rotation or broadcast — and never
        // adopted: otherwise a participant could slip out from under a mute by
        // reconnecting with a different set. The roster entry is untouched.
        //
        // REACHABILITY: this actor-level reconnect is not yet wired to the
        // WebTransport accept path (see the note on
        // `sender_id_continuity_across_actor_level_reconnect`); a browser
        // "reconnect" is a fresh join. The control is proven at this seam.
        if participant.publishes() != publishes {
            // Failure side only; see `record_join_refusal` for the double-count
            // hazard once reconnect is wired to the accept path.
            crate::observability::metrics::record_join_refusal(
                McError::ReconnectPublishSetMismatch.error_type_label(),
            );
            warn!(
                target: "mc.actor.meeting",
                "Refusing reconnect: presented publish set differs from the membership's"
            );
            return Err(McError::ReconnectPublishSetMismatch);
        }

        debug!(
            target: "mc.actor.meeting",
            "Participant reconnecting"
        );

        // Create new participant actor with meeting handle for disconnect notification
        let connection_token = self.cancel_token.child_token();
        let (conn_handle, conn_task) = ParticipantActor::spawn_with_meeting(
            connection_id.clone(),
            participant_id.clone(),
            self.meeting_id.clone(),
            connection_token,
            Arc::clone(&self.metrics),
            self.self_handle.clone(),
        );

        // Store new connection
        self.connections.insert(
            connection_id.clone(),
            ManagedConnection {
                handle: conn_handle.clone(),
                task_handle: conn_task,
                participant_id: participant_id.clone(),
            },
        );

        // Update participant state
        participant.status = ParticipantStatus::Connected;
        participant.disconnected_at = None;
        participant.connection = Some(conn_handle);

        // Remove old binding and correlation mapping
        self.stored_bindings.remove(&correlation_id);
        self.correlation_to_participant.remove(&correlation_id);

        // Generate new correlation ID and binding token (rotation per ADR-0023)
        let new_correlation_id = StoredBinding::generate_correlation_id();
        let (new_binding_token, new_nonce) = self.binding_manager.generate_token(
            &self.meeting_id,
            &new_correlation_id,
            &participant_id,
        );

        // Store new binding
        let new_binding = StoredBinding::new(
            new_correlation_id.clone(),
            participant_id.clone(),
            participant.user_id.clone(),
            new_nonce,
            new_binding_token.clone(),
        );
        self.stored_bindings
            .insert(new_correlation_id.clone(), new_binding);

        // Update mapping
        self.correlation_to_participant
            .insert(new_correlation_id.clone(), participant_id.clone());
        participant.correlation_id = new_correlation_id.clone();

        self.metrics.connection_created();

        // Broadcast reconnection
        self.broadcast_update(
            Some(&participant_id),
            ParticipantStateUpdate::Reconnected {
                participant_id: participant_id.clone(),
            },
        )
        .await;

        info!(
            target: "mc.actor.meeting",
            "Participant reconnected"
        );

        // The declaration's client mute is ADOPTED (security Gate 1 point 2;
        // CQ-7): the kind set matched above, so the change is proven against
        // the membership's publish set and goes through the SAME merge and the
        // same per-meeting fan-out as a `MuteRequest` -- never a direct
        // broadcast. Server mute and its attribution are untouched.
        // The declaration's own kinds ARE the (matched) publish set, so the
        // change is proven by construction (`as_change`); `handle_self_mute`
        // re-checks it against the roster and fails loudly on a mismatch.
        if let Some(adopted) = declared.as_change() {
            self.handle_self_mute(&participant_id, adopted).await;
        }

        // A reconnect keeps rank and slots (the participant never left the
        // roster). Its receive declaration is a REDECLARATION: a different one
        // is applied (structural change, re-push), an identical or absent one
        // keeps the declaration in force. A declaration that would leave the
        // participant neither publishing nor receiving is not applied — the
        // previous one stays (R-2). The new connection has received nothing, so
        // its full current view is re-sent.
        let redeclared = match demand {
            // Absent, or identical to the declaration in force: an uncounted
            // no-op, as on the post-join path.
            None => false,
            Some(demand) if self.media.declaration_of(&participant_id) == Some(&demand) => false,
            // R-2: would leave the participant neither publishing nor
            // receiving. Refused and COUNTED, exactly as the post-join path
            // counts it; the previous declaration stays in force.
            Some(demand)
                if crate::media_signaling::publishes_and_receives_nothing(
                    publishes,
                    demand.declaration(),
                ) =>
            {
                crate::observability::metrics::record_receive_capability(
                    crate::media_signaling::CapabilityOutcome::WouldPublishAndReceiveNothing,
                );
                false
            }
            // A view missing for a roster participant is an MC invariant
            // violation: fail the reconnect loudly rather than silently not
            // applying the redeclaration.
            Some(demand) => {
                self.media.declare(&participant_id, demand)?;
                true
            }
        };
        if redeclared {
            self.reconcile_media(Affected::All).await;
        }
        self.media.reset_view(&participant_id);
        self.flush_views().await;

        // Get current participant list
        let participants: Vec<ParticipantInfo> = self
            .participants
            .values()
            .filter(|p| p.participant_id != participant_id)
            .map(Participant::to_info)
            .collect();

        // R-15: re-issue the CURRENT KEK, generation and W, WITHOUT a rotation.
        // A reconnect is not a roster removal, so the debounce is untouched.
        // The client's install is idempotent on an already-held generation, so
        // a reconnect across no rotation is a no-op for it; across a rotation it
        // is what stops the participant silently holding a stale KEK.
        Ok(ReconnectResult {
            participant_id,
            new_correlation_id,
            new_binding_token,
            participants,
            meeting_kek: Arc::clone(self.admission.keys().kek()),
            kek_generation: self.admission.keys().generation(),
            kek_rotation_debounce_seconds: self.kek_lifecycle.window_seconds(),
        })
    }

    /// Handle participant leaving.
    #[instrument(skip_all, fields(meeting_id = %self.meeting_id))]
    async fn handle_leave(&mut self, participant_id: &str) -> Result<(), McError> {
        if self.participants.contains_key(participant_id) {
            debug!(
                target: "mc.actor.meeting",
                "Participant leaving"
            );

            // Explicit voluntary leave — remove via the single choke-point
            // (emits mc_participant_leaves_total{reason="voluntary"}).
            self.remove_and_broadcast_left(participant_id, LeaveReason::Voluntary)
                .await;

            info!(
                target: "mc.actor.meeting",
                remaining_participants = self.participants.len(),
                "Participant left"
            );

            Ok(())
        } else {
            // MINOR-002 fix: Don't include participant ID in error message
            Err(McError::ParticipantNotFound(
                "Participant not found".to_string(),
            ))
        }
    }

    /// Handle signaling message.
    async fn handle_signaling(&mut self, participant_id: &str, message: SignalingPayload) {
        // Verify participant exists
        if !self.participants.contains_key(participant_id) {
            warn!(
                target: "mc.actor.meeting",
                meeting_id = %self.meeting_id,
                participant_id = %participant_id,
                "Signaling message from unknown participant"
            );
            return;
        }

        debug!(
            target: "mc.actor.meeting",
            meeting_id = %self.meeting_id,
            participant_id = %participant_id,
            message_type = ?std::mem::discriminant(&message),
            "Received signaling message"
        );

        // TODO (Phase 6g): Route signaling messages appropriately
        // There is deliberately no mute arm here: client mute has ONE entry,
        // `MeetingMessage::UpdateSelfMute`, fed by the connection's per-kind
        // parser and rate limit (story 3 MC-B removed a both-kinds
        // `SignalingPayload::MuteUpdate` that bypassed both).
        match message {
            SignalingPayload::LayoutSubscribe { .. } => {
                // TODO: Handle layout subscription
            }
            SignalingPayload::Chat { .. } => {
                // TODO: Handle chat message
            }
            SignalingPayload::Raw { .. } => {
                // TODO: Handle raw protobuf message
            }
        }
    }

    /// Get current meeting state.
    fn get_state(&self) -> MeetingState {
        MeetingState {
            meeting_id: self.meeting_id.clone(),
            participants: self
                .participants
                .values()
                .map(Participant::to_info)
                .collect(),
            fencing_generation: self.fencing_generation,
            created_at: self.created_at,
            mailbox_depth: self.mailbox.current_depth(),
            is_shutting_down: self.is_shutting_down,
        }
    }

    /// Handle a client (self) mute report: record the FACT, tell every other
    /// participant (under the per-meeting bound), and re-emit the changed slot
    /// view to the subscribers holding this source.
    ///
    /// # Roster fan-out (story 3 R-15 fix; reverses story-2 S-2)
    ///
    /// Until 2026-10-24 self-mute deliberately did NOT fan out: security S-2
    /// (2026-09-03) judged slot state (`SLOT_STATE_AVATAR_CLIENT_MUTED`) the
    /// only signal subscribers needed, and objected to an O(roster) awaited
    /// broadcast bounded only per connection. The human decision of 2026-10-24
    /// (ADR-0036 §5 amendment of that date) reversed the first half: every
    /// participant sees every other participant's client mute per kind,
    /// whether or not one of its slots carries them. The cost objection is
    /// answered by a PER-MEETING bound, not by the per-connection mute bucket
    /// alone (which leaves the aggregate O(N²), MC having no per-meeting
    /// participant cap): a real transition ENQUEUES the subject on
    /// [`ClientMuteFanout`], and [`Self::flush_client_mute`] sends one full
    /// snapshot per subject per token, latest-state-wins. Under churn a fact is
    /// delayed (FIFO, bounded by the bucket rate), never dropped by the bound;
    /// per-recipient wire loss is the participant actor's counted drop.
    ///
    /// The SUBJECT gets no copy: its client already holds its own client mute,
    /// and a self copy would only re-assert unchanged server flags to its R-18
    /// resume logic.
    ///
    /// # Definite state (ADR-0036 §5 amendment 2026-10-25)
    ///
    /// Every kind is known from admission, so there is no first-report case:
    /// the merge is the ONE [`ClientMediaStates::apply`], over a change proven
    /// at the boundary to name only published kinds ([`PublishedMuteChange`]),
    /// and a published kind only ever moves `Live <-> ClientMuted`. So a moved
    /// kind is BOTH a wire change (it fans out) and a media change: the source
    /// re-renders (its stream's disposition flips FORWARD <-> PAUSED in
    /// `source_streams` -- NEVER DROP_AT_INGRESS, so a resume keyframe that
    /// beats this update still reaches subscribers) and the subscribers HOLDING
    /// it are marked dirty, under the per-turn slot-view flush bound. A repeat
    /// is a no-op that emits, enqueues and renders nothing.
    ///
    /// The fan-out is enqueued BEFORE the slot re-emit, so when it is not
    /// deferred the roster fact precedes the slot state; a deferred fact may
    /// arrive after it, which is harmless (clients take client mute from the
    /// roster, never from slots).
    ///
    /// PER KIND (story 3 R-17): only the kinds `change` names move, merged
    /// against the ROSTER, never a connection cache, so a reconnect within
    /// grace cannot coerce an unnamed kind. The SEND DIRECTIVE is never
    /// touched: the publisher is not dirtied, and the directive builder has no
    /// client-mute input at all (ADR-0036 §5).
    ///
    /// Returns the per-kind decision, `Err(NotPublished)` if the change names
    /// a kind the ROSTER says this participant does not publish (a mismatched
    /// proof: refused and counted by the caller, never masked as unchanged;
    /// code-quality CR-4), or `None` if the participant is gone.
    #[instrument(skip_all, fields(meeting_id = %self.meeting_id))]
    async fn handle_self_mute(
        &mut self,
        participant_id: &str,
        change: PublishedMuteChange,
    ) -> Option<Result<KindDecision<ClientKindState>, NotPublished>> {
        let participant = self.participants.get_mut(participant_id)?;
        let decision = match participant.media.client.apply(change) {
            Ok(decision) => decision,
            Err(NotPublished) => {
                // Unreachable for a correct connection (it proved the change
                // against the same publish set): loud, never silent.
                debug_assert!(
                    false,
                    "client-mute proof mismatched the roster's publish set"
                );
                warn!(
                    target: "mc.actor.meeting",
                    "Client-mute change named a kind the roster says is not published; refused"
                );
                return Some(Err(NotPublished));
            }
        };

        // IDEMPOTENT: a report that changes no kind is a no-op -- no fan-out,
        // no render, no generation advance, no push, no emission. Ahead of
        // everything else.
        if decision.is_unchanged() {
            return Some(Ok(decision));
        }

        participant.media.client = ClientMediaStates::from(decision);
        let sender = participant.sender_id;

        self.client_mute_fanout.enqueue(participant_id);
        self.flush_client_mute().await;

        // The change re-renders (the source's PAUSED disposition rides
        // `source_streams`; the generation advances only if the output
        // changed) and re-emits to every holder of this source whose view
        // changed, under the per-meeting flush bound.
        self.reconcile_media(Affected::HoldersOf(sender)).await;
        self.flush_views().await;
        Some(Ok(decision))
    }

    /// Send every client-mute fan-out the per-meeting bucket allows now: one
    /// full snapshot per subject, composed from the roster AT FLUSH TIME (all
    /// four controls plus `server_muted_by`, so a frame can never regress a
    /// server mute), to every participant present NOW except the subject.
    /// Subjects left pending wait for the loop's fan-out wake.
    ///
    /// The sends AWAIT each recipient's bounded mailbox, as every roster,
    /// server-mute and KEK send does. Accepted because the participant actor's
    /// `handle_update` never blocks on the network (it `try_send`s to the
    /// wire stream and counts the drop), so the await stalls only behind a
    /// full participant mailbox, and the per-meeting bucket caps how many
    /// fan-outs any participant -- a non-host included -- can make this
    /// actor perform.
    async fn flush_client_mute(&mut self) {
        use crate::media_signaling::ClientMuteFanoutOutcome;
        use crate::observability::metrics::record_client_mute_fanout;
        while let Some(subject) = self.client_mute_fanout.pop_ready(Instant::now()) {
            // A subject is removed on leave (the one removal choke-point), so a
            // miss here is a dropped removal -- an MC-internal invariant
            // violation. Loud (red in tests, a WARN in production; meeting_id
            // only, from the span, no participant identity), and sending
            // nothing is the safe side.
            let Some(participant) = self.participants.get(&subject) else {
                debug_assert!(false, "client-mute fan-out subject missing from roster");
                warn!(
                    target: "mc.actor.meeting",
                    meeting_id = %self.meeting_id,
                    "Client-mute fan-out subject missing from roster (MC-internal invariant \
                     violation); nothing was sent for it"
                );
                continue;
            };
            let update = Self::mute_changed(participant);
            self.broadcast_update(Some(&subject), update).await;
            record_client_mute_fanout(ClientMuteFanoutOutcome::FannedOut, 1);
        }
        record_client_mute_fanout(
            ClientMuteFanoutOutcome::AwaitingToken,
            self.client_mute_fanout.pending_len() as u64,
        );
    }

    /// Register a validated receive-capability declaration: slot demand is now
    /// actor state. Structural change — re-render, re-push, re-emit (the
    /// declarer's own directive and assignments included, directive first).
    async fn handle_receive_capability(
        &mut self,
        participant_id: &str,
        declaration: SlotDemand,
    ) -> Result<(), McError> {
        if !self.participants.contains_key(participant_id) {
            return Err(McError::ParticipantNotFound(
                "Participant not found".to_string(),
            ));
        }
        self.media.declare(participant_id, declaration)?;
        self.reconcile_media(Affected::All).await;
        self.flush_views().await;
        Ok(())
    }

    /// Flush up to the per-turn bound of dirty participants' slot views.
    /// Resolve a token `sub` to exactly one roster participant.
    ///
    /// Looked up inside THIS meeting's roster and nowhere else, so a
    /// cross-meeting answer is unrepresentable: `sender_id` 5 exists
    /// concurrently in every meeting. Matched on `user_id` because that is where
    /// MC stores the token `sub`, the only identifier MH holds. Deliberately
    /// NOT short-circuited on the first match: finding a second is the whole
    /// point, and stopping early would silently convert an ambiguous answer
    /// into a confident wrong one. Runs once per media notification over a
    /// roster bounded by the participant cap.
    fn resolve_user(&self, user_id: &str) -> Result<(String, SenderId), SenderLookup> {
        let mut found: Option<(String, SenderId)> = None;
        for participant in self.participants.values() {
            if participant.user_id == user_id {
                if found.is_some() {
                    // Fail closed: never a coin-flip between two of one user's
                    // participants.
                    return Err(SenderLookup::Ambiguous);
                }
                found = Some((participant.participant_id.clone(), participant.sender_id));
            }
        }
        found.ok_or(SenderLookup::NotFound)
    }

    /// See [`MeetingMessage::MediaConnected`].
    async fn handle_media_connected(
        &mut self,
        user_id: &str,
        handler_id: &str,
        connection_id: String,
    ) -> (SenderLookup, Option<Unapplied>) {
        let (participant_id, sender_id) = match self.resolve_user(user_id) {
            Ok(found) => found,
            // S2: an ambiguous or unknown `sub` applies connectivity to NO
            // entry. The binding answer is the same fail-closed `0`.
            Err(SenderLookup::Ambiguous) => {
                return (SenderLookup::Ambiguous, Some(Unapplied::UserAmbiguous))
            }
            Err(lookup) => return (lookup, Some(Unapplied::ParticipantUnknown)),
        };
        // MH binds on THIS answer, whatever happens to connectivity below, so
        // the binding is recorded here and not from connectivity state (F-1).
        self.mh_bindings
            .record(handler_id, &connection_id, sender_id);
        let now = tokio::time::Instant::now();
        let applied = self.media.media_connected(
            &participant_id,
            handler_id,
            ConnectionKey::new(connection_id),
            now,
        );
        let unapplied = match applied {
            Ok(true) => {
                self.reconcile_media(Affected::All).await;
                self.flush_views().await;
                None
            }
            Ok(false) => None,
            Err(reason) => Some(reason),
        };
        (SenderLookup::Found(sender_id), unapplied)
    }

    /// See [`MeetingMessage::MediaDisconnected`].
    async fn handle_media_disconnected(
        &mut self,
        user_id: &str,
        handler_id: &str,
        connection_id: String,
    ) -> Option<Unapplied> {
        // Release the handed-out binding FIRST, before the roster lookup: the
        // holder may already be off the roster, and its connection closing is
        // exactly what frees its id for a later epoch (F-1). MH clears its own
        // binding before it sends this, so the release is correctly ordered.
        let released = self.mh_bindings.release(handler_id, &connection_id);
        // S12: participant-scoped. A `sub` that resolves to no single entry
        // holds no key this notification could name.
        let Ok((participant_id, _)) = self.resolve_user(user_id) else {
            // A departed holder's close is not "unknown" if it released a
            // binding: it did exactly the work this notification exists for.
            return (!released).then_some(Unapplied::UnknownConnection);
        };
        let now = tokio::time::Instant::now();
        match self.media.media_disconnected(
            &participant_id,
            handler_id,
            &ConnectionKey::new(connection_id),
            now,
        ) {
            Ok(true) => {
                self.reconcile_media(Affected::All).await;
                self.flush_views().await;
                None
            }
            Ok(false) => None,
            Err(reason) => Some(reason),
        }
    }

    /// Settle every establishing participant whose window is due.
    async fn settle_due(&mut self) {
        if self.media.settle_due(tokio::time::Instant::now()) {
            self.reconcile_media(Affected::All).await;
            self.flush_views().await;
        }
    }

    async fn flush_views(&mut self) {
        let roster: std::collections::HashMap<String, RosterEntry<'_>> = self
            .participants
            .values()
            .map(|p| {
                (
                    p.participant_id.clone(),
                    RosterEntry {
                        sender: p.sender_id,
                        connection: p.connection.as_ref(),
                        publishes: p.publishes(),
                        mute: p.source_mute(),
                    },
                )
            })
            .collect();
        self.media.flush(&roster).await;
    }

    /// Apply or lift a server mute (enforced at MH ingress, ADR-0036 §5, §7;
    /// story 2 R-8..R-11).
    ///
    /// # The order is the security property
    ///
    /// 1. **Authority first, fail closed** — the requester must be on the roster
    ///    AND hold host authority. Checked BEFORE the target is looked up, so a
    ///    non-host learns nothing about who is in the meeting: every request it
    ///    sends is `NotPermitted`, whatever it names. That ORDERING is the
    ///    control. The connection layer refuses non-hosts before any actor hop
    ///    and this check repeats it; the two are AND-composed, which is what
    ///    makes them fail closed when they read different vintages of the role
    ///    (`handle_reconnect` deliberately keeps the roster entry's `is_host`
    ///    rather than refreshing it from the reconnecting token).
    /// 2. **Then the target.** `UnknownTarget` is reachable only here, after
    ///    authority — i.e. only for a host, who inherently learns whether an id
    ///    is in their own meeting by the refusal-versus-success difference. The
    ///    connection returns the SAME wire error for both refusals, which is
    ///    defence in depth on top of the ordering, not what the ordering
    ///    depends on.
    ///
    /// # What a change does
    ///
    /// The single writer of `*_server_muted` and `server_muted_by`. Self-mute
    /// fields are never touched here, and `handle_self_mute` never touches these,
    /// so the two compose: clearing one never clears the other.
    ///
    /// PER KIND (story 3 R-15): only the kinds `change` names move, merged
    /// against the roster ([`MuteChange::apply`]); a request about one kind
    /// never changes the other kind's flag, directive, disposition, edges or
    /// slot states.
    ///
    /// Broadcast to EVERY participant — the target included, so the muted
    /// participant learns it cannot clear the mute itself (R-10, R-11). A
    /// moved kind K is ONE structural change in THIS actor turn (R-16):
    /// - K's stream turns DROP_AT_INGRESS in every handler's `source_streams`
    ///   (its slots and edges are KEPT, R-3 stability, so MH counts a patched
    ///   client's frames as `server_muted` drops), and the generation
    ///   advances where the snapshot changed;
    /// - the push workers re-push and confirm the echo (never awaited here);
    /// - the TARGET's send directive lists K with EMPTY targets (withdrawal,
    ///   counted `reason="server_muted"` on delivery), and the holders of this
    ///   source are re-emitted `AVATAR_SERVER_MUTED` for K.
    ///
    /// A server UNMUTE of K restores K's targets and disposition in the same
    /// way, REGARDLESS of client mute (ADR-0036 §5: the directive is blind to
    /// client mute; a client-muted participant keeps capture off locally). The
    /// disposition returns to PAUSED if K is client-muted, else FORWARD.
    ///
    /// # Ordering (R-18): the mute update BEFORE the directive change
    ///
    /// The broadcast below is enqueued BEFORE `reconcile_media`/`flush_views`,
    /// on the same per-participant mailbox the directive and `StreamAssignments`
    /// later ride, so every participant -- the target included -- receives
    /// the `ParticipantMuteUpdate` before the target's directive change and the
    /// holders' slot states. The client keys its "a host resumed your
    /// camera/mic" notice on that pair. Do not move any emission ahead of the
    /// broadcast.
    ///
    /// # Fan-out cost, and what the argument rests on
    ///
    /// The broadcast is O(roster) awaited sends on this actor. That is the SAME
    /// amplification class this actor already accepts, unbounded, for every
    /// `Joined`, `Disconnected`, `Left` and `Reconnected` (all through
    /// `broadcast_update`, none behind a limiter, `Reconnected` reachable on
    /// transport churn). A server mute is strictly cheaper: host-only authority,
    /// and a per-connection rate bound no looser than client mute's. Each send
    /// AWAITS a per-participant BOUNDED mailbox (it does not drop; the counted
    /// drop is the participant actor's wire hop). The client-mute fan-out,
    /// reachable by ANY participant, is the one roster fan-out under a
    /// per-meeting bound ([`Self::flush_client_mute`]). If the membership
    /// fan-out is ever bounded, this argument's comparator changes and it must
    /// be revisited.
    #[instrument(skip_all, fields(meeting_id = %self.meeting_id))]
    async fn handle_server_mute(
        &mut self,
        target_participant_id: &str,
        requester: &str,
        change: MuteChange,
    ) -> Result<KindDecision, ServerMuteRefusal> {
        // 1. Authority, before anything about the target is read.
        let is_host = self.participants.get(requester).is_some_and(|p| p.is_host);
        if !is_host {
            return Err(ServerMuteRefusal::NotPermitted);
        }

        // 2. The target.
        let Some(participant) = self.participants.get_mut(target_participant_id) else {
            return Err(ServerMuteRefusal::UnknownTarget);
        };
        // 3. The target's publish set: a request naming a kind the target does
        // not publish is refused WHOLE (screen included; nothing applied).
        // After authority and the lookup, so a non-host learns nothing.
        if !change.names_only(participant.publishes()) {
            return Err(ServerMuteRefusal::NotPublished);
        }
        // ONE step against the ONE store: flags and attribution move together,
        // per kind (S4); the whole-request short-circuit is derived from the
        // SAME decision, so it cannot drift from it.
        let decision = participant.media.server.apply(change, requester);
        if decision.is_unchanged() {
            return Ok(decision);
        }
        let sender = participant.sender_id;
        let update = Self::mute_changed(participant);

        // R-18: everyone, the target included, BEFORE any directive or slot
        // change below (same per-participant mailbox, FIFO).
        self.broadcast_update(None, update).await;
        // This frame carried the target's CURRENT client state too (a full
        // snapshot from the roster, `mute_changed`), so every other
        // participant now holds what a pending client-mute fan-out for it
        // would have sent: drop that one. DEPENDS on the server-mute frame
        // carrying the client fields -- trimming it to server fields breaks
        // this removal.
        self.client_mute_fanout.remove(target_participant_id);

        // The structural change: DROP_AT_INGRESS (or its lifting) in
        // `source_streams`, the target's directive withdrawn (or restored) for
        // the moved kind, and the holders' slot states -- the target's own view
        // is dirtied for its directive.
        self.publish_mute_census();
        self.reconcile_media(Affected::PublisherAndHoldersOf(sender))
            .await;
        self.flush_views().await;
        Ok(decision)
    }

    /// A participant's current mute state as the wire update: a FULL snapshot
    /// (every kind's client state, server mute and imposer), the ONE shape the
    /// server-mute broadcast and the client-mute fan-out use.
    fn mute_changed(participant: &Participant) -> ParticipantStateUpdate {
        ParticipantStateUpdate::MuteChanged {
            participant_id: participant.participant_id.clone(),
            media: participant.media.clone(),
        }
    }

    /// A participant asks the host(s) to lift its server mute (R-10).
    ///
    /// NOTIFIES ONLY. There is no write to any `*_server_muted` field on this
    /// path — the single writer is `handle_server_mute`, under host authority —
    /// so a participant can never clear its own server mute, however it asks.
    ///
    /// The relay is a FRESH `UnmuteRequest` whose `participant_id` is the
    /// authenticated requester's (passed in from the connection), so a value a
    /// client put in its own request can never be forwarded. Relayed only when
    /// the requester is actually server-muted in a kind it asked about.
    async fn handle_request_unmute(
        &mut self,
        participant_id: &str,
        kinds: RequestedKinds,
    ) -> UnmuteRelay {
        // Only the asked-for kinds that ARE server-muted are relayed (security
        // Gate 1 point 2): the relay is re-encoded from the parsed set, never
        // the client's bytes.
        let Some(relayed) = self
            .participants
            .get(participant_id)
            .map(|p| kinds.server_muted_among(p.media.server.muted_kinds()))
            .filter(|k| !k.is_empty())
        else {
            return UnmuteRelay::NotServerMuted;
        };
        let relay = proto_gen::dark_tower::signaling::v1::ServerMessage {
            message: Some(
                proto_gen::dark_tower::signaling::v1::server_message::Message::UnmuteRequest(
                    proto_gen::dark_tower::signaling::v1::UnmuteRequest {
                        participant_id: participant_id.to_string(),
                        media_kinds: relayed.kinds().map(|k| k.to_proto() as i32).collect(),
                    },
                ),
            ),
            trace_parent: String::new(),
            trace_state: String::new(),
        };
        let mut hosts = 0usize;
        for p in self.participants.values() {
            if !p.is_host || p.participant_id == participant_id {
                continue;
            }
            if let Some(conn) = &p.connection {
                let (trace_parent, trace_state) =
                    crate::webtransport::trace::inject_current_context();
                let mut message = relay.clone();
                message.trace_parent = trace_parent;
                message.trace_state = trace_state;
                if conn
                    .send(SignalingPayload::server_message(&message))
                    .await
                    .is_ok()
                {
                    hosts += 1;
                }
            }
        }
        if hosts == 0 {
            UnmuteRelay::NoHostConnected
        } else {
            UnmuteRelay::Relayed { hosts }
        }
    }

    /// Close the MC-side meeting (called by the system).
    ///
    /// Tells every participant the meeting ended, stops admitting, and leaves
    /// the loop through the clean exit path, which releases every handler. NOT
    /// the MH `EndMeeting` RPC; this RUNS it.
    async fn handle_close_meeting(&mut self, reason: &str) -> Result<(), McError> {
        info!(
            target: "mc.actor.meeting",
            meeting_id = %self.meeting_id,
            reason = %reason,
            participants = self.participants.len(),
            "Closing meeting"
        );

        self.is_shutting_down = true;

        // Notify all participants. The meeting is being torn down, so the roster
        // maps are dropped wholesale rather than removed one-by-one; emit the
        // leave metric per participant so meeting-end departures are counted like
        // every other removal (ADR-0032 metric-path completeness).
        for participant_id in self.participants.keys().cloned().collect::<Vec<_>>() {
            crate::observability::metrics::record_participant_leave(
                LeaveReason::MeetingEnded.label(),
            );
            self.broadcast_update(
                Some(&participant_id),
                ParticipantStateUpdate::Left {
                    participant_id: participant_id.clone(),
                    reason: LeaveReason::MeetingEnded,
                },
            )
            .await;
        }

        // Cancel all connections
        for managed in self.connections.values() {
            managed.handle.cancel();
        }

        // Leave through the CLEAN exit path — deliberately NOT by cancelling
        // this actor's token, which is the process-shutdown path.
        self.end_cause.get_or_insert(MeetingEndCause::Closed);

        Ok(())
    }

    /// Re-render and re-publish with the CURRENT per-kind mute projection —
    /// the one place it is computed, from the roster (the one home for mute).
    /// Every reconcile goes through here; `MeetingMedia::reconcile` takes the
    /// set as a required argument so no path can render without it.
    async fn reconcile_media(&mut self, affected: Affected) {
        let mute = SourceMuteView::from_map(
            self.participants
                .values()
                .map(|p| (p.sender_id, p.source_mute()))
                .collect(),
        );
        self.media.reconcile(affected, &mute).await;
    }

    /// Write this meeting's ABSOLUTE per-kind server-muted counts into the pod
    /// census and publish (freshness only — see `MutedSourceCensus` for the
    /// load-bearing caller).
    ///
    /// Each kind counts its OWN server mute. A participant muted on both kinds is one
    /// source on each. Each kind's server mute reaches MH as that kind's
    /// stream DROP_AT_INGRESS (`source_streams`), so the sum across kinds is
    /// the reading comparable with MH's `server_muted` drops.
    fn publish_mute_census(&self) {
        if let Some(wiring) = &self.wiring {
            let counts = MediaKindLabel::per_kind(|kind| {
                let n = self
                    .participants
                    .values()
                    .filter(|p| p.media.server.muted_kinds().is_muted(kind))
                    .count();
                u64::try_from(n).unwrap_or(u64::MAX)
            });
            wiring.muted_census.set(&self.meeting_id, counts);
            wiring.muted_census.publish();
        }
    }

    /// Hand this meeting's MH-side teardown to a task, as the actor exits
    /// through its clean path, and report the end to the controller.
    ///
    /// `Ended` is sent BEFORE the teardown task exists, on the same channel
    /// its completion will use, so the controller always sees `Ended` (and
    /// fences the id) before `TeardownComplete` (and lifts it).
    ///
    /// The task is detached rather than awaited, so a slow or unreachable
    /// handler never holds this actor — or a rejoin of this meeting id — for
    /// the teardown's full duration; the controller's fence covers ordering
    /// instead. There is no queue, so nothing to drop.
    ///
    /// **Concurrency, in numbers, for the correlated case** (every meeting
    /// ending at once against an unreachable handler — in practice a graceful
    /// shutdown). One teardown holds at most one outbound MH connection per
    /// handler of its frozen set (≤ 2, GC's selection), opened AFTER its push
    /// workers — one per handler, each able to be mid-connect — have drained,
    /// or, on the quiesce-timeout path, after one further bound during which a
    /// still-running worker may overlap its release. So the peak is the
    /// steady-state push-worker bound (`MC_MAX_MEETINGS` × 2 = 2,000 at the
    /// default) and at most twice that on the timeout path, each connection
    /// bounded by `MH_CONNECT_TIMEOUT` and each teardown by
    /// `teardown::TEARDOWN_MAX`. A cap here would lengthen the fence and the
    /// rejoin hold for every queued meeting without lowering the push-side
    /// peak, which exists whether or not a meeting is ending.
    ///
    /// **At graceful shutdown these tasks are WAITED FOR, up to a derived
    /// deadline.** Each is counted on the wiring's `TeardownTracker` from
    /// before its spawn until it ends; `main` drains the actor system and then
    /// settles the tracker, both inside `teardown::SHUTDOWN_RELEASE_BUDGET`,
    /// which is derived from the pod's `terminationGracePeriodSeconds` so the
    /// wait cannot hold the pod into SIGKILL. A healthy release fits with room
    /// to spare. A teardown still running at the deadline (an unreachable
    /// handler) is cut off by process exit, counted in `main`'s shutdown log
    /// line, and releases only the handlers it reached — the crash residual in
    /// `docs/TODO.md` ("A meeting whose MC never sends `EndMeeting` is never
    /// reclaimed").
    fn hand_off_teardown(&mut self, cause: MeetingEndCause) {
        let lifecycle = self.wiring.as_ref().map(|w| w.lifecycle.clone());
        if let Some(wiring) = &self.wiring {
            wiring.muted_census.set(&self.meeting_id, [0; 2]);
            wiring.muted_census.publish();
            let _ = wiring.lifecycle.send(MeetingLifecycleEvent::Ended {
                meeting_id: self.meeting_id.clone(),
                cause,
            });
        }
        let done = TeardownDone {
            lifecycle,
            meeting_id: self.meeting_id.clone(),
            cause,
        };
        let reason = match cause {
            MeetingEndCause::Empty | MeetingEndCause::Closed => {
                crate::media_routing::teardown::TeardownReason::MeetingEnded
            }
            MeetingEndCause::Shutdown => crate::media_routing::teardown::TeardownReason::Shutdown,
        };
        match self.media.take_teardown(reason) {
            Some(teardown) => {
                // Counted BEFORE the spawn, so a shutdown settle can never
                // observe zero while this release is handed off but unstarted.
                let in_flight = self.wiring.as_ref().map(|w| w.teardowns.begin());
                tokio::spawn(async move {
                    // Both dropped when this task ends, on every path.
                    let _done = done;
                    let _in_flight = in_flight;
                    crate::media_routing::teardown::run(teardown).await;
                });
            }
            // No handler was ever programmed: nothing to release, and the
            // teardown is complete now (`done` drops here).
            None => drop(done),
        }
    }

    /// Check for disconnect timeouts.
    async fn check_disconnect_timeouts(&mut self) {
        let now = Instant::now();
        let mut timed_out = Vec::new();

        for (participant_id, participant) in &self.participants {
            if participant.status == ParticipantStatus::Disconnected {
                if let Some(disconnected_at) = participant.disconnected_at {
                    if now.duration_since(disconnected_at) >= DISCONNECT_GRACE_PERIOD {
                        timed_out.push(participant_id.clone());
                    }
                }
            }
        }

        for participant_id in timed_out {
            info!(
                target: "mc.actor.meeting",
                meeting_id = %self.meeting_id,
                participant_id = %participant_id,
                "Disconnect grace period expired, removing participant"
            );

            // Grace expired — remove via the single choke-point
            // (emits mc_participant_leaves_total{reason="timeout"}).
            self.remove_and_broadcast_left(&participant_id, LeaveReason::Timeout)
                .await;
        }
    }

    /// Check health of connection actors.
    async fn check_connection_health(&mut self) {
        let mut finished = Vec::new();

        for (conn_id, managed) in &self.connections {
            if managed.task_handle.is_finished() {
                finished.push(conn_id.clone());
            }
        }

        for conn_id in finished {
            if let Some(managed) = self.connections.remove(&conn_id) {
                match managed.task_handle.await {
                    Ok(()) => {
                        debug!(
                            target: "mc.actor.meeting",
                            meeting_id = %self.meeting_id,
                            connection_id = %conn_id,
                            "Connection actor exited cleanly"
                        );
                    }
                    Err(join_error) => {
                        if join_error.is_panic() {
                            error!(
                                target: "mc.actor.meeting",
                                meeting_id = %self.meeting_id,
                                connection_id = %conn_id,
                                error = ?join_error,
                                "Connection actor panicked"
                            );
                            self.metrics.record_panic(ActorType::Participant);
                        }
                    }
                }

                // Mark participant as disconnected. A task that finished/panicked
                // without a transport-classified clean close is an abrupt loss →
                // ConnectionLost (grace-preserving). If the ParticipantActor's own
                // exit notification (which carries the authoritative cause) already
                // ran, this call is an idempotent no-op.
                self.handle_disconnect(
                    &conn_id,
                    &managed.participant_id,
                    DisconnectCause::ConnectionLost,
                )
                .await;
            }
        }
    }

    /// Send an update to every connected participant except `except` (`None`:
    /// everyone). THE one roster fan-out loop: membership, server mute and the
    /// client-mute flush all go through it.
    ///
    /// Each send AWAITS the recipient's bounded mailbox; it does not drop. The
    /// counted drop is the next hop (the participant actor `try_send`s to the
    /// wire, `mc_participant_outbound_messages_dropped_total{payload_kind}`).
    async fn broadcast_update(&self, except: Option<&str>, update: ParticipantStateUpdate) {
        for participant in self.participants.values() {
            if Some(participant.participant_id.as_str()) != except {
                if let Some(conn) = &participant.connection {
                    let _ = conn.send_update(update.clone()).await;
                }
            }
        }
    }

    /// Perform graceful shutdown.
    async fn graceful_shutdown(&mut self) {
        info!(
            target: "mc.actor.meeting",
            meeting_id = %self.meeting_id,
            participants = self.participants.len(),
            connections = self.connections.len(),
            "Performing graceful shutdown"
        );

        self.is_shutting_down = true;

        // Cancel all connection actors
        for managed in self.connections.values() {
            managed.handle.cancel();
        }

        // Wait for connections to complete
        for (conn_id, managed) in self.connections.drain() {
            match tokio::time::timeout(Duration::from_secs(5), managed.task_handle).await {
                Ok(Ok(())) => {
                    debug!(
                        target: "mc.actor.meeting",
                        meeting_id = %self.meeting_id,
                        connection_id = %conn_id,
                        "Connection completed cleanly"
                    );
                }
                Ok(Err(e)) => {
                    warn!(
                        target: "mc.actor.meeting",
                        meeting_id = %self.meeting_id,
                        connection_id = %conn_id,
                        error = ?e,
                        "Connection task panicked during shutdown"
                    );
                }
                Err(_) => {
                    warn!(
                        target: "mc.actor.meeting",
                        meeting_id = %self.meeting_id,
                        connection_id = %conn_id,
                        "Connection shutdown timed out"
                    );
                }
            }
        }

        info!(
            target: "mc.actor.meeting",
            meeting_id = %self.meeting_id,
            "Graceful shutdown complete"
        );
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    use proto_gen::dark_tower::signaling::v1::ClientMediaState;

    /// Did MC record `kind` as client-muted?
    fn client_muted_now(p: &ParticipantInfo, kind: MediaKindLabel) -> bool {
        p.media.client.get(kind) == ClientKindState::ClientMuted
    }

    /// Did MC record `kind` as server-muted?
    fn server_muted_now(p: &ParticipantInfo, kind: MediaKindLabel) -> bool {
        p.media.server.muted_kinds().is_muted(kind)
    }

    /// Both served kinds: what the in-crate actor tests publish.
    fn av() -> PublishSet {
        PublishSet::of(&MediaKindLabel::ALL)
    }

    /// A mute request naming BOTH kinds explicitly.
    fn both(audio: bool, video: bool) -> MuteChange {
        MuteChange::from_asks([Some(audio), Some(video)]).unwrap()
    }

    /// A mute request naming ONE kind; the other is absent (unchanged).
    fn only(kind: MediaKindLabel, muted: bool) -> MuteChange {
        MuteChange::one(kind, muted)
    }

    /// A client-mute change as the connection hands it over: proven against
    /// the test participants' publish set (both kinds).
    fn published(change: MuteChange) -> PublishedMuteChange {
        change.against(av()).unwrap()
    }

    /// `kind`'s client state on a wire snapshot list (the shared reader:
    /// exactly one entry per kind).
    fn wire_client(
        states: &[proto_gen::dark_tower::signaling::v1::MediaKindState],
        kind: MediaKindLabel,
    ) -> ClientMediaState {
        mc_test_utils::media::client_state_of(states, kind.to_proto())
    }

    /// `kind`'s server mute on a wire snapshot list.
    fn wire_server(
        states: &[proto_gen::dark_tower::signaling::v1::MediaKindState],
        kind: MediaKindLabel,
    ) -> bool {
        mc_test_utils::media::server_mute_of(states, kind.to_proto()).0
    }

    /// The (audio, video) client states on a wire snapshot list.
    fn client_pair(
        states: &[proto_gen::dark_tower::signaling::v1::MediaKindState],
    ) -> (ClientMediaState, ClientMediaState) {
        mc_test_utils::media::client_states_av(states)
    }

    const LIVE: ClientMediaState = ClientMediaState::Live;
    const MUTED: ClientMediaState = ClientMediaState::ClientMuted;

    /// `kind`'s imposer on a wire snapshot list ("" = not server-muted).
    fn wire_by(
        states: &[proto_gen::dark_tower::signaling::v1::MediaKindState],
        kind: MediaKindLabel,
    ) -> String {
        mc_test_utils::media::server_mute_of(states, kind.to_proto()).1
    }

    /// Test secret for session binding (32 bytes as required by ADR-0023).
    fn test_secret() -> SecretBox<Vec<u8>> {
        SecretBox::new(Box::new(vec![0u8; 32]))
    }

    /// Spawn a meeting actor, panicking if KEK generation fails.
    ///
    /// `MeetingActor::spawn` is fallible because it generates the meeting KEK
    /// from the system CSPRNG and fails closed if that fails (ADR-0036 §4).
    /// Tests treat that as unreachable rather than threading a `Result`.
    fn must_spawn(
        meeting_id: String,
        cancel_token: CancellationToken,
        metrics: Arc<ActorMetrics>,
        controller_metrics: Arc<ControllerMetrics>,
        master_secret: SecretBox<Vec<u8>>,
    ) -> (MeetingActorHandle, JoinHandle<()>) {
        MeetingActor::spawn(
            meeting_id,
            cancel_token,
            metrics,
            controller_metrics,
            master_secret,
            crate::media_admission::fixtures::kek_lifecycle(),
        )
        .expect("system CSPRNG must be available in tests")
    }

    /// A syntactically valid identity key for in-crate actor tests.
    ///
    /// Exact-length only — MC does no curve validation, so a fixed pattern is
    /// sufficient. Accepting it asserts nothing about identity: with no `cnf`
    /// binding this is trust-on-first-use, giving same-keyholder consistency
    /// and never a verified identity.
    fn test_identity_key() -> Option<IdentityPublicKey> {
        crate::media_admission::fixtures::sample_identity_key()
    }

    /// Media-routing inputs for in-crate actor tests — the shared fixture.
    fn test_media() -> JoinMedia {
        JoinMedia {
            declared: ClientMediaStates::live(av()),
            ..super::super::meeting_media::test_join_media()
        }
    }

    #[tokio::test]
    async fn test_meeting_actor_spawn() {
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, _task) = must_spawn(
            "meeting-123".to_string(),
            cancel_token.clone(),
            metrics,
            controller_metrics,
            test_secret(),
        );

        assert_eq!(handle.meeting_id(), "meeting-123");
        assert!(!handle.is_cancelled());

        handle.cancel();
        assert!(handle.is_cancelled());
    }

    #[tokio::test]
    async fn test_meeting_actor_join() {
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, _task) = must_spawn(
            "meeting-join-test".to_string(),
            cancel_token.clone(),
            metrics,
            controller_metrics,
            test_secret(),
        );

        let result = handle
            .connection_join(
                "conn-1".to_string(),
                "user-1".to_string(),
                "part-1".to_string(),
                String::new(),
                false, // not host
                test_identity_key(),
                None,
                test_media(),
            )
            .await;

        assert!(result.is_ok());
        let join_result = result.unwrap();
        assert_eq!(join_result.participant_id, "part-1");
        assert!(!join_result.correlation_id.is_empty());
        // Binding token should be 64 hex chars (HMAC-SHA256)
        assert_eq!(join_result.binding_token.len(), 64);

        handle.cancel();
    }

    #[tokio::test]
    async fn test_meeting_actor_duplicate_join() {
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, _task) = must_spawn(
            "meeting-dup-test".to_string(),
            cancel_token.clone(),
            metrics,
            controller_metrics,
            test_secret(),
        );

        let result = handle
            .connection_join(
                "conn-1".to_string(),
                "user-1".to_string(),
                "part-1".to_string(),
                String::new(),
                false,
                test_identity_key(),
                None,
                test_media(),
            )
            .await;
        assert!(result.is_ok());

        let result = handle
            .connection_join(
                "conn-2".to_string(),
                "user-1".to_string(),
                "part-1".to_string(),
                String::new(),
                false,
                test_identity_key(),
                None,
                test_media(),
            )
            .await;
        assert!(matches!(result, Err(McError::Conflict(_))));

        handle.cancel();
    }

    #[tokio::test]
    async fn test_meeting_actor_get_state() {
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, _task) = must_spawn(
            "meeting-state-test".to_string(),
            cancel_token.clone(),
            metrics,
            controller_metrics,
            test_secret(),
        );

        // Join a participant
        let _ = handle
            .connection_join(
                "conn-1".to_string(),
                "user-1".to_string(),
                "part-1".to_string(),
                String::new(),
                false,
                test_identity_key(),
                None,
                test_media(),
            )
            .await;

        let state = handle.get_state().await;
        assert!(state.is_ok());
        let state = state.unwrap();
        assert_eq!(state.meeting_id, "meeting-state-test");
        assert_eq!(state.participants.len(), 1);
        assert!(!state.is_shutting_down);

        handle.cancel();
    }

    /// The registered display_name from the (already-validated, already-bounded)
    /// token claim must flow through the join path onto the participant's own
    /// roster entry — keyed by participant, NOT derived from join order/count.
    /// Distinct names make this discriminating: a position/count-derived label
    /// (the MINOR-003 stopgap) would fail here.
    #[tokio::test]
    async fn test_handle_join_uses_claim_display_name_per_participant() {
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, _task) = must_spawn(
            "meeting-name-test".to_string(),
            cancel_token.clone(),
            metrics,
            controller_metrics,
            test_secret(),
        );

        handle
            .connection_join(
                "conn-1".to_string(),
                "user-1".to_string(),
                "part-1".to_string(),
                "Alice".to_string(),
                false,
                test_identity_key(),
                None,
                test_media(),
            )
            .await
            .expect("join 1");
        handle
            .connection_join(
                "conn-2".to_string(),
                "user-2".to_string(),
                "part-2".to_string(),
                "Bob".to_string(),
                false,
                test_identity_key(),
                None,
                test_media(),
            )
            .await
            .expect("join 2");

        let state = handle.get_state().await.unwrap();
        let alice = state
            .participants
            .iter()
            .find(|p| p.participant_id == "part-1")
            .expect("part-1 present");
        let bob = state
            .participants
            .iter()
            .find(|p| p.participant_id == "part-2")
            .expect("part-2 present");
        assert_eq!(
            alice.display_name, "Alice",
            "each participant must render its OWN registered name"
        );
        assert_eq!(bob.display_name, "Bob");

        handle.cancel();
    }

    /// Genuine-absence fallback: an EMPTY claim display_name (the only case the
    /// generic label is allowed) yields the `Participant N` stopgap. Exact match,
    /// not `contains`, so the fallback shape is pinned.
    #[tokio::test]
    async fn test_handle_join_empty_claim_falls_back_to_generic_label() {
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, _task) = must_spawn(
            "meeting-fallback-test".to_string(),
            cancel_token.clone(),
            metrics,
            controller_metrics,
            test_secret(),
        );

        handle
            .connection_join(
                "conn-1".to_string(),
                "user-1".to_string(),
                "part-1".to_string(),
                String::new(), // genuine absence → generic fallback
                false,
                test_identity_key(),
                None,
                test_media(),
            )
            .await
            .expect("join");

        let state = handle.get_state().await.unwrap();
        assert_eq!(state.participants.len(), 1);
        assert_eq!(
            state.participants[0].display_name, "Participant 1",
            "empty claim must fall back to the generic 'Participant N' label"
        );

        handle.cancel();
    }

    #[tokio::test]
    async fn test_meeting_actor_leave() {
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, task) = must_spawn(
            "meeting-leave-test".to_string(),
            cancel_token.clone(),
            metrics,
            controller_metrics,
            test_secret(),
        );

        // Join a participant
        let _ = handle
            .connection_join(
                "conn-1".to_string(),
                "user-1".to_string(),
                "part-1".to_string(),
                String::new(),
                false,
                test_identity_key(),
                None,
                test_media(),
            )
            .await;

        // Leave
        let result = handle.participant_leave("part-1".to_string()).await;
        assert!(result.is_ok());

        // The only participant is gone, so the meeting has ENDED (story 2 task
        // 12; ADR-0010 §3). A real observable: the actor's task completes
        // through its clean exit path — which it does only because the roster
        // emptied, i.e. only because the removal above happened. Not "the
        // roster is no longer readable", which would pass for any reason the
        // actor stopped answering.
        tokio::time::timeout(Duration::from_secs(30), task)
            .await
            .expect("the meeting ends when its last participant leaves")
            .expect("the meeting actor exits cleanly, not by panic");
    }

    /// `sender_id` continuity across an ADR-0023 reconnect.
    ///
    /// # What this exercises, and what it does NOT prove about production
    ///
    /// This drives the **actor-level** `handle_reconnect`. That path is not yet
    /// wired to the WebTransport accept path — `connection_reconnect` has no
    /// caller outside `actors/` — so it is **not production-reachable today**;
    /// a browser "reconnect" is currently a fresh join with a fresh
    /// `participant_id` and therefore a NEW `sender_id`. That is correct
    /// (non-recycling), just not continuity. Continuity is proven here at the
    /// actor seam, and the test is named for that rather than for a shipped
    /// guarantee.
    ///
    /// # Continuity is not recycling
    ///
    /// Keeping an id across a reconnect does not violate R-35: the participant
    /// never left the roster, so the id was never released and the allocator is
    /// never consulted. The continuity key is the ADR-0023 `correlation_id`
    /// plus its HMAC `binding_token` — never a client-supplied participant id,
    /// which a caller could forge to capture someone else's `sender_id`.
    #[tokio::test]
    async fn sender_id_continuity_across_actor_level_reconnect() {
        let (handle, _task) = must_spawn(
            "meeting-sender-id-continuity".to_string(),
            CancellationToken::new(),
            ActorMetrics::new(),
            ControllerMetrics::new(),
            test_secret(),
        );

        let join = handle
            .connection_join(
                "conn-1".to_string(),
                "user-1".to_string(),
                "part-1".to_string(),
                String::new(),
                false,
                test_identity_key(),
                None,
                test_media(),
            )
            .await
            .expect("join succeeds");
        let original_sender_id = join.sender_id;

        // Mark the connection lost so the participant enters the ADR-0023 grace
        // period rather than being removed.
        handle
            .connection_disconnected(
                "conn-1".to_string(),
                "part-1".to_string(),
                DisconnectCause::ConnectionLost,
            )
            .await
            .expect("disconnect notification delivered");

        handle
            .connection_reconnect(
                "conn-2".to_string(),
                join.correlation_id.clone(),
                join.binding_token.clone(),
                ClientMediaStates::live(av()),
                None,
            )
            .await
            .expect("reconnect within grace succeeds");

        let state = handle.get_state().await.expect("get_state");
        let entry = state
            .participants
            .iter()
            .find(|p| p.participant_id == "part-1")
            .expect("participant survived the reconnect");

        assert_eq!(
            entry.sender_id, original_sender_id,
            "a reconnecting participant keeps its sender_id: it never left the roster, so the id \
             was never released. Issuing a new one here would burn namespace on every transient \
             network blip."
        );
    }

    #[tokio::test]
    async fn test_meeting_actor_reconnect() {
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, _task) = must_spawn(
            "meeting-reconnect-test".to_string(),
            cancel_token.clone(),
            metrics,
            controller_metrics,
            test_secret(),
        );

        // Join
        let join_result = handle
            .connection_join(
                "conn-1".to_string(),
                "user-1".to_string(),
                "part-1".to_string(),
                String::new(),
                false,
                test_identity_key(),
                None,
                test_media(),
            )
            .await
            .unwrap();

        // Disconnect
        let _ = handle
            .connection_disconnected(
                "conn-1".to_string(),
                "part-1".to_string(),
                DisconnectCause::ConnectionLost,
            )
            .await;

        // Reconnect with valid binding token
        let result = handle
            .connection_reconnect(
                "conn-2".to_string(),
                join_result.correlation_id.clone(),
                join_result.binding_token.clone(),
                ClientMediaStates::live(av()),
                None,
            )
            .await;

        assert!(result.is_ok());
        let reconnect_result = result.unwrap();
        assert_eq!(reconnect_result.participant_id, "part-1");
        // New correlation ID should be different (rotation per ADR-0023)
        assert_ne!(
            reconnect_result.new_correlation_id,
            join_result.correlation_id
        );

        handle.cancel();
    }

    #[tokio::test]
    async fn test_meeting_actor_reconnect_invalid_token() {
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, _task) = must_spawn(
            "meeting-reconnect-invalid".to_string(),
            cancel_token.clone(),
            metrics,
            controller_metrics,
            test_secret(),
        );

        // Join
        let join_result = handle
            .connection_join(
                "conn-1".to_string(),
                "user-1".to_string(),
                "part-1".to_string(),
                String::new(),
                false,
                test_identity_key(),
                None,
                test_media(),
            )
            .await
            .unwrap();

        // Disconnect
        let _ = handle
            .connection_disconnected(
                "conn-1".to_string(),
                "part-1".to_string(),
                DisconnectCause::ConnectionLost,
            )
            .await;

        // Reconnect with invalid binding token
        let result = handle
            .connection_reconnect(
                "conn-2".to_string(),
                join_result.correlation_id.clone(),
                "invalid-token".to_string(),
                ClientMediaStates::live(av()),
                None,
            )
            .await;

        // Should fail with InvalidToken error
        assert!(matches!(
            result,
            Err(McError::SessionBinding(
                crate::errors::SessionBindingError::InvalidToken
            ))
        ));

        handle.cancel();
    }

    // ------------------------------------------------------------------
    // Client-mute roster fan-out (story 3 R-15 fix; reverses story-2 S-2)
    // ------------------------------------------------------------------

    use super::super::client_mute_fanout::{
        CLIENT_MUTE_FANOUT_BURST, CLIENT_MUTE_FANOUT_REFILL_INTERVAL_MS,
    };
    use proto_gen::dark_tower::signaling::v1::server_message::Message as Wire;
    use proto_gen::dark_tower::signaling::v1::ParticipantMuteUpdate as WireMute;

    /// A meeting of `ids` (the first is the host), each with its own outbound
    /// stream, roomy enough that nothing in these tests is dropped at the
    /// participant actor's wire hop (asserted where it matters).
    async fn fanout_meeting(
        meeting_id: &str,
        ids: &[&str],
    ) -> (
        MeetingActorHandle,
        JoinHandle<()>,
        HashMap<String, tokio::sync::mpsc::Receiver<bytes::Bytes>>,
    ) {
        fanout_meeting_declared(meeting_id, ids, |_| test_media()).await
    }

    /// [`fanout_meeting`] with the `i`th joiner's join media from `media(i)`.
    async fn fanout_meeting_declared(
        meeting_id: &str,
        ids: &[&str],
        media: impl Fn(usize) -> JoinMedia,
    ) -> (
        MeetingActorHandle,
        JoinHandle<()>,
        HashMap<String, tokio::sync::mpsc::Receiver<bytes::Bytes>>,
    ) {
        let (handle, task) = must_spawn(
            meeting_id.to_string(),
            CancellationToken::new(),
            ActorMetrics::new(),
            ControllerMetrics::new(),
            test_secret(),
        );
        let mut rxs = HashMap::new();
        for (i, id) in ids.iter().enumerate() {
            let (tx, rx) = tokio::sync::mpsc::channel(1024);
            join_as_with(&handle, id, i == 0, tx, media(i)).await;
            rxs.insert((*id).to_string(), rx);
        }
        (handle, task, rxs)
    }

    async fn join_as(
        handle: &MeetingActorHandle,
        id: &str,
        is_host: bool,
        tx: tokio::sync::mpsc::Sender<bytes::Bytes>,
    ) -> JoinResult {
        join_as_with(handle, id, is_host, tx, test_media()).await
    }

    async fn join_as_with(
        handle: &MeetingActorHandle,
        id: &str,
        is_host: bool,
        tx: tokio::sync::mpsc::Sender<bytes::Bytes>,
        media: JoinMedia,
    ) -> JoinResult {
        handle
            .connection_join(
                format!("conn-{id}"),
                format!("user-{id}"),
                id.to_string(),
                String::new(),
                is_host,
                test_identity_key(),
                Some(tx),
                media,
            )
            .await
            .unwrap()
    }

    async fn self_mute(handle: &MeetingActorHandle, id: &str, change: MuteChange) {
        handle
            .update_self_mute(id.to_string(), published(change))
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }

    /// The next mute update on `rx` (any subject).
    async fn next_mute(rx: &mut tokio::sync::mpsc::Receiver<bytes::Bytes>) -> WireMute {
        let Wire::ParticipantMuteUpdate(u) =
            next_matching(rx, |m| matches!(m, Wire::ParticipantMuteUpdate(_))).await
        else {
            unreachable!()
        };
        u
    }

    /// Every frame on `rx` up to and including the mute update about `fence`
    /// -- the ordering fence: FIFO fan-out puts it after everything owed
    /// before it, so nothing is inferred from a timed silence.
    async fn frames_until_mute_of(
        rx: &mut tokio::sync::mpsc::Receiver<bytes::Bytes>,
        fence: &str,
    ) -> Vec<Wire> {
        let mut seen = Vec::new();
        loop {
            let m = next_matching(rx, |_| true).await;
            let done = matches!(&m, Wire::ParticipantMuteUpdate(u) if u.participant_id == fence);
            seen.push(m);
            if done {
                return seen;
            }
        }
    }

    fn mutes_about<'a>(frames: &'a [Wire], subject: &'a str) -> impl Iterator<Item = &'a WireMute> {
        frames.iter().filter_map(move |m| match m {
            Wire::ParticipantMuteUpdate(u) if u.participant_id == subject => Some(u),
            _ => None,
        })
    }

    /// Spend every per-meeting token: `churner` makes exactly
    /// `CLIENT_MUTE_FANOUT_BURST` real transitions with the clock paused, each
    /// fanned out at once. The NEXT transition anyone makes is deferred.
    async fn exhaust_fanout_bucket(handle: &MeetingActorHandle, churner: &str) {
        for i in 0..CLIENT_MUTE_FANOUT_BURST {
            self_mute(handle, churner, only(MediaKindLabel::Audio, i % 2 == 0)).await;
        }
    }

    /// A REAL client-mute transition reaches every OTHER participant as a full
    /// snapshot; a no-op emits nothing; the subject gets no copy. Ordering
    /// only: a no-op emission would be distinguishable from the next real
    /// frame, and the subject's absence is fenced by a later broadcast to it.
    #[tokio::test]
    async fn a_real_client_mute_fans_out_to_everyone_else_and_a_no_op_to_no_one() {
        let (handle, _task, mut rx) = fanout_meeting("cm-fanout", &["a", "b"]).await;
        let mut rx_a = rx.remove("a").unwrap();
        let mut rx_b = rx.remove("b").unwrap();

        self_mute(&handle, "a", only(MediaKindLabel::Audio, true)).await;
        // A no-op (same fact again): must emit NOTHING.
        let noop = handle
            .update_self_mute(
                "a".to_string(),
                published(only(MediaKindLabel::Audio, true)),
            )
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(noop.is_unchanged());
        // A video-only transition: audio's fact must ride along unchanged.
        self_mute(&handle, "a", only(MediaKindLabel::Video, true)).await;

        let first = next_mute(&mut rx_b).await;
        assert_eq!(first.participant_id, "a");
        assert_eq!(
            client_pair(&first.media_states),
            (MUTED, LIVE),
            "the first frame is the real audio transition; video is still live"
        );
        for k in MediaKindLabel::ALL {
            assert!(!wire_server(&first.media_states, k));
            assert!(wire_by(&first.media_states, k).is_empty());
        }
        let second = next_mute(&mut rx_b).await;
        assert_eq!(
            client_pair(&second.media_states),
            (MUTED, MUTED),
            "the NEXT frame is the video transition -- a no-op emission would \
             have arrived here as (MUTED, LIVE)"
        );

        // Fence for the subject: B's transition is broadcast to A, so the first
        // mute update A sees must be B's -- A got no copy of its own.
        self_mute(&handle, "b", only(MediaKindLabel::Audio, true)).await;
        assert_eq!(next_mute(&mut rx_a).await.participant_id, "b");
    }

    /// ADR-0036 §5 amendment 2026-10-25: the state is DEFINITE from the join
    /// declaration (the R-15 "first unmuted report" case no longer exists), so
    /// an explicit unmute of an already-live kind is a no-op that emits
    /// nothing, and the next real transition is the peer's next frame.
    #[tokio::test]
    async fn the_join_declaration_is_the_state_and_a_live_unmute_is_a_no_op() {
        let (handle, _task, mut rx) = fanout_meeting("cm-definite", &["a", "b"]).await;
        let mut rx_b = rx.remove("b").unwrap();
        let a = state_of(&handle, "a").await;
        for k in MediaKindLabel::ALL {
            assert_eq!(a.media.client.get(k), ClientKindState::Live, "{k:?}");
        }

        let d = handle
            .update_self_mute("a".to_string(), published(both(false, false)))
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(d.is_unchanged(), "LIVE -> LIVE moves nothing");
        // Fence: the next frame b sees about a is the REAL transition.
        self_mute(&handle, "a", only(MediaKindLabel::Video, true)).await;
        let u = next_mute(&mut rx_b).await;
        assert_eq!(u.participant_id, "a");
        assert_eq!(client_pair(&u.media_states), (LIVE, MUTED));
    }

    /// Security Gate 1 (2): a client-mute frame is a FULL snapshot, so it
    /// carries the subject's CURRENT server mute and its imposer -- it can
    /// never regress a recipient's server-mute view; and after a server-mute
    /// and a client-mute in consecutive turns the last frame reflects both.
    #[tokio::test]
    async fn a_client_mute_frame_carries_the_current_server_mute_and_its_imposer() {
        let (handle, _task, mut rx) = fanout_meeting("cm-server-wins", &["host", "a", "b"]).await;
        let mut rx_b = rx.remove("b").unwrap();
        handle
            .server_mute(
                "a".to_string(),
                "host".to_string(),
                only(MediaKindLabel::Audio, true),
            )
            .await
            .unwrap()
            .unwrap();
        self_mute(&handle, "a", only(MediaKindLabel::Video, true)).await;

        let server = next_mute(&mut rx_b).await;
        assert!(wire_server(&server.media_states, MediaKindLabel::Audio));
        assert_eq!(
            wire_client(&server.media_states, MediaKindLabel::Video),
            LIVE,
            "not yet muted"
        );
        let client = next_mute(&mut rx_b).await;
        assert_eq!(client.participant_id, "a");
        assert!(
            wire_server(&client.media_states, MediaKindLabel::Audio),
            "the server mute rides the client frame"
        );
        assert_eq!(wire_by(&client.media_states, MediaKindLabel::Audio), "host");
        assert_eq!(wire_by(&client.media_states, MediaKindLabel::Video), "");
        assert_eq!(
            wire_client(&client.media_states, MediaKindLabel::Video),
            MUTED
        );
    }

    /// The per-meeting bound, at ACTOR level with the clock paused (no QUIC,
    /// no rig timeouts for auto-advance to fire). Many transitions from
    /// several subjects:
    /// - coalescing HAPPENED: each observer receives STRICTLY fewer updates
    ///   per subject than that subject made (goes red by itself with the burst
    ///   at `u32::MAX`), and `awaiting_token` moved;
    /// - nobody starves: every observer receives every subject;
    /// - nothing is lost: no wire-hop drop, and each subject's LAST frame
    ///   equals MC's final state (reached by the paused clock advancing to the
    ///   fan-out wake, then a FIFO fence).
    #[tokio::test(start_paused = true)]
    async fn a_client_mute_storm_is_coalesced_per_meeting_and_converges() {
        use common::observability::testing::MetricAssertion;
        /// Odd, so each subject ends muted; together well past the burst.
        const TOGGLES: u32 = 11;
        let subjects = ["s1", "s2", "s3"];
        let (handle, _task, mut rx) =
            fanout_meeting("cm-storm", &["o1", "s1", "s2", "s3", "o2", "fence"]).await;
        let snap = MetricAssertion::snapshot();

        for i in 0..TOGGLES {
            for s in subjects {
                self_mute(&handle, s, only(MediaKindLabel::Audio, i % 2 == 0)).await;
            }
        }
        assert!(
            snap.counter("mc_media_client_mute_fanouts_total")
                .with_labels(&[("outcome", "awaiting_token")])
                .delta()
                > 0,
            "positive control: the per-meeting bound engaged"
        );
        self_mute(&handle, "fence", only(MediaKindLabel::Audio, true)).await;

        for observer in ["o1", "o2"] {
            let frames = frames_until_mute_of(rx.get_mut(observer).unwrap(), "fence").await;
            for s in subjects {
                let got: Vec<&WireMute> = mutes_about(&frames, s).collect();
                assert!(!got.is_empty(), "{observer} never heard {s}: starvation");
                assert!(
                    got.len() < TOGGLES as usize,
                    "{observer} got {} updates for {s}'s {TOGGLES} transitions: nothing coalesced",
                    got.len()
                );
                let last = got.last().unwrap();
                assert_eq!(
                    client_pair(&last.media_states),
                    (MUTED, LIVE),
                    "{observer}'s last frame for {s} is MC's final state"
                );
            }
        }
        snap.counter("mc_participant_outbound_messages_dropped_total")
            .with_labels(&[("payload_kind", "participant_update_muted")])
            .assert_delta(0);
    }

    /// A participant that joins while a subject is pending: its roster
    /// snapshot already carries the subject's current fact, and the later
    /// flush (to everyone present AT FLUSH TIME) reaches it with the same.
    #[tokio::test(start_paused = true)]
    async fn a_joiner_during_a_pending_fan_out_gets_the_current_fact_twice_consistently() {
        let (handle, _task, _rx) = fanout_meeting("cm-join-pending", &["churn", "s"]).await;
        exhaust_fanout_bucket(&handle, "churn").await;
        self_mute(&handle, "s", only(MediaKindLabel::Video, true)).await; // pending

        let (tx, mut rx_j) = tokio::sync::mpsc::channel(1024);
        let joined = join_as(&handle, "j", false, tx).await;
        let s = joined
            .participants
            .iter()
            .find(|p| p.participant_id == "s")
            .unwrap();
        assert!(client_muted_now(s, MediaKindLabel::Video));

        let later = next_mute(&mut rx_j).await;
        assert_eq!(later.participant_id, "s", "FIFO: s was pending first");
        assert_eq!(
            wire_client(&later.media_states, MediaKindLabel::Video),
            MUTED
        );
    }

    /// A subject that leaves while pending is told about to no one after its
    /// `ParticipantLeft`.
    #[tokio::test(start_paused = true)]
    async fn a_pending_subject_that_leaves_is_never_fanned_out_after_it_left() {
        let (handle, _task, mut rx) =
            fanout_meeting("cm-leave-pending", &["churn", "a", "b", "fence"]).await;
        use common::observability::testing::MetricAssertion;
        exhaust_fanout_bucket(&handle, "churn").await;
        let snap = MetricAssertion::snapshot();
        self_mute(&handle, "a", only(MediaKindLabel::Audio, true)).await; // pending
        assert!(
            snap.counter("mc_media_client_mute_fanouts_total")
                .with_labels(&[("outcome", "awaiting_token")])
                .delta()
                >= 1,
            "positive control: a was left pending"
        );
        handle.participant_leave("a".to_string()).await.unwrap();
        self_mute(&handle, "fence", only(MediaKindLabel::Audio, true)).await;

        let frames = frames_until_mute_of(rx.get_mut("b").unwrap(), "fence").await;
        assert!(
            frames
                .iter()
                .any(|m| matches!(m, Wire::ParticipantLeft(l) if l.participant_id == "a")),
            "b saw a leave"
        );
        // Over ALL frames up to the fence: nothing else announces a's mute, so
        // zero proves a was pending AND was removed (not fanned out early).
        assert_eq!(
            mutes_about(&frames, "a").count(),
            0,
            "a's pending fan-out was dropped with a, never sent"
        );
    }

    /// A server-mute broadcast of a pending subject carries its current client
    /// facts, so the subject leaves the queue: no client-only frame follows
    /// that could contradict it.
    #[tokio::test(start_paused = true)]
    async fn a_server_mute_of_a_pending_subject_supersedes_its_client_fan_out() {
        let (handle, _task, mut rx) =
            fanout_meeting("cm-server-dedupe", &["host", "s", "b", "fence"]).await;
        exhaust_fanout_bucket(&handle, "host").await;
        self_mute(&handle, "s", only(MediaKindLabel::Video, true)).await; // pending
        handle
            .server_mute(
                "s".to_string(),
                "host".to_string(),
                only(MediaKindLabel::Audio, true),
            )
            .await
            .unwrap()
            .unwrap();
        self_mute(&handle, "fence", only(MediaKindLabel::Audio, true)).await;

        let frames = frames_until_mute_of(rx.get_mut("b").unwrap(), "fence").await;
        let about_s: Vec<&WireMute> = mutes_about(&frames, "s").collect();
        assert_eq!(about_s.len(), 1, "only the server-mute frame: {about_s:?}");
        assert!(wire_server(&about_s[0].media_states, MediaKindLabel::Audio));
        assert_eq!(
            wire_client(&about_s[0].media_states, MediaKindLabel::Video),
            MUTED,
            "client state carried"
        );
    }

    /// Security Gate 1 (8), re-pointed 2026-10-25: a join storm enqueues
    /// NOTHING on the per-meeting fan-out (each joiner's state rides its own
    /// `ParticipantJoined`; there is no post-join report). The joiners declare
    /// a NON-default client mute (alternating audio / video muted) -- the
    /// state the retired post-join report used to carry -- so the zero is not
    /// vacuous over all-live joiners. Positive control in the same test: after
    /// the storm one real self-mute toggle fans out exactly once and reaches
    /// every peer, so a zero above is not an unwired fan-out.
    #[tokio::test(start_paused = true)]
    async fn a_join_storm_enqueues_no_fan_out_and_a_later_toggle_still_emits_once() {
        use common::observability::testing::MetricAssertion;
        let ids: Vec<String> = (0..=CLIENT_MUTE_FANOUT_BURST + 4)
            .map(|i| format!("p{i}"))
            .collect();
        let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        let snap = MetricAssertion::snapshot();
        let (handle, _task, mut rx) = fanout_meeting_declared("cm-join-storm", &refs, |i| {
            let audio_muted = i % 2 == 0;
            JoinMedia {
                declared: ClientMediaStates::from_declared([Some(audio_muted), Some(!audio_muted)]),
                ..test_media()
            }
        })
        .await;
        for outcome in ["awaiting_token", "fanned_out"] {
            snap.counter("mc_media_client_mute_fanouts_total")
                .with_labels(&[("outcome", outcome)])
                .assert_delta(0);
        }

        // p0 joined audio-muted: unmuting it is a real transition.
        self_mute(&handle, "p0", only(MediaKindLabel::Audio, false)).await;
        snap.counter("mc_media_client_mute_fanouts_total")
            .with_labels(&[("outcome", "fanned_out")])
            .assert_delta(1);
        for id in refs.iter().filter(|id| **id != "p0") {
            let u = next_mute(rx.get_mut(*id).unwrap()).await;
            assert_eq!(u.participant_id, "p0", "{id} heard the toggle");
        }
    }

    /// Test plan 7 / security Gate 1 point 3 (CQ-7): a reconnect's declaration
    /// client mute is ADOPTED through the same merge and the same per-meeting
    /// fan-out as a `MuteRequest`: (b) a changed value fans out exactly once;
    /// (c) an unchanged one fans out nothing, fenced by a later real toggle
    /// being the observer's next update about the subject.
    #[tokio::test(start_paused = true)]
    async fn a_reconnect_adopts_a_changed_client_mute_through_the_fan_out_once() {
        use common::observability::testing::MetricAssertion;
        let (handle, _task) = must_spawn(
            "cm-reconnect".to_string(),
            CancellationToken::new(),
            ActorMetrics::new(),
            ControllerMetrics::new(),
            test_secret(),
        );
        let (tx_b, mut rx_b) = tokio::sync::mpsc::channel(1024);
        join_as(&handle, "b", true, tx_b).await;
        let (tx_a, _rx_a) = tokio::sync::mpsc::channel(1024);
        let joined = join_as(&handle, "a", false, tx_a).await;
        let mut correlation = joined.correlation_id.clone();
        let mut binding = joined.binding_token.clone();

        for (step, declared, expect_fanout) in [
            // (c) unchanged: both kinds still live.
            ("unchanged", [Some(false), Some(false)], 0u64),
            // (b) changed: the camera muted while disconnected.
            ("changed", [Some(false), Some(true)], 1u64),
        ] {
            handle
                .connection_disconnected(
                    "conn-a".to_string(),
                    "a".to_string(),
                    DisconnectCause::ConnectionLost,
                )
                .await
                .unwrap();
            tokio::time::sleep(Duration::from_millis(10)).await;
            let snap = MetricAssertion::snapshot();
            let r = handle
                .connection_reconnect(
                    "conn-a".to_string(),
                    correlation.clone(),
                    binding.clone(),
                    ClientMediaStates::from_declared(declared),
                    None,
                )
                .await
                .unwrap();
            correlation = r.new_correlation_id;
            binding = r.new_binding_token;
            snap.counter("mc_media_client_mute_fanouts_total")
                .with_labels(&[("outcome", "fanned_out")])
                .assert_delta(expect_fanout);
            if expect_fanout == 1 {
                let u = next_mute(&mut rx_b).await;
                assert_eq!(u.participant_id, "a", "{step}");
                assert_eq!(client_pair(&u.media_states), (LIVE, MUTED), "{step}");
            }
        }
        // Fence for the unchanged step: the only mute update b saw about a
        // before this real toggle was the changed step's.
        self_mute(&handle, "a", only(MediaKindLabel::Audio, true)).await;
        let u = next_mute(&mut rx_b).await;
        assert_eq!(client_pair(&u.media_states), (MUTED, MUTED));
    }

    /// Teardown with a pending queue: the wake is a per-iteration timer inside
    /// the actor's `select!` and the queue is actor state, so ending the
    /// meeting drops both -- a clean exit, no wake into a dead actor.
    #[tokio::test(start_paused = true)]
    async fn a_meeting_ending_with_pending_fan_outs_exits_cleanly() {
        use common::observability::testing::MetricAssertion;
        let (handle, task, _rx) = fanout_meeting("cm-teardown", &["churn", "s"]).await;
        exhaust_fanout_bucket(&handle, "churn").await;
        let snap = MetricAssertion::snapshot();
        self_mute(&handle, "s", only(MediaKindLabel::Audio, true)).await; // pending
        assert!(
            snap.counter("mc_media_client_mute_fanouts_total")
                .with_labels(&[("outcome", "awaiting_token")])
                .delta()
                >= 1,
            "positive control: s was pending when the meeting ended"
        );
        handle.cancel();
        // Past the fan-out wake BEFORE the task is awaited: had the wake outlived
        // the loop, it would fire into a dead actor here.
        tokio::time::advance(Duration::from_millis(
            CLIENT_MUTE_FANOUT_REFILL_INTERVAL_MS * 4,
        ))
        .await;
        tokio::time::timeout(Duration::from_secs(30), task)
            .await
            .expect("the actor exits")
            .expect("without panicking");
    }

    #[tokio::test]
    async fn test_meeting_actor_self_mute() {
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, _task) = must_spawn(
            "meeting-mute-test".to_string(),
            cancel_token.clone(),
            metrics,
            controller_metrics,
            test_secret(),
        );

        // Join
        let _ = handle
            .connection_join(
                "conn-1".to_string(),
                "user-1".to_string(),
                "part-1".to_string(),
                String::new(),
                false,
                test_identity_key(),
                None,
                test_media(),
            )
            .await;

        // Update mute
        let result = handle
            .update_self_mute("part-1".to_string(), published(both(true, false)))
            .await;
        assert!(result.is_ok());

        // Check state
        let state = handle.get_state().await.unwrap();
        let participant = state
            .participants
            .iter()
            .find(|p| p.participant_id == "part-1")
            .unwrap();
        assert!(client_muted_now(participant, MediaKindLabel::Audio));
        assert!(!client_muted_now(participant, MediaKindLabel::Video));

        handle.cancel();
    }

    /// A meeting with a HOST (`host`, with an outbound stream) and a non-host
    /// (`part-2`, with its own stream) — the shape every server-mute test needs.
    /// Returns the handle, the actor task, and each participant's outbound
    /// stream.
    async fn host_meeting(
        meeting_id: &str,
    ) -> (
        MeetingActorHandle,
        JoinHandle<()>,
        tokio::sync::mpsc::Receiver<bytes::Bytes>,
        tokio::sync::mpsc::Receiver<bytes::Bytes>,
    ) {
        let (handle, task) = must_spawn(
            meeting_id.to_string(),
            CancellationToken::new(),
            ActorMetrics::new(),
            ControllerMetrics::new(),
            test_secret(),
        );
        let (host_tx, host_rx) = tokio::sync::mpsc::channel(64);
        let (p2_tx, p2_rx) = tokio::sync::mpsc::channel(64);
        handle
            .connection_join(
                "conn-host".to_string(),
                "user-host".to_string(),
                "host".to_string(),
                String::new(),
                true,
                test_identity_key(),
                Some(host_tx),
                test_media(),
            )
            .await
            .unwrap();
        handle
            .connection_join(
                "conn-2".to_string(),
                "user-2".to_string(),
                "part-2".to_string(),
                String::new(),
                false,
                test_identity_key(),
                Some(p2_tx),
                test_media(),
            )
            .await
            .unwrap();
        (handle, task, host_rx, p2_rx)
    }

    /// Read `ServerMessage`s off a participant's outbound stream until one
    /// matches, or panic after a bound.
    async fn next_matching(
        rx: &mut tokio::sync::mpsc::Receiver<bytes::Bytes>,
        want: impl Fn(&proto_gen::dark_tower::signaling::v1::server_message::Message) -> bool,
    ) -> proto_gen::dark_tower::signaling::v1::server_message::Message {
        use prost::Message as _;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            let bytes = tokio::time::timeout_at(deadline, rx.recv())
                .await
                .expect("the expected message arrives")
                .expect("stream open");
            // The participant actor's outbound channel carries the encoded
            // `ServerMessage`; the length prefix is added later, by the bridge.
            let msg = proto_gen::dark_tower::signaling::v1::ServerMessage::decode(&bytes[..])
                .expect("a framed ServerMessage");
            if let Some(m) = msg.message {
                if want(&m) {
                    return m;
                }
            }
        }
    }

    async fn state_of(handle: &MeetingActorHandle, participant_id: &str) -> ParticipantInfo {
        handle
            .get_state()
            .await
            .unwrap()
            .participants
            .into_iter()
            .find(|p| p.participant_id == participant_id)
            .unwrap()
    }

    #[tokio::test]
    async fn a_host_server_mute_applies_and_a_repeat_is_unchanged() {
        let (handle, _task, _h, _p) = host_meeting("sm-apply").await;

        let result = handle
            .server_mute("part-2".to_string(), "host".to_string(), both(true, false))
            .await
            .unwrap();
        let decision = result.unwrap();
        assert!(decision.changed(MediaKindLabel::Audio));
        assert!(!decision.changed(MediaKindLabel::Video));
        let p2 = state_of(&handle, "part-2").await;
        assert!(server_muted_now(&p2, MediaKindLabel::Audio));
        assert!(!server_muted_now(&p2, MediaKindLabel::Video));

        let again = handle
            .server_mute("part-2".to_string(), "host".to_string(), both(true, false))
            .await
            .unwrap();
        assert!(again.unwrap().is_unchanged());
    }

    /// R-8, the ORDERING invariant: authority is checked BEFORE the target is
    /// looked up. A non-host naming a participant who does NOT exist gets
    /// `NotPermitted` — never `UnknownTarget` — so a non-host learns nothing
    /// about who is in the meeting. If the checks were ever reordered, this goes
    /// red.
    #[tokio::test]
    async fn a_non_host_is_refused_before_the_target_is_looked_at() {
        let (handle, _task, _h, _p) = host_meeting("sm-order").await;

        let nonexistent = handle
            .server_mute("ghost".to_string(), "part-2".to_string(), both(true, false))
            .await
            .unwrap();
        assert_eq!(
            nonexistent,
            Err(ServerMuteRefusal::NotPermitted),
            "a non-host naming a NON-EXISTENT participant must be NotPermitted, not UnknownTarget"
        );
        let existing = handle
            .server_mute("host".to_string(), "part-2".to_string(), both(true, false))
            .await
            .unwrap();
        assert_eq!(existing, Err(ServerMuteRefusal::NotPermitted));
        assert!(
            !server_muted_now(&state_of(&handle, "host").await, MediaKindLabel::Audio),
            "a refused request changes nothing"
        );
    }

    /// A requester that is not on the roster at all fails CLOSED.
    #[tokio::test]
    async fn an_unknown_requester_is_not_permitted() {
        let (handle, _task, _h, _p) = host_meeting("sm-unknown-requester").await;
        let result = handle
            .server_mute(
                "part-2".to_string(),
                "nobody".to_string(),
                both(true, false),
            )
            .await
            .unwrap();
        assert_eq!(result, Err(ServerMuteRefusal::NotPermitted));
    }

    /// `UnknownTarget` is reachable only by an authorized requester.
    #[tokio::test]
    async fn a_host_naming_an_absent_participant_is_unknown_target() {
        let (handle, _task, _h, _p) = host_meeting("sm-unknown-target").await;
        let result = handle
            .server_mute("ghost".to_string(), "host".to_string(), both(true, false))
            .await
            .unwrap();
        assert_eq!(result, Err(ServerMuteRefusal::UnknownTarget));
    }

    /// R-10 compose rule, BOTH directions: client mute and server mute have
    /// separate writers, so clearing one never clears the other.
    #[tokio::test]
    async fn self_mute_and_server_mute_compose_in_both_directions() {
        let (handle, _task, _h, _p) = host_meeting("sm-compose").await;

        // Direction 1: self-muted AND server-muted; lifting the server mute
        // leaves the self mute in place.
        handle
            .update_self_mute("part-2".to_string(), published(both(true, false)))
            .await
            .unwrap();
        handle
            .server_mute("part-2".to_string(), "host".to_string(), both(true, false))
            .await
            .unwrap()
            .unwrap();
        handle
            .server_mute("part-2".to_string(), "host".to_string(), both(false, false))
            .await
            .unwrap()
            .unwrap();
        let p2 = state_of(&handle, "part-2").await;
        assert!(
            !server_muted_now(&p2, MediaKindLabel::Audio),
            "the server mute was lifted"
        );
        assert!(
            client_muted_now(&p2, MediaKindLabel::Audio),
            "lifting the server mute left the SELF mute alone"
        );

        // Direction 2: server-muted again; the participant unmuting ITSELF
        // leaves the server mute in place.
        handle
            .server_mute("part-2".to_string(), "host".to_string(), both(true, false))
            .await
            .unwrap()
            .unwrap();
        handle
            .update_self_mute("part-2".to_string(), published(both(false, false)))
            .await
            .unwrap();
        let p2 = state_of(&handle, "part-2").await;
        assert!(
            !client_muted_now(&p2, MediaKindLabel::Audio),
            "the self mute was lifted"
        );
        assert!(
            server_muted_now(&p2, MediaKindLabel::Audio),
            "a self-unmute did NOT clear the server mute"
        );
    }

    /// R-15: a request about ONE kind never changes the other kind's flag, for
    /// either writer, and an identical repeat is a per-kind no-op.
    #[tokio::test]
    async fn a_one_kind_request_never_changes_the_other_kind_for_either_writer() {
        let (handle, _task, _h, _p) = host_meeting("sm-one-kind").await;
        for kind in MediaKindLabel::ALL {
            let other = kind.other();
            // Server mute of `kind` only.
            let d = handle
                .server_mute("part-2".to_string(), "host".to_string(), only(kind, true))
                .await
                .unwrap()
                .unwrap();
            assert!(d.changed(kind) && !d.changed(other));
            let p2 = state_of(&handle, "part-2").await;
            assert!(server_muted_now(&p2, kind));
            assert!(!server_muted_now(&p2, other));
            // Identical repeat: unchanged.
            let again = handle
                .server_mute("part-2".to_string(), "host".to_string(), only(kind, true))
                .await
                .unwrap()
                .unwrap();
            assert!(again.is_unchanged());
            // Unmuting the OTHER kind (already unmuted) changes nothing.
            let noop = handle
                .server_mute("part-2".to_string(), "host".to_string(), only(other, false))
                .await
                .unwrap()
                .unwrap();
            assert!(noop.is_unchanged());
            let p2 = state_of(&handle, "part-2").await;
            assert!(server_muted_now(&p2, kind));
            handle
                .server_mute("part-2".to_string(), "host".to_string(), only(kind, false))
                .await
                .unwrap()
                .unwrap();

            // Client mute of `kind` only.
            let d = handle
                .update_self_mute("part-2".to_string(), published(only(kind, true)))
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            assert!(d.changed(kind) && !d.changed(other));
            assert_eq!(d.state(kind), ClientKindState::ClientMuted);
            assert_eq!(
                d.state(other),
                ClientKindState::Live,
                "the other kind never moved to muted"
            );
            let again = handle
                .update_self_mute("part-2".to_string(), published(only(kind, true)))
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            assert!(again.is_unchanged());
            handle
                .update_self_mute("part-2".to_string(), published(only(kind, false)))
                .await
                .unwrap()
                .unwrap()
                .unwrap();
        }
    }

    /// S4: each kind's `server_muted_by` names whoever IMPOSED that kind's
    /// mute in force -- per kind, so an unmute of one kind never
    /// re-attributes the other's -- and is empty iff that kind is not
    /// server-muted. Read off the late joiner's ROSTER entry (the one carrier;
    /// there is no replay since 2026-10-25).
    #[tokio::test]
    async fn server_muted_by_names_who_imposed_the_remaining_mute() {
        let (handle, _task, _h, _p) = host_meeting("sm-muted-by").await;
        let (tx, _rx) = tokio::sync::mpsc::channel(64);
        handle
            .connection_join(
                "conn-host-b".to_string(),
                "user-host-b".to_string(),
                "host-b".to_string(),
                String::new(),
                true,
                test_identity_key(),
                Some(tx),
                test_media(),
            )
            .await
            .unwrap();
        // Who a fresh joiner's roster entry names as imposing each of part-2's
        // kinds ("" = not server-muted). A new probe joiner each time.
        let probes = std::cell::Cell::new(0u32);
        let muted_by = |handle: MeetingActorHandle| {
            probes.set(probes.get() + 1);
            let n = probes.get();
            async move {
                let joined = handle
                    .connection_join(
                        format!("conn-probe-{n}"),
                        format!("user-probe-{n}"),
                        format!("probe-{n}"),
                        String::new(),
                        false,
                        test_identity_key(),
                        None,
                        test_media(),
                    )
                    .await
                    .unwrap();
                let p2 = joined
                    .participants
                    .iter()
                    .find(|p| p.participant_id == "part-2")
                    .unwrap();
                let wire = crate::webtransport::handler::roster_entry(p2).media_states;
                for k in MediaKindLabel::ALL {
                    assert_eq!(wire_server(&wire, k), !wire_by(&wire, k).is_empty());
                }
                (
                    wire_by(&wire, MediaKindLabel::Audio).to_string(),
                    wire_by(&wire, MediaKindLabel::Video).to_string(),
                )
            }
        };

        // Host A mutes video.
        handle
            .server_mute(
                "part-2".to_string(),
                "host".to_string(),
                only(MediaKindLabel::Video, true),
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            muted_by(handle.clone()).await,
            (String::new(), "host".into())
        );

        // Host B mutes and then unmutes AUDIO: the remaining video mute is
        // still attributed to host A after the unmute.
        handle
            .server_mute(
                "part-2".to_string(),
                "host-b".to_string(),
                only(MediaKindLabel::Audio, true),
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            muted_by(handle.clone()).await,
            ("host-b".into(), "host".into()),
            "a request that MUTES a kind names its requester; the other kind keeps its own"
        );
        handle
            .server_mute(
                "part-2".to_string(),
                "host".to_string(),
                only(MediaKindLabel::Audio, false),
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            muted_by(handle.clone()).await,
            (String::new(), "host".into()),
            "the remaining VIDEO mute is attributed to its imposer, host A, not \
             to host B whose audio mute is gone"
        );

        // Lifting the last kind clears it.
        handle
            .server_mute(
                "part-2".to_string(),
                "host".to_string(),
                only(MediaKindLabel::Video, false),
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            muted_by(handle.clone()).await,
            (String::new(), String::new())
        );
    }

    /// R-10: `UnmuteRequest` NOTIFIES the host and NEVER clears the mute. The
    /// relay carries the requester's id as the actor was given it (the
    /// connection's authenticated id) — asserted POSITIVELY, on a path where the
    /// relay actually happened.
    #[tokio::test]
    async fn an_unmute_request_is_relayed_to_the_host_and_never_clears_the_mute() {
        use proto_gen::dark_tower::signaling::v1::server_message::Message as M;
        let (handle, _task, mut host_rx, _p) = host_meeting("sm-unmute-request").await;
        handle
            .server_mute("part-2".to_string(), "host".to_string(), both(true, false))
            .await
            .unwrap()
            .unwrap();

        let relay = handle
            .request_unmute(
                "part-2".to_string(),
                RequestedKinds::of(&[MediaKindLabel::Audio]),
            )
            .await
            .unwrap();
        assert_eq!(
            relay,
            UnmuteRelay::Relayed { hosts: 1 },
            "the relay happened"
        );

        let M::UnmuteRequest(relayed) =
            next_matching(&mut host_rx, |m| matches!(m, M::UnmuteRequest(_))).await
        else {
            unreachable!()
        };
        assert_eq!(relayed.participant_id, "part-2");
        assert_eq!(
            relayed.media_kinds,
            vec![MediaKindLabel::Audio.to_proto() as i32]
        );

        assert!(
            server_muted_now(&state_of(&handle, "part-2").await, MediaKindLabel::Audio),
            "asking to be unmuted did NOT unmute"
        );
    }

    #[tokio::test]
    async fn an_unmute_request_from_a_participant_not_server_muted_is_not_relayed() {
        let (handle, _task, _h, _p) = host_meeting("sm-unmute-not-muted").await;
        let relay = handle
            .request_unmute(
                "part-2".to_string(),
                RequestedKinds::of(&MediaKindLabel::ALL),
            )
            .await
            .unwrap();
        assert_eq!(relay, UnmuteRelay::NotServerMuted);
    }

    /// Test plan 8 (security Gate 1 point 2): a request naming BOTH kinds, of
    /// which only the camera is server-muted, is relayed naming the camera
    /// ONLY -- the relay is re-encoded from the parsed set restricted to the
    /// kinds actually server-muted, never the client's list.
    #[tokio::test]
    async fn a_mixed_unmute_request_is_relayed_naming_only_the_server_muted_kind() {
        use proto_gen::dark_tower::signaling::v1::server_message::Message as M;
        let (handle, _task, mut host_rx, _p) = host_meeting("sm-unmute-mixed").await;
        handle
            .server_mute(
                "part-2".to_string(),
                "host".to_string(),
                only(MediaKindLabel::Video, true),
            )
            .await
            .unwrap()
            .unwrap();
        let relay = handle
            .request_unmute(
                "part-2".to_string(),
                RequestedKinds::of(&MediaKindLabel::ALL),
            )
            .await
            .unwrap();
        assert_eq!(relay, UnmuteRelay::Relayed { hosts: 1 });
        let M::UnmuteRequest(relayed) =
            next_matching(&mut host_rx, |m| matches!(m, M::UnmuteRequest(_))).await
        else {
            unreachable!()
        };
        assert_eq!(
            relayed.media_kinds,
            vec![MediaKindLabel::Video.to_proto() as i32]
        );
    }

    /// R-11: the broadcast reaches EVERY participant — the target included, so
    /// it can render that it cannot clear the mute itself — carrying who muted
    /// whom.
    #[tokio::test]
    async fn a_server_mute_is_broadcast_to_the_target_itself_with_who_muted_whom() {
        use proto_gen::dark_tower::signaling::v1::server_message::Message as M;
        let (handle, _task, _h, mut p2_rx) = host_meeting("sm-broadcast").await;
        handle
            .server_mute("part-2".to_string(), "host".to_string(), both(true, false))
            .await
            .unwrap()
            .unwrap();
        let M::ParticipantMuteUpdate(update) =
            next_matching(&mut p2_rx, |m| matches!(m, M::ParticipantMuteUpdate(_))).await
        else {
            unreachable!()
        };
        assert_eq!(update.participant_id, "part-2");
        assert!(wire_server(&update.media_states, MediaKindLabel::Audio));
        assert_eq!(wire_by(&update.media_states, MediaKindLabel::Audio), "host");
        assert!(!wire_server(&update.media_states, MediaKindLabel::Video));
        assert_eq!(wire_by(&update.media_states, MediaKindLabel::Video), "");
    }

    /// R-11 late joiner, since 2026-10-25: the ROSTER entry carries who is
    /// server-muted NOW and who muted them (there is no replay); lifting
    /// clears the imposer in lockstep, so a later joiner's entry shows none.
    #[tokio::test]
    async fn a_late_joiners_roster_carries_the_current_server_mutes() {
        let (handle, _task, _h, _p) = host_meeting("sm-roster").await;
        handle
            .server_mute("part-2".to_string(), "host".to_string(), both(true, false))
            .await
            .unwrap()
            .unwrap();

        let join = |id: &'static str| {
            let handle = handle.clone();
            async move {
                let joined = handle
                    .connection_join(
                        format!("conn-{id}"),
                        format!("user-{id}"),
                        id.to_string(),
                        String::new(),
                        false,
                        test_identity_key(),
                        None,
                        test_media(),
                    )
                    .await
                    .unwrap();
                let p2 = joined
                    .participants
                    .into_iter()
                    .find(|p| p.participant_id == "part-2")
                    .unwrap();
                crate::webtransport::handler::roster_entry(&p2).media_states
            }
        };
        let entry = join("part-3").await;
        assert!(wire_server(&entry, MediaKindLabel::Audio));
        assert_eq!(wire_by(&entry, MediaKindLabel::Audio), "host");
        assert!(!wire_server(&entry, MediaKindLabel::Video));

        handle
            .server_mute("part-2".to_string(), "host".to_string(), both(false, false))
            .await
            .unwrap()
            .unwrap();
        let later = join("part-4").await;
        for k in MediaKindLabel::ALL {
            assert!(!wire_server(&later, k), "a lifted mute is gone ({k:?})");
            assert_eq!(wire_by(&later, k), "", "its imposer cleared with it");
        }
    }

    /// R-10: a server mute survives a RECONNECT within grace (the roster entry
    /// is reused) and does NOT survive a fresh JOIN (a new roster entry starts
    /// unmuted). No side table is involved in either.
    #[tokio::test(start_paused = true)]
    async fn a_server_mute_survives_reconnect_but_not_a_fresh_join() {
        let (handle, _task, _h, _p) = host_meeting("sm-reconnect").await;
        let joined = handle
            .connection_join(
                "conn-3".to_string(),
                "user-3".to_string(),
                "part-3".to_string(),
                String::new(),
                false,
                test_identity_key(),
                None,
                test_media(),
            )
            .await
            .unwrap();
        handle
            .server_mute("part-3".to_string(), "host".to_string(), both(true, false))
            .await
            .unwrap()
            .unwrap();
        // Story 3: a VIDEO server mute and a video self mute ride the roster
        // entry too.
        handle
            .server_mute(
                "part-3".to_string(),
                "host".to_string(),
                only(MediaKindLabel::Video, true),
            )
            .await
            .unwrap()
            .unwrap();
        handle
            .update_self_mute(
                "part-3".to_string(),
                published(only(MediaKindLabel::Video, true)),
            )
            .await
            .unwrap();

        // Reconnect within grace: the SAME roster entry, still muted.
        let _ = handle
            .connection_disconnected(
                "conn-3".to_string(),
                "part-3".to_string(),
                DisconnectCause::ConnectionLost,
            )
            .await;
        tokio::time::sleep(Duration::from_millis(10)).await;
        tokio::time::advance(Duration::from_secs(10)).await;
        let reconnected = handle
            .connection_reconnect(
                "conn-3b".to_string(),
                joined.correlation_id.clone(),
                joined.binding_token.clone(),
                // The declaration carries the CURRENT client mute (video muted).
                ClientMediaStates::from_declared([Some(false), Some(true)]),
                None,
            )
            .await
            .unwrap();
        assert_eq!(reconnected.participant_id, "part-3");
        let p3 = state_of(&handle, "part-3").await;
        assert!(
            server_muted_now(&p3, MediaKindLabel::Audio)
                && server_muted_now(&p3, MediaKindLabel::Video),
            "both kinds' server mutes survive a reconnect within grace"
        );
        assert!(
            client_muted_now(&p3, MediaKindLabel::Video)
                && !client_muted_now(&p3, MediaKindLabel::Audio)
        );

        // S3: after the reconnect, a request naming ONLY audio merges against
        // the ROSTER (the new connection knows nothing): video's self mute is
        // untouched.
        let decision = handle
            .update_self_mute(
                "part-3".to_string(),
                published(only(MediaKindLabel::Audio, true)),
            )
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(decision.changed(MediaKindLabel::Audio));
        assert!(!decision.changed(MediaKindLabel::Video));
        let p3 = state_of(&handle, "part-3").await;
        assert!(
            client_muted_now(&p3, MediaKindLabel::Audio)
                && client_muted_now(&p3, MediaKindLabel::Video)
        );

        // A fresh join by the same user: a NEW roster entry, unmuted.
        let rejoined = handle
            .connection_join(
                "conn-3c".to_string(),
                "user-3".to_string(),
                "part-3-new".to_string(),
                String::new(),
                false,
                test_identity_key(),
                None,
                test_media(),
            )
            .await
            .unwrap();
        assert!(
            !server_muted_now(
                &state_of(&handle, &rejoined.participant_id).await,
                MediaKindLabel::Audio
            ),
            "a fresh join starts unmuted"
        );
    }

    /// A participant in its grace window is still on the roster, so another
    /// participant's departure does NOT end the meeting — the reconnect path
    /// stays open.
    #[tokio::test(start_paused = true)]
    async fn a_participant_in_grace_keeps_the_meeting_alive() {
        let (handle, task, _h, _p) = host_meeting("grace-keeps-alive").await;
        let _ = handle
            .connection_disconnected(
                "conn-2".to_string(),
                "part-2".to_string(),
                DisconnectCause::ConnectionLost,
            )
            .await;
        handle.participant_leave("host".to_string()).await.unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(
            !task.is_finished(),
            "a grace member keeps the meeting alive"
        );
        assert_eq!(handle.get_state().await.unwrap().participants.len(), 1);
    }

    /// Closing a meeting leaves through the clean exit path (not the
    /// process-shutdown cancel), so its handlers are released.
    #[tokio::test]
    async fn closing_a_meeting_ends_the_actor_through_the_clean_path() {
        let (handle, task, _h, _p) = host_meeting("close").await;
        handle.close_meeting("test".to_string()).await.unwrap();
        tokio::time::timeout(Duration::from_secs(10), task)
            .await
            .expect("a closed meeting's actor exits")
            .expect("cleanly");
        assert!(
            !handle.is_cancelled(),
            "closing does not use the process-shutdown cancel"
        );
    }

    #[tokio::test]
    async fn test_meeting_actor_child_token() {
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, _task) = must_spawn(
            "meeting-token-test".to_string(),
            cancel_token.clone(),
            metrics,
            controller_metrics,
            test_secret(),
        );

        let child = handle.child_token();
        assert!(!child.is_cancelled());

        handle.cancel();

        // Give time for cancellation to propagate
        tokio::time::sleep(Duration::from_millis(10)).await;

        assert!(child.is_cancelled());
    }

    /// Test that the 30-second disconnect grace period expires correctly.
    /// Uses `tokio::time::pause()` to control time advancement.
    #[tokio::test(start_paused = true)]
    async fn test_disconnect_grace_period_expires() {
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, task) = must_spawn(
            "meeting-grace-period-test".to_string(),
            cancel_token.clone(),
            metrics,
            controller_metrics,
            test_secret(),
        );

        // Join a participant
        let join_result = handle
            .connection_join(
                "conn-1".to_string(),
                "user-1".to_string(),
                "part-1".to_string(),
                String::new(),
                false,
                test_identity_key(),
                None,
                test_media(),
            )
            .await
            .unwrap();

        assert_eq!(join_result.participant_id, "part-1");

        // Verify participant is in the meeting
        let state = handle.get_state().await.unwrap();
        assert_eq!(state.participants.len(), 1);
        assert_eq!(state.participants[0].status, ParticipantStatus::Connected);

        // Disconnect the participant
        let _ = handle
            .connection_disconnected(
                "conn-1".to_string(),
                "part-1".to_string(),
                DisconnectCause::ConnectionLost,
            )
            .await;

        // Give actor time to process the disconnect message
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Verify participant is disconnected but still in the meeting
        let state = handle.get_state().await.unwrap();
        assert_eq!(state.participants.len(), 1);
        assert_eq!(
            state.participants[0].status,
            ParticipantStatus::Disconnected
        );

        // Advance time by 29 seconds - participant should still be present
        tokio::time::advance(Duration::from_secs(29)).await;

        // Give actor time for the grace check interval to tick
        tokio::time::sleep(Duration::from_millis(10)).await;

        let state = handle.get_state().await.unwrap();
        assert_eq!(
            state.participants.len(),
            1,
            "Participant should still be present before grace period expires"
        );

        // Advance time past the 30-second grace period (total now > 30s)
        tokio::time::advance(Duration::from_secs(6)).await;

        // Wait for the grace check interval (every 5 seconds) to process
        tokio::time::sleep(Duration::from_millis(10)).await;

        // The only participant is gone, so the meeting has ENDED (story 2 task
        // 12; ADR-0010 §3). A real observable: the actor's task completes
        // through its clean exit path — which it does only because the roster
        // emptied, i.e. only because the removal above happened. Not "the
        // roster is no longer readable", which would pass for any reason the
        // actor stopped answering.
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("grace expiry of the last participant ends the meeting")
            .expect("the meeting actor exits cleanly, not by panic");
    }

    /// Test that reconnection within grace period preserves participant.
    #[tokio::test(start_paused = true)]
    async fn test_reconnect_within_grace_period() {
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, _task) = must_spawn(
            "meeting-reconnect-grace-test".to_string(),
            cancel_token.clone(),
            metrics,
            controller_metrics,
            test_secret(),
        );

        // Join a participant
        let join_result = handle
            .connection_join(
                "conn-1".to_string(),
                "user-1".to_string(),
                "part-1".to_string(),
                String::new(),
                false,
                test_identity_key(),
                None,
                test_media(),
            )
            .await
            .unwrap();

        // Disconnect the participant
        let _ = handle
            .connection_disconnected(
                "conn-1".to_string(),
                "part-1".to_string(),
                DisconnectCause::ConnectionLost,
            )
            .await;

        // Give actor time to process
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Advance time by 20 seconds (within grace period)
        tokio::time::advance(Duration::from_secs(20)).await;

        // Reconnect with valid token
        let reconnect_result = handle
            .connection_reconnect(
                "conn-2".to_string(),
                join_result.correlation_id.clone(),
                join_result.binding_token.clone(),
                ClientMediaStates::live(av()),
                None,
            )
            .await;

        assert!(reconnect_result.is_ok());
        let reconnect_result = reconnect_result.unwrap();
        assert_eq!(reconnect_result.participant_id, "part-1");

        // Verify participant is connected again
        let state = handle.get_state().await.unwrap();
        assert_eq!(state.participants.len(), 1);
        assert_eq!(state.participants[0].status, ParticipantStatus::Connected);

        handle.cancel();
    }

    /// Task #64: a CLEAN transport close (`DisconnectCause::ClientClosed`) removes
    /// the participant from the roster IMMEDIATELY — the 30s grace period is
    /// SKIPPED. Uses `start_paused`: the participant is gone without any time
    /// being advanced past the grace window, which pins "grace skipped".
    #[tokio::test(start_paused = true)]
    async fn test_clean_close_skips_grace_removes_immediately() {
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, task) = must_spawn(
            "meeting-clean-close-test".to_string(),
            cancel_token.clone(),
            metrics,
            controller_metrics,
            test_secret(),
        );

        // Join a participant.
        let _ = handle
            .connection_join(
                "conn-1".to_string(),
                "user-1".to_string(),
                "part-1".to_string(),
                String::new(),
                false,
                test_identity_key(),
                None,
                test_media(),
            )
            .await
            .unwrap();

        let state = handle.get_state().await.unwrap();
        assert_eq!(state.participants.len(), 1);

        // Clean transport close — user closed the tab.
        let _ = handle
            .connection_disconnected(
                "conn-1".to_string(),
                "part-1".to_string(),
                DisconnectCause::ClientClosed,
            )
            .await;

        // Give the actor time to process the disconnect message. NOTE: we do NOT
        // advance past the 30s grace window — this small settle is only for
        // mailbox processing under start_paused.
        tokio::time::sleep(Duration::from_millis(10)).await;

        // The only participant is gone, so the meeting has ENDED (story 2 task
        // 12; ADR-0010 §3). A real observable: the actor's task completes
        // through its clean exit path — which it does only because the roster
        // emptied, i.e. only because the removal above happened. Not "the
        // roster is no longer readable", which would pass for any reason the
        // actor stopped answering.
        // ONE virtual second, far inside the 30 s grace window: under
        // `start_paused` a longer bound would let tokio auto-advance past grace,
        // and the meeting would then end by grace expiry — passing this test
        // for the wrong reason.
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect(
                "a clean close of the last participant ends the meeting WITHOUT waiting out grace",
            )
            .expect("the meeting actor exits cleanly, not by panic");
    }

    /// Task #64: an ABRUPT loss (`DisconnectCause::ConnectionLost`) must PRESERVE
    /// the ADR-0023 grace period — the participant stays visible (Disconnected)
    /// and a reconnect within grace still succeeds. Guards against the skip-grace
    /// change regressing session recovery.
    #[tokio::test(start_paused = true)]
    async fn test_connection_lost_preserves_grace_and_allows_reconnect() {
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, _task) = must_spawn(
            "meeting-conn-lost-grace-test".to_string(),
            cancel_token.clone(),
            metrics,
            controller_metrics,
            test_secret(),
        );

        let join_result = handle
            .connection_join(
                "conn-1".to_string(),
                "user-1".to_string(),
                "part-1".to_string(),
                String::new(),
                false,
                test_identity_key(),
                None,
                test_media(),
            )
            .await
            .unwrap();

        // Abrupt loss — keep grace.
        let _ = handle
            .connection_disconnected(
                "conn-1".to_string(),
                "part-1".to_string(),
                DisconnectCause::ConnectionLost,
            )
            .await;
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Still present, but Disconnected (grace running) — NOT removed.
        let state = handle.get_state().await.unwrap();
        assert_eq!(state.participants.len(), 1);
        assert_eq!(
            state.participants[0].status,
            ParticipantStatus::Disconnected
        );

        // Reconnect within grace still works (ADR-0023 preserved).
        tokio::time::advance(Duration::from_secs(10)).await;
        let reconnect_result = handle
            .connection_reconnect(
                "conn-2".to_string(),
                join_result.correlation_id.clone(),
                join_result.binding_token.clone(),
                ClientMediaStates::live(av()),
                None,
            )
            .await;
        assert!(reconnect_result.is_ok());

        let state = handle.get_state().await.unwrap();
        assert_eq!(state.participants.len(), 1);
        assert_eq!(state.participants[0].status, ParticipantStatus::Connected);

        handle.cancel();
    }
}
