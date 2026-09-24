//! One push worker per (meeting, handler): the MC→MH `RegisterMeeting` driver
//! (ADR-0036 §8; story 2 R-4).
//!
//! # Why a worker, and why one per handler
//!
//! Story 1 pushed once, at the first join, from a task spawned on the joining
//! connection. Story 2 re-pushes the FULL snapshot on every structural change
//! (join, leave, capability, mute), and spawning a task per change breaks two
//! things MH relies on:
//!
//! - a task that rendered an OLDER state could take its generation later and get
//!   the HIGHER number, so MH installs stale content and ignores the fresh one
//!   as stale — silent misrouting under a confirm that reads `match`; and
//! - with correct numbering, out-of-order arrival makes push N see applied
//!   N+1, which the retry loop reads as a transient mismatch, retries N (which
//!   MH ignores), and fails loudly on a policy that is actually newer.
//!
//! So the meeting actor — the single writer — renders and takes the generation,
//! then publishes `(generation, snapshot)` here through a `tokio::sync::watch`.
//! The watch is latest-wins, so a burst of changes coalesces, and this worker
//! pushes one handler's snapshots strictly in order.
//!
//! # Lifetime is the MEETING's, never a connection's
//!
//! The worker runs under the meeting actor's cancellation token. A
//! leave-triggered push must not be cancelled by the leave that caused it.
//!
//! # When nothing is sent
//!
//! The worker remembers the last generation MH CONFIRMED. A publish carrying
//! that generation is a no-op — no RPC — which is how "a cross-handler join
//! changes nothing on this handler" costs MH nothing. The actor publishes with
//! `send_replace` on EVERY reconcile, never behind an equality guard, so a
//! push that failed (and is therefore unconfirmed) is re-sent by the next
//! structural change of any kind even when this handler's snapshot did not
//! change. Recovery from a failed push is therefore "the next structural
//! change", or — after an MH restart with no change following — the story-4
//! periodic re-assert. A participant reconnect is not a structural change and
//! does not re-push.
//!
//! # After an MC restart: adopt MH's number
//!
//! [`PolicyGenerations`] is process memory; MH is not. A restarted MC pushes
//! generation 1 into a meeting MH holds at K. MH truthfully echoes K, so on a
//! classified `generation_mismatch` with `applied > sent` the worker adopts K
//! as a floor ([`PolicyGenerations::adopt_floor`]) and re-pushes at K+1 —
//! strictly above, because MH treats an equal generation as a no-op even when
//! the content differs. The re-push also replaces MH's pre-restart table
//! wholesale, so the window in which MH forwards under a stale topology is one
//! round trip after the first post-restart structural change. This is the
//! MC-restarted direction; story 4's restart detector (keyed on the echoed
//! `process_start_epoch_ms`) is the MH-restarted one, and cannot cover this.
//!
//! What no push can correct: a client→MH connection that OUTLIVES an MC
//! restart keeps the sender binding MH made when it connected, so if the
//! client's post-restart rejoin yields a different `sender_id`, frames on that
//! connection match no edge. A client that fully reconnects its MH transport
//! re-binds and recovers; closing the gap needs the client or MC to drop the MH
//! transport on MC-side session loss.
//!
//! # Retryable versus terminal
//!
//! Split per [`PushDisposition`]: `generation_mismatch` and
//! `no_applied_generation` are transient apply failures and consume retries (an
//! identical re-send is an idempotent MH no-op), while
//! `transport_mode_mismatch` is terminal — a version-skewed handler does not
//! become correct after backoff. A newer published snapshot abandons the retry
//! loop in favour of the newer snapshot.
//!
//! # No key material
//!
//! A job carries a generation and a [`HandlerAssignment`] — sender ids, slot
//! ids, behaviours. No KEK, no transmit key, no identity key.

use super::assignment::HandlerAssignment;
use super::confirm::{PolicyPushOutcome, PushDisposition};
use super::generation::PolicyGenerations;
use super::placement::HandlerEndpoint;
use crate::errors::McError;
use crate::grpc::mh_client::{MeetingProgramming, MhRegistrationClient};
use crate::observability::metrics;
use std::num::NonZeroU64;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn, Instrument};

