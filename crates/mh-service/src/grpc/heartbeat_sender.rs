//! The MH -> MC per-meeting heartbeat SENDER task (story 4 MH-2; ADR-0039
//! §1, §6).
//!
//! The session actor BUILDS every beat (so a sent beat implies a live actor)
//! and hands it here over a bounded queue. This task owns only transport
//! concerns and never touches session state (ADR-0001): a RELEASED verdict
//! goes back to the actor as a lifecycle-mailbox message
//! ([`crate::session::VerdictSink`]).
//!
//! # Delivery rules (`MeetingHeartbeatRequest` DELIVERY)
//!
//! - **At most one outstanding beat per meeting.** A beat whose meeting's
//!   previous beat is still in flight is skipped and counted
//!   (`skipped_in_flight`).
//! - **Each beat runs on its own task** in a `JoinSet`, so one meeting's hung
//!   call never delays another meeting's beat.
//! - **Per-attempt deadline below the interval**
//!   ([`crate::session::heartbeat::per_attempt_deadline`]); **no retry** — the
//!   next tick is the retry.
//! - **The destination is the meeting's REGISTERED endpoint only**, and the
//!   ids MH acts on are the BEAT's, never the response's.
//! - MH never ends a meeting because MC is unreachable: an error, a timeout,
//!   `FAILED_PRECONDITION` (owner mismatch) or an `UNSPECIFIED` verdict is
//!   counted and nothing else.
//!
//! # Bounded state
//!
//! The channel cache and the failure-streak map are keyed by caller-chosen
//! strings (an MC endpoint, a meeting id), so both are BOUNDED: entries are
//! dropped on the actor's best-effort `Evict` / `Forget` commands, and each map
//! is capped at the registered-meeting cap (live endpoints and live meetings
//! never exceed it), evicting the oldest insert at the cap. Evicting a live
//! channel only costs a rebuild.
//!
//! # Logging
//!
//! Transitions only, never per beat (ADR-0039 §8): WARN when a meeting's beats
//! start failing (or the failure kind changes), INFO when they recover. Ids go
//! in log fields, never in metric labels. A `tonic::Status` is never logged
//! (its Debug carries metadata): only its `Code`, as the WARN's `grpc_code`
//! field (a change of code is a transition and re-WARNs).

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::sync::{Arc, Mutex, PoisonError};

use common::observability::otel_grpc::client_interceptor;
use common::token_manager::TokenReceiver;
use proto_gen::dark_tower::internal::v1::media_coordination_service_client::MediaCoordinationServiceClient;
use proto_gen::dark_tower::internal::v1::{
    HeartbeatVerdict, MeetingHeartbeatRequest, MeetingHeartbeatResponse,
};
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tonic::transport::{Channel, Endpoint};
use tracing::{info, warn};

use crate::grpc::mc_client::MC_CONNECT_TIMEOUT;
use crate::observability::metrics::{self, McHeartbeatOutcome, McHeartbeatVerdict};
use crate::session::heartbeat::{per_attempt_deadline, OutboundBeat, SenderCommand};
use crate::session::VerdictSink;

/// Why a heartbeat call failed. Bounded: no message text, no metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeartbeatError {
    /// MC answered with a gRPC status (a transport failure also surfaces as
    /// one, e.g. `UNAVAILABLE`).
    Status(tonic::Code),
    /// The registered endpoint cannot be dialled.
    EndpointInvalid,
    /// No usable service token: nothing was sent (fail closed).
    AuthUnavailable,
}

impl HeartbeatError {
    /// A bounded kind for log lines.
    #[must_use]
    pub const fn kind(self) -> &'static str {
        match self {
            Self::Status(tonic::Code::FailedPrecondition) => "owner_mismatch",
            Self::Status(_) => "status",
            Self::EndpointInvalid => "endpoint_invalid",
            Self::AuthUnavailable => "auth_unavailable",
        }
    }
}

