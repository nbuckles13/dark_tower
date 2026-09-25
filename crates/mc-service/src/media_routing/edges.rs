//! Which shared handler carries one publisher→subscriber edge (ADR-0036 §9;
//! story 2 R-33).
//!
//! # The rule this module does NOT decide
//!
//! WHETHER an edge exists is decided elsewhere (`slots.rs`): a subscriber hears
//! a sender if and only if the two share at least one connected handler, and
//! the edge is then assigned to exactly one handler IN that shared set. This
//! module only picks WHICH member of a non-empty shared set. That choice is
//! policy, and **correctness must hold for any choice**: the slot table's model
//! check runs under [`colocate`] AND an adversarial chooser, and no test, no
//! client and no other code may depend on which handler was picked.
//!
//! # The type makes a non-candidate unreturnable
//!
//! An [`EdgeChooser`] returns `&'a HandlerId` borrowed from the candidate slice
//! it was given (never from the context), so it cannot name a handler outside
//! the shared set. Together with `ConnectedHandlers` (whose only insert takes a
//! resolved `HandlerEndpoint`), that carries "every edge handler is one both
//! parties are observed on" by type rather than by a check at each call site.
//!
//! # The production policy: fewest send targets per sender
//!
//! [`colocate`] prefers, in order: the handler already carrying most of THIS
//! sender's out-edges (a sender sends to every handler owning one of its edges,
//! so reusing one adds no send target — the §9 client-uplink factor); then the
//! handler carrying most of the MEETING's edges (so an all-connected meeting
//! lands every edge on one handler, which is what the rule produces rather than
//! a special case); then a deterministic tiebreak on candidate order, which
//! exists only so an unchanged input renders byte-identically.
//!
//! # The tiebreak is SEEDED PER MEETING, and that part IS relied on
//!
//! The tiebreak decides nothing about correctness — that must hold for any
//! chooser, and the model check proves it — but it does decide fleet spread,
//! and read naively it decided it badly. The context is ONE meeting's, so every
//! meeting's first edge sees zero loads and hits the tiebreak; an unseeded
//! tiebreak on candidate order therefore sent EVERY all-connected meeting in
//! the fleet to the same (lowest-id) handler, halving usable egress capacity
//! while the sibling idled.
//!
//! So the candidate order is rotated by [`meeting_seed`], an FNV-1a hash of the
//! meeting id: first edges spread statistically across handlers, and the
//! outcome differs from the unrotated one ONLY when both load keys tie.
//! **Do not delete the rotation as decoration** — it is load-bearing for
//! capacity, and nothing (no test, no client, no other code) may depend on
//! WHICH handler it yields for a given meeting.
//!
//! Seeded from the MEETING and nothing else — no clock, no counter, no RNG — so
//! the choice is reproducible across MC pods and across a restart, stable for a
//! meeting's whole life (a tiebreak that varied mid-meeting would advance
//! `policy_generation` and re-push on no real change), and replayable from a
//! bug report.
//!
//! What the seed does NOT do: it knows no handler's ceiling, so one LARGE
//! meeting still concentrates its own egress on one handler and can still be
//! refused whole there. Capacity-aware spreading stays open — it needs the
//! per-handler stream ceiling exposed to MC (`docs/TODO.md` §Media Path
//! Obligations, item 1), which is also its trigger.
//!
//! Edges already placed are never re-chosen while still valid (edge stability,
//! `slots.rs`), so the objective is pursued at placement time only.

use super::assignment::HandlerId;
use std::collections::BTreeMap;

/// FNV-1a (64-bit) over the meeting id, for the per-meeting tiebreak rotation.
///
/// Hand-written rather than taken from `DefaultHasher`, which is explicitly not
/// guaranteed stable across releases: this value must be reproducible across
/// processes and versions, or two MC pods could tie-break one meeting
/// differently. Never security-relevant — it only rotates a candidate list.
#[must_use]
pub fn meeting_seed(meeting_id: &str) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    meeting_id.as_bytes().iter().fold(OFFSET, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(PRIME)
    })
}

/// What a chooser may weigh. Counts only; no identity beyond handler ids.
#[derive(Debug, Clone, Copy)]
pub struct EdgeContext<'c> {
    /// Out-edges the sender already has, per handler.
    pub sender_out: &'c BTreeMap<HandlerId, usize>,
    /// Edges the whole meeting already has, per handler.
    pub meeting: &'c BTreeMap<HandlerId, usize>,
    /// [`meeting_seed`] of the meeting these loads belong to: the tiebreak
    /// rotation, and the only thing here that is not a count.
    pub meeting_seed: u64,
}

/// Picks one handler from a NON-EMPTY candidate slice. `None` only for an
/// empty slice, which callers never pass.
pub type EdgeChooser =
    for<'a, 'c> fn(candidates: &'a [HandlerId], ctx: &EdgeContext<'c>) -> Option<&'a HandlerId>;

