//! `MeetingControllerActor` - singleton supervisor for meeting actors (ADR-0023).
//!
//! The `MeetingControllerActor` is the top-level actor in the MC hierarchy:
//!
//! - Singleton per MC instance
//! - Supervises N `MeetingActor` instances
//! - Handles meeting creation/removal
//! - Owns the root `CancellationToken` for graceful shutdown
//! - Monitors child actor health (panic detection via `JoinHandle`)
//!
//! # Graceful Shutdown
//!
//! On SIGTERM, the controller:
//! 1. Sets `accepting_new = false`
//! 2. Cancels the root `CancellationToken` (propagates to all children)
//! 3. Waits for meetings to drain or migrate
//! 4. Reports completion to GC

use crate::errors::McError;
use crate::media_routing::PolicyGenerations;

use super::meeting::{MeetingActor, MeetingActorHandle, MeetingWiring};
use super::meeting_media::MutedSourceCensus;
use super::messages::{
    ControllerMessage, ControllerStatus, JoinResult, MeetingEndCause, MeetingInfo,
    MeetingLifecycleEvent, MeetingLifecycleSink,
};
use super::metrics::{ActorMetrics, ActorType, ControllerMetrics, MailboxMonitor};

use common::secret::{ExposeSecret, SecretBox};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, instrument, warn};

/// Default channel buffer size for the controller mailbox.
const CONTROLLER_CHANNEL_BUFFER: usize = 1000;

/// Creates that may wait behind one meeting id's teardown. Beyond it a create
/// is refused `MeetingTeardownInProgress` rather than queued without bound. A
/// rejoin normally reaches MC only after the teardown (GC is notified after
/// it), so a legitimate queue here is short; this bounds the pathological one.
const MAX_QUEUED_CREATES: usize = 16;

/// How far past the teardown's own worst case the fence's backstop deadline
/// sits. The margin covers the teardown task being scheduled and reporting;
/// the deadline exists ONLY for a completion report that was lost, which the
/// `Drop`-guarded report makes a can't-happen, so it must never race a real
/// (slow but finishing) teardown.
const FENCE_BACKSTOP_MARGIN: Duration = Duration::from_secs(5);

/// Told when a meeting has ENDED (emptied or closed) and its handlers have
/// been released — the point at which GC may re-assign the meeting id.
///
/// A trait so the controller's notify path is testable without a GC; the
/// production implementation (`main.rs`) queues the notification for the GC
/// client, which owns the send policy.
pub trait MeetingEndedNotifier: Send + Sync {
    /// The meeting `meeting_id` has ended and its handlers are released.
    fn meeting_ended(&self, meeting_id: &str);
}

/// The notifier for a controller with no GC to tell (tests that do not
/// exercise the notify path).
#[derive(Debug, Default)]
pub struct NoMeetingEndedNotifier;

impl MeetingEndedNotifier for NoMeetingEndedNotifier {
    fn meeting_ended(&self, _meeting_id: &str) {}
}

/// Handle to the `MeetingControllerActor`.
///
/// This is the public interface for interacting with the controller.
/// All methods are async and return results via oneshot channels.
#[derive(Clone)]
pub struct MeetingControllerActorHandle {
    sender: mpsc::Sender<ControllerMessage>,
    cancel_token: CancellationToken,
    teardowns: Arc<crate::media_routing::teardown::TeardownTracker>,
}

impl MeetingControllerActorHandle {
    /// Create a new `MeetingControllerActor` and return a handle to it.
    ///
    /// This spawns the actor task and returns immediately.
    ///
    /// # Arguments
    ///
    /// * `mc_id` - MC instance ID
    /// * `metrics` - Shared actor metrics
    /// * `controller_metrics` - Controller metrics for GC heartbeat reporting (participant count)
    /// * `master_secret` - Master secret for session binding tokens (must be >= 32 bytes).
    ///   Wrapped in SecretBox to ensure secure memory handling (zeroization on drop,
    ///   redacted Debug output).
    /// * `policy_generations` - Per-(meeting, handler) `policy_generation`
    ///   registry (ADR-0036 §8). Evicted on meeting teardown.
    /// * `kek_lifecycle` - W, the overdue threshold and the per-meeting rotation
    ///   state the KEK fleet gauges sample (ADR-0036 §4). Evicted on meeting
    ///   teardown, on the same sites as `policy_generations`.
    #[must_use]
    pub fn new(
        mc_id: String,
        metrics: Arc<ActorMetrics>,
        controller_metrics: Arc<ControllerMetrics>,
        master_secret: SecretBox<Vec<u8>>,
        policy_generations: Arc<PolicyGenerations>,
        kek_lifecycle: Arc<crate::media_admission::KekLifecycle>,
    ) -> Self {
        Self::with_meeting_ended_notifier(
            mc_id,
            metrics,
            controller_metrics,
            master_secret,
            policy_generations,
            kek_lifecycle,
            Arc::new(NoMeetingEndedNotifier),
        )
    }

    /// As [`Self::new`], telling `meeting_ended` when a meeting has ended and
    /// its handlers are released (production: the GC notification queue).
    #[must_use]
    pub fn with_meeting_ended_notifier(
        mc_id: String,
        metrics: Arc<ActorMetrics>,
        controller_metrics: Arc<ControllerMetrics>,
        master_secret: SecretBox<Vec<u8>>,
        policy_generations: Arc<PolicyGenerations>,
        kek_lifecycle: Arc<crate::media_admission::KekLifecycle>,
        meeting_ended: Arc<dyn MeetingEndedNotifier>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(CONTROLLER_CHANNEL_BUFFER);
        let cancel_token = CancellationToken::new();

        let mut actor = MeetingControllerActor::new(
            mc_id,
            receiver,
            cancel_token.clone(),
            Arc::clone(&metrics),
            Arc::clone(&controller_metrics),
            master_secret,
            policy_generations,
            kek_lifecycle,
        );
        actor.meeting_ended = meeting_ended;
        let teardowns = Arc::clone(&actor.teardowns);

        tokio::spawn(actor.run());

        Self {
            sender,
            cancel_token,
            teardowns,
        }
    }

    /// The process's in-flight meeting-teardown count. A graceful shutdown
    /// settles on it after [`Self::shutdown`] returns, so handed-off releases
    /// are not dropped with the runtime.
    #[must_use]
    pub fn teardowns(&self) -> Arc<crate::media_routing::teardown::TeardownTracker> {
        Arc::clone(&self.teardowns)
    }

    /// Create a new meeting.
    ///
    /// Returns `Ok(())` if the meeting was created, or an error if creation failed.
    pub async fn create_meeting(&self, meeting_id: String) -> Result<(), McError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(ControllerMessage::CreateMeeting {
                meeting_id,
                respond_to: tx,
            })
            .await
            .map_err(|e| McError::Internal(format!("channel send failed: {e}")))?;

