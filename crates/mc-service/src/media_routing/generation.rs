//! `policy_generation`, derived from the assignment computation's OUTPUT CHANGE
//! (ADR-0036 §8).
//!
//! # Three generations in this codebase, and they do not collapse
//!
//! Stated once, here, in the sibling-boundary style `media_admission/` uses,
//! because "generation" appears three times in MC and each is a different
//! instrument with a different owner:
//!
//! | Generation | Home | Advances on |
//! |---|---|---|
//! | Redis **fencing** generation | `redis/client.rs` (`get_generation()` / `increment_generation()`) | any fenced write, including writes that change no forwarding output |
//! | `JoinResponse.kek_generation` | `media_admission/kek.rs` | KEK rotation (deferred; always 0 today) |
//! | **`policy_generation`** | this module | the forwarding assignment's output actually changing |
//!
//! The one that matters is the first. `internal.proto` names the field
//! `policy_generation` rather than `generation` specifically to stop an
//! implementer sourcing it from the fencing counter — a fencing counter
//! advances on writes that leave forwarding untouched, so MH would see a new
//! number for an unchanged policy and reinstall on every fenced write, while
//! §8's "an unchanged policy carries the same number" property silently died.
//!
//! **Nothing in this module imports `redis`,** and that absence is the check.

use super::assignment::{HandlerAssignment, HandlerId};
use std::collections::HashMap;
use std::num::NonZeroU64;
use tokio::sync::RwLock;

/// The generation space for one (meeting, handler) pair is exhausted.
///
/// Terminal for that pair within this process. Modelled on
/// [`crate::media_admission::SenderIdSpaceExhausted`]: a bounded namespace that
/// **refuses** rather than wraps or saturates.
///
/// Saturating at `u64::MAX` would be a silent regression of exactly the kind
/// this module exists to prevent — a *changed* assignment would carry the
/// *same* number, MH would honour §8 and no-op it, and MC's confirm would read
/// `applied == sent` and report `match`. A green metric over a policy that was
/// never installed. So this rejects instead.
///
/// Unreachable in practice (it needs 2^64 - 1 distinct assignments for one
/// (meeting, handler) inside one MC process), which is why it is an error type
/// and not an alert.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenerationSpaceExhausted;

impl std::fmt::Display for GenerationSpaceExhausted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("policy generation space exhausted for this meeting and handler")
    }
}

impl std::error::Error for GenerationSpaceExhausted {}

/// What MC last programmed onto one (meeting, handler) pair.
#[derive(Debug, Clone)]
struct Programmed {
    /// The assignment that generation carries. Retained so an identical
    /// recomputation is recognised as identical.
    assignment: HandlerAssignment,
    /// The number that assignment was pushed under.
    generation: NonZeroU64,
}

/// Process-wide registry of `(meeting_id, handler_id) -> (assignment, generation)`.
///
/// # Why this shape
///
/// This is exactly the state a re-assert cadence needs (ADR-0036 §8's
/// deferred half): a cadence task walks this map and re-pushes each entry at
/// its existing generation, which MH treats as an idempotent no-op. Building
/// the one-shot push against a *registry* rather than against a join event is
/// what makes that later story purely additive.
///
/// `tokio::sync::RwLock`, matching `redis/client.rs` — the in-tree idiom for a shared map read on an async
/// path. A `std::sync::Mutex` would need poison handling that cannot use
/// `unwrap`/`expect` under the workspace lints.
#[derive(Debug, Default)]
pub struct PolicyGenerations {
    programmed: RwLock<HashMap<(String, HandlerId), Programmed>>,
}

impl PolicyGenerations {
    /// Create an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The number to push `new_assignment` to this handler under, recording it
    /// as the pair's latest programmed state.
    ///
    /// - First ever push for the pair -> **1**. Never 0: the type is
    ///   [`NonZeroU64`], so "0 on the wire" is unrepresentable rather than
    ///   merely forbidden, and MC can never ship the
    ///   `policy_generation: 1`-with-empty-policy workaround `internal.proto`
    ///   warns against.
    /// - Structurally identical to what was last pushed -> the **same** number,
    ///   so a re-assert is MH's no-op (§8).
    /// - Different -> previous + 1.
    ///
    /// Independent per (meeting, handler): two handlers of one meeting advance
    /// separately, because each is programmed with its own policy.
    ///
    /// # Errors
    ///
    /// [`GenerationSpaceExhausted`] if the pair's counter cannot advance.
    pub async fn next_generation(
        &self,
        meeting_id: &str,
        handler: &HandlerId,
        new_assignment: &HandlerAssignment,
    ) -> Result<NonZeroU64, GenerationSpaceExhausted> {
        let key = (meeting_id.to_string(), handler.clone());
        let mut programmed = self.programmed.write().await;

        let next = match programmed.get(&key) {
            // Unchanged output carries the unchanged number. This is the §8
            // property the whole module exists for; it is why the assignment is
            // retained rather than only the number.
            Some(prev) if prev.assignment == *new_assignment => prev.generation,
            Some(prev) => prev
                .generation
                .checked_add(1)
                .ok_or(GenerationSpaceExhausted)?,
            None => NonZeroU64::MIN,
        };

        programmed.insert(
            key,
            Programmed {
                assignment: new_assignment.clone(),
                generation: next,
            },
        );
        Ok(next)
    }