/// The production policy: co-locate (see the module doc).
#[must_use]
pub fn colocate<'a>(candidates: &'a [HandlerId], ctx: &EdgeContext<'_>) -> Option<&'a HandlerId> {
    let load = |map: &BTreeMap<HandlerId, usize>, h: &HandlerId| map.get(h).copied().unwrap_or(0);
    let n = candidates.len();
    if n == 0 {
        return None;
    }
    // Rotate the candidate order by the meeting's seed, then take the maximum
    // load key. `max_by_key` keeps the LAST maximum, so walking the rotated
    // order in REVERSE resolves a tie to rotated position 0 — i.e. to
    // `candidates[offset]`, this meeting's own starting point, instead of to
    // the lowest handler id for every meeting in the fleet.
    #[allow(clippy::cast_possible_truncation)]
    let offset = (ctx.meeting_seed % n as u64) as usize;
    // `offset < n`, so this never panics; `split_at` + `chain` rather than
    // indexing keeps the rotation panic-free by construction.
    let (head, tail) = candidates.split_at(offset);
    tail.iter()
        .chain(head.iter())
        .rev()
        .max_by_key(|h| (load(ctx.sender_out, h), load(ctx.meeting, h)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(name: &str) -> HandlerId {
        HandlerId::new(name)
    }

    fn loads(pairs: &[(&str, usize)]) -> BTreeMap<HandlerId, usize> {
        pairs.iter().map(|(n, c)| (h(n), *c)).collect()
    }

    #[test]
    fn an_empty_candidate_set_chooses_nothing() {
        let empty = BTreeMap::new();
        let ctx = EdgeContext {
            sender_out: &empty,
            meeting: &empty,
            meeting_seed: 0,
        };
        assert!(colocate(&[], &ctx).is_none());
    }

    /// The fewest-targets objective dominates meeting-wide co-location.
    #[test]
    fn prefers_the_handler_already_carrying_the_senders_edges() {
        let sender_out = loads(&[("mh-1", 1)]);
        let meeting = loads(&[("mh-0", 9)]);
        let ctx = EdgeContext {
            sender_out: &sender_out,
            meeting: &meeting,
            meeting_seed: 0,
        };
        let candidates = [h("mh-0"), h("mh-1")];
        assert_eq!(colocate(&candidates, &ctx).unwrap(), &h("mh-1"));
    }

    #[test]
    fn otherwise_prefers_the_meetings_busiest_handler() {
        let sender_out = BTreeMap::new();
        let meeting = loads(&[("mh-1", 2), ("mh-0", 1)]);
        let ctx = EdgeContext {
            sender_out: &sender_out,
            meeting: &meeting,
            meeting_seed: 0,
        };
        let candidates = [h("mh-0"), h("mh-1")];
        assert_eq!(colocate(&candidates, &ctx).unwrap(), &h("mh-1"));
    }

    /// Load precedence beats the seed: with a real load difference every seed
    /// gives the same answer, so the rotation can only decide genuine ties.
    #[test]
    fn the_seed_never_overrides_a_load_difference() {
        let sender_out = BTreeMap::new();
        let meeting = loads(&[("mh-1", 1)]);
        let candidates = [h("mh-0"), h("mh-1")];
        for meeting_seed in 0..64_u64 {
            let ctx = EdgeContext {
                sender_out: &sender_out,
                meeting: &meeting,
                meeting_seed,
            };
            assert_eq!(colocate(&candidates, &ctx).unwrap(), &h("mh-1"));
        }
    }

    /// The capacity property the seed exists for: on an EMPTY context (every
    /// meeting's first edge) distinct meetings do not all land on one handler.
    /// Unseeded, this test sees `mh-0` 64 times.
    #[test]
    fn first_edges_of_distinct_meetings_spread_over_both_handlers() {
        let empty = BTreeMap::new();
        let candidates = [h("mh-0"), h("mh-1")];
        let mut chosen: BTreeMap<HandlerId, usize> = BTreeMap::new();
        for n in 0..64 {
            let ctx = EdgeContext {
                sender_out: &empty,
                meeting: &empty,
                meeting_seed: meeting_seed(&format!("meeting-{n}")),
            };
            *chosen
                .entry(colocate(&candidates, &ctx).unwrap().clone())
                .or_default() += 1;
        }
        assert_eq!(chosen.len(), 2, "both handlers must be chosen: {chosen:?}");
        for (handler, count) in &chosen {
            assert!(
                *count >= 8,
                "{handler:?} took only {count}/64 first edges — the rotation is barely spreading"
            );
        }
    }

    /// One meeting id ALWAYS renders the same choice — across calls, pods and
    /// restarts. A varying tiebreak would advance `policy_generation` and
    /// re-push with no real change.
    #[test]
    fn one_meeting_id_always_yields_the_same_choice() {
        let empty = BTreeMap::new();
        let candidates = [h("mh-0"), h("mh-1")];
        let ctx = EdgeContext {
            sender_out: &empty,
            meeting: &empty,
            meeting_seed: meeting_seed("meeting-stable"),
        };
        let first = colocate(&candidates, &ctx).unwrap().clone();
        for _ in 0..16 {
            assert_eq!(colocate(&candidates, &ctx).unwrap(), &first);
        }
        // And the seed itself is a pure function of the id, not of ambient state.
        assert_eq!(
            meeting_seed("meeting-stable"),
            meeting_seed("meeting-stable")
        );
        assert_ne!(
            meeting_seed("meeting-stable"),
            meeting_seed("meeting-other")
        );
    }

    /// FNV-1a's published vectors, so a refactor cannot silently change every
    /// meeting's placement.
    #[test]
    fn the_seed_is_fnv_1a_64() {
        assert_eq!(meeting_seed(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(meeting_seed("a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(meeting_seed("foobar"), 0x8594_4171_f739_67e8);
    }

    /// Whatever the loads say, the answer is a member of the candidates.
    #[test]
    fn never_returns_a_handler_outside_the_candidates() {
        let sender_out = loads(&[("mh-9", 100)]);
        let meeting = loads(&[("mh-9", 100)]);
        let ctx = EdgeContext {
            sender_out: &sender_out,
            meeting: &meeting,
            meeting_seed: 0,
        };
        let candidates = [h("mh-0")];
        assert_eq!(colocate(&candidates, &ctx).unwrap(), &h("mh-0"));
    }
}