        rx.await
            .map_err(|e| McError::Internal(format!("response receive failed: {e}")))?
    }

    /// Create a meeting with test-only construction overrides — the sender-id
    /// cursor, placement pins and flush bound. **Test builds only** — see
    /// [`MeetingActor::spawn_with_seams`]. Compiled only under the non-default
    /// `test-seams` feature; a release build with it on fails to compile.
    ///
    /// # Errors
    ///
    /// As [`Self::create_meeting`].
    #[cfg(feature = "test-seams")]
    pub async fn create_meeting_with_seams(
        &self,
        meeting_id: String,
        seams: super::meeting_media::MeetingSeams,
    ) -> Result<(), McError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(ControllerMessage::CreateMeetingWithSeams {
                meeting_id,
                seams,
                respond_to: tx,
            })
            .await
            .map_err(|e| McError::Internal(format!("channel send failed: {e}")))?;

        rx.await
            .map_err(|e| McError::Internal(format!("response receive failed: {e}")))?
    }

    /// Get information about an existing meeting.
    pub async fn get_meeting(&self, meeting_id: String) -> Result<MeetingInfo, McError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(ControllerMessage::GetMeeting {
                meeting_id,
                respond_to: tx,
            })
            .await
            .map_err(|e| McError::Internal(format!("channel send failed: {e}")))?;

        rx.await
            .map_err(|e| McError::Internal(format!("response receive failed: {e}")))?
    }

    /// Get a live handle to a meeting actor.
    ///
    /// Unlike [`Self::get_meeting`], which returns a state snapshot, this hands
    /// back the handle so a caller can address the meeting directly.
    ///
    /// # Errors
    ///
    /// [`McError::MeetingNotFound`] if no such meeting is running on this MC.
    pub async fn get_meeting_handle(
        &self,
        meeting_id: String,
    ) -> Result<super::meeting::MeetingActorHandle, McError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(ControllerMessage::GetMeetingHandle {
                meeting_id,
                respond_to: tx,
            })
            .await
            .map_err(|e| McError::Internal(format!("channel send failed: {e}")))?;

        rx.await
            .map_err(|e| McError::Internal(format!("response receive failed: {e}")))?
    }

    /// Fire-and-forget: route a new connection to the correct meeting.
    ///
    /// The controller looks up the meeting and forwards the join request.
    /// Returns a oneshot receiver that the caller can await for the result.
    #[expect(
        clippy::too_many_arguments,
        reason = "actor-handle join signature threads the full join tuple (ids + display_name + host flag + stream); bundling into a JoinConnectionParams struct is a larger cross-message refactor tracked in docs/TODO.md"
    )]
    pub async fn join_connection(
        &self,
        meeting_id: String,
        connection_id: String,
        user_id: String,
        participant_id: String,
        display_name: String,
        is_host: bool,
        identity_public_key: Option<crate::media_admission::IdentityPublicKey>,
        stream_tx: tokio::sync::mpsc::Sender<bytes::Bytes>,
        media: super::meeting_media::JoinMedia,
    ) -> Result<tokio::sync::oneshot::Receiver<Result<JoinResult, McError>>, McError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(ControllerMessage::JoinConnection {
                meeting_id,
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
        Ok(rx)
    }

    /// Remove a meeting without ENDING it: the actor is cancelled, releases
    /// its handlers as a shutdown teardown (`TeardownReason::Shutdown`), and GC
    /// is not told. A meeting whose participants all leave ends itself — this is
    /// not that path.
    pub async fn remove_meeting(&self, meeting_id: String) -> Result<(), McError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(ControllerMessage::RemoveMeeting {
                meeting_id,
                respond_to: tx,
            })
            .await
            .map_err(|e| McError::Internal(format!("channel send failed: {e}")))?;

        rx.await
            .map_err(|e| McError::Internal(format!("response receive failed: {e}")))?
    }

    /// Get the current controller status.
    pub async fn get_status(&self) -> Result<ControllerStatus, McError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(ControllerMessage::GetStatus { respond_to: tx })
            .await
            .map_err(|e| McError::Internal(format!("channel send failed: {e}")))?;

        rx.await
            .map_err(|e| McError::Internal(format!("response receive failed: {e}")))
    }

    /// Shut the actor system down gracefully, returning once every meeting
    /// actor has exited (so every clean-path teardown has been HANDED OFF and
    /// counted on [`Self::teardowns`]) — not merely once shutdown has begun.
    /// A caller that needs a bound wraps this in a timeout; `main` does.
    pub async fn shutdown(&self, deadline: Duration) -> Result<(), McError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(ControllerMessage::Shutdown {
                deadline,
                respond_to: tx,
            })
            .await
            .map_err(|e| McError::Internal(format!("channel send failed: {e}")))?;

        rx.await
            .map_err(|e| McError::Internal(format!("response receive failed: {e}")))?
    }

    /// Cancel the actor (for immediate shutdown).
    pub fn cancel(&self) {
        self.cancel_token.cancel();
    }

    /// Check if the actor is cancelled.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancel_token.is_cancelled()
    }

    /// Get a child token for spawning child actors.
    #[must_use]
    pub fn child_token(&self) -> CancellationToken {
        self.cancel_token.child_token()
    }
}

/// Internal state for a managed meeting.
struct ManagedMeeting {
    /// Handle to the meeting actor.
    handle: MeetingActorHandle,
    /// Join handle for monitoring the actor task.
    task_handle: JoinHandle<()>,
    /// Meeting creation timestamp.
    created_at: i64,
}

/// The `MeetingControllerActor` implementation.
///
/// This struct owns the actor state and runs the message loop.
pub struct MeetingControllerActor {
    /// MC instance ID.
    mc_id: String,
    /// Message receiver.
    receiver: mpsc::Receiver<ControllerMessage>,
    /// Cancellation token (root).
    cancel_token: CancellationToken,
    /// Managed meetings by ID.
    meetings: HashMap<String, ManagedMeeting>,
    /// Whether the controller is accepting new meetings.
    accepting_new: bool,
    /// Shared actor metrics.
    metrics: Arc<ActorMetrics>,
    /// Controller metrics for GC heartbeat reporting (participant count).
    controller_metrics: Arc<ControllerMetrics>,
    /// Mailbox monitor.
    mailbox: MailboxMonitor,
    /// Master secret for session binding tokens (ADR-0023).
    /// Wrapped in SecretBox to ensure secure memory handling.
    master_secret: SecretBox<Vec<u8>>,
    /// Per-(meeting, handler) `policy_generation` registry (ADR-0036 §8).
    /// Cleaned up when meetings are removed, on the same choke point.
    policy_generations: Arc<PolicyGenerations>,
    /// KEK rotation lifecycle: W, the overdue threshold, and each meeting's
    /// rotation state for the fleet gauges. Evicted on BOTH teardown paths,
    /// beside `policy_generations`.
    kek_lifecycle: Arc<crate::media_admission::KekLifecycle>,
    /// Meeting actors report their lifecycle here. Drained BEFORE any reap and
    /// before any message is handled (see `drain_lifecycle`).
    lifecycle_tx: MeetingLifecycleSink,
    lifecycle_rx: mpsc::UnboundedReceiver<MeetingLifecycleEvent>,
    /// Meeting ids whose previous incarnation is still being released on its
    /// handlers. A create for one of these WAITS (see [`TeardownFence`]).
    tearing_down: HashMap<String, TeardownFence>,
    /// The pod's server-mute census, recomputed on every health walk.
    muted_census: Arc<MutedSourceCensus>,
    /// Told when a meeting has ended and its handlers are released.
    meeting_ended: Arc<dyn MeetingEndedNotifier>,
    /// In-flight teardowns, handed to every meeting actor's wiring.
    teardowns: Arc<crate::media_routing::teardown::TeardownTracker>,
    /// Answered once the drain completes (see
    /// [`MeetingControllerActorHandle::shutdown`]).
    shutdown_waiter: Option<tokio::sync::oneshot::Sender<Result<(), McError>>>,
}