    /// The pair's current (latest recorded) generation, or `None` if nothing
    /// was ever rendered for it. Read-only: lets a caller prove "nothing new
    /// was rendered" deterministically — the actor records a generation BEFORE
    /// publishing it, and the pusher never pushes a generation it already
    /// confirmed — rather than by waiting to see whether a push shows up.
    pub async fn current(&self, meeting_id: &str, handler: &HandlerId) -> Option<NonZeroU64> {
        self.programmed
            .read()
            .await
            .get(&(meeting_id.to_string(), handler.clone()))
            .map(|p| p.generation)
    }

    /// Raise this pair's counter above a generation the handler reports it has
    /// ALREADY applied, recording `assignment` as the pair's latest programmed
    /// state, and return the number to push it under.
    ///
    /// # Why this exists: an MC restart against a live handler
    ///
    /// This registry is process memory, so an MC restart starts every pair at
    /// 1 again, while MH still holds the meeting at the K it applied before the
    /// restart. MH ignores a LOWER generation as stale, and ignores an EQUAL one
    /// even when the content differs, so the "next structural change re-pushes"
    /// recovery would climb 1, 2, 3 … and need K changes to escape — with
    /// `MCMediaGenerationDivergence` paging throughout. MH echoes its
    /// truthfully applied number on every reply, so the pusher adopts it: the
    /// returned number is **strictly above** `applied` (K+1), never equal to it.
    ///
    /// Covers the MC-restarted direction. The MH-restarted direction does not
    /// reach here at all: a restarted MH has applied nothing, so MC's next push
    /// lands `applied == sent`.
    ///
    /// # Not a fencing bypass, and it only ever raises
    ///
    /// `policy_generation` was never the ownership instrument — fencing lives in
    /// `redis/client.rs` (see this module's table) and meeting ownership is
    /// sticky to one MC. Adopting a floor corrects the counter's ORIGIN; it does
    /// not advance an unchanged assignment, so "an unchanged policy carries the
    /// same number" still holds from here on. If this pair is already above
    /// `applied` AND its recorded assignment is this one, the counter is left
    /// alone and its current number is returned; if the pair has moved on to a
    /// different render, this content gets a number above both.
    ///
    /// Callers must take `applied` only from a reply the confirm step actually
    /// classified. A structural reject carries no body and no echo; reading a
    /// default 0 from it would be meaningless.
    ///
    /// # Errors
    ///
    /// [`GenerationSpaceExhausted`] if `applied + 1` does not fit.
    pub async fn adopt_floor(
        &self,
        meeting_id: &str,
        handler: &HandlerId,
        applied: u64,
        assignment: &HandlerAssignment,
    ) -> Result<NonZeroU64, GenerationSpaceExhausted> {
        let key = (meeting_id.to_string(), handler.clone());
        let mut programmed = self.programmed.write().await;

        // The returned generation is ALWAYS the one recorded against the
        // returned content. Reusing the pair's current number is correct only
        // when it already carries THIS assignment; if the actor has since
        // recorded a newer render under that number, pushing this (older)
        // content at it would install stale edges that a later push of the
        // newer render at the same number could never replace — MH treats an
        // equal generation as a no-op — while the confirm read `match`.
        let floor = match programmed.get(&key) {
            Some(prev) if prev.generation.get() > applied && prev.assignment == *assignment => {
                return Ok(prev.generation);
            }
            Some(prev) => prev.generation.get().max(applied),
            None => applied,
        };
        let next = floor
            .checked_add(1)
            .and_then(NonZeroU64::new)
            .ok_or(GenerationSpaceExhausted)?;
        programmed.insert(
            key,
            Programmed {
                assignment: assignment.clone(),
                generation: next,
            },
        );
        Ok(next)
    }