/// Attempts per published snapshot before the worker gives up on it.
const MAX_REGISTER_ATTEMPTS: u32 = 3;

/// Backoff between attempts (one fewer entry than attempts).
const REGISTER_BACKOFF_DELAYS: [Duration; 2] = [Duration::from_secs(1), Duration::from_secs(2)];

/// One snapshot for one handler, and the generation the actor took for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushJob {
    /// The number this snapshot is pushed under.
    pub generation: NonZeroU64,
    /// The handler's complete forwarding policy for the meeting.
    pub assignment: HandlerAssignment,
}

/// What every push from one worker shares.
pub struct PushTarget {
    /// The MH registration client.
    pub mh_client: Arc<dyn MhRegistrationClient>,
    /// The shared generation registry (for floor adoption).
    pub policy_generations: Arc<PolicyGenerations>,
    /// The meeting being programmed.
    pub meeting_id: String,
    /// This MC's id.
    pub mc_id: String,
    /// This MC's advertised gRPC endpoint, for MH→MC callbacks.
    pub mc_grpc_endpoint: String,
    /// The handler this worker programs.
    pub handler: HandlerEndpoint,
}

/// Handle to one (meeting, handler) push worker.
pub struct HandlerPusher {
    jobs: watch::Sender<Option<PushJob>>,
    task: JoinHandle<()>,
}

impl HandlerPusher {
    /// Spawn the worker. It lives until `cancel` fires or this handle drops.
    #[must_use]
    pub fn spawn(target: PushTarget, cancel: CancellationToken) -> Self {
        let (jobs, rx) = watch::channel(None);
        let span = tracing::info_span!(
            target: "mc.register_meeting.trigger",
            "register_meeting_worker",
            meeting_id = %target.meeting_id,
            mh_grpc_endpoint = %target.handler.grpc_endpoint,
        );
        let task = tokio::spawn(run(target, rx, cancel).instrument(span));
        Self { jobs, task }
    }

    /// Publish the latest snapshot. Always wakes the worker, even when the
    /// value equals the last one — that is what re-sends an unconfirmed push.
    pub fn publish(&self, job: PushJob) {
        self.jobs.send_replace(Some(job));
    }

    /// Has the worker exited?
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.task.is_finished()
    }
}

impl Drop for HandlerPusher {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Disposition of a RESTART-FLOOR reply — one MH reply that `confirm`
/// deliberately did not record on `mc_media_policy_pushes_total` (see
/// [`crate::grpc::mh_client::MeetingProgramming::restart_floor_adoptable`]).
///
/// The two values are a rare recovery event and a routine coalescing event, so
/// they are separate label values rather than one merged count: merging them
/// would put a churn rate inside a number a responder reads as "how many
/// meetings recovered". Their SUM is what keeps
/// `sum(mc_media_policy_pushes_total) + sum(mc_media_policy_generation_adoptions_total)`
/// the exact count of evaluated replies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloorAdoption {
    /// The floor was adopted and the content re-pushed strictly above it. The
    /// recovery event: at most one per (meeting, handler) per MC process.
    Adopted,
    /// The reply was discarded because a newer render was already published,
    /// which is pushed (and evaluated) in its place. **No adoption happened**,
    /// so this (meeting, handler) stays adoptable and a later reply may record
    /// here again — this value has NO per-(meeting, handler) upper bound.
    /// Routine: it means renders are arriving faster than pushes settle, which
    /// is ordinary churn during a restart.
    Superseded,
}

impl FloorAdoption {
    /// Every variant, for exhaustive metric-vocabulary tests.
    pub const ALL: [Self; 2] = [Self::Adopted, Self::Superseded];

    /// Bounded metric-label form.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Adopted => "adopted",
            Self::Superseded => "superseded",
        }
    }
}