/// The heartbeat RPC seam: production is [`TonicHeartbeatTransport`]; Tier 1b
/// tests inject a scripted fake (paused time over real sockets would let the
/// clock auto-advance mid-read and fire deadlines spuriously).
pub trait HeartbeatTransport: Send + Sync + 'static {
    /// Send one beat to `endpoint`. No retry.
    fn send(
        &self,
        endpoint: &str,
        request: MeetingHeartbeatRequest,
    ) -> impl Future<Output = Result<MeetingHeartbeatResponse, HeartbeatError>> + Send;

    /// Forget any cached connection state for `endpoint`.
    fn evict(&self, endpoint: &str);
}

/// Production transport: ONE cached, lazily-connected tonic `Channel` per MC
/// endpoint (endpoint-keyed, cloned out per call — the GC `McClient` shape),
/// so beats ride an established HTTP/2 connection instead of dialling per
/// call. The service token is read per call through
/// [`super::authorized_request`] and is never stored here.
pub struct TonicHeartbeatTransport {
    token_rx: TokenReceiver,
    channels: Mutex<ChannelCache>,
}

/// Endpoint -> (channel, insert order), capped.
struct ChannelCache {
    channels: HashMap<String, (Channel, u64)>,
    next_insert: u64,
    cap: usize,
}

impl TonicHeartbeatTransport {
    /// A transport whose cache holds at most `cap` endpoints (production:
    /// `MH_MAX_REGISTERED_MEETINGS`; live endpoints never exceed it).
    #[must_use]
    pub fn new(token_rx: TokenReceiver, cap: usize) -> Self {
        Self {
            token_rx,
            channels: Mutex::new(ChannelCache {
                channels: HashMap::new(),
                next_insert: 0,
                cap: cap.max(1),
            }),
        }
    }

    fn channel(&self, endpoint: &str) -> Result<Channel, HeartbeatError> {
        let mut cache = self.channels.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((channel, _)) = cache.channels.get(endpoint) {
            return Ok(channel.clone());
        }
        // Lazy: building never awaits, the connect happens inside the call
        // (and so inside its per-attempt deadline).
        let channel = Endpoint::from_shared(endpoint.to_owned())
            .map_err(|_| HeartbeatError::EndpointInvalid)?
            .connect_timeout(MC_CONNECT_TIMEOUT)
            .connect_lazy();
        let cap = cache.cap;
        evict_oldest_at_cap(&mut cache.channels, cap, |(_, order)| *order);
        let order = cache.next_insert;
        cache.next_insert = cache.next_insert.wrapping_add(1);
        cache
            .channels
            .insert(endpoint.to_owned(), (channel.clone(), order));
        Ok(channel)
    }

    /// How many endpoints are cached (tests).
    #[must_use]
    pub fn cached_endpoints(&self) -> usize {
        self.channels
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .channels
            .len()
    }
}

