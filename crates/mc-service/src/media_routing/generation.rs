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
/// `tokio::sync::RwLock`, matching `mh_connection_registry.rs` and
/// `redis/client.rs` — the in-tree idiom for a shared map read on an async
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

    /// Drop every entry for a meeting.
    ///
    /// Called from `actors/controller.rs::remove_meeting()`, on the same line
    /// that already clears `mh_connection_registry` — the meeting-teardown
    /// choke point, matching the in-tree precedent where `redis/client.rs`
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
        EgressStreamPlan, AUDIO_PRIORITY_GROUP, MAIN_AUDIO_SLOT_ID, MAIN_AUDIO_STREAM_NUMBER,
    };
    use proto_gen::dark_tower::signaling::v1::TransportMode;
    use std::num::NonZeroU16;

    fn assignment(subscriber: u16) -> HandlerAssignment {
        HandlerAssignment {
            egress_streams: vec![EgressStreamPlan {
                egress_stream_id: u32::from(subscriber) << 8,
                subscriber: SenderId::from_nonzero(NonZeroU16::new(subscriber).unwrap()),
                slot_id: MAIN_AUDIO_SLOT_ID,
                candidate_sources: vec![SenderId::from_nonzero(
                    NonZeroU16::new(subscriber).unwrap(),
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
}