async fn run(
    target: PushTarget,
    mut jobs: watch::Receiver<Option<PushJob>>,
    cancel: CancellationToken,
) {
    let mut confirmed: Option<NonZeroU64> = None;
    // Set once a restart floor has been adopted; with `confirmed`, the
    // "first confirm in this process lifetime" discriminator for
    // `MeetingProgramming::restart_floor_adoptable`.
    let mut floor_adopted = false;
    // Set when a push was abandoned for a newer snapshot that the retry loop
    // has already observed; the newer job is taken without waiting again.
    let mut pending = false;
    loop {
        if !pending {
            tokio::select! {
                () = cancel.cancelled() => return,
                changed = jobs.changed() => {
                    if changed.is_err() {
                        return;
                    }
                }
            }
        }
        pending = false;
        let Some(job) = jobs.borrow_and_update().clone() else {
            continue;
        };
        if confirmed == Some(job.generation) {
            continue;
        }
        let first_confirm = confirmed.is_none();
        match push_until_settled(
            &target,
            job,
            first_confirm,
            &mut floor_adopted,
            &mut jobs,
            &cancel,
        )
        .await
        {
            Settled::Confirmed(generation) => confirmed = Some(generation),
            Settled::Failed => {}
            Settled::Superseded => pending = true,
            Settled::Cancelled => return,
        }
    }
}

enum Settled {
    Confirmed(NonZeroU64),
    Failed,
    Superseded,
    Cancelled,
}