impl HeartbeatTransport for TonicHeartbeatTransport {
    async fn send(
        &self,
        endpoint: &str,
        request: MeetingHeartbeatRequest,
    ) -> Result<MeetingHeartbeatResponse, HeartbeatError> {
        let channel = self.channel(endpoint)?;
        let request = super::authorized_request(&self.token_rx, request)
            .map_err(|_| HeartbeatError::AuthUnavailable)?;
        let mut client =
            MediaCoordinationServiceClient::with_interceptor(channel, client_interceptor());
        client
            .meeting_heartbeat(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(|status| HeartbeatError::Status(status.code()))
    }

    fn evict(&self, endpoint: &str) {
        self.channels
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .channels
            .remove(endpoint);
    }
}

/// How one beat ended, for transition logging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BeatClass {
    /// MC answered with a usable verdict.
    Ok,
    /// Anything else: a bounded kind token, plus the gRPC code when MC
    /// answered with a status (UNAVAILABLE, UNAUTHENTICATED, UNIMPLEMENTED ...
    /// have different remedies, so they are logged and compared distinctly).
    Failing(&'static str, Option<tonic::Code>),
}

/// One finished beat task.
struct Finished {
    meeting_id: String,
    mc_id: String,
    class: BeatClass,
}

/// A meeting whose beats are failing: for WARN-once / INFO-on-recovery.
struct Streak {
    kind: (&'static str, Option<tonic::Code>),
    consecutive: u64,
    order: u64,
}

/// The sender task. Generic over the transport (no dynamic dispatch needed).
pub struct HeartbeatSender<S: HeartbeatTransport> {
    transport: Arc<S>,
    commands: mpsc::Receiver<SenderCommand>,
    verdicts: VerdictSink,
    in_flight: HashSet<String>,
    tasks: JoinSet<Finished>,
    /// Which meeting each beat task carries, so a panicked task still frees
    /// its meeting's in-flight slot.
    task_meetings: HashMap<tokio::task::Id, String>,
    streaks: HashMap<String, Streak>,
    next_streak: u64,
    cap: usize,
}

impl<S: HeartbeatTransport> HeartbeatSender<S> {
    /// A sender reading `commands` (the actor's queue), relaying RELEASED
    /// verdicts to `verdicts`, with per-meeting state capped at `cap`.
    #[must_use]
    pub fn new(
        transport: Arc<S>,
        commands: mpsc::Receiver<SenderCommand>,
        verdicts: VerdictSink,
        cap: usize,
    ) -> Self {
        Self {
            transport,
            commands,
            verdicts,
            in_flight: HashSet::new(),
            tasks: JoinSet::new(),
            task_meetings: HashMap::new(),
            streaks: HashMap::new(),
            next_streak: 0,
            cap: cap.max(1),
        }
    }

    /// Run until the actor's queue closes or `shutdown` fires. Dropping the
    /// `JoinSet` on exit aborts every in-flight beat, so shutdown never waits
    /// on MC.
    pub async fn run(mut self, shutdown: CancellationToken) {
        loop {
            tokio::select! {
                () = shutdown.cancelled() => break,
                command = self.commands.recv() => match command {
                    Some(command) => self.handle(command),
                    None => break,
                },
                Some(joined) = self.tasks.join_next_with_id(), if !self.tasks.is_empty() => {
                    let id = match &joined {
                        Ok((id, _)) => *id,
                        Err(error) => error.id(),
                    };
                    // The slot is freed on EVERY end, a panicked task's too,
                    // or that meeting would skip every later beat.
                    let meeting_id = self.task_meetings.remove(&id);
                    if let Some(meeting_id) = &meeting_id {
                        self.in_flight.remove(meeting_id);
                    }
                    match joined {
                        Ok((_, finished)) => self.log_transition(finished),
                        Err(error) => warn!(
                            target: "mh.grpc.heartbeat",
                            meeting_id = meeting_id.as_deref().unwrap_or(""),
                            panicked = error.is_panic(),
                            "MC heartbeat task ended abnormally; its slot is freed"
                        ),
                    }
                }
            }
        }
        self.tasks.abort_all();
    }

    fn handle(&mut self, command: SenderCommand) {
        match command {
            SenderCommand::Beat(beat) => self.dispatch(beat),
            SenderCommand::Evict { endpoint } => self.transport.evict(&endpoint),
            SenderCommand::Forget { meeting_id } => {
                self.streaks.remove(&meeting_id);
            }
        }
    }

    fn dispatch(&mut self, beat: OutboundBeat) {
        let meeting_id = beat.request.meeting_id.clone();
        if self.in_flight.contains(&meeting_id) {
            metrics::record_mc_heartbeat(McHeartbeatOutcome::SkippedInFlight);
            return;
        }
        let transport = Arc::clone(&self.transport);
        let verdicts = self.verdicts.clone();
        let handle = self.tasks.spawn(send_one(transport, verdicts, beat));
        self.task_meetings.insert(handle.id(), meeting_id.clone());
        self.in_flight.insert(meeting_id);
    }