    /// Drop every entry for a meeting.
    ///
    /// Called from BOTH meeting-teardown paths in `actors/controller.rs` —
    /// `remove_meeting()` and the `check_meeting_health()` reaper, one teardown
    /// contract at two sites — matching the in-tree precedent where `redis/client.rs`
    /// clears its `local_generation` on `delete_meeting`. Without it the map
    /// retains a whole `HandlerAssignment` per ended meeting for the pod's
    /// lifetime.
    ///
    /// **Keyed on meeting teardown, never on handler removal.** A meeting whose
    /// handler set changes must keep its generations: dropping them would
    /// restart that pair at 1 *inside a live meeting*, which MH correctly
    /// ignores as lower than what it has applied — the restart-at-1 problem,
    /// reintroduced without even a process restart to explain it.
    pub async fn remove_meeting(&self, meeting_id: &str) {
        let mut programmed = self.programmed.write().await;
        programmed.retain(|(meeting, _handler), _| meeting != meeting_id);
    }

    /// Number of tracked (meeting, handler) pairs. Test and diagnostics only.
    #[cfg(test)]
    async fn len(&self) -> usize {
        self.programmed.read().await.len()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::media_admission::SenderId;
    use crate::media_routing::assignment::{
        EgressStreamPlan, AUDIO_PRIORITY_GROUP, MAIN_AUDIO_STREAM_NUMBER,
    };
    use proto_gen::dark_tower::signaling::v1::TransportMode;
    use std::num::NonZeroU16;

    fn assignment(subscriber: u16) -> HandlerAssignment {
        HandlerAssignment {
            egress_streams: vec![EgressStreamPlan {
                egress_stream_id: u32::from(subscriber) << 8,
                subscriber: SenderId::from_nonzero(NonZeroU16::new(subscriber).unwrap()),
                slot_id: 0,
                // Any other participant; the generation is blind to who.
                candidate_sources: vec![SenderId::from_nonzero(
                    NonZeroU16::new(subscriber + 100).unwrap(),
                )],
                stream_number: MAIN_AUDIO_STREAM_NUMBER,
                priority_group: AUDIO_PRIORITY_GROUP,
                supersede_on_independent_frame: false,
                transport_mode: TransportMode::Datagram,
            }],
        }
    }

    fn handler(name: &str) -> HandlerId {
        HandlerId::new(name)
    }

    #[tokio::test]
    async fn first_generation_for_a_pair_is_one_never_zero() {
        let registry = PolicyGenerations::new();
        let g = registry
            .next_generation("m1", &handler("mh-0"), &assignment(1))
            .await
            .unwrap();
        assert_eq!(g.get(), 1);
    }

    #[tokio::test]
    async fn identical_recomputation_carries_the_same_number() {
        let registry = PolicyGenerations::new();
        let first = registry
            .next_generation("m1", &handler("mh-0"), &assignment(1))
            .await
            .unwrap();
        let second = registry
            .next_generation("m1", &handler("mh-0"), &assignment(1))
            .await
            .unwrap();
        assert_eq!(
            first, second,
            "an unchanged assignment must re-assert at the same generation, or MH \
             reinstalls on every push"
        );
    }

    #[tokio::test]
    async fn changed_output_advances_by_one() {
        let registry = PolicyGenerations::new();
        let first = registry
            .next_generation("m1", &handler("mh-0"), &assignment(1))
            .await
            .unwrap();
        let second = registry
            .next_generation("m1", &handler("mh-0"), &assignment(2))
            .await
            .unwrap();
        assert_eq!(second.get(), first.get() + 1);
    }

    /// Advance, then go back to the original assignment: it is a *change* from
    /// what was last programmed, so it must advance again rather than reuse the
    /// old number. Reusing it would tell MH "you already have this" about a
    /// policy MH replaced two pushes ago.
    #[tokio::test]
    async fn returning_to_an_earlier_assignment_still_advances() {
        let registry = PolicyGenerations::new();
        let a = registry
            .next_generation("m1", &handler("mh-0"), &assignment(1))
            .await
            .unwrap();
        registry
            .next_generation("m1", &handler("mh-0"), &assignment(2))
            .await
            .unwrap();
        let c = registry
            .next_generation("m1", &handler("mh-0"), &assignment(1))
            .await
            .unwrap();
        assert_eq!(c.get(), a.get() + 2);
    }

    #[tokio::test]
    async fn generations_are_independent_per_handler_and_per_meeting() {
        let registry = PolicyGenerations::new();

        registry
            .next_generation("m1", &handler("mh-0"), &assignment(1))
            .await
            .unwrap();
        registry
            .next_generation("m1", &handler("mh-0"), &assignment(2))
            .await
            .unwrap();

        // A different handler of the same meeting starts fresh.
        let other_handler = registry
            .next_generation("m1", &handler("mh-1"), &assignment(1))
            .await
            .unwrap();
        assert_eq!(other_handler.get(), 1);

        // A different meeting on the same handler starts fresh.
        let other_meeting = registry
            .next_generation("m2", &handler("mh-0"), &assignment(1))
            .await
            .unwrap();
        assert_eq!(other_meeting.get(), 1);
    }

    #[tokio::test]
    async fn remove_meeting_evicts_every_handler_of_that_meeting_only() {
        let registry = PolicyGenerations::new();
        registry
            .next_generation("m1", &handler("mh-0"), &assignment(1))
            .await
            .unwrap();
        registry
            .next_generation("m1", &handler("mh-1"), &assignment(1))
            .await
            .unwrap();
        registry
            .next_generation("m2", &handler("mh-0"), &assignment(1))
            .await
            .unwrap();
        assert_eq!(registry.len().await, 3);

        registry.remove_meeting("m1").await;

        assert_eq!(registry.len().await, 1, "both m1 handlers evicted");
        let m2_again = registry
            .next_generation("m2", &handler("mh-0"), &assignment(1))
            .await
            .unwrap();
        assert_eq!(m2_again.get(), 1, "the surviving meeting kept its state");
    }

    #[tokio::test]
    async fn remove_meeting_is_idempotent_and_tolerates_an_unknown_meeting() {
        let registry = PolicyGenerations::new();
        registry.remove_meeting("never-seen").await;
        registry
            .next_generation("m1", &handler("mh-0"), &assignment(1))
            .await
            .unwrap();
        registry.remove_meeting("m1").await;
        registry.remove_meeting("m1").await;
        assert_eq!(registry.len().await, 0);
    }

    #[tokio::test]
    async fn adopting_a_floor_pushes_strictly_above_it_and_then_holds_steady() {
        let registry = PolicyGenerations::new();
        let a = assignment(1);
        let h = handler("mh-0");
        // A restarted MC starts at 1 while the handler holds 7.
        assert_eq!(
            registry.next_generation("m", &h, &a).await.unwrap().get(),
            1
        );
        assert_eq!(registry.adopt_floor("m", &h, 7, &a).await.unwrap().get(), 8);
        // An unchanged assignment now carries the adopted number...
        assert_eq!(
            registry.next_generation("m", &h, &a).await.unwrap().get(),
            8
        );
        // ...and a change advances from it.
        assert_eq!(
            registry
                .next_generation("m", &h, &assignment(2))
                .await
                .unwrap()
                .get(),
            9
        );
    }

    #[tokio::test]
    async fn a_floor_below_the_counter_never_lowers_it() {
        let registry = PolicyGenerations::new();
        let h = handler("mh-0");
        registry
            .next_generation("m", &h, &assignment(1))
            .await
            .unwrap();
        registry
            .next_generation("m", &h, &assignment(2))
            .await
            .unwrap();
        registry
            .next_generation("m", &h, &assignment(3))
            .await
            .unwrap();
        assert_eq!(
            registry
                .adopt_floor("m", &h, 1, &assignment(3))
                .await
                .unwrap()
                .get(),
            3
        );
    }

    /// @paired-media-handler F1: an MC restart followed by a burst of renders.
    /// The worker is confirming an OLD job while the actor has already recorded
    /// a newer render above the handler's number. Adoption must never hand the
    /// old content the newer render's number.
    #[tokio::test]
    async fn a_floor_never_pairs_an_old_assignment_with_a_newer_renders_number() {
        let registry = PolicyGenerations::new();
        let h = handler("mh-0");
        for n in 1..=4 {
            registry
                .next_generation("m", &h, &assignment(n))
                .await
                .unwrap();
        }
        // Registry holds (assignment(4), 4). The worker's in-flight job carried
        // assignment(1) at 1, and MH echoes 3.
        let adopted = registry
            .adopt_floor("m", &h, 3, &assignment(1))
            .await
            .unwrap();
        assert_eq!(
            adopted.get(),
            5,
            "above both the recorded 4 and the applied 3"
        );
        // And the newest render then advances past it rather than colliding.
        let next = registry
            .next_generation("m", &h, &assignment(4))
            .await
            .unwrap();
        assert_eq!(next.get(), 6);
    }

    #[tokio::test]
    async fn a_floor_at_the_top_of_the_space_refuses_rather_than_wrapping() {
        let registry = PolicyGenerations::new();
        assert_eq!(
            registry
                .adopt_floor("m", &handler("mh-0"), u64::MAX, &assignment(1))
                .await,
            Err(GenerationSpaceExhausted)
        );
    }
}