async fn push_until_settled(
    target: &PushTarget,
    mut job: PushJob,
    first_confirm: bool,
    floor_adopted: &mut bool,
    jobs: &mut watch::Receiver<Option<PushJob>>,
    cancel: &CancellationToken,
) -> Settled {
    let grpc_endpoint = target.handler.grpc_endpoint.as_str();
    let mut last_error = None;
    let mut attempts_made = 0_u32;
    let mut ended_terminally = false;

    for attempt in 1..=MAX_REGISTER_ATTEMPTS {
        if cancel.is_cancelled() {
            info!(target: "mc.register_meeting.trigger", "RegisterMeeting cancelled during shutdown");
            return Settled::Cancelled;
        }
        attempts_made = attempt;
        let programming = MeetingProgramming {
            mh_grpc_endpoint: grpc_endpoint,
            expected_handler_id: target.handler.id.as_str(),
            meeting_id: &target.meeting_id,
            mc_id: &target.mc_id,
            mc_grpc_endpoint: &target.mc_grpc_endpoint,
            assignment: &job.assignment,
            policy_generation: job.generation,
            restart_floor_adoptable: first_confirm && !*floor_adopted,
        };
        match target.mh_client.register_meeting(&programming).await {
            Ok(()) => {
                debug!(
                    target: "mc.register_meeting.trigger",
                    mh_grpc_endpoint = %grpc_endpoint,
                    policy_generation = job.generation.get(),
                    "RegisterMeeting succeeded"
                );
                return Settled::Confirmed(job.generation);
            }
            // MC restarted under a live handler: adopt its number as a floor
            // and re-push strictly above it. Only from a CLASSIFIED reply (this
            // variant exists only when MH answered), and only on the first
            // confirm of this (meeting, handler) in this process lifetime — the
            // same predicate `confirm` used to NOT record the reply, so exactly
            // one of the adoption counter or `generation_mismatch` is recorded
            // below (or neither, if superseded: the newer job's reply is).
            Err(McError::MediaPolicyDivergence {
                outcome,
                applied_generation,
            }) if programming.is_restart_floor(outcome, applied_generation) => {
                // A newer render is already published: push THAT (it will be
                // adopted on its own reply) rather than install this stale
                // content at a fresh number, even transiently.
                // Counted with the adoptions (it is a restart-floor reply NOT
                // recorded as `generation_mismatch`), so pushes + adoptions
                // stays the exact count of evaluated replies.
                //
                // `floor_adopted` MUST STAY FALSE here, and that is
                // load-bearing: no adoption happened, so the pair has to remain
                // adoptable for the newer job's reply to adopt from.
                // Accountability moves to the newer push rather than
                // evaporating. Setting it here would cost the pair its one
                // adoption and the restart recovery would silently stop
                // working — with nothing to see, because this path is the
                // routine one.
                if jobs.has_changed().unwrap_or(false) {
                    metrics::record_policy_generation_adoption(FloorAdoption::Superseded);
                    return Settled::Superseded;
                }
                match target
                    .policy_generations
                    .adopt_floor(
                        &target.meeting_id,
                        &target.handler.id,
                        applied_generation,
                        &job.assignment,
                    )
                    .await
                {
                    Ok(generation) => {
                        *floor_adopted = true;
                        metrics::record_policy_generation_adoption(FloorAdoption::Adopted);
                        warn!(
                            target: "mc.register_meeting.trigger",
                            mh_grpc_endpoint = %grpc_endpoint,
                            sent_generation = job.generation.get(),
                            applied_generation = applied_generation,
                            adopted_generation = generation.get(),
                            "Handler holds a newer policy generation than this MC issued (an MC \
                             restart under a live meeting); adopting it as a floor and re-pushing \
                             above it"
                        );
                        job.generation = generation;
                    }
                    Err(e) => {
                        // A FAILED adoption pages: `confirm` did not record this
                        // reply, so record it now as what it is.
                        metrics::record_media_policy_push(
                            PolicyPushOutcome::GenerationMismatch,
                            job.generation,
                            applied_generation,
                        );
                        error!(
                            target: "mc.register_meeting.trigger",
                            mh_grpc_endpoint = %grpc_endpoint,
                            error = %e,
                            "Could not adopt the handler's generation; MH not programmed"
                        );
                        return Settled::Failed;
                    }
                }
            }
            Err(e) => {
                // A terminal divergence does not become correct after backoff.
                let terminal = matches!(
                    &e,
                    McError::MediaPolicyDivergence { outcome, .. }
                        if outcome.disposition() == PushDisposition::Terminal
                );
                warn!(
                    target: "mc.register_meeting.trigger",
                    attempt = attempt,
                    max_attempts = MAX_REGISTER_ATTEMPTS,
                    mh_grpc_endpoint = %grpc_endpoint,
                    terminal = terminal,
                    error = %e,
                    "RegisterMeeting attempt failed"
                );
                last_error = Some(e);
                if terminal {
                    ended_terminally = true;
                    break;
                }
                // A newer snapshot supersedes this one: stop retrying stale
                // content. `run` picks the newer job up immediately.
                if jobs.has_changed().unwrap_or(false) {
                    return Settled::Superseded;
                }
                if let Some(&delay) = attempt
                    .checked_sub(1)
                    .and_then(|i| REGISTER_BACKOFF_DELAYS.get(i as usize))
                {
                    tokio::select! {
                        () = cancel.cancelled() => {
                            info!(target: "mc.register_meeting.trigger", "RegisterMeeting cancelled during shutdown");
                            return Settled::Cancelled;
                        }
                        // A newer snapshot during backoff supersedes this one.
                        changed = jobs.changed() => {
                            return if changed.is_ok() { Settled::Superseded } else { Settled::Cancelled };
                        }
                        () = tokio::time::sleep(delay) => {}
                    }
                }
            }
        }
    }

    // TWO EXIT MESSAGES, because the two dispositions have OPPOSITE remedies.
    // `mc-incident-response.md` Scenario 12 keys on the literal string
    // "RegisterMeeting retries exhausted" (flaky coordination: investigate the
    // transport). A terminal `transport_mode_mismatch` is a genuine two-ends
    // version skew whose remedy is to roll MH FORWARD; reporting it as retries
    // exhausted would assert a retry history that never happened.
    if let Some(e) = last_error {
        if ended_terminally {
            error!(
                target: "mc.register_meeting.trigger",
                mh_grpc_endpoint = %grpc_endpoint,
                attempts_made = attempts_made,
                terminal = true,
                error = %e,
                "RegisterMeeting failed terminally; not retried"
            );
        } else {
            error!(
                target: "mc.register_meeting.trigger",
                mh_grpc_endpoint = %grpc_endpoint,
                total_attempts = MAX_REGISTER_ATTEMPTS,
                attempts_made = attempts_made,
                terminal = false,
                error = %e,
                "RegisterMeeting retries exhausted"
            );
        }
    }
    Settled::Failed
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::media_routing::assignment::HandlerId;
    use std::collections::VecDeque;
    use std::pin::Pin;
    use std::sync::Mutex;
    use tokio::sync::Notify;

    /// Scripted MH client: pops one result per call and records every
    /// generation it was sent.
    struct ScriptedMh {
        results: Mutex<VecDeque<Result<(), McError>>>,
        sent: Mutex<Vec<u64>>,
        /// Every assignment sent, paired with the generation it carried.
        sent_assignments: Mutex<Vec<(u64, HandlerAssignment)>>,
        /// `restart_floor_adoptable` as sent on each call.
        sent_adoptable: Mutex<Vec<bool>>,
        called: Notify,
        /// When set, each call waits here before returning (holds an RPC
        /// "in flight" under test control).
        gate: Option<Arc<tokio::sync::Semaphore>>,
    }

    impl ScriptedMh {
        fn new(results: Vec<Result<(), McError>>) -> Arc<Self> {
            Arc::new(Self {
                results: Mutex::new(VecDeque::from(results)),
                sent: Mutex::new(Vec::new()),
                sent_assignments: Mutex::new(Vec::new()),
                sent_adoptable: Mutex::new(Vec::new()),
                called: Notify::new(),
                gate: None,
            })
        }

        fn gated(
            results: Vec<Result<(), McError>>,
            gate: Arc<tokio::sync::Semaphore>,
        ) -> Arc<Self> {
            Arc::new(Self {
                results: Mutex::new(VecDeque::from(results)),
                sent: Mutex::new(Vec::new()),
                sent_assignments: Mutex::new(Vec::new()),
                sent_adoptable: Mutex::new(Vec::new()),
                called: Notify::new(),
                gate: Some(gate),
            })
        }

        fn sent(&self) -> Vec<u64> {
            self.sent.lock().unwrap().clone()
        }
    }

    impl MhRegistrationClient for ScriptedMh {
        fn register_meeting<'a>(
            &'a self,
            programming: &'a MeetingProgramming<'a>,
        ) -> Pin<Box<dyn std::future::Future<Output = Result<(), McError>> + Send + 'a>> {
            self.sent
                .lock()
                .unwrap()
                .push(programming.policy_generation.get());
            self.sent_adoptable
                .lock()
                .unwrap()
                .push(programming.restart_floor_adoptable);
            self.sent_assignments.lock().unwrap().push((
                programming.policy_generation.get(),
                programming.assignment.clone(),
            ));
            let result = self.results.lock().unwrap().pop_front().unwrap_or(Ok(()));
            self.called.notify_one();
            let gate = self.gate.clone();
            Box::pin(async move {
                if let Some(gate) = gate {
                    gate.acquire().await.expect("gate never closed").forget();
                }
                result
            })
        }
    }

    fn target(mh: Arc<ScriptedMh>, generations: Arc<PolicyGenerations>) -> PushTarget {
        PushTarget {
            mh_client: mh,
            policy_generations: generations,
            meeting_id: "m".to_string(),
            mc_id: "mc".to_string(),
            mc_grpc_endpoint: "http://mc:50052".to_string(),
            handler: HandlerEndpoint {
                id: HandlerId::new("mh-0"),
                webtransport_url: "https://mh-0:4434".to_string(),
                grpc_endpoint: "http://mh-0:50053".to_string(),
            },
        }
    }

    fn job(generation: u64) -> PushJob {
        PushJob {
            generation: NonZeroU64::new(generation).unwrap(),
            assignment: HandlerAssignment::default(),
        }
    }

    fn mismatch(applied: u64) -> Result<(), McError> {
        Err(McError::MediaPolicyDivergence {
            outcome: PolicyPushOutcome::GenerationMismatch,
            applied_generation: applied,
        })
    }

    /// Settle: let the worker drain whatever it has been published.
    async fn settle() {
        for _ in 0..50 {
            tokio::task::yield_now().await;
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_confirmed_generation_is_not_pushed_again() {
        let mh = ScriptedMh::new(vec![]);
        let pusher = HandlerPusher::spawn(
            target(Arc::clone(&mh), Arc::default()),
            CancellationToken::new(),
        );
        pusher.publish(job(1));
        settle().await;
        pusher.publish(job(1));
        settle().await;
        assert_eq!(
            mh.sent(),
            vec![1],
            "an unchanged, confirmed snapshot costs MH nothing"
        );
    }

    /// A failed push stays unconfirmed, so the next publish of the SAME
    /// generation — e.g. after a cross-handler join left this handler's
    /// snapshot unchanged — re-sends it exactly once.
    #[tokio::test(start_paused = true)]
    async fn an_unconfirmed_generation_is_retried_on_the_next_publish_at_the_same_number() {
        let mh = ScriptedMh::new(vec![
            Err(McError::Grpc("down".into())),
            Err(McError::Grpc("down".into())),
            Err(McError::Grpc("down".into())),
        ]);
        let pusher = HandlerPusher::spawn(
            target(Arc::clone(&mh), Arc::default()),
            CancellationToken::new(),
        );
        pusher.publish(job(1));
        tokio::time::sleep(Duration::from_secs(10)).await;
        assert_eq!(mh.sent(), vec![1, 1, 1], "three attempts, then give up");

        pusher.publish(job(1));
        settle().await;
        assert_eq!(
            mh.sent(),
            vec![1, 1, 1, 1],
            "exactly one re-send at the same generation"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_terminal_divergence_is_not_retried() {
        let mh = ScriptedMh::new(vec![Err(McError::MediaPolicyDivergence {
            outcome: PolicyPushOutcome::TransportModeMismatch,
            applied_generation: 1,
        })]);
        let pusher = HandlerPusher::spawn(
            target(Arc::clone(&mh), Arc::default()),
            CancellationToken::new(),
        );
        pusher.publish(job(1));
        tokio::time::sleep(Duration::from_secs(10)).await;
        assert_eq!(mh.sent(), vec![1]);
    }

    /// OPS-16: an MC restart against a handler holding K. The registry is
    /// rebuilt from scratch (the restart), MC sends 1, MH echoes 7, MC adopts
    /// and re-pushes at 8, and MH confirms — no unbounded mismatch loop.
    #[tokio::test(start_paused = true)]
    async fn an_mc_restart_adopts_the_handlers_generation_and_pushes_strictly_above_it() {
        let generations = Arc::new(PolicyGenerations::new());
        let first = generations
            .next_generation("m", &HandlerId::new("mh-0"), &HandlerAssignment::default())
            .await
            .unwrap();
        assert_eq!(first.get(), 1);

        let mh = ScriptedMh::new(vec![mismatch(7), Ok(())]);
        let pusher = HandlerPusher::spawn(
            target(Arc::clone(&mh), Arc::clone(&generations)),
            CancellationToken::new(),
        );
        pusher.publish(PushJob {
            generation: first,
            assignment: HandlerAssignment::default(),
        });
        settle().await;
        assert_eq!(mh.sent(), vec![1, 8], "re-push at K+1, never at K");

        // Steady state afterwards: the unchanged snapshot carries 8 and is a no-op.
        let again = generations
            .next_generation("m", &HandlerId::new("mh-0"), &HandlerAssignment::default())
            .await
            .unwrap();
        assert_eq!(again.get(), 8);
        pusher.publish(PushJob {
            generation: again,
            assignment: HandlerAssignment::default(),
        });
        settle().await;
        assert_eq!(mh.sent(), vec![1, 8]);
    }

    const KEY_CUSTODY: (&str, &str) = ("key_custody", "operator");

    /// Exactly one of the adoption counter or `generation_mismatch` per
    /// restart-floor reply: a successful adopt records the adoption and NOT a
    /// mismatch, so `MCMediaGenerationDivergence` does not page on a rollout.
    #[tokio::test(start_paused = true)]
    async fn a_successful_adoption_is_counted_and_not_recorded_as_a_mismatch() {
        use ::common::observability::testing::MetricAssertion;
        let snap = MetricAssertion::snapshot();
        let generations = Arc::new(PolicyGenerations::new());
        let first = generations
            .next_generation("m", &HandlerId::new("mh-0"), &HandlerAssignment::default())
            .await
            .unwrap();
        let mh = ScriptedMh::new(vec![mismatch(7), Ok(())]);
        let pusher = HandlerPusher::spawn(
            target(Arc::clone(&mh), Arc::clone(&generations)),
            CancellationToken::new(),
        );
        pusher.publish(PushJob {
            generation: first,
            assignment: HandlerAssignment::default(),
        });
        settle().await;
        assert_eq!(mh.sent(), vec![1, 8]);
        assert_eq!(
            *mh.sent_adoptable.lock().unwrap(),
            vec![true, false],
            "only the first confirm is adoptable; the re-push above the floor is not"
        );
        snap.counter("mc_media_policy_generation_adoptions_total")
            .with_labels(&[("outcome", "adopted"), KEY_CUSTODY])
            .assert_delta(1);
        snap.counter("mc_media_policy_generation_adoptions_total")
            .with_labels(&[("outcome", "superseded"), KEY_CUSTODY])
            .assert_delta(0);
        snap.counter("mc_media_policy_pushes_total")
            .with_labels(&[("outcome", "generation_mismatch"), KEY_CUSTODY])
            .assert_delta(0);
    }

    /// A FAILED adoption still pages: the reply `confirm` declined to record is
    /// recorded as `generation_mismatch`, and nothing is counted as adopted.
    /// `u64::MAX` is the ratchet wedge: no floor above it exists.
    #[tokio::test(start_paused = true)]
    async fn a_failed_adoption_is_recorded_as_a_mismatch() {
        use ::common::observability::testing::MetricAssertion;
        let snap = MetricAssertion::snapshot();
        let mh = ScriptedMh::new(vec![mismatch(u64::MAX)]);
        let pusher = HandlerPusher::spawn(
            target(Arc::clone(&mh), Arc::default()),
            CancellationToken::new(),
        );
        pusher.publish(job(1));
        settle().await;
        assert_eq!(mh.sent(), vec![1], "no re-push without a floor");
        snap.counter("mc_media_policy_pushes_total")
            .with_labels(&[("outcome", "generation_mismatch"), KEY_CUSTODY])
            .assert_delta(1);
        for o in FloorAdoption::ALL {
            snap.counter("mc_media_policy_generation_adoptions_total")
                .with_labels(&[("outcome", o.label()), KEY_CUSTODY])
                .assert_delta(0);
        }
    }

    /// Fail-closed discriminator: once a push is confirmed, a later higher
    /// echo is NOT adoptable (so `confirm` records it and it pages) and the
    /// pusher does not adopt it.
    #[tokio::test(start_paused = true)]
    async fn a_higher_echo_after_a_confirm_is_not_adopted() {
        let mh = ScriptedMh::new(vec![Ok(()), mismatch(50)]);
        let pusher = HandlerPusher::spawn(
            target(Arc::clone(&mh), Arc::default()),
            CancellationToken::new(),
        );
        pusher.publish(job(1));
        settle().await;
        pusher.publish(job(2));
        settle().await;
        assert_eq!(&mh.sent()[..2], &[1, 2], "no adoption re-push at 51");
        assert!(
            !mh.sent().contains(&51),
            "never adopted after a confirm: {:?}",
            mh.sent()
        );
        assert_eq!(mh.sent_adoptable.lock().unwrap()[..2], [true, false]);
    }

    fn assignment_for(subscriber: u16) -> HandlerAssignment {
        use crate::media_routing::assignment::{
            EgressStreamPlan, AUDIO_PRIORITY_GROUP, MAIN_AUDIO_STREAM_NUMBER,
        };
        use proto_gen::dark_tower::signaling::v1::TransportMode;
        use std::num::NonZeroU16;
        let sender =
            |n: u16| crate::media_admission::SenderId::from_nonzero(NonZeroU16::new(n).unwrap());
        HandlerAssignment {
            egress_streams: vec![EgressStreamPlan {
                egress_stream_id: u32::from(subscriber) << 8,
                subscriber: sender(subscriber),
                slot_id: 0,
                candidate_sources: vec![sender(subscriber + 100)],
                stream_number: MAIN_AUDIO_STREAM_NUMBER,
                priority_group: AUDIO_PRIORITY_GROUP,
                supersede_on_independent_frame: false,
                transport_mode: TransportMode::Datagram,
            }],
        }
    }

    /// @paired-media-handler F1: MC restart, then a burst of renders while the
    /// first (stale) push is in flight. MH holds 3. The LAST assignment MH
    /// accepts must be the newest render, never the stale one under a borrowed
    /// number.
    #[tokio::test(start_paused = true)]
    async fn a_render_burst_during_adoption_never_leaves_stale_content_installed() {
        let generations = Arc::new(PolicyGenerations::new());
        let h = HandlerId::new("mh-0");
        let a = assignment_for(1);
        let d = assignment_for(4);
        let g1 = generations.next_generation("m", &h, &a).await.unwrap();

        let gate = Arc::new(tokio::sync::Semaphore::new(0));
        // Reply 1: MH holds 3 (echo 3 > 1). Every later reply: applied.
        let mh = ScriptedMh::gated(vec![mismatch(3)], Arc::clone(&gate));
        let pusher = HandlerPusher::spawn(
            target(Arc::clone(&mh), Arc::clone(&generations)),
            CancellationToken::new(),
        );
        pusher.publish(PushJob {
            generation: g1,
            assignment: a.clone(),
        });
        mh.called.notified().await; // (1, A) is in flight

        // The actor renders B, C and D meanwhile and publishes (4, D).
        for n in 2..=3 {
            generations
                .next_generation("m", &h, &assignment_for(n))
                .await
                .unwrap();
        }
        let g4 = generations.next_generation("m", &h, &d).await.unwrap();
        assert_eq!(g4.get(), 4);
        pusher.publish(PushJob {
            generation: g4,
            assignment: d.clone(),
        });

        gate.add_permits(100); // let every RPC complete
        settle().await;

        let accepted = mh.sent_assignments.lock().unwrap().clone();
        let (last_gen, last) = accepted.last().cloned().unwrap();
        assert_eq!(
            last, d,
            "MH must end on the newest render, not the stale one: {accepted:?}"
        );
        assert!(
            !accepted.iter().any(|(g, x)| *x == a && *g > 1),
            "stale content must never be re-pushed at a fresh number: {accepted:?}"
        );
        assert!(last_gen > 3, "above the handler's pre-restart generation");
    }

    /// A lower echo is the ordinary transient mismatch, not an adoption.
    #[tokio::test(start_paused = true)]
    async fn a_lower_echo_is_retried_not_adopted() {
        let mh = ScriptedMh::new(vec![mismatch(3), Ok(())]);
        let pusher = HandlerPusher::spawn(
            target(Arc::clone(&mh), Arc::default()),
            CancellationToken::new(),
        );
        pusher.publish(job(5));
        tokio::time::sleep(Duration::from_secs(5)).await;
        assert_eq!(mh.sent(), vec![5, 5]);
    }

    /// Two structural changes in flight: the worker is serialized, the newest
    /// snapshot wins, and a stale retry is abandoned rather than failing loudly
    /// on a policy that is actually newer.
    #[tokio::test(start_paused = true)]
    async fn a_newer_publish_supersedes_a_retrying_older_one() {
        let mh = ScriptedMh::new(vec![Err(McError::Grpc("blip".into())), Ok(())]);
        let pusher = HandlerPusher::spawn(
            target(Arc::clone(&mh), Arc::default()),
            CancellationToken::new(),
        );
        pusher.publish(job(1));
        mh.called.notified().await;
        pusher.publish(job(2));
        settle().await;
        tokio::time::sleep(Duration::from_secs(5)).await;
        assert_eq!(
            mh.sent(),
            vec![1, 2],
            "generation 1 abandoned, 2 pushed and confirmed"
        );
    }

    /// Rapid distinct publishes coalesce: the worker pushes the latest, not one
    /// RPC per publish.
    #[tokio::test(start_paused = true)]
    async fn a_burst_of_publishes_coalesces() {
        let mh = ScriptedMh::new(vec![]);
        let pusher = HandlerPusher::spawn(
            target(Arc::clone(&mh), Arc::default()),
            CancellationToken::new(),
        );
        for g in 1..=20 {
            pusher.publish(job(g));
        }
        settle().await;
        let sent = mh.sent();
        assert_eq!(sent.last(), Some(&20));
        assert!(
            sent.len() < 5,
            "20 publishes coalesced into {} pushes",
            sent.len()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn cancellation_stops_the_worker() {
        let cancel = CancellationToken::new();
        let mh = ScriptedMh::new(vec![]);
        let pusher = HandlerPusher::spawn(target(Arc::clone(&mh), Arc::default()), cancel.clone());
        cancel.cancel();
        settle().await;
        assert!(pusher.is_finished());
    }
}