    fn log_transition(&mut self, finished: Finished) {
        let Finished {
            meeting_id,
            mc_id,
            class,
        } = finished;
        match class {
            BeatClass::Ok => {
                if let Some(streak) = self.streaks.remove(&meeting_id) {
                    info!(
                        target: "mh.grpc.heartbeat",
                        meeting_id = %meeting_id,
                        mc_id = %mc_id,
                        failed_beats = streak.consecutive,
                        "MC heartbeats for this meeting recovered"
                    );
                }
            }
            BeatClass::Failing(token, grpc_code) => {
                if let Some(consecutive) = self.note_failure(&meeting_id, (token, grpc_code)) {
                    warn!(
                        target: "mh.grpc.heartbeat",
                        meeting_id = %meeting_id,
                        mc_id = %mc_id,
                        kind = token,
                        // `Code`'s Debug is the variant name only: no message,
                        // no metadata.
                        grpc_code = ?grpc_code,
                        consecutive_failures = consecutive,
                        "MC heartbeats for this meeting failing; no action is taken (MH never \
                         ends a meeting because MC is unreachable). Logged on transition only"
                    );
                }
            }
        }
    }

    /// Record one failed beat for `meeting_id`. Returns the consecutive count
    /// when this is a TRANSITION (the first failure, or a different kind or
    /// gRPC code), `None` while the same failure repeats.
    fn note_failure(
        &mut self,
        meeting_id: &str,
        kind: (&'static str, Option<tonic::Code>),
    ) -> Option<u64> {
        if let Some(streak) = self.streaks.get_mut(meeting_id) {
            streak.consecutive = streak.consecutive.saturating_add(1);
            let changed = streak.kind != kind;
            streak.kind = kind;
            return changed.then_some(streak.consecutive);
        }
        let order = self.next_streak;
        self.next_streak = self.next_streak.wrapping_add(1);
        evict_oldest_at_cap(&mut self.streaks, self.cap, |s| s.order);
        self.streaks.insert(
            meeting_id.to_owned(),
            Streak {
                kind,
                consecutive: 1,
                order,
            },
        );
        Some(1)
    }
}

/// Make room in a capped, caller-keyed map: at `cap`, drop the entry with the
/// smallest insert `order`. The ONE bound for both of this module's
/// caller-keyed maps (channel cache, failure streaks).
fn evict_oldest_at_cap<V>(map: &mut HashMap<String, V>, cap: usize, order: impl Fn(&V) -> u64) {
    if map.len() < cap {
        return;
    }
    let oldest = map
        .iter()
        .min_by_key(|(_, v)| order(v))
        .map(|(k, _)| k.clone());
    if let Some(oldest) = oldest {
        map.remove(&oldest);
    }
}

/// One beat, on its own task: send under the per-attempt deadline, count
/// exactly one outcome, classify the verdict, relay RELEASED to the actor.
async fn send_one<S: HeartbeatTransport>(
    transport: Arc<S>,
    verdicts: VerdictSink,
    beat: OutboundBeat,
) -> Finished {
    let OutboundBeat {
        request,
        endpoint,
        fence,
        interval,
    } = beat;
    let meeting_id = request.meeting_id.clone();
    let mc_id = request.mc_id.clone();
    let deadline = per_attempt_deadline(interval);
    let started = tokio::time::Instant::now();
    let class = match tokio::time::timeout(deadline, transport.send(&endpoint, request)).await {
        Err(_elapsed) => {
            metrics::record_mc_heartbeat(McHeartbeatOutcome::Timeout);
            BeatClass::Failing("timeout", None)
        }
        Ok(Err(error)) => {
            metrics::record_mc_heartbeat(McHeartbeatOutcome::Error);
            // Latency only for a call that completed ON THE WIRE (a status):
            // an undialable endpoint or a missing token sent nothing, and
            // its ~0 s would flatter the distribution.
            if matches!(error, HeartbeatError::Status(_)) {
                metrics::record_mc_heartbeat_latency(started.elapsed());
            }
            if error == HeartbeatError::Status(tonic::Code::FailedPrecondition) {
                metrics::record_mc_heartbeat_verdict(McHeartbeatVerdict::OwnerMismatch);
            }
            let grpc_code = match error {
                HeartbeatError::Status(code) => Some(code),
                HeartbeatError::EndpointInvalid | HeartbeatError::AuthUnavailable => None,
            };
            BeatClass::Failing(error.kind(), grpc_code)
        }
        Ok(Ok(response)) => {
            metrics::record_mc_heartbeat(McHeartbeatOutcome::Success);
            metrics::record_mc_heartbeat_latency(started.elapsed());
            match HeartbeatVerdict::try_from(response.verdict) {
                Ok(HeartbeatVerdict::Owned) => {
                    metrics::record_mc_heartbeat_verdict(McHeartbeatVerdict::Owned);
                    BeatClass::Ok
                }
                Ok(HeartbeatVerdict::Released) => {
                    // The actor fences, counts (released / released_stale /
                    // misroute) and releases. The ids are the beat's.
                    if verdicts
                        .released(meeting_id.clone(), fence, response.cause)
                        .await
                    {
                        BeatClass::Ok
                    } else {
                        // The actor is gone (shutdown): the verdict is neither
                        // applied nor counted. Never silent.
                        warn!(
                            target: "mh.grpc.heartbeat",
                            meeting_id = %meeting_id,
                            mc_id = %mc_id,
                            "Session actor gone; RELEASED verdict not applied"
                        );
                        BeatClass::Failing("actor_unavailable", None)
                    }
                }
                Ok(HeartbeatVerdict::Unspecified) | Err(_) => {
                    metrics::record_mc_heartbeat_verdict(McHeartbeatVerdict::UnspecifiedVerdict);
                    BeatClass::Failing("unspecified_verdict", None)
                }
            }
        }
    };
    Finished {
        meeting_id,
        mc_id,
        class,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use common::secret::SecretString;

    fn transport(cap: usize) -> TonicHeartbeatTransport {
        let (tx, rx) = tokio::sync::watch::channel(SecretString::from("tok"));
        std::mem::forget(tx);
        TonicHeartbeatTransport::new(TokenReceiver::from_test_channel(rx), cap)
    }

    fn cached(t: &TonicHeartbeatTransport, endpoint: &str) -> bool {
        t.channels.lock().unwrap().channels.contains_key(endpoint)
    }

    /// The cache is BOUNDED: at the cap the OLDEST insert goes; a hit does
    /// not re-insert; `evict` removes exactly the named endpoint.
    #[tokio::test]
    async fn the_channel_cache_is_capped_oldest_first_and_evictable() {
        let t = transport(2);
        for e in ["http://a:1", "http://b:1", "http://a:1", "http://c:1"] {
            t.channel(e).unwrap();
        }
        assert_eq!(t.cached_endpoints(), 2);
        assert!(
            !cached(&t, "http://a:1"),
            "the oldest insert is evicted at the cap"
        );
        assert!(cached(&t, "http://b:1") && cached(&t, "http://c:1"));
        t.evict("http://b:1");
        assert!(!cached(&t, "http://b:1"));
        assert!(
            cached(&t, "http://c:1"),
            "evict removes only the named endpoint"
        );
    }

    /// An undialable endpoint is `EndpointInvalid` and caches nothing.
    #[tokio::test]
    async fn an_undialable_endpoint_is_refused_and_not_cached() {
        let t = transport(2);
        assert_eq!(
            t.channel("not a uri").unwrap_err(),
            HeartbeatError::EndpointInvalid
        );
        assert_eq!(t.cached_endpoints(), 0);
    }

    /// No token: `AuthUnavailable`, and nothing is sent (fail closed).
    #[tokio::test]
    async fn an_empty_token_sends_nothing() {
        let (tx, rx) = tokio::sync::watch::channel(SecretString::from(""));
        std::mem::forget(tx);
        let t = TonicHeartbeatTransport::new(TokenReceiver::from_test_channel(rx), 2);
        let err = t
            .send("http://127.0.0.1:1", MeetingHeartbeatRequest::default())
            .await
            .unwrap_err();
        assert_eq!(err, HeartbeatError::AuthUnavailable);
    }
}