/// Why a meeting id is fenced, and who is waiting on it.
///
/// # Why a fence is REQUIRED, not an optimisation
///
/// `EndMeeting` releases by meeting id alone, with no generation. If a meeting
/// were re-created under this id while its previous incarnation's release was
/// still in flight, the release would land on the NEW registration: its routes
/// gone, MC believing it programmed, a silent dark meeting until the next
/// structural change. So no create for this id runs until the teardown reports
/// completion.
///
/// # Park, never await
///
/// A waiting create's responder is STORED here and answered from the
/// completion arm of this same loop. Awaiting the completion inline would
/// self-deadlock the whole controller — every meeting on the pod — because the
/// completion arrives on the loop that would be blocked waiting for it.
struct TeardownFence {
    /// When the backstop lifts this fence if no completion ever arrives.
    deadline: tokio::time::Instant,
    /// Creates parked behind the teardown, answered when it lifts.
    waiting: Vec<ParkedCreate>,
    /// Why the meeting ended, recorded at reap so the BACKSTOP path — where no
    /// completion report (and so no cause) ever arrives — still knows whether
    /// GC must be told.
    cause: MeetingEndCause,
}

/// A create parked behind a teardown fence.
enum ParkedCreate {
    Plain(tokio::sync::oneshot::Sender<Result<(), McError>>),
    #[cfg(feature = "test-seams")]
    WithSeams(
        super::meeting_media::MeetingSeams,
        tokio::sync::oneshot::Sender<Result<(), McError>>,
    ),
}

impl ParkedCreate {
    fn respond(self, result: Result<(), McError>) {
        match self {
            Self::Plain(respond_to) => {
                let _ = respond_to.send(result);
            }
            #[cfg(feature = "test-seams")]
            Self::WithSeams(_, respond_to) => {
                let _ = respond_to.send(result);
            }
        }
    }
}

impl MeetingControllerActor {
    /// Create a new controller actor (not started).
    ///
    /// # Arguments
    ///
    /// * `mc_id` - MC instance ID
    /// * `receiver` - Message receiver channel
    /// * `cancel_token` - Root cancellation token
    /// * `metrics` - Shared actor metrics
    /// * `controller_metrics` - Controller metrics for GC heartbeat reporting (participant count)
    /// * `master_secret` - Master secret for session binding tokens (must be >= 32 bytes).
    ///   Wrapped in SecretBox to ensure secure memory handling.
    ///   Cleaned up when meetings are removed.
    /// * `policy_generations` - Per-(meeting, handler) `policy_generation`
    ///   registry. Cleaned up when meetings are removed.
    /// * `kek_lifecycle` - KEK rotation lifecycle. Cleaned up when meetings are
    ///   removed.
    #[expect(
        clippy::too_many_arguments,
        reason = "constructor threads the two shared per-meeting registries alongside the actor wiring"
    )]
    fn new(
        mc_id: String,
        receiver: mpsc::Receiver<ControllerMessage>,
        cancel_token: CancellationToken,
        metrics: Arc<ActorMetrics>,
        controller_metrics: Arc<ControllerMetrics>,
        master_secret: SecretBox<Vec<u8>>,
        policy_generations: Arc<PolicyGenerations>,
        kek_lifecycle: Arc<crate::media_admission::KekLifecycle>,
    ) -> Self {
        let mailbox = MailboxMonitor::new(ActorType::Controller, &mc_id);
        let (lifecycle_tx, lifecycle_rx) = mpsc::unbounded_channel();

        Self {
            mc_id,
            receiver,
            cancel_token,
            meetings: HashMap::new(),
            accepting_new: true,
            metrics,
            controller_metrics,
            mailbox,
            master_secret,
            policy_generations,
            kek_lifecycle,
            lifecycle_tx,
            lifecycle_rx,
            tearing_down: HashMap::new(),
            muted_census: Arc::new(MutedSourceCensus::default()),
            meeting_ended: Arc::new(NoMeetingEndedNotifier),
            teardowns: Arc::default(),
            shutdown_waiter: None,
        }
    }

    /// Run the actor message loop.
    #[instrument(skip_all, name = "mc.actor.controller", fields(mc_id = %self.mc_id))]
    async fn run(mut self) {
        info!(
            target: "mc.actor.controller",
            mc_id = %self.mc_id,
            "MeetingControllerActor started"
        );

        loop {
            // Lifecycle events FIRST: a meeting actor sends `Ended` before its
            // task finishes, so draining here guarantees the fence is up before
            // the health walk could reap that actor, and before any create for
            // the id is handled.
            self.drain_lifecycle().await;

            // Check for terminated meeting actors
            self.check_meeting_health().await;

            let backstop = self.tearing_down.values().map(|f| f.deadline).min();

            tokio::select! {
                // A lifecycle event arrived while idle.
                Some(event) = self.lifecycle_rx.recv() => {
                    self.on_lifecycle(event).await;
                }

                // A fence's backstop deadline passed with no completion.
                () = async {
                    match backstop {
                        Some(at) => tokio::time::sleep_until(at).await,
                        None => std::future::pending().await,
                    }
                } => {
                    self.expire_fences().await;
                }

                // Handle cancellation
                () = self.cancel_token.cancelled() => {
                    info!(
                        target: "mc.actor.controller",
                        mc_id = %self.mc_id,
                        "MeetingControllerActor received cancellation signal"
                    );
                    self.graceful_shutdown().await;
                    break;
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
                            // Channel closed, exit
                            info!(
                                target: "mc.actor.controller",
                                mc_id = %self.mc_id,
                                "MeetingControllerActor channel closed, exiting"
                            );
                            break;
                        }
                    }
                }
            }
        }

        info!(
            target: "mc.actor.controller",
            mc_id = %self.mc_id,
            meetings_remaining = self.meetings.len(),
            messages_processed = self.mailbox.messages_processed(),
            "MeetingControllerActor stopped"
        );
        if let Some(waiter) = self.shutdown_waiter.take() {
            let _ = waiter.send(Ok(()));
        }
    }

    /// Handle a single message.
    async fn handle_message(&mut self, message: ControllerMessage) {
        match message {
            ControllerMessage::CreateMeeting {
                meeting_id,
                respond_to,
            } => {
                // Park, never await: see `TeardownFence`.
                if self.tearing_down.contains_key(&meeting_id) {
                    self.park_create(&meeting_id, ParkedCreate::Plain(respond_to));
                } else {
                    let result = self.create_meeting(meeting_id).await;
                    let _ = respond_to.send(result);
                }
            }

            #[cfg(feature = "test-seams")]
            ControllerMessage::CreateMeetingWithSeams {
                meeting_id,
                seams,
                respond_to,
            } => {
                if self.tearing_down.contains_key(&meeting_id) {
                    self.park_create(&meeting_id, ParkedCreate::WithSeams(seams, respond_to));
                } else {
                    let result = self.create_meeting_with_seams(meeting_id, seams).await;
                    let _ = respond_to.send(result);
                }
            }

            ControllerMessage::GetMeeting {
                meeting_id,
                respond_to,
            } => {
                let result = self.get_meeting(&meeting_id).await;
                let _ = respond_to.send(result);
            }

            ControllerMessage::GetMeetingHandle {
                meeting_id,
                respond_to,
            } => {
                let result = self.get_meeting_handle(&meeting_id);
                let _ = respond_to.send(result);
            }

            ControllerMessage::JoinConnection {
                meeting_id,
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
                match self.get_meeting_handle(&meeting_id) {
                    Ok(meeting_handle) => {
                        // Spawn the join as a background task so we don't block the controller
                        tokio::spawn(async move {
                            let result = meeting_handle
                                .connection_join(
                                    connection_id,
                                    user_id,
                                    participant_id,
                                    display_name,
                                    is_host,
                                    identity_public_key,
                                    Some(stream_tx),
                                    media,
                                )
                                .await;
                            let _ = respond_to.send(result);
                        });
                    }
                    Err(e) => {
                        let _ = respond_to.send(Err(e));
                    }
                }
            }

            ControllerMessage::RemoveMeeting {
                meeting_id,
                respond_to,
            } => {
                let result = self.remove_meeting(&meeting_id).await;
                let _ = respond_to.send(result);
            }

            ControllerMessage::GetStatus { respond_to } => {
                let status = self.get_status();
                let _ = respond_to.send(status);
            }

            ControllerMessage::Shutdown {
                deadline,
                respond_to,
            } => match self.initiate_shutdown(deadline).await {
                // Answered after `graceful_shutdown` has drained the meeting
                // actors, not now: the caller's next step is settling their
                // teardowns, which must all have been handed off first.
                Ok(()) => self.shutdown_waiter = Some(respond_to),
                Err(e) => {
                    let _ = respond_to.send(Err(e));
                }
            },
        }
    }

    /// Create a new meeting actor.
    async fn create_meeting(&mut self, meeting_id: String) -> Result<(), McError> {
        self.create_meeting_inner(meeting_id, None).await
    }

    /// Test-only entry point threading construction overrides.
    #[cfg(feature = "test-seams")]
    async fn create_meeting_with_seams(
        &mut self,
        meeting_id: String,
        seams: super::meeting_media::MeetingSeams,
    ) -> Result<(), McError> {
        self.create_meeting_inner(meeting_id, Some(seams)).await
    }

    /// Shared body. `seams` is `None` in production; the test seam supplies
    /// `Some`.
    ///
    /// One body rather than two so seam tests drive the real creation path — a
    /// forked copy would drift and the tests would stop testing production
    /// behaviour.
    async fn create_meeting_inner(
        &mut self,
        meeting_id: String,
        #[cfg(feature = "test-seams")] seams: Option<super::meeting_media::MeetingSeams>,
        #[cfg(not(feature = "test-seams"))] seams: Option<std::convert::Infallible>,
    ) -> Result<(), McError> {
        // Check if we're accepting new meetings
        if !self.accepting_new {
            return Err(McError::Draining);
        }

        // Check if meeting already exists (MINOR-001: don't include meeting_id in error)
        if self.meetings.contains_key(&meeting_id) {
            return Err(McError::Conflict("Meeting already exists".to_string()));
        }

        debug!(
            target: "mc.actor.controller",
            mc_id = %self.mc_id,
            meeting_id = %meeting_id,
            "Creating new meeting actor"
        );

        // Create child token for the meeting
        let meeting_token = self.cancel_token.child_token();

        // Create the meeting actor (with master_secret for session binding tokens)
        // Create a new SecretBox from the exposed secret bytes for each meeting
        let meeting_secret = SecretBox::new(Box::new(self.master_secret.expose_secret().clone()));
        // Fails closed if the system CSPRNG cannot produce the meeting KEK
        // (ADR-0036 §4): no meeting is created rather than one with predictable
        // or absent key material.
        let wiring = MeetingWiring {
            lifecycle: self.lifecycle_tx.clone(),
            muted_census: Arc::clone(&self.muted_census),
            teardowns: Arc::clone(&self.teardowns),
        };
        #[cfg(feature = "test-seams")]
        let (handle, task_handle) = match seams {
            Some(seams) => MeetingActor::spawn_with_seams_wired(
                meeting_id.clone(),
                meeting_token,
                Arc::clone(&self.metrics),
                Arc::clone(&self.controller_metrics),
                meeting_secret,
                Arc::clone(&self.kek_lifecycle),
                &seams,
                Some(wiring),
            )?,
            None => MeetingActor::spawn_managed(
                meeting_id.clone(),
                meeting_token,
                Arc::clone(&self.metrics),
                Arc::clone(&self.controller_metrics),
                meeting_secret,
                Arc::clone(&self.kek_lifecycle),
                wiring,
            )?,
        };
        #[cfg(not(feature = "test-seams"))]
        let _ = seams;
        #[cfg(not(feature = "test-seams"))]
        let (handle, task_handle) = MeetingActor::spawn_managed(
            meeting_id.clone(),
            meeting_token,
            Arc::clone(&self.metrics),
            Arc::clone(&self.controller_metrics),
            meeting_secret,
            Arc::clone(&self.kek_lifecycle),
            wiring,
        )?;

        let created_at = chrono::Utc::now().timestamp();

        self.meetings.insert(
            meeting_id.clone(),
            ManagedMeeting {
                handle,
                task_handle,
                created_at,
            },
        );

        self.metrics.meeting_created();

        info!(
            target: "mc.actor.controller",
            mc_id = %self.mc_id,
            meeting_id = %meeting_id,
            total_meetings = self.meetings.len(),
            "Meeting actor created"
        );

        Ok(())
    }

    /// Get a cloned handle to a meeting actor for connection handling.
    ///
    /// A meeting id whose previous incarnation is still being released answers
    /// `MeetingTeardownInProgress` rather than `MeetingNotFound`: a rejoin that
    /// lands here during a teardown is "try again in a moment", and an operator
    /// must be able to tell that apart from a genuinely unknown meeting.
    fn get_meeting_handle(&self, meeting_id: &str) -> Result<MeetingActorHandle, McError> {
        match self.meetings.get(meeting_id) {
            Some(managed) => Ok(managed.handle.clone()),
            None if self.tearing_down.contains_key(meeting_id) => {
                Err(McError::MeetingTeardownInProgress)
            }
            None => Err(McError::MeetingNotFound(meeting_id.to_string())),
        }
    }

    /// Get information about a meeting.
    ///
    /// Queries the `MeetingActor` to get the actual participant count and fencing generation.
    async fn get_meeting(&self, meeting_id: &str) -> Result<MeetingInfo, McError> {
        match self.meetings.get(meeting_id) {
            Some(managed) => {
                // Query the meeting actor to get actual participant count and state
                match managed.handle.get_state().await {
                    Ok(state) => Ok(MeetingInfo {
                        meeting_id: meeting_id.to_string(),
                        participant_count: state.participants.len(),
                        created_at: managed.created_at,
                        fencing_generation: state.fencing_generation,
                    }),
                    Err(_) => {
                        // Meeting actor may have shut down - return cached info
                        warn!(
                            target: "mc.actor.controller",
                            mc_id = %self.mc_id,
                            meeting_id = %meeting_id,
                            "Failed to query meeting actor state, returning cached info"
                        );
                        Ok(MeetingInfo {
                            meeting_id: meeting_id.to_string(),
                            participant_count: 0,
                            created_at: managed.created_at,
                            fencing_generation: 0,
                        })
                    }
                }
            }
            None => Err(McError::MeetingNotFound(meeting_id.to_string())),
        }
    }

    /// Remove a meeting.
    ///
    /// This method initiates meeting removal but does not block waiting for
    /// the meeting actor task to complete. The cleanup is spawned as a
    /// background task to avoid blocking the message loop (ADR-0023).
    async fn remove_meeting(&mut self, meeting_id: &str) -> Result<(), McError> {
        match self.meetings.remove(meeting_id) {
            Some(managed) => {
                debug!(
                    target: "mc.actor.controller",
                    mc_id = %self.mc_id,
                    meeting_id = %meeting_id,
                    "Removing meeting actor"
                );

                // Cancel the meeting actor
                managed.handle.cancel();

                // Spawn background task to wait for cleanup - don't block the message loop
                let meeting_id_owned = meeting_id.to_string();
                let mc_id = self.mc_id.clone();
                tokio::spawn(async move {
                    match tokio::time::timeout(Duration::from_secs(5), managed.task_handle).await {
                        Ok(Ok(())) => {
                            debug!(
                                target: "mc.actor.controller",
                                mc_id = %mc_id,
                                meeting_id = %meeting_id_owned,
                                "Meeting actor task completed cleanly"
                            );
                        }
                        Ok(Err(e)) => {
                            warn!(
                                target: "mc.actor.controller",
                                mc_id = %mc_id,
                                meeting_id = %meeting_id_owned,
                                error = ?e,
                                "Meeting actor task panicked during removal"
                            );
                        }
                        Err(_) => {
                            warn!(
                                target: "mc.actor.controller",
                                mc_id = %mc_id,
                                meeting_id = %meeting_id_owned,
                                "Meeting actor task cleanup timed out"
                            );
                        }
                    }
                });

                self.metrics.meeting_removed();

                // Release this meeting's `policy_generation` state (ADR-0036 §8).
                // SIBLING: `check_meeting_health()` reaps meetings on a second
                // teardown path and must release the same state; the two sites
                // are one teardown contract. (Participant connectivity needs no
                // release here: it lives in the meeting actor and dies with it.)
                // Without this
                // the registry retains a whole `HandlerAssignment` per ended
                // meeting for the pod's lifetime. Keyed on MEETING teardown
                // only — a live meeting whose handler set changes keeps its
                // generations, or it would restart at 1 mid-meeting and MH would
                // correctly ignore the push as stale.
                self.policy_generations.remove_meeting(meeting_id).await;
                // Same teardown contract: a removed meeting must not pin the KEK
                // fleet gauges (the actor also evicts itself on a clean exit).
                self.kek_lifecycle.remove_meeting(meeting_id).await;

                info!(
                    target: "mc.actor.controller",
                    mc_id = %self.mc_id,
                    meeting_id = %meeting_id,
                    total_meetings = self.meetings.len(),
                    "Meeting actor removed"
                );

                Ok(())
            }
            None => Err(McError::MeetingNotFound(meeting_id.to_string())),
        }
    }

    /// Get current controller status.
    fn get_status(&self) -> ControllerStatus {
        ControllerStatus {
            meeting_count: self.meetings.len(),
            connection_count: self.metrics.connection_count(),
            is_draining: !self.accepting_new,
            mailbox_depth: self.mailbox.current_depth(),
        }
    }

    /// Initiate graceful shutdown.
    async fn initiate_shutdown(&mut self, _deadline: Duration) -> Result<(), McError> {
        info!(
            target: "mc.actor.controller",
            mc_id = %self.mc_id,
            meeting_count = self.meetings.len(),
            "Initiating graceful shutdown"
        );

        // Stop accepting new meetings
        self.accepting_new = false;

        // Cancel the root token (propagates to all children)
        self.cancel_token.cancel();

        Ok(())
    }

    /// Perform graceful shutdown.
    async fn graceful_shutdown(&mut self) {
        info!(
            target: "mc.actor.controller",
            mc_id = %self.mc_id,
            meeting_count = self.meetings.len(),
            "Performing graceful shutdown"
        );

        // Stop accepting new meetings
        self.accepting_new = false;

        // Cancel all meeting actors (already done via parent token, but be explicit)
        for (meeting_id, managed) in &self.meetings {
            debug!(
                target: "mc.actor.controller",
                mc_id = %self.mc_id,
                meeting_id = %meeting_id,
                "Cancelling meeting actor"
            );
            managed.handle.cancel();
        }

        // Wait for all meeting tasks to complete
        for (meeting_id, managed) in self.meetings.drain() {
            match tokio::time::timeout(Duration::from_secs(30), managed.task_handle).await {
                Ok(Ok(())) => {
                    debug!(
                        target: "mc.actor.controller",
                        mc_id = %self.mc_id,
                        meeting_id = %meeting_id,
                        "Meeting actor completed cleanly"
                    );
                }
                Ok(Err(e)) => {
                    warn!(
                        target: "mc.actor.controller",
                        mc_id = %self.mc_id,
                        meeting_id = %meeting_id,
                        error = ?e,
                        "Meeting actor task panicked during shutdown"
                    );
                }
                Err(_) => {
                    warn!(
                        target: "mc.actor.controller",
                        mc_id = %self.mc_id,
                        meeting_id = %meeting_id,
                        "Meeting actor shutdown timed out"
                    );
                }
            }
        }

        info!(
            target: "mc.actor.controller",
            mc_id = %self.mc_id,
            "Graceful shutdown complete"
        );
    }

    /// Check health of managed meeting actors.
    async fn check_meeting_health(&mut self) {
        let mut failed_meetings = Vec::new();

        for (meeting_id, managed) in &self.meetings {
            if managed.task_handle.is_finished() {
                warn!(
                    target: "mc.actor.controller",
                    mc_id = %self.mc_id,
                    meeting_id = %meeting_id,
                    "Meeting actor task finished unexpectedly"
                );
                failed_meetings.push(meeting_id.clone());
            }
        }

        // Handle failed meetings
        for meeting_id in failed_meetings {
            if let Some(managed) = self.meetings.remove(&meeting_id) {
                // Check if it was a panic
                match managed.task_handle.await {
                    Ok(()) => {
                        // Clean exit, meeting ended naturally
                        info!(
                            target: "mc.actor.controller",
                            mc_id = %self.mc_id,
                            meeting_id = %meeting_id,
                            "Meeting actor exited cleanly"
                        );
                    }
                    Err(join_error) => {
                        // Panic or cancellation
                        if join_error.is_panic() {
                            error!(
                                target: "mc.actor.controller",
                                mc_id = %self.mc_id,
                                meeting_id = %meeting_id,
                                error = ?join_error,
                                "Meeting actor panicked - triggering investigation"
                            );
                            self.metrics.record_panic(ActorType::Meeting);

                            // TODO (Phase 6e): Trigger meeting migration to another MC
                        }
                    }
                }

                self.metrics.meeting_removed();

                // SECOND TEARDOWN PATH — the generation registry must be released here
                // too, and this is not the exotic one: the clean-exit arm above
                // logs "Meeting actor exited cleanly", i.e. an ordinary
                // end-of-meeting, and this reaper runs on every iteration of the
                // controller loop. `meeting_removed()` already fires on both
                // paths; before this, the metric counted the teardown and the
                // registry did not observe it, so a meeting reaped here
                // retained a whole `HandlerAssignment` for the pod's lifetime.
                //
                // Safe to evict rather than preserve: a reaped meeting is not
                // restarted on this pod (migration is the Phase 6e TODO above),
                // so the restart-at-1 hazard that makes eviction key on MEETING
                // teardown rather than handler change does not arise — the
                // meeting is gone.
                //
                // Keep this call in step with `remove_meeting()`: it is one
                // teardown contract expressed at two sites, and the reason this
                // gap once existed is that only one site was wired. (Before
                // story 2 task 20 a second call released the per-meeting
                // `MhConnectionRegistry`; connectivity now lives in the meeting
                // actor and is released with it.)
                self.policy_generations.remove_meeting(&meeting_id).await;
                // The reaping path is the one that matters most here: a
                // panicked actor never reaches its own eviction, and a stale
                // pending age would page forever.
                self.kek_lifecycle.remove_meeting(&meeting_id).await;
            }
        }

        // The LOAD-BEARING publish of `mc_media_server_muted_sources`: prune the
        // census to the meetings that still exist (a panicked actor never
        // cleared its own entry) and re-set the pod total from it. Mute changes
        // also publish, but only for freshness — see `MutedSourceCensus`.
        let meetings = &self.meetings;
        self.muted_census.retain(|id| meetings.contains_key(id));
        self.muted_census.publish();
    }

    /// Park `create` behind `meeting_id`'s teardown fence (the caller has
    /// checked there is one); answer it at once if the fence's queue is full.
    fn park_create(&mut self, meeting_id: &str, create: ParkedCreate) {
        let Some(fence) = self.tearing_down.get_mut(meeting_id) else {
            create.respond(Err(McError::Internal("no teardown fence".to_string())));
            return;
        };
        if fence.waiting.len() >= MAX_QUEUED_CREATES {
            warn!(
                target: "mc.actor.controller",
                mc_id = %self.mc_id,
                meeting_id = %meeting_id,
                "Create refused: too many creates already waiting behind this meeting's teardown"
            );
            create.respond(Err(McError::MeetingTeardownInProgress));
            return;
        }
        debug!(
            target: "mc.actor.controller",
            mc_id = %self.mc_id,
            meeting_id = %meeting_id,
            "Create parked behind the previous incarnation's teardown"
        );
        fence.waiting.push(create);
    }

    /// Process every lifecycle event already queued, without waiting.
    async fn drain_lifecycle(&mut self) {
        while let Ok(event) = self.lifecycle_rx.try_recv() {
            self.on_lifecycle(event).await;
        }
    }

    async fn on_lifecycle(&mut self, event: MeetingLifecycleEvent) {
        match event {
            MeetingLifecycleEvent::Ended { meeting_id, cause } => {
                self.reap_ended(&meeting_id, cause).await;
            }
            MeetingLifecycleEvent::TeardownComplete { meeting_id, cause } => {
                self.teardown_complete(&meeting_id, cause, false).await;
            }
        }
    }

    /// A meeting actor ended through its clean exit path: reap it and FENCE
    /// the id until its handlers are released.
    ///
    /// Generations are NOT evicted here: the teardown evicts them after its
    /// drain, as `EndMeetingRequest` requires. (The panic path in
    /// `check_meeting_health` evicts immediately — no worker survives a
    /// dropped actor there.)
    async fn reap_ended(&mut self, meeting_id: &str, cause: MeetingEndCause) {
        if self.meetings.remove(meeting_id).is_some() {
            self.metrics.meeting_removed();
        }
        self.kek_lifecycle.remove_meeting(meeting_id).await;
        self.tearing_down
            .entry(meeting_id.to_string())
            .or_insert_with(|| TeardownFence {
                deadline: tokio::time::Instant::now()
                    + crate::media_routing::teardown::TEARDOWN_MAX
                    + FENCE_BACKSTOP_MARGIN,
                waiting: Vec::new(),
                cause,
            });
        info!(
            target: "mc.actor.controller",
            mc_id = %self.mc_id,
            meeting_id = %meeting_id,
            cause = cause.label(),
            total_meetings = self.meetings.len(),
            "Meeting ended; releasing it on its media handlers"
        );
    }

    /// A teardown finished (or its backstop fired): lift the fence, serve the
    /// creates that waited on it, and tell GC — for a meeting that is actually
    /// OVER. Told only now, after the release, so GC cannot re-assign the id
    /// while its handlers still hold the previous incarnation (and so that a
    /// release mismatch stays a genuine MC-defect signal).
    async fn teardown_complete(
        &mut self,
        meeting_id: &str,
        cause: MeetingEndCause,
        backstop: bool,
    ) {
        let Some(fence) = self.tearing_down.remove(meeting_id) else {
            return;
        };
        if backstop {
            crate::observability::metrics::record_teardown_fence_backstop();
            error!(
                target: "mc.actor.controller",
                mc_id = %self.mc_id,
                meeting_id = %meeting_id,
                cause = fence.cause.label(),
                "Teardown fence lifted by its deadline, not by the teardown reporting completion \
                 (the teardown task hung or vanished). Creates for this id proceed, and GC is \
                 told the meeting ended if it did; if the teardown is still releasing, it may \
                 release the new incarnation."
            );
        }
        for create in fence.waiting {
            match create {
                ParkedCreate::Plain(respond_to) => {
                    let result = self.create_meeting(meeting_id.to_string()).await;
                    let _ = respond_to.send(result);
                }
                #[cfg(feature = "test-seams")]
                ParkedCreate::WithSeams(seams, respond_to) => {
                    let result = self
                        .create_meeting_with_seams(meeting_id.to_string(), seams)
                        .await;
                    let _ = respond_to.send(result);
                }
            }
        }
        // The backstop notifies too. Suppressing it would leave GC reusing a
        // live assignment row for a meeting MC no longer holds, so every join
        // to the id gets `meeting_not_found` until MC restarts — a permanently
        // unjoinable id, strictly worse than the release-crossing race that
        // lifting the fence has already accepted.
        if cause.notifies_gc() {
            self.meeting_ended.meeting_ended(meeting_id);
        }
    }

    /// Lift every fence whose backstop deadline has passed.
    async fn expire_fences(&mut self) {
        let now = tokio::time::Instant::now();
        let expired: Vec<(String, MeetingEndCause)> = self
            .tearing_down
            .iter()
            .filter(|(_, f)| f.deadline <= now)
            .map(|(id, f)| (id.clone(), f.cause))
            .collect();
        for (meeting_id, cause) in expired {
            // No report arrived, so the cause is the one recorded at reap.
            self.teardown_complete(&meeting_id, cause, true).await;
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// Test secret for session binding (32 bytes as required by ADR-0023).
    fn test_secret() -> SecretBox<Vec<u8>> {
        SecretBox::new(Box::new(vec![0u8; 32]))
    }

    /// A non-empty assignment, distinct from `HandlerAssignment::default()`, so
    /// eviction tests can tell "released" from "retained".
    fn seeded_assignment() -> crate::media_routing::HandlerAssignment {
        crate::media_routing::HandlerAssignment {
            egress_streams: vec![crate::media_routing::EgressStreamPlan {
                egress_stream_id: 0x0000_0100,
                subscriber: crate::media_admission::SenderId::from_nonzero(
                    std::num::NonZeroU16::new(1).unwrap(),
                ),
                slot_id: 0,
                candidate_sources: Vec::new(),
                stream_number: 0,
                priority_group: 1,
                supersede_on_independent_frame: false,
                transport_mode: proto_gen::dark_tower::signaling::v1::TransportMode::Datagram,
            }],
            server_muted_sources: std::collections::BTreeSet::new(),
        }
    }

    fn test_generations() -> Arc<PolicyGenerations> {
        Arc::new(PolicyGenerations::new())
    }

    /// SEC-5 regression: the SECOND teardown path must release the generation registry.
    ///
    /// `remove_meeting()` was wired at Gate 1; `check_meeting_health()` — which
    /// runs on every controller-loop iteration and reaps meetings whose actor
    /// task has finished, including the ordinary "exited cleanly" case — was
    /// not. A meeting reaped there retained a whole `HandlerAssignment` for the
    /// pod's lifetime.
    ///
    /// This drives the reaper directly rather than through the handle, because
    /// there is no public seam to end one meeting actor without going through
    /// `remove_meeting()` — i.e. through the path that was already correct,
    /// which is exactly why the existing eviction test stayed green with the bug
    /// present. Adding a production seam to reach it would be a larger change
    /// than the fix.
    #[tokio::test]
    async fn check_meeting_health_releases_generation_state_for_a_reaped_meeting() {
        let generations = test_generations();
        let (_tx, rx) = mpsc::channel(8);
        let mut actor = MeetingControllerActor::new(
            "mc-test-reap".to_string(),
            rx,
            CancellationToken::new(),
            ActorMetrics::new(),
            ControllerMetrics::new(),
            test_secret(),
            Arc::clone(&generations),
            crate::media_admission::fixtures::kek_lifecycle(),
        );

        actor
            .create_meeting("meeting-reaped".to_string())
            .await
            .expect("create");

        // Seed the generation registry as a live meeting would.
        // Seeded assignment must DIFFER from the one probed after the reap:
        // `next_generation` returns the SAME number for an identical assignment,
        // so seeding and probing with equal values would read 1 whether or not
        // the entry survived — an assertion that cannot fail.
        generations
            .next_generation(
                "meeting-reaped",
                &crate::media_routing::HandlerId::new("mh-0"),
                &seeded_assignment(),
            )
            .await
            .expect("generation");

        // End the meeting actor the way an ordinary meeting ends, then wait for
        // its task to actually finish so the reaper sees it.
        actor
            .meetings
            .get("meeting-reaped")
            .expect("meeting present")
            .handle
            .cancel();
        for _ in 0..200 {
            if actor.meetings["meeting-reaped"].task_handle.is_finished() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(
            actor.meetings["meeting-reaped"].task_handle.is_finished(),
            "meeting actor task should have exited after cancel"
        );

        actor.check_meeting_health().await;

        assert!(
            !actor.meetings.contains_key("meeting-reaped"),
            "the reaper should have removed the meeting"
        );
        // Probe with an assignment DIFFERENT from the seeded one: evicted -> 1
        // (first ever for the pair), retained -> 2 (a changed assignment
        // advances). This is what makes the assertion able to fail.
        let after = generations
            .next_generation(
                "meeting-reaped",
                &crate::media_routing::HandlerId::new("mh-0"),
                &crate::media_routing::HandlerAssignment::default(),
            )
            .await
            .expect("generation");
        assert_eq!(
            after.get(),
            1,
            "policy_generations must be released on the reaping path too"
        );
    }

    #[tokio::test]
    async fn test_controller_handle_create_meeting() {
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let handle = MeetingControllerActorHandle::new(
            "mc-test-001".to_string(),
            metrics,
            controller_metrics,
            test_secret(),
            test_generations(),
            crate::media_admission::fixtures::kek_lifecycle(),
        );

        // Create a meeting
        let result = handle.create_meeting("meeting-123".to_string()).await;
        assert!(result.is_ok());

        // Get the meeting
        let info = handle.get_meeting("meeting-123".to_string()).await;
        assert!(info.is_ok());
        let info = info.unwrap();
        assert_eq!(info.meeting_id, "meeting-123");

        // Cleanup
        handle.cancel();
    }

    #[tokio::test]
    async fn test_controller_handle_duplicate_meeting() {
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let handle = MeetingControllerActorHandle::new(
            "mc-test-002".to_string(),
            metrics,
            controller_metrics,
            test_secret(),
            test_generations(),
            crate::media_admission::fixtures::kek_lifecycle(),
        );

        // Create first meeting
        let result = handle.create_meeting("meeting-456".to_string()).await;
        assert!(result.is_ok());

        // Try to create duplicate
        let result = handle.create_meeting("meeting-456".to_string()).await;
        assert!(matches!(result, Err(McError::Conflict(_))));

        // Cleanup
        handle.cancel();
    }

    #[tokio::test]
    async fn test_controller_handle_get_nonexistent_meeting() {
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let handle = MeetingControllerActorHandle::new(
            "mc-test-003".to_string(),
            metrics,
            controller_metrics,
            test_secret(),
            test_generations(),
            crate::media_admission::fixtures::kek_lifecycle(),
        );

        let result = handle.get_meeting("nonexistent".to_string()).await;
        assert!(matches!(result, Err(McError::MeetingNotFound(_))));

        // Cleanup
        handle.cancel();
    }

    #[tokio::test]
    async fn test_controller_handle_remove_meeting() {
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let handle = MeetingControllerActorHandle::new(
            "mc-test-004".to_string(),
            metrics,
            controller_metrics,
            test_secret(),
            test_generations(),
            crate::media_admission::fixtures::kek_lifecycle(),
        );

        // Create a meeting
        let result = handle.create_meeting("meeting-789".to_string()).await;
        assert!(result.is_ok());

        // Remove it
        let result = handle.remove_meeting("meeting-789".to_string()).await;
        assert!(result.is_ok());

        // Verify it's gone
        let result = handle.get_meeting("meeting-789".to_string()).await;
        assert!(matches!(result, Err(McError::MeetingNotFound(_))));

        // Cleanup
        handle.cancel();
    }

    #[tokio::test]
    async fn test_controller_handle_status() {
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let handle = MeetingControllerActorHandle::new(
            "mc-test-005".to_string(),
            metrics,
            controller_metrics,
            test_secret(),
            test_generations(),
            crate::media_admission::fixtures::kek_lifecycle(),
        );

        // Get initial status
        let status = handle.get_status().await;
        assert!(status.is_ok());
        let status = status.unwrap();
        assert_eq!(status.meeting_count, 0);
        assert!(!status.is_draining);

        // Create some meetings
        let _ = handle.create_meeting("m1".to_string()).await;
        let _ = handle.create_meeting("m2".to_string()).await;

        let status = handle.get_status().await.unwrap();
        assert_eq!(status.meeting_count, 2);

        // Cleanup
        handle.cancel();
    }

    #[tokio::test]
    async fn test_controller_handle_shutdown() {
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let handle = MeetingControllerActorHandle::new(
            "mc-test-006".to_string(),
            metrics,
            controller_metrics,
            test_secret(),
            test_generations(),
            crate::media_admission::fixtures::kek_lifecycle(),
        );

        // Create a meeting
        let _ = handle.create_meeting("meeting-shutdown".to_string()).await;

        // Initiate shutdown - this triggers cancellation
        let result = handle.shutdown(Duration::from_secs(30)).await;
        assert!(result.is_ok());

        // Give time for cancellation to start
        tokio::time::sleep(Duration::from_millis(10)).await;

        // After shutdown, the controller is cancelled
        assert!(handle.is_cancelled());

        // Operations after shutdown may fail since actor is shutting down
        // This is expected behavior - the actor cancels after shutdown
    }

    #[tokio::test]
    async fn test_controller_cancellation_token() {
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let handle = MeetingControllerActorHandle::new(
            "mc-test-007".to_string(),
            metrics,
            controller_metrics,
            test_secret(),
            test_generations(),
            crate::media_admission::fixtures::kek_lifecycle(),
        );

        assert!(!handle.is_cancelled());

        let child = handle.child_token();
        assert!(!child.is_cancelled());

        handle.cancel();

        // Give time for cancellation to propagate
        tokio::time::sleep(Duration::from_millis(10)).await;

        assert!(handle.is_cancelled());
        assert!(child.is_cancelled());
    }

    // ------------------------------------------------------------------
    // Fence backstop (OPS-19): the cause recorded at reap decides the GC
    // notification even though no completion report ever arrives.
    // ------------------------------------------------------------------

    #[derive(Default)]
    struct RecordingNotifier(std::sync::Mutex<Vec<String>>);

    impl MeetingEndedNotifier for RecordingNotifier {
        fn meeting_ended(&self, meeting_id: &str) {
            self.0.lock().unwrap().push(meeting_id.to_string());
        }
    }

    fn backstop_actor(notifier: Arc<RecordingNotifier>) -> MeetingControllerActor {
        let (_tx, rx) = mpsc::channel(8);
        let mut actor = MeetingControllerActor::new(
            "mc-test-backstop".to_string(),
            rx,
            CancellationToken::new(),
            ActorMetrics::new(),
            ControllerMetrics::new(),
            test_secret(),
            test_generations(),
            crate::media_admission::fixtures::kek_lifecycle(),
        );
        actor.meeting_ended = notifier;
        actor
    }

    #[tokio::test(start_paused = true)]
    async fn a_backstop_lifted_fence_still_tells_gc_an_ended_meeting_ended() {
        let notifier = Arc::new(RecordingNotifier::default());
        let mut actor = backstop_actor(Arc::clone(&notifier));
        actor.reap_ended("m-empty", MeetingEndCause::Empty).await;
        actor
            .reap_ended("m-shutdown", MeetingEndCause::Shutdown)
            .await;

        // Before the deadline nothing lifts (positive control on the timer).
        actor.expire_fences().await;
        assert_eq!(actor.tearing_down.len(), 2);
        assert!(notifier.0.lock().unwrap().is_empty());

        tokio::time::advance(
            crate::media_routing::teardown::TEARDOWN_MAX
                + FENCE_BACKSTOP_MARGIN
                + Duration::from_millis(1),
        )
        .await;
        actor.expire_fences().await;

        assert!(actor.tearing_down.is_empty(), "both fences lifted");
        assert_eq!(
            *notifier.0.lock().unwrap(),
            vec!["m-empty".to_string()],
            "the ended meeting is reported to GC on the backstop path; the shutdown one never is"
        );

        // A late real completion finds no fence and does not notify twice.
        actor
            .teardown_complete("m-empty", MeetingEndCause::Empty, false)
            .await;
        assert_eq!(notifier.0.lock().unwrap().len(), 1);
    }
}
